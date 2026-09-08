//! Feed parsers. Each converts one upstream schema into `Advisory`.
//!
//! Malformed records are rejected with context rather than skipped: a feed
//! that silently drops entries produces a clean dashboard that means nothing.

pub mod kev;
pub mod nvd;
pub mod osv;

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FeedError {
    #[error("malformed JSON: {0}")]
    Json(String),
    #[error("record is missing required field `{0}`")]
    MissingField(&'static str),
    #[error("record names no ecosystem this tool can compare versions for")]
    NoSupportedEcosystem,
}
