//! CISA Known Exploited Vulnerabilities catalog parsing.

use super::FeedError;
use serde_json::Value;

/// One catalog entry: a CVE known to be exploited, and the date CISA added it.
/// `date_added` is the upstream `dateAdded` (`YYYY-MM-DD`); it is `None` only
/// if an entry omits it, which the real catalog never does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KevEntry {
    pub cve_id: String,
    pub date_added: Option<String>,
}

/// The KEV catalog reduced to what the sync and matcher need: which CVEs are
/// known to be exploited (with the date each was catalogued), and which
/// version of the catalog said so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KevCatalog {
    pub version: String,
    pub entries: Vec<KevEntry>,
}

impl KevCatalog {
    pub fn contains(&self, cve_id: &str) -> bool {
        self.entries.iter().any(|e| e.cve_id == cve_id)
    }

    /// Just the CVE ids, in catalog order.
    pub fn cve_ids(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|e| e.cve_id.as_str())
    }

    /// Rows to write for a full mirror of this catalog: `(cve_id,
    /// catalog_version, date_added)`, one per entry. The KEV table is a
    /// snapshot, so a sync upserts all of these and prunes anything else.
    pub fn rows(&self) -> Vec<(&str, &str, Option<&str>)> {
        self.entries
            .iter()
            .map(|e| {
                (
                    e.cve_id.as_str(),
                    self.version.as_str(),
                    e.date_added.as_deref(),
                )
            })
            .collect()
    }
}

/// Parse the KEV catalog. An entry missing its `cveID` is skipped, since it
/// cannot be joined to anything; the catalog version is required.
pub fn parse(json: &str) -> Result<KevCatalog, FeedError> {
    let catalog: Value =
        serde_json::from_str(json).map_err(|error| FeedError::Json(error.to_string()))?;

    let version = catalog
        .get("catalogVersion")
        .and_then(Value::as_str)
        .ok_or(FeedError::MissingField("catalogVersion"))?
        .to_owned();

    let entries = catalog
        .get("vulnerabilities")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    let cve_id = entry.get("cveID").and_then(Value::as_str)?.to_owned();
                    let date_added = entry
                        .get("dateAdded")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    Some(KevEntry { cve_id, date_added })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(KevCatalog { version, entries })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../../fixtures/kev_sample.json");

    #[test]
    fn parses_the_real_catalog_sample() {
        let catalog = parse(FIXTURE).unwrap();
        assert_eq!(catalog.version, "2026.09.04");
        assert_eq!(catalog.entries.len(), 3);
    }

    #[test]
    fn captures_the_date_each_cve_was_added() {
        let catalog = parse(FIXTURE).unwrap();
        let entry = catalog
            .entries
            .iter()
            .find(|e| e.cve_id == "CVE-2026-85046")
            .expect("fixture lists this CVE");
        assert_eq!(entry.date_added.as_deref(), Some("2026-09-04"));
    }

    #[test]
    fn cve_ids_helper_yields_every_id_in_order() {
        let catalog = parse(FIXTURE).unwrap();
        let ids: Vec<&str> = catalog.cve_ids().collect();
        assert_eq!(ids, ["CVE-2026-85046", "CVE-2026-59822", "CVE-2026-48710"]);
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
    fn rows_carry_cve_version_and_date_for_a_full_mirror() {
        let catalog = parse(FIXTURE).unwrap();
        let rows = catalog.rows();
        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows[0],
            ("CVE-2026-85046", "2026.09.04", Some("2026-09-04"))
        );
        // Every row carries the same catalog version.
        assert!(rows.iter().all(|(_, v, _)| *v == "2026.09.04"));
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
