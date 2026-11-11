//! Schema 30: the Crew Protocol tables. Kept apart from `migrations.rs`, which lists it.

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

// Schema 31: the router's decision for a task. Immutable like every crew object; a task
// can hold more than one (a re-route after `needs_person` is a new decision). The decision
// names the brief it answers and carries only engines, models and reasons from this
// machine's own configuration, never the answer key.
pub(super) const CREW_ROUTES: &str = r#"
CREATE TABLE crew_route_decisions (
    id TEXT PRIMARY KEY CHECK(id GLOB 'sha256:*'),
    task_id TEXT NOT NULL REFERENCES crew_tasks(id) ON DELETE CASCADE,
    engine TEXT NOT NULL,
    assigned_by TEXT NOT NULL,
    body TEXT NOT NULL CHECK(json_valid(body))
);
CREATE INDEX crew_route_decisions_by_task ON crew_route_decisions(task_id);
CREATE TRIGGER crew_route_decisions_immutable BEFORE UPDATE ON crew_route_decisions
BEGIN SELECT RAISE(ABORT, 'crew objects are immutable'); END;
"#;

// Schema 32: how a task runs. One row per task holds who leads and who works, the checkout the
// verifier runs in, the commit the worker started from and the loop guard's counters
// (SPEC.md section 6). The row is mutable, since only a task's state and counters move; the
// objects a run produces stay immutable. A report has its own immutable table. Nothing here
// can hold the answer key: the guard keeps a fingerprint (a hash), never a check.
pub(super) const CREW_RUNS: &str = r#"
CREATE TABLE crew_runs (
    task_id TEXT PRIMARY KEY REFERENCES crew_tasks(id) ON DELETE CASCADE,
    lead_thread_id TEXT NOT NULL,
    member TEXT NOT NULL,
    worker_thread_id TEXT UNIQUE,
    worktree TEXT,
    base_commit TEXT,
    attempts INTEGER NOT NULL DEFAULT 0,
    failures INTEGER NOT NULL DEFAULT 0,
    streak INTEGER NOT NULL DEFAULT 0,
    fingerprint TEXT,
    approach_changed INTEGER NOT NULL DEFAULT 0 CHECK(approach_changed IN (0, 1)),
    last_report_run TEXT
);
CREATE TABLE crew_reports (
    id TEXT PRIMARY KEY CHECK(id GLOB 'sha256:*'),
    task_id TEXT NOT NULL REFERENCES crew_tasks(id) ON DELETE CASCADE,
    body TEXT NOT NULL CHECK(json_valid(body))
);
CREATE INDEX crew_reports_by_task ON crew_reports(task_id);
CREATE TRIGGER crew_reports_immutable BEFORE UPDATE ON crew_reports
BEGIN SELECT RAISE(ABORT, 'crew objects are immutable'); END;
"#;

// Schema 33: what the protected paths held when the task started (a hash per file, never a
// check) and the claim that makes one verification run at a time.
pub(super) const CREW_PROTECTED: &str = r#"
ALTER TABLE crew_runs ADD COLUMN protected TEXT;
ALTER TABLE crew_runs ADD COLUMN verifying_since TEXT;
"#;

// Schema 34: the bus. A message records who sent it (empty for the core), so a pair's exchanges
// and a worker's posts are counted from the table. A fact is one revision of a board key; the
// newest revision of a key in a crew is the one that stands, and (crew, key, rev) is unique, so
// two writes on the same revision cannot both land (H12). A delivery row says how a message
// reached one recipient: `live` (handed to a running turn's input), `next_turn` (queued, or
// sent with the next prompt), `tool_result` (returned in the recipient's next bus tool result),
// `held` (no running turn took it, so nothing is sent until a prompt or a tool call) or `failed`. Rows are only added; the newest row for a message and recipient says
// where it stands. The mode lives here, not in the message, so a message's id does not depend on it.
pub(super) const CREW_BUS: &str = r#"
ALTER TABLE crew_messages ADD COLUMN sender TEXT NOT NULL DEFAULT '';
CREATE INDEX crew_messages_by_sender ON crew_messages(task_id, sender);

CREATE TABLE crew_facts (
    id TEXT PRIMARY KEY CHECK(id GLOB 'sha256:*'),
    task_id TEXT NOT NULL REFERENCES crew_tasks(id) ON DELETE CASCADE,
    crew_id TEXT NOT NULL,
    key TEXT NOT NULL,
    rev INTEGER NOT NULL CHECK(rev >= 1),
    author TEXT NOT NULL,
    body TEXT NOT NULL CHECK(json_valid(body)),
    UNIQUE(crew_id, key, rev)
);
CREATE INDEX crew_facts_by_crew ON crew_facts(crew_id, key);
CREATE TRIGGER crew_facts_immutable BEFORE UPDATE ON crew_facts
BEGIN SELECT RAISE(ABORT, 'crew objects are immutable'); END;

CREATE TABLE crew_deliveries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id TEXT NOT NULL REFERENCES crew_messages(id) ON DELETE CASCADE,
    recipient TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    mode TEXT NOT NULL CHECK(mode IN ('held', 'live', 'next_turn', 'tool_result', 'failed')),
    note TEXT,
    at TEXT NOT NULL
);
CREATE INDEX crew_deliveries_by_message ON crew_deliveries(message_id, recipient);
CREATE TRIGGER crew_deliveries_immutable BEFORE UPDATE ON crew_deliveries
BEGIN SELECT RAISE(ABORT, 'crew objects are immutable'); END;
"#;

// Schema 35: the bus ledger. One row per event the bus decides on its own, appended by the core only and never
// changed. Today the only kind is `hop_reset`: a lead message to one member restarted the exchange count of every
// pair that member is in (SPEC.md H4). The body names the member, the message by id and each pair with its count
// before the reset. It holds no message text, and passes the secret scan and the answer-key check before it is stored.
pub(super) const CREW_BUS_EVENTS: &str = r#"
CREATE TABLE crew_bus_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    crew_id TEXT NOT NULL,
    task_id TEXT NOT NULL REFERENCES crew_tasks(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK(kind IN ('hop_reset')),
    body TEXT NOT NULL CHECK(json_valid(body)),
    at TEXT NOT NULL
);
CREATE INDEX crew_bus_events_by_crew ON crew_bus_events(crew_id, id);
CREATE TRIGGER crew_bus_events_immutable BEFORE UPDATE ON crew_bus_events
BEGIN SELECT RAISE(ABORT, 'crew objects are immutable'); END;
"#;
