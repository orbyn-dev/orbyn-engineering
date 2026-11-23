-- Migration 77: watches (docs/crew.md, "Kiro Crew features" KC6, slice A1). `crew_background.trigger_kind` gains `watch`, which a
-- CHECK cannot alter in place, so the table is rebuilt with every column and row. Its two dependent tables are rebuilt in the same
-- shape too: migrations run inside a transaction, where `PRAGMA foreign_keys` cannot change, and a plain DROP of the parent would
-- cascade the runs and the daily ledger away. The rebuilt tables reference the new parent, then take the old names, so every foreign
-- key and ON DELETE CASCADE still holds. Ids, states, commands in the crew's own settings and counts only; the two watch tables
-- hold no output and no repository text until a later slice writes the results it needs.
CREATE TABLE crew_background_next (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 80),
    template TEXT NOT NULL CHECK(template IN ('fix_failing_ci', 'flaky_test', 'dependency_bump', 'security_advisory')),
    trigger_kind TEXT NOT NULL CHECK(trigger_kind IN ('ci_failure', 'issue_label', 'schedule', 'dependency_update', 'watch')),
    trigger_settings TEXT NOT NULL CHECK(json_valid(trigger_settings)),
    budget_turns INTEGER NOT NULL CHECK(budget_turns BETWEEN 1 AND 1000),
    budget_minutes INTEGER NOT NULL CHECK(budget_minutes BETWEEN 1 AND 1440),
    enabled INTEGER NOT NULL DEFAULT 0 CHECK(enabled IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    daily_runs INTEGER NOT NULL DEFAULT 0 CHECK(daily_runs >= 0),
    daily_turns INTEGER NOT NULL DEFAULT 0 CHECK(daily_turns >= 0),
    utc_offset_minutes INTEGER NOT NULL DEFAULT 0 CHECK(utc_offset_minutes BETWEEN -840 AND 840)
);
INSERT INTO crew_background_next
    (rowid, id, project_id, name, template, trigger_kind, trigger_settings, budget_turns, budget_minutes, enabled, created_at,
     updated_at, daily_runs, daily_turns, utc_offset_minutes)
    SELECT rowid, id, project_id, name, template, trigger_kind, trigger_settings, budget_turns, budget_minutes, enabled, created_at,
           updated_at, daily_runs, daily_turns, utc_offset_minutes
    FROM crew_background;

CREATE TABLE crew_background_runs_next (
    id TEXT PRIMARY KEY,
    background_id TEXT NOT NULL REFERENCES crew_background_next(id) ON DELETE CASCADE,
    state TEXT NOT NULL CHECK(state IN ('running', 'stopped', 'needs_review')),
    stop_reason TEXT CHECK(stop_reason IS NULL OR stop_reason IN ('budget', 'repeated_failure', 'engine_error', 'disabled')),
    turns_spent INTEGER NOT NULL DEFAULT 0 CHECK(turns_spent >= 0),
    minutes_spent INTEGER NOT NULL DEFAULT 0 CHECK(minutes_spent >= 0),
    budget_turns INTEGER NOT NULL CHECK(budget_turns BETWEEN 1 AND 1000),
    budget_minutes INTEGER NOT NULL CHECK(budget_minutes BETWEEN 1 AND 1440),
    review_waiting INTEGER NOT NULL DEFAULT 0 CHECK(review_waiting IN (0, 1)),
    review_reason TEXT,
    started_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    ended_at TEXT,
    CHECK((state = 'stopped') = (stop_reason IS NOT NULL)),
    CHECK((state = 'running') = (ended_at IS NULL))
);
-- `rowid` is kept too: the runs page breaks a tie in `started_at` with it.
INSERT INTO crew_background_runs_next
    (rowid, id, background_id, state, stop_reason, turns_spent, minutes_spent, budget_turns, budget_minutes, review_waiting,
     review_reason, started_at, updated_at, ended_at)
    SELECT rowid, id, background_id, state, stop_reason, turns_spent, minutes_spent, budget_turns, budget_minutes, review_waiting,
           review_reason, started_at, updated_at, ended_at
    FROM crew_background_runs;

CREATE TABLE crew_background_daily_next (
    background_id TEXT NOT NULL REFERENCES crew_background_next(id) ON DELETE CASCADE,
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
DROP TABLE crew_background;
ALTER TABLE crew_background_next RENAME TO crew_background;
ALTER TABLE crew_background_runs_next RENAME TO crew_background_runs;
ALTER TABLE crew_background_daily_next RENAME TO crew_background_daily;

CREATE UNIQUE INDEX crew_background_name ON crew_background(project_id, name COLLATE NOCASE);
CREATE TRIGGER crew_background_budget_fixed BEFORE UPDATE OF
    project_id, template, budget_turns, budget_minutes, daily_runs, daily_turns, utc_offset_minutes
    ON crew_background
BEGIN
    SELECT RAISE(ABORT, 'a background crew keeps its project, template and budget');
END;
CREATE UNIQUE INDEX crew_background_one_running ON crew_background_runs(background_id) WHERE state = 'running';
CREATE INDEX crew_background_runs_by_crew ON crew_background_runs(background_id, started_at);
CREATE TRIGGER crew_background_runs_budget_fixed BEFORE UPDATE OF budget_turns, budget_minutes ON crew_background_runs
BEGIN SELECT RAISE(ABORT, 'a background run keeps the budget it was given'); END;
CREATE INDEX crew_background_daily_by_crew_day ON crew_background_daily (background_id, day, engine);

-- For later slices: the last result of a watch's command, its baseline pass and what the newest check found, and one row per state
-- change the person reads. `tail` is the command's own output, bounded by the writer when it exists; nothing reads either table yet.
CREATE TABLE crew_watch_state (
    background_id TEXT PRIMARY KEY REFERENCES crew_background(id) ON DELETE CASCADE,
    last_result TEXT CHECK(last_result IN ('pass', 'fail')),
    baseline_done INTEGER NOT NULL DEFAULT 0,
    error_streak INTEGER NOT NULL DEFAULT 0,
    last_checked_unix INTEGER,
    last_exit_code INTEGER,
    last_state TEXT NOT NULL DEFAULT 'not_checked'
        CHECK(last_state IN ('not_checked', 'passing', 'failing', 'could_not_run')),
    last_reason TEXT,
    tail TEXT
);
CREATE TABLE crew_watch_changes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    background_id TEXT NOT NULL REFERENCES crew_background(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK(kind IN ('baseline', 'failing', 'passing', 'turned_off')),
    at_unix INTEGER NOT NULL,
    exit_code INTEGER,
    action TEXT NOT NULL CHECK(action IN ('needs_you', 'started_template', 'resolved', 'none')),
    tail TEXT
);
CREATE INDEX crew_watch_changes_by_crew ON crew_watch_changes(background_id, id);
