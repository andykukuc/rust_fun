//! Feed freshness reporting for the Worker's `/health` route (task 3.1).
//!
//! Pure by construction: the current time is passed in rather than read, so
//! this compiles for `wasm32-unknown-unknown` and stays testable. The Worker
//! supplies `now` from its request context and the `last_success` value it
//! read from `sync_state`.
//!
//! The realistic failure this guards against (see task 7.3) is a sync that
//! quietly stops while the dashboard keeps serving yesterday's clean result.
//! Age is measured from `last_success`, so "never succeeded" and "succeeded
//! long ago" are both visible rather than hidden behind an error count.

use core::fmt;

/// Parse an RFC 3339 / ISO 8601 UTC timestamp (`YYYY-MM-DDTHH:MM:SSZ`, with an
/// optional fractional second) into a Unix epoch second count.
///
/// Deliberately strict and dependency-free: `sync_state` timestamps are always
/// written by this project in UTC with a trailing `Z`. Anything else is a bug
/// in the writer, not input to tolerate, so it is rejected rather than guessed.
pub fn parse_epoch_secs(ts: &str) -> Result<i64, TimeError> {
    let bytes = ts.as_bytes();
    // Minimum: "1970-01-01T00:00:00Z" is 20 chars.
    if bytes.len() < 20 {
        return Err(TimeError::Malformed);
    }
    let num = |lo: usize, hi: usize| -> Result<i64, TimeError> {
        let mut v: i64 = 0;
        for &b in &bytes[lo..hi] {
            if !b.is_ascii_digit() {
                return Err(TimeError::Malformed);
            }
            v = v * 10 + (b - b'0') as i64;
        }
        Ok(v)
    };
    let sep = |idx: usize, ch: u8| -> Result<(), TimeError> {
        if bytes[idx] == ch {
            Ok(())
        } else {
            Err(TimeError::Malformed)
        }
    };

    let year = num(0, 4)?;
    sep(4, b'-')?;
    let month = num(5, 7)?;
    sep(7, b'-')?;
    let day = num(8, 10)?;
    sep(10, b'T')?;
    let hour = num(11, 13)?;
    sep(13, b':')?;
    let min = num(14, 16)?;
    sep(16, b':')?;
    let sec = num(17, 19)?;
    // Trailing must be 'Z' or '.<frac>Z'; fractional seconds are ignored.
    match bytes[19] {
        b'Z' if bytes.len() == 20 => {}
        b'.' => {
            if bytes[bytes.len() - 1] != b'Z' {
                return Err(TimeError::Malformed);
            }
            for &b in &bytes[20..bytes.len() - 1] {
                if !b.is_ascii_digit() {
                    return Err(TimeError::Malformed);
                }
            }
        }
        _ => return Err(TimeError::Malformed),
    }

    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(TimeError::OutOfRange);
    }
    if hour > 23 || min > 59 || sec > 60 {
        // second 60 permitted for leap seconds; treated as :59 by the count.
        return Err(TimeError::OutOfRange);
    }

    Ok(days_from_civil(year, month, day) * 86_400 + hour * 3_600 + min * 60 + sec.min(59))
}

/// Days since the Unix epoch for a civil (proleptic Gregorian) date.
/// Howard Hinnant's `days_from_civil`, which is exact and branch-light.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The `(year, month, day)` of a day count since the Unix epoch. Hinnant's
/// `civil_from_days`, the exact inverse of [`days_from_civil`].
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Format a Unix-epoch-millisecond count as an RFC 3339 UTC timestamp
/// (`YYYY-MM-DDTHH:MM:SSZ`), the exact shape [`parse_epoch_secs`] accepts.
///
/// The Worker writes `sync_state` timestamps through this so the writer and the
/// freshness reader can never disagree on format. Sub-second precision is
/// dropped, which matters to nothing here.
pub fn format_epoch_millis(millis: i64) -> String {
    let total_secs = millis.div_euclid(1000);
    let days = total_secs.div_euclid(86_400);
    let secs_of_day = total_secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    let (hh, mm, ss) = (
        secs_of_day / 3_600,
        (secs_of_day % 3_600) / 60,
        secs_of_day % 60,
    );
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

/// Freshness of one feed's cursor at a given moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Freshness {
    /// The feed has never recorded a successful sync.
    NeverSynced,
    /// Last success was `age_secs` ago. `stale` is set once that exceeds the
    /// caller's threshold for this feed.
    Synced { age_secs: i64, stale: bool },
}

impl Freshness {
    /// Compute freshness from a `last_success` value (as stored in
    /// `sync_state`, possibly `NULL`), the current epoch second, and the
    /// staleness threshold in seconds for this feed.
    ///
    /// A `last_success` in the future (clock skew, or a bad write) reports an
    /// age of zero rather than a negative one, so skew never masquerades as
    /// freshness by underflowing.
    pub fn evaluate(
        last_success: Option<&str>,
        now_secs: i64,
        stale_after_secs: i64,
    ) -> Result<Self, TimeError> {
        match last_success {
            None => Ok(Freshness::NeverSynced),
            Some(ts) => {
                let then = parse_epoch_secs(ts)?;
                let age = (now_secs - then).max(0);
                Ok(Freshness::Synced {
                    age_secs: age,
                    stale: age > stale_after_secs,
                })
            }
        }
    }

