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
}
