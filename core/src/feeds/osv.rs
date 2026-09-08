use super::FeedError;
use crate::advisory::{Advisory, AffectedRange, Severity};
use crate::package::{Ecosystem, PackageRef};
use serde_json::Value;

/// Map an OSV ecosystem label onto a comparison scheme.
///
/// OSV labels carry a release, e.g. `Debian:11` or `Alpine:v3.18`, so only
/// the distribution prefix is significant here.
fn ecosystem_from_label(label: &str) -> Option<Ecosystem> {
    let distribution = label.split(':').next().unwrap_or(label);
    match distribution {
        "Debian" | "Ubuntu" => Some(Ecosystem::Deb),
        "Alpine" => Some(Ecosystem::Apk),
        "Red Hat" | "Rocky Linux" | "AlmaLinux" | "CentOS" | "openSUSE" | "SUSE" => {
            Some(Ecosystem::Rpm)
        }
        _ => None,
    }
}

/// Prefer the purl, which names the packaging format directly; fall back to
/// the ecosystem label when no purl is present.
fn ecosystem_of(package: Option<&Value>) -> Option<Ecosystem> {
    let package = package?;
    if let Some(purl) = package.get("purl").and_then(Value::as_str) {
        if let Ok(parsed) = PackageRef::from_purl(purl) {
            return Some(parsed.ecosystem);
        }
    }
    ecosystem_from_label(package.get("ecosystem").and_then(Value::as_str)?)
}

fn string_list(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// Parse one OSV record.
pub fn parse(json: &str) -> Result<Advisory, FeedError> {
    let record: Value =
        serde_json::from_str(json).map_err(|error| FeedError::Json(error.to_string()))?;

    let id = record
        .get("id")
        .and_then(Value::as_str)
        .ok_or(FeedError::MissingField("id"))?
        .to_owned();
    let summary = record
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();

    // Debian OSV records list CVEs under `upstream`; the base schema uses
    // `aliases`. Take both so the KEV join works either way.
    let mut aliases = string_list(&record, "aliases");
    aliases.extend(string_list(&record, "upstream"));

    let mut affected = Vec::new();
    let mut saw_a_package = false;

    for entry in array(&record, "affected") {
        let package = entry.get("package");
        let Some(name) = package.and_then(|p| p.get("name")).and_then(Value::as_str) else {
            continue;
        };
        saw_a_package = true;
        let Some(ecosystem) = ecosystem_of(package) else {
            continue;
        };

        for range in array(entry, "ranges") {
            let mut introduced = None;
            let mut fixed = None;
            for event in array(range, "events") {
                if let Some(value) = event.get("introduced").and_then(Value::as_str) {
                    introduced = Some(value.to_owned());
                }
                if let Some(value) = event.get("fixed").and_then(Value::as_str) {
                    fixed = Some(value.to_owned());
                }
            }
            affected.push(AffectedRange {
                ecosystem,
                package: name.to_owned(),
                introduced: introduced.unwrap_or_else(|| "0".to_owned()),
                fixed,
            });
        }
    }

    // A record that names packages but none we can compare versions for is a
    // gap in coverage, not an empty advisory. Say so rather than storing a
    // row that can never match.
    if saw_a_package && affected.is_empty() {
        return Err(FeedError::NoSupportedEcosystem);
    }

    Ok(Advisory {
        id,
        summary,
        // OSV carries severity as a CVSS vector string rather than a score,
        // and distribution records usually omit it entirely. NVD supplies
        // the score for the same CVE, so leave it unset here rather than
        // guess a band from a vector we have not parsed.
        severity: Severity::None,
        aliases,
        affected,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::{Ecosystem, PackageRef};

    const FIXTURE: &str = include_str!("../../../fixtures/osv_debian_openssl.json");

    fn openssl(version: &str) -> PackageRef {
        PackageRef::from_purl(&format!("pkg:deb/debian/openssl@{version}")).unwrap()
    }

    #[test]
    fn parses_the_real_debian_openssl_advisory() {
        let advisory = parse(FIXTURE).unwrap();
        assert_eq!(advisory.id, "DLA-3942-1");
        assert_eq!(advisory.summary, "openssl - security update");
        assert!(advisory.aliases.contains(&"CVE-2024-9143".to_owned()));
    }

    #[test]
    fn extracts_the_affected_range_from_the_events_list() {
        let advisory = parse(FIXTURE).unwrap();
        assert_eq!(advisory.affected.len(), 1);
        let range = &advisory.affected[0];
        assert_eq!(range.ecosystem, Ecosystem::Deb);
        assert_eq!(range.package, "openssl");
        assert_eq!(range.introduced, "0");
        assert_eq!(range.fixed.as_deref(), Some("1.1.1n-0+deb11u6"));
    }

    #[test]
    fn every_version_osv_lists_as_affected_actually_matches() {
        let advisory = parse(FIXTURE).unwrap();
        for version in [
            "1.1.1k-1",
            "1.1.1n-0+deb11u3",
            "1.1.1n-0+deb11u5",
            "1.1.1m-1",
        ] {
            assert!(
                advisory.affects(&openssl(version)),
                "{version} should be affected"
            );
        }
    }

    #[test]
    fn the_fixed_version_and_later_are_not_affected() {
        let advisory = parse(FIXTURE).unwrap();
        for version in ["1.1.1n-0+deb11u6", "1.1.1n-0+deb11u7", "3.0.0-1"] {
            assert!(
                !advisory.affects(&openssl(version)),
                "{version} should be fixed"
            );
        }
    }

    #[test]
    fn a_different_package_at_a_vulnerable_version_is_not_affected() {
        let advisory = parse(FIXTURE).unwrap();
        let curl = PackageRef::from_purl("pkg:deb/debian/curl@1.1.1k-1").unwrap();
        assert!(!advisory.affects(&curl));
    }

    #[test]
    fn the_same_version_under_a_different_ecosystem_is_not_affected() {
        let advisory = parse(FIXTURE).unwrap();
        let rpm = PackageRef::from_purl("pkg:rpm/rocky/openssl@1.1.1k-1").unwrap();
        assert!(!advisory.affects(&rpm));
    }

    #[test]
    fn a_package_with_no_version_is_never_reported_as_affected() {
        let advisory = parse(FIXTURE).unwrap();
        let unversioned = PackageRef::from_purl("pkg:deb/debian/openssl?arch=source").unwrap();
        assert!(!advisory.affects(&unversioned));
    }

    #[test]
    fn severity_defaults_to_none_when_the_record_carries_no_score() {
        // OSV Debian records routinely omit CVSS; that must not read as Critical.
        let advisory = parse(FIXTURE).unwrap();
        assert_eq!(advisory.severity, Severity::None);
    }

    #[test]
    fn malformed_json_is_rejected() {
        assert!(matches!(parse("not json"), Err(FeedError::Json(_))));
    }

    #[test]
    fn a_record_without_an_id_is_rejected() {
        let json = r#"{"summary":"no id here"}"#;
        assert_eq!(parse(json).unwrap_err(), FeedError::MissingField("id"));
    }

    #[test]
    fn a_record_for_an_uncomparable_ecosystem_is_rejected() {
        let json =
            r#"{"id":"X-1","affected":[{"package":{"name":"serde","ecosystem":"crates.io"}}]}"#;
        assert_eq!(parse(json).unwrap_err(), FeedError::NoSupportedEcosystem);
    }
}
