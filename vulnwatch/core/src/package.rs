use percent_encoding::percent_decode_str;
use thiserror::Error;

/// The packaging scheme a version string must be compared under.
///
/// This is deliberately narrow: it names only the schemes whose version
/// ordering we implement, because guessing at a scheme we cannot compare
/// would produce silent false negatives in the matcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ecosystem {
    Deb,
    Rpm,
    Apk,
}

/// One installed package, or one package an advisory names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageRef {
    pub ecosystem: Ecosystem,
    pub namespace: Option<String>,
    pub name: String,
    pub version: Option<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PurlError {
    #[error("not a package URL: missing the `pkg:` scheme")]
    NotAPurl,
    #[error("unsupported package type `{0}`")]
    UnsupportedType(String),
    #[error("package URL has no name")]
    MissingName,
    #[error("invalid percent-encoding in package URL")]
    BadEncoding,
}

impl PackageRef {
    /// Parse a package URL, e.g. `pkg:deb/debian/openssl@1.1.1n-0+deb11u3`.
    ///
    /// Layout is `pkg:type/namespace/name@version?qualifiers#subpath`.
    /// Qualifiers and the subpath are dropped: nothing downstream reads
    /// them, and OSV ships `?arch=source` on entries that carry no version.
    pub fn from_purl(input: &str) -> Result<Self, PurlError> {
        let rest = input.strip_prefix("pkg:").ok_or(PurlError::NotAPurl)?;
        let rest = rest.split('#').next().unwrap_or(rest);
        let rest = rest.split('?').next().unwrap_or(rest);

        // Split the version at the LAST `@`, so a namespace containing one
        // cannot swallow it.
        let (path, version) = match rest.rfind('@') {
            Some(at) => (&rest[..at], Some(&rest[at + 1..])),
            None => (rest, None),
        };

        let mut segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if segments.is_empty() {
            return Err(PurlError::MissingName);
        }
        let package_type = segments.remove(0);
        let ecosystem = match package_type {
            "deb" => Ecosystem::Deb,
            "rpm" => Ecosystem::Rpm,
            "apk" => Ecosystem::Apk,
            other => return Err(PurlError::UnsupportedType(other.to_owned())),
        };

        let name = decode(segments.pop().ok_or(PurlError::MissingName)?)?;
        if name.is_empty() {
            return Err(PurlError::MissingName);
        }
        let namespace = if segments.is_empty() {
            None
        } else {
            Some(decode(&segments.join("/"))?)
        };
        let version = match version {
            Some(raw) if !raw.is_empty() => Some(decode(raw)?),
            _ => None,
        };

        Ok(Self {
            ecosystem,
            namespace,
            name,
            version,
        })
    }
}

/// Percent-decode one purl component.
///
/// `percent_decode_str` passes malformed escapes through as literal text,
/// which would let `lib%ZZ` parse into a name that silently matches nothing.
/// Validate the escapes first so bad input is rejected instead.
fn decode(value: &str) -> Result<String, PurlError> {
    let bytes = value.as_bytes();
    let mut index = 0;
    while let Some(offset) = value[index..].find('%') {
        let start = index + offset;
        let hex = bytes
            .get(start + 1..start + 3)
            .ok_or(PurlError::BadEncoding)?;
        if !hex.iter().all(|b| b.is_ascii_hexdigit()) {
            return Err(PurlError::BadEncoding);
        }
        index = start + 3;
    }
    percent_decode_str(value)
        .decode_utf8()
        .map(|decoded| decoded.into_owned())
        .map_err(|_| PurlError::BadEncoding)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_versioned_debian_purl() {
        let p = PackageRef::from_purl("pkg:deb/debian/openssl@1.1.1n-0+deb11u3").unwrap();
        assert_eq!(p.ecosystem, Ecosystem::Deb);
        assert_eq!(p.namespace.as_deref(), Some("debian"));
        assert_eq!(p.name, "openssl");
        assert_eq!(p.version.as_deref(), Some("1.1.1n-0+deb11u3"));
    }

    #[test]
    fn parses_the_qualifier_only_purl_that_osv_actually_ships() {
        // Taken verbatim from fixtures/osv_debian_openssl.json: no version,
        // and a qualifier that must not be mistaken for one.
        let p = PackageRef::from_purl("pkg:deb/debian/openssl?arch=source").unwrap();
        assert_eq!(p.name, "openssl");
        assert_eq!(p.version, None);
        assert_eq!(p.namespace.as_deref(), Some("debian"));
    }

    #[test]
    fn parses_rpm_and_apk_types() {
        assert_eq!(
            PackageRef::from_purl("pkg:rpm/rocky/openssl@3.0.7-6.el9")
                .unwrap()
                .ecosystem,
            Ecosystem::Rpm
        );
        assert_eq!(
            PackageRef::from_purl("pkg:apk/alpine/busybox@1.36.1-r5")
                .unwrap()
                .ecosystem,
            Ecosystem::Apk
        );
    }

    #[test]
    fn decodes_percent_encoded_components() {
        let p = PackageRef::from_purl("pkg:deb/debian/lib%2Bplus@1%3A2.0-1").unwrap();
        assert_eq!(p.name, "lib+plus");
        assert_eq!(p.version.as_deref(), Some("1:2.0-1"));
    }

    #[test]
    fn a_namespace_is_optional() {
        let p = PackageRef::from_purl("pkg:deb/openssl@1.0").unwrap();
        assert_eq!(p.namespace, None);
        assert_eq!(p.name, "openssl");
    }

    #[test]
    fn ignores_a_subpath_fragment() {
        let p = PackageRef::from_purl("pkg:deb/debian/openssl@1.0#some/subpath").unwrap();
        assert_eq!(p.version.as_deref(), Some("1.0"));
        assert_eq!(p.name, "openssl");
    }

    #[test]
    fn rejects_input_that_is_not_a_usable_purl() {
        // Every one of these must fail loudly rather than parse into a
        // PackageRef that silently never matches anything.
        assert_eq!(
            PackageRef::from_purl("openssl").unwrap_err(),
            PurlError::NotAPurl
        );
        assert_eq!(
            PackageRef::from_purl("https://example.com/x").unwrap_err(),
            PurlError::NotAPurl
        );
        assert_eq!(
            PackageRef::from_purl("pkg:cargo/serde@1.0").unwrap_err(),
            PurlError::UnsupportedType("cargo".into())
        );
        assert_eq!(
            PackageRef::from_purl("pkg:deb/").unwrap_err(),
            PurlError::MissingName
        );
        assert_eq!(
            PackageRef::from_purl("pkg:deb").unwrap_err(),
            PurlError::MissingName
        );
        assert_eq!(
            PackageRef::from_purl("pkg:deb/debian/lib%ZZ@1.0").unwrap_err(),
            PurlError::BadEncoding
        );
    }
}
