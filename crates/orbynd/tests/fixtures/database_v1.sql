-- A database as the first release left it: schema version 1 (migration 1, which never changes) holding one project, one
-- thread with a finished run, and one audit row. `old_database.rs` starts the real service on it.
-- `{{PROJECT_PATH}}` is replaced with a real folder by the test.
CREATE TABLE projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    path TEXT NOT NULL UNIQUE,
    trusted INTEGER NOT NULL DEFAULT 0,
    is_git INTEGER NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE threads (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id),
    title TEXT NOT NULL,
    engine TEXT NOT NULL,
    model TEXT,
    cwd TEXT NOT NULL,
    worktree_path TEXT,
    worktree_branch TEXT,
    worktree_base TEXT,
    skill_profile TEXT,
    engine_session TEXT,
    next_seq INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX threads_by_project ON threads(project_id, updated_at);

CREATE TABLE runs (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    engine TEXT NOT NULL,
    state TEXT NOT NULL,
    owner_epoch TEXT NOT NULL,
    error TEXT,
    started_at TEXT NOT NULL,
    ended_at TEXT
);
-- One active run per thread, enforced by the database.
CREATE UNIQUE INDEX runs_one_active ON runs(thread_id)
    WHERE state IN ('admitted', 'starting', 'running', 'awaiting_approval');
CREATE INDEX runs_by_thread ON runs(thread_id, started_at);
-- Terminal states never change, whichever code path tries.
CREATE TRIGGER runs_terminal_is_final BEFORE UPDATE ON runs
    WHEN OLD.state IN ('completed', 'failed', 'cancelled', 'interrupted')
BEGIN
    SELECT RAISE(ABORT, 'terminal run is final');
END;

CREATE TABLE thread_events (
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    seq INTEGER NOT NULL,
    run_id TEXT,
    ts TEXT NOT NULL,
    event TEXT NOT NULL,
    PRIMARY KEY (thread_id, seq)
) WITHOUT ROWID;

CREATE TABLE approvals (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    engine TEXT NOT NULL,
    kind TEXT NOT NULL,
    target TEXT NOT NULL,
    summary TEXT NOT NULL,
    detail TEXT NOT NULL,
    state TEXT NOT NULL,
    decided_by TEXT,
    decided_at TEXT,
    created_at TEXT NOT NULL,
    event_seq INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX approvals_by_state ON approvals(state, created_at);
CREATE INDEX approvals_by_run ON approvals(run_id, state);

CREATE TABLE policy_rules (
    id TEXT PRIMARY KEY,
    effect TEXT NOT NULL,
    kind TEXT NOT NULL,
    pattern TEXT NOT NULL,
    engine TEXT,
    note TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE audit_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ts TEXT NOT NULL,
    actor TEXT NOT NULL,
    action TEXT NOT NULL,
    target TEXT NOT NULL,
    decision TEXT,
    source TEXT NOT NULL,
    thread_id TEXT,
    run_id TEXT,
    detail TEXT NOT NULL
);
CREATE INDEX audit_by_thread ON audit_events(thread_id, id);
CREATE TRIGGER audit_events_no_update BEFORE UPDATE ON audit_events
BEGIN
    SELECT RAISE(ABORT, 'audit log is append-only');
END;
CREATE TRIGGER audit_events_no_delete BEFORE DELETE ON audit_events
BEGIN
    SELECT RAISE(ABORT, 'audit log is append-only');
END;

CREATE TABLE skill_profiles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    skills TEXT NOT NULL
);

CREATE TABLE custom_engines (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    tier TEXT NOT NULL,
    target TEXT NOT NULL,
    args TEXT NOT NULL
);

CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
INSERT INTO schema_migrations VALUES (1, '2026-01-01T00:00:00Z');

INSERT INTO projects (id, name, path, trusted, is_git, created_at)
VALUES ('p-old', 'Kept project', '{{PROJECT_PATH}}', 1, 0, '2026-01-01T00:00:00Z');
INSERT INTO threads (id, project_id, title, engine, model, cwd, next_seq, created_at, updated_at)
VALUES ('t-old', 'p-old', 'A thread from the first release', 'claude', NULL, '{{PROJECT_PATH}}', 1,
        '2026-01-01T00:00:01Z', '2026-01-01T00:00:02Z');
INSERT INTO runs (id, thread_id, engine, state, owner_epoch, error, started_at, ended_at)
VALUES ('r-old', 't-old', 'claude', 'completed', 'old-epoch', NULL, '2026-01-01T00:00:01Z', '2026-01-01T00:00:02Z');
INSERT INTO audit_events (ts, actor, action, target, decision, source, thread_id, run_id, detail)
VALUES ('2026-01-01T00:00:02Z', 'user', 'thread.create', 't-old', NULL, 'user', 't-old', NULL, '{}');
