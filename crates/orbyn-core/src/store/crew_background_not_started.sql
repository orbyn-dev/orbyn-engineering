-- Migration 81: a background run refused by the disk guard keeps one history row (KC4 PR B). A run that never
-- started is `stopped` with no stop reason and a `reason_code` (`disk_low`); the runs table's CHECK requires a stop
-- reason for a stopped row, and a CHECK cannot be altered in place, so the table is rebuilt with every column and
-- row. Its dependent daily ledger is rebuilt in the same shape too: migrations run inside a transaction, where
-- `PRAGMA foreign_keys` cannot change, and a plain DROP of the parent would cascade the ledger away. The rebuilt
-- tables reference the new parent, then take the old names, so every foreign key and ON DELETE CASCADE still holds.
-- Ids, states, codes and counts only: no text and no path.
CREATE TABLE crew_background_runs_next (
    id TEXT PRIMARY KEY,
    background_id TEXT NOT NULL REFERENCES crew_background(id) ON DELETE CASCADE,
    state TEXT NOT NULL CHECK(state IN ('running', 'stopped', 'needs_review')),
    stop_reason TEXT CHECK(stop_reason IS NULL OR stop_reason IN ('budget', 'repeated_failure', 'engine_error', 'disabled')),
    reason_code TEXT CHECK(reason_code IS NULL OR (length(reason_code) BETWEEN 1 AND 64)),
    turns_spent INTEGER NOT NULL DEFAULT 0 CHECK(turns_spent >= 0),
    minutes_spent INTEGER NOT NULL DEFAULT 0 CHECK(minutes_spent >= 0),
    budget_turns INTEGER NOT NULL CHECK(budget_turns BETWEEN 1 AND 1000),
    budget_minutes INTEGER NOT NULL CHECK(budget_minutes BETWEEN 1 AND 1440),
    review_waiting INTEGER NOT NULL DEFAULT 0 CHECK(review_waiting IN (0, 1)),
    review_reason TEXT,
    started_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    ended_at TEXT,
    -- A run stops for one reason and one only: a stopped row carries its stop reason, or the code of a refusal that
    -- started nothing, never both.
    CHECK(stop_reason IS NULL OR reason_code IS NULL),
    CHECK((state = 'stopped') = (stop_reason IS NOT NULL OR reason_code IS NOT NULL)),
    CHECK((state = 'running') = (ended_at IS NULL))
);
-- `rowid` is kept too: the runs page breaks a tie in `started_at` with it.
INSERT INTO crew_background_runs_next
    (rowid, id, background_id, state, stop_reason, reason_code, turns_spent, minutes_spent, budget_turns, budget_minutes,
     review_waiting, review_reason, started_at, updated_at, ended_at)
    SELECT rowid, id, background_id, state, stop_reason, NULL, turns_spent, minutes_spent, budget_turns, budget_minutes,
           review_waiting, review_reason, started_at, updated_at, ended_at
    FROM crew_background_runs;

CREATE TABLE crew_background_daily_next (
    background_id TEXT NOT NULL REFERENCES crew_background(id) ON DELETE CASCADE,
    run_id TEXT NOT NULL REFERENCES crew_background_runs_next(id) ON DELETE CASCADE,
    day TEXT NOT NULL CHECK(length(day) = 10),
    engine TEXT NOT NULL CHECK(length(engine) BETWEEN 1 AND 80),
    turns_spent INTEGER NOT NULL DEFAULT 0 CHECK(turns_spent >= 0),
    PRIMARY KEY (run_id, day, engine)
) WITHOUT ROWID;
INSERT INTO crew_background_daily_next (background_id, run_id, day, engine, turns_spent)
    SELECT background_id, run_id, day, engine, turns_spent FROM crew_background_daily;

-- The old children go first, so the old parent has no foreign keys left when it is dropped. Every row already has a copy.
DROP TABLE crew_background_daily;
DROP TABLE crew_background_runs;
ALTER TABLE crew_background_runs_next RENAME TO crew_background_runs;
ALTER TABLE crew_background_daily_next RENAME TO crew_background_daily;

CREATE UNIQUE INDEX crew_background_one_running ON crew_background_runs(background_id) WHERE state = 'running';
CREATE INDEX crew_background_runs_by_crew ON crew_background_runs(background_id, started_at);
CREATE TRIGGER crew_background_runs_budget_fixed BEFORE UPDATE OF budget_turns, budget_minutes ON crew_background_runs
BEGIN SELECT RAISE(ABORT, 'a background run keeps the budget it was given'); END;
CREATE INDEX crew_background_daily_by_crew_day ON crew_background_daily (background_id, day, engine);
