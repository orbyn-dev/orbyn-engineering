-- Migration 57: additive (docs/crew/background-engine-start.md, slice 4h, PR 2). A background crew carries what each engine may
-- spend on it in a day, runs and turns, fixed at creation like the run budget: the crew row gains the two numbers and the person's
-- UTC offset, so the day is theirs and the budget resets at their midnight. A crew made before this migration keeps 0 and 0: it has
-- no daily budget, nothing is counted for it and nothing is refused. The ledger is one row per run, day and engine: a turn of a run
-- the engine ran adds one to that row, and the runs an engine spent are the rows themselves, so no engine's count can touch
-- another's. Ids, days and counts only: no task text and no engine output.
ALTER TABLE crew_background ADD COLUMN daily_runs INTEGER NOT NULL DEFAULT 0 CHECK(daily_runs >= 0);
ALTER TABLE crew_background ADD COLUMN daily_turns INTEGER NOT NULL DEFAULT 0 CHECK(daily_turns >= 0);
ALTER TABLE crew_background ADD COLUMN utc_offset_minutes INTEGER NOT NULL DEFAULT 0
    CHECK(utc_offset_minutes BETWEEN -1440 AND 1440);
DROP TRIGGER crew_background_budget_fixed;
CREATE TRIGGER crew_background_budget_fixed BEFORE UPDATE OF
    project_id, template, budget_turns, budget_minutes, daily_runs, daily_turns, utc_offset_minutes
    ON crew_background
BEGIN
    SELECT RAISE(ABORT, 'a background crew keeps its project, template and budget');
END;
CREATE TABLE crew_background_daily (
    background_id TEXT NOT NULL REFERENCES crew_background(id) ON DELETE CASCADE,
    run_id TEXT NOT NULL REFERENCES crew_background_runs(id) ON DELETE CASCADE,
    day TEXT NOT NULL CHECK(length(day) = 10),
    engine TEXT NOT NULL CHECK(length(engine) BETWEEN 1 AND 80),
    turns_spent INTEGER NOT NULL DEFAULT 0 CHECK(turns_spent >= 0),
    PRIMARY KEY (run_id, day, engine)
) WITHOUT ROWID;
CREATE INDEX crew_background_daily_by_crew_day ON crew_background_daily (background_id, day, engine);
