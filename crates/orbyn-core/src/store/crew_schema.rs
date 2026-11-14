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

// Schema 36: the bus ledger. One row per event the bus decides on its own, appended by the core only and never
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

// Schema 37: review lanes, findings and the fix loop (docs/crew.md, "Review stages"). A review run is a task of its own; its
// row names the work it reviews (`subject_id`), its lane, its round (a re-review of a lane is the next round) and the commit
// it read. `check_of` is set when the run is the independent second run of a finding check. A finding is the protocol's
// object, kept beside the lane, round and a fingerprint of (lane, path, claim) so a later round can tell the same
// finding from another. A finding has no state of its own: its confirmation is a Gate at stage `finding_check` stored with the
// reviewed task. A repro row keeps the lead's repro check that confirmed or cleared a finding (core only: it is part of the key
// for the fix, and never shown to a worker). A fix row ties a confirmed finding to the builder task that answers it. All rows are immutable.
pub(super) const CREW_REVIEW: &str = r#"
-- The index entries that hid a change from git when the task started (`git ls-files -v` showing `h` or `S`), as a JSON list.
-- A check fails if the worker adds one: an edit to such a file does not show in any git diff.
ALTER TABLE crew_runs ADD COLUMN index_flags TEXT;
CREATE TABLE crew_reviews (
    task_id TEXT PRIMARY KEY REFERENCES crew_tasks(id) ON DELETE CASCADE,
    subject_id TEXT NOT NULL REFERENCES crew_tasks(id) ON DELETE CASCADE,
    lane TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('review', 'finding_check')),
    round INTEGER NOT NULL CHECK(round >= 1),
    commit_id TEXT NOT NULL,
    check_of TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX crew_reviews_by_subject ON crew_reviews(subject_id, lane, round);

CREATE TABLE crew_findings (
    id TEXT PRIMARY KEY CHECK(id GLOB 'sha256:*'),
    task_id TEXT NOT NULL REFERENCES crew_tasks(id) ON DELETE CASCADE,
    subject_id TEXT NOT NULL,
    lane TEXT NOT NULL,
    round INTEGER NOT NULL,
    fingerprint TEXT NOT NULL,
    body TEXT NOT NULL CHECK(json_valid(body))
);
CREATE INDEX crew_findings_by_subject ON crew_findings(subject_id, lane, round);
CREATE INDEX crew_findings_by_task ON crew_findings(task_id);

CREATE TABLE crew_finding_repros (
    finding_id TEXT PRIMARY KEY REFERENCES crew_findings(id) ON DELETE CASCADE,
    subject_id TEXT NOT NULL,
    repro TEXT NOT NULL CHECK(json_valid(repro)),
    created_at TEXT NOT NULL
);

