-- Migration 63: Code Mode run records (docs/code-mode.md, "Run record"). Kept 7 days. A call keeps its argument
-- digest and never its arguments; the full result is kept only when it is at most 256 KiB.
CREATE TABLE code_runs (
    run_id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    status TEXT NOT NULL,
    script TEXT NOT NULL,
    script_sha256 TEXT NOT NULL,
    seed_start_ms INTEGER NOT NULL,
    seed INTEGER NOT NULL,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    error TEXT,
    result TEXT,
    logs TEXT NOT NULL DEFAULT '[]',
    logs_total INTEGER,
    truncated INTEGER NOT NULL DEFAULT 0,
    resumes INTEGER NOT NULL DEFAULT 0,
    crew_worker INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX code_runs_by_thread ON code_runs(thread_id, started_at);

CREATE TABLE code_run_calls (
    run_id TEXT NOT NULL REFERENCES code_runs(run_id) ON DELETE CASCADE,
    idx INTEGER NOT NULL,
    tool TEXT NOT NULL,
    args_digest TEXT NOT NULL,
    effect INTEGER NOT NULL,
    status TEXT NOT NULL,
    preview TEXT,
    result_bytes INTEGER,
    result TEXT,
    duration_ms INTEGER,
    PRIMARY KEY (run_id, idx)
);
