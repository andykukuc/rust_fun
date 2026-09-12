use crate::package::{Ecosystem, PackageRef};
use crate::version;
use serde::{Deserialize, Serialize};

/// Qualitative severity, ordered least to most urgent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    None,
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    /// Map a CVSS v3.1 base score onto its qualitative band.
    ///
    /// Scores arrive from untrusted feed data, so anything outside the
    /// 0.0-10.0 scale clamps to the nearest band rather than panicking.
    /// NaN fails every comparison below and so lands on `None`.
    pub fn from_cvss_score(score: f64) -> Self {
        if score >= 9.0 {
            Severity::Critical
        } else if score >= 7.0 {
            Severity::High
        } else if score >= 4.0 {
            Severity::Medium
        } else if score >= 0.1 {
            Severity::Low
        } else {
            Severity::None
        }
    }
}

/// A half-open version window `[introduced, fixed)` for one package.
///
/// `fixed == None` means no fix has shipped, so everything at or above
/// `introduced` is affected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AffectedRange {
    pub ecosystem: Ecosystem,
    pub package: String,
    pub introduced: String,
    pub fixed: Option<String>,
}

impl AffectedRange {
    /// Does this range cover `version`?
    pub fn covers(&self, version: &str) -> bool {
        let at_or_after_introduced = self.introduced == "0"
            || version::compare(self.ecosystem, version, &self.introduced).is_ge();
        let before_fix = match &self.fixed {
            Some(fixed) => version::compare(self.ecosystem, version, fixed).is_lt(),
            None => true,
        };
        at_or_after_introduced && before_fix
    }
}

/// A single advisory, normalised from whichever feed produced it.
#[derive(Debug, Clone, PartialEq)]
pub struct Advisory {
    pub id: String,
    pub summary: String,
    pub severity: Severity,
    pub aliases: Vec<String>,
    pub affected: Vec<AffectedRange>,
}

impl Advisory {
    /// Is `package` affected by this advisory?
    ///
    /// A package with no version cannot be judged, and is reported as not
    /// affected rather than guessed at.
    pub fn affects(&self, package: &PackageRef) -> bool {
        let Some(version) = package.version.as_deref() else {
            return false;
        };
        self.affected.iter().any(|range| {
            range.ecosystem == package.ecosystem
                && range.package == package.name
                && range.covers(version)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cvss_scores_map_to_their_severity_band() {
        assert_eq!(Severity::from_cvss_score(0.0), Severity::None);
        assert_eq!(Severity::from_cvss_score(0.1), Severity::Low);
        assert_eq!(Severity::from_cvss_score(3.9), Severity::Low);
        assert_eq!(Severity::from_cvss_score(4.0), Severity::Medium);
        assert_eq!(Severity::from_cvss_score(6.9), Severity::Medium);
        assert_eq!(Severity::from_cvss_score(7.0), Severity::High);
        assert_eq!(Severity::from_cvss_score(8.9), Severity::High);
        assert_eq!(Severity::from_cvss_score(9.0), Severity::Critical);
        assert_eq!(Severity::from_cvss_score(10.0), Severity::Critical);
    }

    #[test]
    fn scores_outside_the_scale_clamp_instead_of_panicking() {
        assert_eq!(Severity::from_cvss_score(-1.0), Severity::None);
        assert_eq!(Severity::from_cvss_score(99.0), Severity::Critical);
        assert_eq!(Severity::from_cvss_score(f64::NAN), Severity::None);
    }

    #[test]
    fn severity_orders_least_to_most_urgent() {
        assert!(Severity::Critical > Severity::High);
        assert!(Severity::High > Severity::Medium);
        assert!(Severity::Medium > Severity::Low);
        assert!(Severity::Low > Severity::None);
    }
}
