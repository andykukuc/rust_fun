//! Pure domain logic for Elysium Vuln Watch.
//!
//! No I/O lives here: this crate is compiled for `wasm32-unknown-unknown`
//! and linked into the Worker, so any dependency that reaches for the
//! network, the clock or the filesystem breaks that build.

pub mod advisory;
pub mod batch;
pub mod feeds;
pub mod package;
pub mod version;

pub use advisory::{Advisory, AffectedRange, Severity};
pub use package::{Ecosystem, PackageRef, PurlError};
