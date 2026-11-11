//! Schema 29: the Crew Protocol tables. Kept apart from `migrations.rs`, which lists it.

// Crew Protocol v0 objects (docs/crew/protocol). Every object is canonical JSON in `body`,
// keyed by its content address, so a row can be verified by hashing it again. A task is
// keyed by its brief's id. The answer key has a table of its own: the brief a worker reads
// comes from `crew_briefs`, which has no column that can hold the key (H1). Objects are
// immutable, so an update is refused; only a task's `state` moves.
pub(super) const CREW_TABLES: &str = r#"
CREATE TABLE crew_tasks (
    id TEXT PRIMARY KEY CHECK(id GLOB 'sha256:*'),
    crew_id TEXT NOT NULL,
    run_id TEXT NOT NULL UNIQUE,
    state TEXT NOT NULL DEFAULT 'briefed' CHECK(state IN (
        'briefed', 'routed', 'running', 'verifying', 'failed', 'changed_approach',
        'needs_person', 'passed', 'stopped', 'cancelled'
    )),
    stop_reason TEXT CHECK(stop_reason IS NULL OR stop_reason IN (
        'repeated_failure', 'budget', 'too_many_denials', 'injection_suspected',
        'engine_error', 'lead_stopped'
    )),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX crew_tasks_by_crew ON crew_tasks(crew_id, created_at);

CREATE TABLE crew_briefs (
    id TEXT PRIMARY KEY REFERENCES crew_tasks(id) ON DELETE CASCADE,
    body TEXT NOT NULL CHECK(json_valid(body))
);

CREATE TABLE crew_answer_keys (
    task_id TEXT PRIMARY KEY REFERENCES crew_tasks(id) ON DELETE CASCADE,
    id TEXT NOT NULL UNIQUE CHECK(id GLOB 'sha256:*'),
    body TEXT NOT NULL CHECK(json_valid(body))
);

CREATE TABLE crew_messages (
    id TEXT PRIMARY KEY CHECK(id GLOB 'sha256:*'),
    task_id TEXT NOT NULL REFERENCES crew_tasks(id) ON DELETE CASCADE,
    recipient TEXT NOT NULL,
    kind TEXT NOT NULL,
    body TEXT NOT NULL CHECK(json_valid(body))
);
CREATE INDEX crew_messages_by_task ON crew_messages(task_id);

CREATE TABLE crew_leases (
    id TEXT PRIMARY KEY CHECK(id GLOB 'sha256:*'),
    task_id TEXT NOT NULL REFERENCES crew_tasks(id) ON DELETE CASCADE,
    holder TEXT NOT NULL,
    state TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    body TEXT NOT NULL CHECK(json_valid(body))
);
CREATE INDEX crew_leases_by_task ON crew_leases(task_id);

CREATE TABLE crew_evidence (
    id TEXT PRIMARY KEY CHECK(id GLOB 'sha256:*'),
    task_id TEXT NOT NULL REFERENCES crew_tasks(id) ON DELETE CASCADE,
    check_id TEXT NOT NULL,
    outcome TEXT NOT NULL,
    body TEXT NOT NULL CHECK(json_valid(body))
);
CREATE INDEX crew_evidence_by_task ON crew_evidence(task_id);

CREATE TABLE crew_gates (
    id TEXT PRIMARY KEY CHECK(id GLOB 'sha256:*'),
    task_id TEXT NOT NULL REFERENCES crew_tasks(id) ON DELETE CASCADE,
    stage TEXT NOT NULL,
    decision TEXT NOT NULL,
    body TEXT NOT NULL CHECK(json_valid(body))
);
CREATE INDEX crew_gates_by_task ON crew_gates(task_id);

CREATE TABLE crew_results (
    id TEXT PRIMARY KEY CHECK(id GLOB 'sha256:*'),
    task_id TEXT NOT NULL REFERENCES crew_tasks(id) ON DELETE CASCADE,
    state TEXT NOT NULL,
    final INTEGER NOT NULL CHECK(final IN (0, 1)),
    body TEXT NOT NULL CHECK(json_valid(body))
);
CREATE INDEX crew_results_by_task ON crew_results(task_id);

CREATE TRIGGER crew_briefs_immutable BEFORE UPDATE ON crew_briefs
BEGIN SELECT RAISE(ABORT, 'crew objects are immutable'); END;
CREATE TRIGGER crew_answer_keys_immutable BEFORE UPDATE ON crew_answer_keys
BEGIN SELECT RAISE(ABORT, 'crew objects are immutable'); END;
CREATE TRIGGER crew_messages_immutable BEFORE UPDATE ON crew_messages
BEGIN SELECT RAISE(ABORT, 'crew objects are immutable'); END;
CREATE TRIGGER crew_leases_immutable BEFORE UPDATE ON crew_leases
BEGIN SELECT RAISE(ABORT, 'crew objects are immutable'); END;
CREATE TRIGGER crew_evidence_immutable BEFORE UPDATE ON crew_evidence
BEGIN SELECT RAISE(ABORT, 'crew objects are immutable'); END;
CREATE TRIGGER crew_gates_immutable BEFORE UPDATE ON crew_gates
BEGIN SELECT RAISE(ABORT, 'crew objects are immutable'); END;
CREATE TRIGGER crew_results_immutable BEFORE UPDATE ON crew_results
BEGIN SELECT RAISE(ABORT, 'crew objects are immutable'); END;
"#;