CREATE TABLE crew_fixes (
    finding_id TEXT PRIMARY KEY REFERENCES crew_findings(id) ON DELETE CASCADE,
    subject_id TEXT NOT NULL,
    fix_task_id TEXT NOT NULL UNIQUE REFERENCES crew_tasks(id) ON DELETE CASCADE,
    fingerprint TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX crew_fixes_by_subject ON crew_fixes(subject_id, fingerprint);

CREATE TRIGGER crew_reviews_immutable BEFORE UPDATE ON crew_reviews
BEGIN SELECT RAISE(ABORT, 'crew objects are immutable'); END;
CREATE TRIGGER crew_findings_immutable BEFORE UPDATE ON crew_findings
BEGIN SELECT RAISE(ABORT, 'crew objects are immutable'); END;
CREATE TRIGGER crew_finding_repros_immutable BEFORE UPDATE ON crew_finding_repros
BEGIN SELECT RAISE(ABORT, 'crew objects are immutable'); END;
CREATE TRIGGER crew_fixes_immutable BEFORE UPDATE ON crew_fixes
BEGIN SELECT RAISE(ABORT, 'crew objects are immutable'); END;
"#;

// Schema 38: Autopilot (docs/crew.md, "Autopilot"). `crew_autopilot_modes` holds the person's choice, `off`, `on` or `full`, per
// scope: `default`, `repo:<project id>` or `crew:<crew id>`; a crew's choice beats its repository's, which beats the default, and
// with none the mode is `full`. `crew_autopilot_events` is the approval ledger: one row per approval, denial, asked person,
// disagreement or trip wire, appended by the core only and never changed. A row names the action by kind, target and paths,
// who decided, the rule or the reasons and the tier. It holds no secret and no answer-key text: the core redacts, withholds and
// scans each row before it is stored. Counters (denials in a row, denials in a run) are read from these rows, never kept apart.
pub(super) const CREW_AUTOPILOT: &str = r#"
CREATE TABLE crew_autopilot_modes (
    scope TEXT PRIMARY KEY,
    mode TEXT NOT NULL CHECK(mode IN ('off', 'on', 'full')),
    updated_at TEXT NOT NULL
);
CREATE TABLE crew_autopilot_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    crew_id TEXT NOT NULL,
    task_id TEXT NOT NULL REFERENCES crew_tasks(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    body TEXT NOT NULL CHECK(json_valid(body)),
    at TEXT NOT NULL
);
CREATE INDEX crew_autopilot_events_by_task ON crew_autopilot_events(task_id, id);
CREATE INDEX crew_autopilot_events_by_crew ON crew_autopilot_events(crew_id, id);
CREATE TRIGGER crew_autopilot_events_immutable BEFORE UPDATE ON crew_autopilot_events
BEGIN SELECT RAISE(ABORT, 'crew objects are immutable'); END;
"#;

// Schema 39: the merge queue and the after-merge Review queue (docs/crew.md, "Autopilot"). `crew_merge_queue` holds one row per
// task asked to merge: its candidate commit, the risk tier with its reasons, and where it stands (`queued`, `blocked` until its
// stages are green, `waiting` for a person, `merging`, `merged`, `rejected`). `crew_merges` holds one row per merge that
// reached the target branch, which is one item of the Review queue: the commit before, the merge commit, the proof, what was
// not verified, the review window and the person's answer, and what the watch on main found. A merge is a `--no-ff` merge commit,
// so a revert is one `git revert -m 1`. A person's release (`released`) lasts for the entry. The queue is local: it moves a local branch and nothing is pushed.
pub(super) const CREW_MERGES: &str = r#"
CREATE TABLE crew_merge_queue (
    task_id TEXT PRIMARY KEY REFERENCES crew_tasks(id) ON DELETE CASCADE,
    crew_id TEXT NOT NULL,
    repo TEXT NOT NULL,
    branch TEXT NOT NULL,
    candidate TEXT NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('queued', 'blocked', 'waiting', 'merging', 'merged', 'rejected')),
    risk TEXT NOT NULL CHECK(risk IN ('low', 'medium', 'high')),
    reasons TEXT NOT NULL CHECK(json_valid(reasons)),
    areas TEXT NOT NULL CHECK(json_valid(areas)),
    samples TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(samples)),
    engine TEXT NOT NULL,
    files INTEGER NOT NULL,
    lines INTEGER NOT NULL,
    mode TEXT NOT NULL,
    note TEXT,
    released INTEGER NOT NULL DEFAULT 0 CHECK(released IN (0, 1)),
    queued_unix INTEGER NOT NULL,
    updated_unix INTEGER NOT NULL
);
CREATE INDEX crew_merge_queue_by_target ON crew_merge_queue(repo, branch, queued_unix);

CREATE TABLE crew_merges (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id TEXT NOT NULL UNIQUE REFERENCES crew_tasks(id) ON DELETE CASCADE,
    crew_id TEXT NOT NULL,
    repo TEXT NOT NULL,
    branch TEXT NOT NULL,
    before_commit TEXT NOT NULL,
    merge_commit TEXT NOT NULL,
    candidate TEXT NOT NULL,
    risk TEXT NOT NULL CHECK(risk IN ('low', 'medium', 'high')),
    reasons TEXT NOT NULL CHECK(json_valid(reasons)),
    areas TEXT NOT NULL CHECK(json_valid(areas)),
    engine TEXT NOT NULL,
    mode TEXT NOT NULL,
    files INTEGER NOT NULL,
    lines INTEGER NOT NULL,
    proof TEXT NOT NULL CHECK(json_valid(proof)),
    not_verified TEXT NOT NULL CHECK(json_valid(not_verified)),
    merged_unix INTEGER NOT NULL,
    due_unix INTEGER,
    review_state TEXT NOT NULL DEFAULT 'waiting' CHECK(review_state IN ('waiting', 'looks_good', 'commented', 'reverted')),
    comment TEXT,
    reviewed_unix INTEGER,
    watch TEXT NOT NULL DEFAULT 'unknown' CHECK(watch IN ('unknown', 'green', 'broken')),
    reverted_commit TEXT,
    needs_person TEXT
);
CREATE INDEX crew_merges_by_target ON crew_merges(repo, branch, id);
"#;

