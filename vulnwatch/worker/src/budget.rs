//! The daily write-budget guard (task 3.5), D1 side.
//!
//! D1 caps rows written per calendar day, shared across all feeds, and fails
//! outright past the cap. A sync run therefore opens the budget, writes only
//! what fits, and records what it wrote so the next run resumes from its
//! cursor. The arithmetic and day-rollover reset live in
//! `vulnwatch_core::batch::DailyBudget`; this file only reads and writes the
//! single `budget_state` row.

use vulnwatch_core::batch::DailyBudget;
use worker::*;

/// D1 free-tier daily row-write cap. Load-bearing under either plan (see
/// AGENTS.md 0.3): on Paid it simply never trips.
pub const DAILY_WRITE_CAP: u32 = 100_000;

/// Read the persisted counter and open the budget for `today` (a `YYYY-MM-DD`
/// UTC key). A missing or stale-day row opens with the full cap.
pub async fn open(db: &D1Database, today: &str) -> Result<DailyBudget> {
    let row: Option<serde_json::Value> = db
        .prepare("SELECT day, rows_written FROM budget_state WHERE id = 1")
        .first(None)
        .await?;

    let stored = row.as_ref().and_then(|v| {
        let day = v.get("day").and_then(serde_json::Value::as_str)?;
        let spent = v.get("rows_written").and_then(serde_json::Value::as_u64)? as u32;
        Some((day, spent))
    });

    Ok(DailyBudget::open(today, stored, DAILY_WRITE_CAP))
}

/// A statement that persists the budget's `(day, spent)` back to the single
/// `budget_state` row. Include it in the same batch as the data it accounts
/// for, so the counter and the writes commit together.
pub fn persist_statement(db: &D1Database, budget: &DailyBudget) -> Result<D1PreparedStatement> {
    let (day, spent) = budget.state();
    db.prepare("UPDATE budget_state SET day = ?1, rows_written = ?2 WHERE id = 1")
        .bind(&[day.into(), (spent as f64).into()])
}

/// The `YYYY-MM-DD` UTC day key for an RFC 3339 timestamp this project writes
/// (`YYYY-MM-DDTHH:MM:SSZ`). Takes the date portion by slicing at `T`, so it
/// never depends on locale or a date library.
pub fn day_key(rfc3339: &str) -> &str {
    match rfc3339.find('T') {
        Some(i) => &rfc3339[..i],
        None => rfc3339,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_key_takes_the_date_before_the_time() {
        assert_eq!(day_key("2026-09-12T07:24:51Z"), "2026-09-12");
    }

    #[test]
    fn day_key_of_a_bare_date_is_itself() {
        assert_eq!(day_key("2026-09-12"), "2026-09-12");
    }
}
