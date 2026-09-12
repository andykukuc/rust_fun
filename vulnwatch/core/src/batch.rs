//! Write batching and budget accounting.
//!
//! D1 caps rows written per day and, past the cap, queries fail outright
//! rather than throttling. A sync run therefore has to know when to stop and
//! leave the rest for the next run. This module is the pure arithmetic of
//! that decision; the Worker supplies the actual statements.

/// Half-open `[start, end)` slices of a work list.
pub type Batch = (usize, usize);

/// Split `total` items into batches of at most `cap`.
///
/// A `cap` of zero would otherwise plan an infinite number of empty
/// batches, so it is read as one.
pub fn plan_batches(total: usize, cap: usize) -> Vec<Batch> {
    let cap = cap.max(1);
    let mut batches = Vec::with_capacity(total.div_ceil(cap));
    let mut start = 0;
    while start < total {
        let end = (start + cap).min(total);
        batches.push((start, end));
        start = end;
    }
    batches
}

/// A run's remaining row-write allowance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Budget {
    remaining: u32,
}

impl Budget {
    pub fn new(remaining: u32) -> Self {
        Self { remaining }
    }

    /// Spend `rows` if the whole amount fits. Partial spends are refused, so
    /// a batch is either written in full or deferred in full.
    pub fn take(&mut self, rows: u32) -> bool {
        if rows > self.remaining {
            return false;
        }
        self.remaining -= rows;
        true
    }

    pub fn remaining(&self) -> u32 {
        self.remaining
    }
}

/// The shared, day-scoped write allowance (task 3.5).
///
/// D1's write cap is per calendar day and across all feeds, not per feed, so
/// the counter has to be shared and has to reset when the day rolls over. This
/// is the pure arithmetic of that; the Worker persists the `(day, spent)` pair
/// between runs and supplies today's day string.
///
/// The day is an opaque UTC date key (e.g. `"2026-09-12"`); this type only
/// compares it for equality, never parses it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyBudget {
    day: String,
    spent: u32,
    cap: u32,
}

impl DailyBudget {
    /// Open the budget for `today`, given the last persisted `(day, spent)` and
    /// the daily `cap`. If the stored day is not today (or there is no stored
    /// state), the count resets to zero: a new day starts with the full cap.
    pub fn open(today: &str, stored: Option<(&str, u32)>, cap: u32) -> Self {
        let spent = match stored {
            Some((day, spent)) if day == today => spent.min(cap),
            _ => 0,
        };
        Self {
            day: today.to_owned(),
            spent,
            cap,
        }
    }

    /// Rows still writable today.
    pub fn remaining(&self) -> u32 {
        self.cap.saturating_sub(self.spent)
    }

    /// A [`Budget`] for this run, capped at what remains today.
    pub fn run_budget(&self) -> Budget {
        Budget::new(self.remaining())
    }

    /// Record `rows` as written and return the new state to persist. Saturates
    /// at the cap rather than overflowing, so a miscount can never wrap to a
    /// huge remaining allowance.
    pub fn record(&self, rows: u32) -> DailyBudget {
        DailyBudget {
            day: self.day.clone(),
            spent: self.spent.saturating_add(rows).min(self.cap),
            cap: self.cap,
        }
    }

    /// The `(day, spent)` pair to write back to storage.
    pub fn state(&self) -> (&str, u32) {
        (&self.day, self.spent)
    }