    /// True when the operator should be alerted: never synced, or gone stale.
    pub fn needs_attention(&self) -> bool {
        match self {
            Freshness::NeverSynced => true,
            Freshness::Synced { stale, .. } => *stale,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeError {
    /// Not the expected `YYYY-MM-DDTHH:MM:SS[.fff]Z` shape.
    Malformed,
    /// Shape is right but a field is outside its valid range.
    OutOfRange,
}

impl fmt::Display for TimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimeError::Malformed => {
                write!(f, "timestamp is not RFC 3339 UTC (YYYY-MM-DDTHH:MM:SSZ)")
            }
            TimeError::OutOfRange => write!(f, "timestamp field out of range"),
        }
    }
}

impl std::error::Error for TimeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_at_unix_zero() {
        assert_eq!(parse_epoch_secs("1970-01-01T00:00:00Z").unwrap(), 0);
    }

    #[test]
    fn epoch_known_value() {
        // 2026-09-04T16:47:03Z — the KEV catalog release stamp in the fixtures.
        // Cross-checked against an independent epoch converter.
        assert_eq!(
            parse_epoch_secs("2026-09-04T16:47:03Z").unwrap(),
            1_788_540_423
        );
    }

    #[test]
    fn fractional_seconds_are_ignored_not_rejected() {
        let a = parse_epoch_secs("2026-09-04T16:47:03Z").unwrap();
        let b = parse_epoch_secs("2026-09-04T16:47:03.5197Z").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn rejects_missing_zulu() {
        assert_eq!(
            parse_epoch_secs("2026-09-04T16:47:03"),
            Err(TimeError::Malformed)
        );
    }

    #[test]
    fn rejects_local_offset() {
        // Offsets are not accepted; the writer always emits UTC.
        assert_eq!(
            parse_epoch_secs("2026-09-04T16:47:03+02:00"),
            Err(TimeError::Malformed)
        );
    }

    #[test]
    fn rejects_garbage_and_out_of_range() {
        assert_eq!(
            parse_epoch_secs("not-a-date-at-all!!"),
            Err(TimeError::Malformed)
        );
        assert_eq!(
            parse_epoch_secs("2026-13-01T00:00:00Z"),
            Err(TimeError::OutOfRange)
        );
        assert_eq!(
            parse_epoch_secs("2026-09-04T25:00:00Z"),
            Err(TimeError::OutOfRange)
        );
    }

    #[test]
    fn formats_epoch_millis_as_rfc3339() {
        // 1789197759000 ms = a known instant; round-trip through the parser.
        assert_eq!(format_epoch_millis(0), "1970-01-01T00:00:00Z");
        assert_eq!(
            format_epoch_millis(1_788_540_423_000),
            "2026-09-04T16:47:03Z"
        );
    }

    #[test]
    fn format_and_parse_round_trip() {
        for secs in [0_i64, 1, 1_788_540_423, 1_789_197_769, 4_102_444_800] {
            let s = format_epoch_millis(secs * 1000);
            assert_eq!(parse_epoch_secs(&s).unwrap(), secs, "round-trip {s}");
        }
    }

    #[test]
    fn sub_second_millis_are_truncated_not_rounded() {
        assert_eq!(
            format_epoch_millis(1_788_540_423_999),
            "2026-09-04T16:47:03Z"
        );
    }

    #[test]
    fn never_synced_needs_attention() {
        let f = Freshness::evaluate(None, 1_788_540_423, 86_400).unwrap();
        assert_eq!(f, Freshness::NeverSynced);
        assert!(f.needs_attention());
    }

    #[test]
    fn fresh_within_threshold_is_calm() {
        let then = "2026-09-04T16:47:03Z";
        let now = 1_788_540_423 + 3_600; // one hour later
        let f = Freshness::evaluate(Some(then), now, 86_400).unwrap();
        assert_eq!(
            f,
            Freshness::Synced {
                age_secs: 3_600,
                stale: false
            }
        );
        assert!(!f.needs_attention());
    }

    #[test]
    fn stale_past_threshold_needs_attention() {
        let then = "2026-09-04T16:47:03Z";
        let now = 1_788_540_423 + 90_000; // just over a day later
        let f = Freshness::evaluate(Some(then), now, 86_400).unwrap();
        match f {
            Freshness::Synced { age_secs, stale } => {
                assert_eq!(age_secs, 90_000);
                assert!(stale);
            }
            other => panic!("expected Synced, got {other:?}"),
        }
        assert!(f.needs_attention());
    }

    #[test]
    fn future_last_success_reads_as_zero_age_not_negative() {
        let then = "2026-09-04T16:47:03Z";
        let now = 1_788_540_423 - 500; // clock skew: "then" is in the future
        let f = Freshness::evaluate(Some(then), now, 86_400).unwrap();
        assert_eq!(
            f,
            Freshness::Synced {
                age_secs: 0,
                stale: false
            }
        );
    }
}
