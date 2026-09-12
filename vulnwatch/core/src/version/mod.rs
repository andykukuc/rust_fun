//! Per-ecosystem version ordering.
//!
//! Debian, RPM and Alpine each order version strings differently, and the
//! differences are not cosmetic: epochs, tildes and pre-release suffixes all
//! change which side of a "fixed in" boundary a package falls on. Getting
//! this wrong produces false negatives — a clean dashboard that means
//! nothing — so each scheme is implemented against its own package
//! manager's documented rules rather than approximated with semver.

use crate::package::Ecosystem;
use core::cmp::Ordering;

pub mod apk;
pub mod deb;
pub mod rpm;

/// Order two version strings under the given ecosystem's rules.
pub fn compare(ecosystem: Ecosystem, left: &str, right: &str) -> Ordering {
    match ecosystem {
        Ecosystem::Deb => deb::compare(left, right),
        Ecosystem::Rpm => rpm::compare(left, right),
        Ecosystem::Apk => apk::compare(left, right),
    }
}
