-- Elysium Vuln Watch - daily write-budget counter (task 3.5)
--
-- D1's row-write cap is per calendar day and shared across every feed, not
-- per feed. sync_state.rows_written already means "rows in this feed's last
-- run" and is kept for reporting, so the shared daily counter is its own
-- single-row table rather than an overload of that column.
--
-- `day` is a UTC date key (YYYY-MM-DD). A run reads this row, resets the count
-- to zero if the day has rolled over, spends against the cap, and writes the
-- new (day, rows_written) back. The id column pins it to exactly one row.

CREATE TABLE budget_state (
    id           INTEGER PRIMARY KEY CHECK (id = 1),
    day          TEXT NOT NULL DEFAULT '',
    rows_written INTEGER NOT NULL DEFAULT 0
);

INSERT INTO budget_state (id, day, rows_written) VALUES (1, '', 0);