// Schema 41: background crews (docs/crew.md, "Background crews"), kept on this device in the data folder and never in a repository.
// `crew_background` is one crew the person defined: its project, name, template, trigger and the run budget set at creation (no
// column changes it after, and the core refuses the attempt). It is created off. `crew_background_runs` is one row per run: its
// state, what it spent against the budget it was given, why it stopped and whether it waits for the person's review. At most one
// run per crew is `running`. No project key: a removed project keeps its crews, which a later trigger refuses to start.
pub(super) const CREW_BACKGROUND: &str = r#"
CREATE TABLE crew_background (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 80),
    template TEXT NOT NULL CHECK(template IN ('fix_failing_ci', 'flaky_test', 'dependency_bump', 'security_advisory')),
    trigger_kind TEXT NOT NULL CHECK(trigger_kind IN ('ci_failure', 'issue_label', 'schedule', 'dependency_update')),
    trigger_settings TEXT NOT NULL CHECK(json_valid(trigger_settings)),
    budget_turns INTEGER NOT NULL CHECK(budget_turns BETWEEN 1 AND 1000),
    budget_minutes INTEGER NOT NULL CHECK(budget_minutes BETWEEN 1 AND 1440),
    enabled INTEGER NOT NULL DEFAULT 0 CHECK(enabled IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE UNIQUE INDEX crew_background_name ON crew_background(project_id, name COLLATE NOCASE);
CREATE TRIGGER crew_background_budget_fixed BEFORE UPDATE OF project_id, template, budget_turns, budget_minutes ON crew_background
BEGIN SELECT RAISE(ABORT, 'a background crew keeps its project, template and budget'); END;

CREATE TABLE crew_background_runs (
    id TEXT PRIMARY KEY,
    background_id TEXT NOT NULL REFERENCES crew_background(id) ON DELETE CASCADE,
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
CREATE UNIQUE INDEX crew_background_one_running ON crew_background_runs(background_id) WHERE state = 'running';
CREATE INDEX crew_background_runs_by_crew ON crew_background_runs(background_id, started_at);
"#;

// Schema 42: a run keeps the budget it was given. `crew_background_runs.budget_turns` and `budget_minutes` are copied from the crew
// when the run opens, and the crew's own budget is fixed by a trigger; this fixes the copy the same way, so a stopped run can never
// be edited to look as if it stayed within a budget it exceeded. The core never writes these columns after the insert.
pub(super) const CREW_BACKGROUND_RUN_BUDGET: &str = r#"
CREATE TRIGGER crew_background_runs_budget_fixed BEFORE UPDATE OF budget_turns, budget_minutes ON crew_background_runs
BEGIN SELECT RAISE(ABORT, 'a background run keeps the budget it was given'); END;
"#;

// Schema 43: the blast-radius guard's two lists (docs/crew.md, "As built (slice 4i, guard)"). `crew_guard_targets` holds the names the
// person marked `prod` (always production-linked) or `safe` (exempt from the second key), on this device only: never a file in a
// repository, so a repository file cannot loosen the guard. A pattern is a bare name or `*.suffix`, 1 to 253 characters; the
// writer checks the charset and holds no more than 200 entries, and writes an audit row in the same transaction before the change.
// Nothing here is a secret: a name with a password, a token or a URL is refused before it is stored.
pub(super) const CREW_GUARD_TARGETS: &str = r#"
CREATE TABLE crew_guard_targets (
    id TEXT PRIMARY KEY,
    list TEXT NOT NULL CHECK(list IN ('prod', 'safe')),
    pattern TEXT NOT NULL CHECK(length(pattern) BETWEEN 1 AND 253),
    kind TEXT NOT NULL CHECK(kind IN ('host', 'profile', 'project', 'context', 'workspace', 'bucket')),
    note TEXT CHECK(note IS NULL OR length(note) <= 200),
    added_at TEXT NOT NULL,
    UNIQUE(list, kind, pattern)
);
"#;

// Schema 44: autonomy by area, in shadow (docs/crew.md, "As built (slice 4j, shadow)"). `crew_area_modes` holds, per project and area
// (the first two folders of a path), whether the circuit breaker has lowered the area (`demoted`) or it follows its ceiling
// (`follows`), when and why, where the recovery clock started and a person's override (nothing writes an override yet). The record
// the breaker reads is never stored: it is counted from `crew_merges` each time. `crew_area_events` is the audit trail: one row
// per breaker or person write, added in the same transaction before the mode row changes, and never changed or removed. The
// `cause` is the revert commit that tripped or reset the breaker, `restore:<recovery start>` for a return, or `person:<audit id>`.
// On this device only: never a file in a repository. Nothing reads these rows for a decision yet.
pub(super) const CREW_AUTONOMY: &str = r#"
CREATE TABLE crew_area_modes (
    project_id TEXT NOT NULL,
    area TEXT NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('follows', 'demoted')),
    tripped_at INTEGER,
    trip_reason TEXT CHECK(trip_reason IS NULL OR json_valid(trip_reason)),
    recovery_since INTEGER,
    override TEXT CHECK(override IS NULL OR override IN ('keep_ceiling', 'off')),
    override_at INTEGER,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(project_id, area)
);
CREATE TABLE crew_area_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id TEXT NOT NULL,
    area TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('demoted', 'recovery_reset', 'restored', 'override_set', 'override_cleared')),
    from_mode TEXT,
    to_mode TEXT,
    decided_by TEXT NOT NULL CHECK(decided_by IN ('breaker', 'person')),
    cause TEXT NOT NULL,
    record TEXT NOT NULL CHECK(json_valid(record)),
    at INTEGER NOT NULL,
    UNIQUE(project_id, area, kind, cause)
);
CREATE INDEX crew_area_events_by_area ON crew_area_events(project_id, area, id);
CREATE TRIGGER crew_area_events_no_update BEFORE UPDATE ON crew_area_events
BEGIN SELECT RAISE(ABORT, 'crew objects are immutable'); END;
CREATE TRIGGER crew_area_events_no_delete BEFORE DELETE ON crew_area_events
BEGIN SELECT RAISE(ABORT, 'crew objects are immutable'); END;
"#;
