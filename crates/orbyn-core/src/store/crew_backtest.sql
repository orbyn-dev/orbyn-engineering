-- Migration 49: additive reviewer backtest (slice 4n). A run keeps the person's per-engine token budgets as JSON and
-- why it stopped; a lane keeps its engine, model, tokens used and progress (and, once scored, how many escapes it
-- faced and caught: NULL means the escape log could not say, which is not zero); a result is one finding fingerprint
-- on one past pull request and how it fared ('not_measured' when the escape log could not say: never a miss). Ids, counts and fingerprints only: no prompt, no diff, no file text, no
-- money. A removed project takes its runs, lanes and results with it.
CREATE TABLE crew_backtest_runs (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    state TEXT NOT NULL CHECK(state IN ('estimated', 'running', 'stopped', 'finished', 'failed')),
    budgets TEXT NOT NULL CHECK(json_valid(budgets)),
    stop_reason TEXT,
    created_unix INTEGER NOT NULL,
    finished_unix INTEGER
);
CREATE INDEX crew_backtest_runs_by_project ON crew_backtest_runs(project_id, created_unix);

CREATE TABLE crew_backtest_lanes (
    run_id TEXT NOT NULL REFERENCES crew_backtest_runs(id) ON DELETE CASCADE,
    lane TEXT NOT NULL,
    engine TEXT NOT NULL,
    model TEXT NOT NULL,
    tokens_used INTEGER NOT NULL DEFAULT 0 CHECK(tokens_used >= 0),
    state TEXT NOT NULL CHECK(state IN ('pending', 'running', 'stopped', 'failed', 'done')),
    escapes_n INTEGER CHECK(escapes_n IS NULL OR escapes_n >= 0),
    caught_n INTEGER CHECK(caught_n IS NULL OR caught_n >= 0),
    PRIMARY KEY (run_id, lane, engine)
);

CREATE TABLE crew_backtest_results (
    run_id TEXT NOT NULL REFERENCES crew_backtest_runs(id) ON DELETE CASCADE,
    lane TEXT NOT NULL,
    engine TEXT NOT NULL,
    pr INTEGER NOT NULL CHECK(pr >= 0),
    finding_fp TEXT NOT NULL,
    outcome TEXT NOT NULL CHECK(outcome IN ('caught', 'unmatched', 'no_escape', 'not_measured')),
    PRIMARY KEY (run_id, lane, engine, pr, finding_fp),
    FOREIGN KEY (run_id, lane, engine) REFERENCES crew_backtest_lanes(run_id, lane, engine) ON DELETE CASCADE
);
