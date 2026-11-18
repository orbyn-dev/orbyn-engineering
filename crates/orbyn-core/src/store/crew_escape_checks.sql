-- Migration 52: additive created checks (slice 4k, PR 4; docs/crew/escapes.md, "Create a check"). `crew_escape_checks` holds at most one
-- check per escape: the proposal a person made from a confirmed escape (the new test file's path, what it asserts, the command that runs
-- it, the commit it must pass on), the crew task of the worker that writes it, its state and its proof. `proof` is JSON: the two commit
-- ids and two booleans, never a test body or command output. Older tables are untouched; the escape events already carry the
-- `check_*` kinds (migration 51), so no kind is added here. A proposal writes no task: only a person's accept does.
CREATE TABLE crew_escape_checks (
    escape_id TEXT PRIMARY KEY REFERENCES crew_escapes(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL,
    path TEXT NOT NULL CHECK(length(path) BETWEEN 1 AND 200),
    asserts TEXT NOT NULL CHECK(length(asserts) BETWEEN 1 AND 300),
    run TEXT NOT NULL CHECK(length(run) BETWEEN 1 AND 300),
    fix_commit TEXT NOT NULL CHECK(length(fix_commit) = 40),
    task_id TEXT,
    state TEXT NOT NULL CHECK(state IN ('proposed', 'writing', 'proven', 'failed', 'withdrawn', 'merged')),
    proof TEXT CHECK(proof IS NULL OR json_valid(proof)),
    note TEXT CHECK(note IS NULL OR length(note) <= 200),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK(attempts >= 0),
    -- The worker's change as the core read it from git: the commit, a SHA-256 over its paths and bytes, and its paths. A person's approval
    -- (a machine that cannot restrict the proof) names this commit and hash, so a different change needs a new approval.
    diff_commit TEXT CHECK(diff_commit IS NULL OR length(diff_commit) = 40),
    diff_hash TEXT CHECK(diff_hash IS NULL OR length(diff_hash) = 64),
    diff_paths TEXT CHECK(diff_paths IS NULL OR json_valid(diff_paths)),
    approved_commit TEXT CHECK(approved_commit IS NULL OR length(approved_commit) = 40),
    approved_hash TEXT CHECK(approved_hash IS NULL OR length(approved_hash) = 64),
    proposed_unix INTEGER NOT NULL,
    updated_unix INTEGER NOT NULL,
    CHECK(state NOT IN ('writing', 'proven', 'merged') OR task_id IS NOT NULL)
);
CREATE INDEX crew_escape_checks_by_task ON crew_escape_checks(task_id);
CREATE INDEX crew_escape_checks_by_state ON crew_escape_checks(state);

-- Every task an accept ever created for a check, written in the accept transaction and never deleted or cleared (a retry clears
-- `crew_escape_checks.task_id`, a withdraw followed by a new proposal deletes the row). The merge queue refuses a recorded task unless the
-- escape's current check names that same task and is proven, so an old worker's change can never be queued as an unproven one.
CREATE TABLE crew_escape_check_tasks (
    task_id TEXT PRIMARY KEY,
    escape_id TEXT NOT NULL,
    created_unix INTEGER NOT NULL
);
CREATE INDEX crew_escape_check_tasks_by_escape ON crew_escape_check_tasks(escape_id);
