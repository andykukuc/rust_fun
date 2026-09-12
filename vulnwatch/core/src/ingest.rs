//! The versioned wire contract between the collector (on elysium) and the
//! Worker's ingest routes (tasks 4.3, 5.1).
//!
//! Versioned from day one: `version` lets the Worker reject a payload shape it
//! does not understand instead of silently mis-parsing it. Everything here is
//! plain serde data with no I/O, so it compiles for both the native collector
//! and the `wasm32` Worker and is the single source of truth for the shape.

use serde::{Deserialize, Serialize};

/// Current OSV ingest contract version. Bump when the shape changes
/// incompatibly; the Worker checks it and refuses anything else.
pub const OSV_INGEST_VERSION: u32 = 1;

/// One affected version window for one package, tied to the advisory (CVE) it
/// came from. Mirrors a row of `advisory_ranges`, plus the CVE id the range
/// should attach to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OsvRange {
    /// The CVE this range belongs to, e.g. `"CVE-2022-0778"`. The Worker joins
    /// this to an existing advisory; ranges for unknown advisories are dropped.
    pub cve: String,
    /// `"deb"`, `"rpm"`, or `"apk"` — `Ecosystem::as_db_str`.
    pub ecosystem: String,
    pub package: String,
    /// Introduced-at version; `"0"` means "from the beginning".
    pub introduced: String,
    /// Fixed-at version, or `None` if no fix has shipped.
    pub fixed: Option<String>,
}

/// The full OSV ingest payload the collector POSTs to the Worker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OsvIngest {
    /// Must equal [`OSV_INGEST_VERSION`]; the Worker rejects anything else.
    pub version: u32,
    /// Which OSV ecosystem streams this batch was filtered to, for auditing on
    /// the Worker side (e.g. `["Debian:12", "Ubuntu:24.04"]`).
    pub sources: Vec<String>,
    /// The ranges. May be a slice of a larger run; the collector chunks so no
    /// single POST is unbounded.
    pub ranges: Vec<OsvRange>,
}

impl OsvIngest {
    /// True when the payload declares the version this build understands.
    pub fn is_supported(&self) -> bool {
        self.version == OSV_INGEST_VERSION
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_json() {
        let payload = OsvIngest {
            version: OSV_INGEST_VERSION,
            sources: vec!["Debian:12".into()],
            ranges: vec![OsvRange {
                cve: "CVE-2022-0778".into(),
                ecosystem: "deb".into(),
                package: "openssl".into(),
                introduced: "0".into(),
                fixed: Some("1.1.1n-0+deb11u3".into()),
            }],
        };
        let json = serde_json::to_string(&payload).unwrap();
        let back: OsvIngest = serde_json::from_str(&json).unwrap();
        assert_eq!(payload, back);
        assert!(back.is_supported());
    }

    #[test]
    fn a_future_version_is_not_supported() {
        let payload = OsvIngest {
            version: OSV_INGEST_VERSION + 1,
            sources: vec![],
            ranges: vec![],
        };
        assert!(!payload.is_supported());
    }

    #[test]
    fn a_missing_fixed_is_null_not_absent() {
        let payload = OsvIngest {
            version: OSV_INGEST_VERSION,
            sources: vec![],
            ranges: vec![OsvRange {
                cve: "CVE-1".into(),
                ecosystem: "apk".into(),
                package: "musl".into(),
                introduced: "0".into(),
                fixed: None,
            }],
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"fixed\":null"));
    }
}
