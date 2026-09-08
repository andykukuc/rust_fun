//! CISA Known Exploited Vulnerabilities catalog parsing.

use super::FeedError;
use serde_json::Value;

/// The KEV catalog reduced to what the matcher needs: which CVEs are known
/// to be exploited, and which version of the catalog said so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KevCatalog {
    pub version: String,
    pub cve_ids: Vec<String>,
}

impl KevCatalog {
    pub fn contains(&self, cve_id: &str) -> bool {
        self.cve_ids.iter().any(|id| id == cve_id)
    }
}

/// Parse the KEV catalog.
pub fn parse(json: &str) -> Result<KevCatalog, FeedError> {
    let catalog: Value =
        serde_json::from_str(json).map_err(|error| FeedError::Json(error.to_string()))?;

    let version = catalog
        .get("catalogVersion")
        .and_then(Value::as_str)
        .ok_or(FeedError::MissingField("catalogVersion"))?
        .to_owned();

    let cve_ids = catalog
        .get("vulnerabilities")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.get("cveID").and_then(Value::as_str))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();

    Ok(KevCatalog { version, cve_ids })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../../fixtures/kev_sample.json");

    #[test]
    fn parses_the_real_catalog_sample() {
        let catalog = parse(FIXTURE).unwrap();
        assert_eq!(catalog.version, "2026.09.04");
        assert_eq!(catalog.cve_ids.len(), 3);
    }

    #[test]
    fn reports_membership_for_a_listed_cve() {
        let catalog = parse(FIXTURE).unwrap();
        assert!(catalog.contains("CVE-2026-85046"));
    }

    #[test]
    fn reports_non_membership_for_an_unlisted_cve() {
        let catalog = parse(FIXTURE).unwrap();
        assert!(!catalog.contains("CVE-2022-0778"));
    }

    #[test]
    fn malformed_json_is_rejected() {
        assert!(matches!(parse("]"), Err(FeedError::Json(_))));
    }

    #[test]
    fn a_catalog_without_a_version_is_rejected() {
        let json = r#"{"vulnerabilities":[]}"#;
        assert_eq!(
            parse(json).unwrap_err(),
            FeedError::MissingField("catalogVersion")
        );
    }
}