    /// True when nothing more may be written today.
    pub fn is_exhausted(&self) -> bool {
        self.remaining() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_work_plans_no_batches() {
        assert_eq!(plan_batches(0, 50), Vec::<Batch>::new());
    }

    #[test]
    fn work_smaller_than_the_cap_is_one_batch() {
        assert_eq!(plan_batches(7, 50), vec![(0, 7)]);
    }

    #[test]
    fn work_is_split_at_the_cap_with_a_short_final_batch() {
        assert_eq!(plan_batches(10, 4), vec![(0, 4), (4, 8), (8, 10)]);
    }

    #[test]
    fn an_exact_multiple_of_the_cap_leaves_no_empty_batch() {
        assert_eq!(plan_batches(8, 4), vec![(0, 4), (4, 8)]);
    }

    #[test]
    fn a_zero_cap_is_treated_as_one_rather_than_looping_forever() {
        assert_eq!(plan_batches(3, 0), vec![(0, 1), (1, 2), (2, 3)]);
    }

    #[test]
    fn a_spend_within_budget_succeeds_and_decrements() {
        let mut budget = Budget::new(100);
        assert!(budget.take(30));
        assert_eq!(budget.remaining(), 70);
    }

    #[test]
    fn a_spend_over_budget_is_refused_and_changes_nothing() {
        // Refusing in full is what lets the caller defer a whole batch and
        // resume it next run from the cursor.
        let mut budget = Budget::new(10);
        assert!(!budget.take(11));
        assert_eq!(budget.remaining(), 10);
    }

    #[test]
    fn a_spend_of_exactly_the_remainder_succeeds() {
        let mut budget = Budget::new(10);
        assert!(budget.take(10));
        assert_eq!(budget.remaining(), 0);
        assert!(!budget.take(1));
    }

    #[test]
    fn an_index_multiplies_the_cost_of_a_write() {
        // Two indexes on a table mean each row written costs three rows of
        // budget. Callers price that in; the budget just has to be honest
        // about refusing what will not fit.
        let mut budget = Budget::new(100);
        let rows_per_item = 3;
        let affordable = (0..50).filter(|_| budget.take(rows_per_item)).count();
        assert_eq!(affordable, 33);
        assert_eq!(budget.remaining(), 1);
    }

    #[test]
    fn a_fresh_day_starts_with_the_full_cap() {
        let b = DailyBudget::open("2026-09-12", None, 100_000);
        assert_eq!(b.remaining(), 100_000);
        assert!(!b.is_exhausted());
    }

    #[test]
    fn same_day_state_carries_the_spend_forward() {
        let b = DailyBudget::open("2026-09-12", Some(("2026-09-12", 40_000)), 100_000);
        assert_eq!(b.remaining(), 60_000);
    }

    #[test]
    fn a_new_day_resets_the_spend_to_zero() {
        let b = DailyBudget::open("2026-09-13", Some(("2026-09-12", 90_000)), 100_000);
        assert_eq!(b.remaining(), 100_000);
    }

    #[test]
    fn recording_writes_decrements_what_remains_today() {
        let b = DailyBudget::open("2026-09-12", None, 100_000);
        let b = b.record(1_709);
        assert_eq!(b.remaining(), 98_291);
        assert_eq!(b.state(), ("2026-09-12", 1_709));
    }

    #[test]
    fn spend_saturates_at_the_cap_rather_than_wrapping() {
        let b = DailyBudget::open("2026-09-12", Some(("2026-09-12", 99_000)), 100_000);
        let b = b.record(5_000); // would exceed the cap
        assert_eq!(b.remaining(), 0);
        assert!(b.is_exhausted());
    }

    #[test]
    fn a_stored_spend_above_the_cap_reads_as_exhausted_not_negative() {
        let b = DailyBudget::open("2026-09-12", Some(("2026-09-12", 200_000)), 100_000);
        assert_eq!(b.remaining(), 0);
        assert!(b.is_exhausted());
    }

    #[test]
    fn the_run_budget_reflects_todays_remaining_and_refuses_an_overlarge_batch() {
        let daily = DailyBudget::open("2026-09-12", Some(("2026-09-12", 99_990)), 100_000);
        let mut run = daily.run_budget();
        assert_eq!(run.remaining(), 10);
        assert!(!run.take(11)); // a batch bigger than what is left is deferred whole
        assert!(run.take(10));
        assert_eq!(run.remaining(), 0);
    }
}
