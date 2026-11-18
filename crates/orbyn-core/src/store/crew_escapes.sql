-- Migration 51: additive escape log (slice 4k, PR 1; docs/crew/escapes.md). `crew_escapes` is one row per merge that is, or may be,
-- an escape: a merge that got past review and checks and was later found defective. One row per merge (UNIQUE), so a revert and a
-- later signal never make two. `state` starts `possible`; only a person makes it `confirmed` (the CHECK refuses any other decider),
-- and a possible row has no decision. `crew_escape_events` is the audit trail: added in the same transaction as the state change,
-- never changed or removed (triggers), and unique per (escape, kind, cause) so replaying a cause adds nothing. Stored: ids, commit
-- ids, enums, areas, engine and model names and one short reason (at most 200 characters). Never stored: diffs, test bodies, command
-- output, prompts, secrets or comment text. No foreign key from an event to its escape, so the immutable trail outlives its row.
CREATE TABLE crew_escapes (
    id TEXT PRIMARY KEY CHECK(length(id) BETWEEN 1 AND 96),
    project_id TEXT NOT NULL,
    merge_task_id TEXT NOT NULL UNIQUE REFERENCES crew_merges(task_id) ON DELETE CASCADE,
    source TEXT NOT NULL CHECK(source IN ('revert', 'person_marked', 'post_merge_check', 'follow_up_fix')),
    state TEXT NOT NULL CHECK(state IN ('possible', 'confirmed', 'dismissed')),
    gap TEXT CHECK(gap IS NULL OR gap IN ('test_gap', 'tooling_gap')),
    gap_by TEXT CHECK(gap_by IS NULL OR gap_by IN ('core', 'person')),
    engine TEXT NOT NULL,
    model TEXT,
    areas TEXT NOT NULL CHECK(json_valid(areas)),
    lane TEXT,
    reason TEXT CHECK(reason IS NULL OR length(reason) <= 200),
    found_unix INTEGER NOT NULL,
    decided_unix INTEGER,
    decided_by TEXT CHECK(decided_by IS NULL OR decided_by IN ('core', 'person')),
    CHECK(state != 'possible' OR (decided_unix IS NULL AND decided_by IS NULL)),
    CHECK(state = 'possible' OR (decided_unix IS NOT NULL AND decided_by IS NOT NULL)),
    CHECK(state != 'confirmed' OR decided_by = 'person')
);
CREATE INDEX crew_escapes_by_project ON crew_escapes(project_id, found_unix);

CREATE TABLE crew_escape_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    escape_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('detected', 'confirmed', 'dismissed', 'reclassified', 'check_proposed', 'check_failed', 'check_proven', 'check_merged', 'check_withdrawn')),
    from_state TEXT CHECK(from_state IS NULL OR from_state IN ('possible', 'confirmed', 'dismissed')),
    to_state TEXT CHECK(to_state IS NULL OR to_state IN ('possible', 'confirmed', 'dismissed')),
    decided_by TEXT NOT NULL CHECK(decided_by IN ('core', 'person')),
    cause TEXT NOT NULL CHECK(length(cause) BETWEEN 1 AND 200),
    at INTEGER NOT NULL,
    UNIQUE(escape_id, kind, cause)
);
CREATE INDEX crew_escape_events_by_escape ON crew_escape_events(escape_id, id);
CREATE TRIGGER crew_escape_events_no_update BEFORE UPDATE ON crew_escape_events
BEGIN SELECT RAISE(ABORT, 'crew objects are immutable'); END;
CREATE TRIGGER crew_escape_events_no_delete BEFORE DELETE ON crew_escape_events
BEGIN SELECT RAISE(ABORT, 'crew objects are immutable'); END;

-- The `revert` source, back-filled from `crew_merges.review_state = 'reverted'` as POSSIBLE. A merge whose lead thread is gone has no
-- project and drops out (OR IGNORE skips the NOT NULL), as it does for 4j. Idempotent: the merge is unique and so is the event.
INSERT OR IGNORE INTO crew_escapes (id, project_id, merge_task_id, source, state, engine, areas, reason, found_unix)
SELECT 'esc_' || lower(hex(randomblob(12))), t.project_id, m.task_id, 'revert', 'possible', m.engine, m.areas,
       'The merge was reverted.', COALESCE(m.reviewed_unix, m.merged_unix)
FROM crew_merges m
JOIN crew_runs r ON r.task_id = m.task_id
JOIN threads t ON t.id = r.lead_thread_id
WHERE m.review_state = 'reverted';
INSERT OR IGNORE INTO crew_escape_events (escape_id, project_id, kind, from_state, to_state, decided_by, cause, at)
SELECT e.id, e.project_id, 'detected', NULL, 'possible', 'core', 'revert:' || substr(COALESCE(m.reverted_commit, 'unknown'), 1, 100), e.found_unix
FROM crew_escapes e JOIN crew_merges m ON m.task_id = e.merge_task_id
WHERE e.source = 'revert'
  AND NOT EXISTS (SELECT 1 FROM crew_escape_events d WHERE d.escape_id = e.id AND d.kind = 'detected');

-- From now on a revert makes its possible escape in the same transaction as the revert itself, so no caller can forget it and a
-- repeat makes nothing. OR IGNORE: a failure to log must never undo the revert.
CREATE TRIGGER crew_merges_revert_escape AFTER UPDATE OF review_state ON crew_merges
WHEN NEW.review_state = 'reverted'
BEGIN
    INSERT OR IGNORE INTO crew_escapes (id, project_id, merge_task_id, source, state, engine, areas, reason, found_unix)
    SELECT 'esc_' || lower(hex(randomblob(12))), t.project_id, NEW.task_id, 'revert', 'possible', NEW.engine, NEW.areas,
           'The merge was reverted.', COALESCE(NEW.reviewed_unix, NEW.merged_unix)
    FROM crew_runs r JOIN threads t ON t.id = r.lead_thread_id
    WHERE r.task_id = NEW.task_id;
    INSERT OR IGNORE INTO crew_escape_events (escape_id, project_id, kind, from_state, to_state, decided_by, cause, at)
    SELECT e.id, e.project_id, 'detected', NULL, 'possible', 'core', 'revert:' || substr(COALESCE(NEW.reverted_commit, 'unknown'), 1, 100), e.found_unix
    FROM crew_escapes e WHERE e.merge_task_id = NEW.task_id AND e.source = 'revert'
      AND NOT EXISTS (SELECT 1 FROM crew_escape_events d WHERE d.escape_id = e.id AND d.kind = 'detected');
END;
