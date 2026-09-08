use super::FeedError;
use crate::advisory::{Advisory, Severity};
use serde_json::Value;

/// Metric blocks in preference order. A record often carries several; taking
/// the legacy v2 score when a v3.1 score exists would under-report real
/// findings, so newer schemas win.
const METRIC_KEYS: [&str; 4] = [
    "cvssMetricV40",
    "cvssMetricV31",
    "cvssMetricV30",
    "cvssMetricV2",
];

fn severity_of(cve: &Value) -> Severity {
    let metrics = cve.get("metrics");
    for key in METRIC_KEYS {
        let score = metrics
            .and_then(|m| m.get(key))
            .and_then(Value::as_array)
            .and_then(|scores| scores.first())
            .and_then(|metric| metric.get("cvssData"))
            .and_then(|data| data.get("baseScore"))
            .and_then(Value::as_f64);
        if let Some(score) = score {
            return Severity::from_cvss_score(score);
        }
    }
    Severity::None
}

fn english_description(cve: &Value) -> String {
    cve.get("descriptions")
        .and_then(Value::as_array)
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry.get("lang").and_then(Value::as_str) == Some("en"))
        })
        .and_then(|entry| entry.get("value"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// Parse every CVE in one NVD API page.
///
/// NVD supplies the identity, description and score. It expresses what is
/// affected as CPEs rather than package versions, so `affected` is left
/// empty here and the version ranges come from OSV, joined on the CVE id.
pub fn parse_page(json: &str) -> Result<Vec<Advisory>, FeedError> {
    let page: Value =
        serde_json::from_str(json).map_err(|error| FeedError::Json(error.to_string()))?;

    let entries = page
        .get("vulnerabilities")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    let mut advisories = Vec::with_capacity(entries.len());
    for entry in entries {
        let cve = entry.get("cve").ok_or(FeedError::MissingField("cve"))?;
        let id = cve
            .get("id")
            .and_then(Value::as_str)
            .ok_or(FeedError::MissingField("id"))?
            .to_owned();

        advisories.push(Advisory {
            id,
            summary: english_description(cve),
            severity: severity_of(cve),
            aliases: Vec::new(),
            affected: Vec::new(),
        });
    }
    Ok(advisories)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../../fixtures/nvd_cve_2022_0778.json");

    #[test]
    fn parses_the_real_cve_page() {
        let advisories = parse_page(FIXTURE).unwrap();
        assert_eq!(advisories.len(), 1);
        assert_eq!(advisories[0].id, "CVE-2022-0778");
    }

    #[test]
    fn takes_the_english_description_as_the_summary() {
        let advisories = parse_page(FIXTURE).unwrap();
        assert!(
            advisories[0].summary.starts_with("The BN_mod_sqrt()"),
            "got: {}",
            advisories[0].summary
        );
    }

    #[test]
    fn prefers_the_v31_score_over_the_legacy_v2_score() {
        // This record carries both: v3.1 is 7.5 HIGH, v2 is 5.0 MEDIUM.
        // Taking v2 would under-report the severity of a real finding.
        let advisories = parse_page(FIXTURE).unwrap();
        assert_eq!(advisories[0].severity, Severity::High);
    }

    #[test]
    fn an_empty_page_parses_to_no_advisories() {
        let json = r#"{"totalResults":0,"vulnerabilities":[]}"#;
        assert_eq!(parse_page(json).unwrap().len(), 0);
    }

    #[test]
    fn malformed_json_is_rejected() {
        assert!(matches!(parse_page("{"), Err(FeedError::Json(_))));
    }

    #[test]
    fn a_cve_without_an_id_is_rejected() {
        let json = r#"{"vulnerabilities":[{"cve":{"descriptions":[]}}]}"#;
        assert_eq!(parse_page(json).unwrap_err(), FeedError::MissingField("id"));
    }

    #[test]
    fn a_cve_with_no_metrics_reads_as_unscored_not_critical() {
        let json = r#"{"vulnerabilities":[{"cve":{"id":"CVE-1","descriptions":[]}}]}"#;
        let advisories = parse_page(json).unwrap();
        assert_eq!(advisories[0].severity, Severity::None);
    }
}
