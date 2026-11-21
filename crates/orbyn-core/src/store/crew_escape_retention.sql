-- Migration 58 (escape writer protocol 57): guarded retention and permanent suppression receipts. Existing rows and events are unchanged.
CREATE TABLE crew_escape_tombstones (
    merge_task_id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('possible','dismissed')),
    evicted_unix INTEGER NOT NULL,
    audit_id INTEGER NOT NULL REFERENCES audit_events(id)
);
CREATE TRIGGER crew_escape_tombstones_no_update BEFORE UPDATE ON crew_escape_tombstones
BEGIN SELECT RAISE(ABORT,'escape tombstones are permanent'); END;
CREATE TRIGGER crew_escape_tombstones_no_delete BEFORE DELETE ON crew_escape_tombstones
BEGIN SELECT RAISE(ABORT,'escape tombstones are permanent'); END;

-- A grant exists only inside the clearing transaction. It cannot authorize confirmed, check-bearing or cross-project history.
CREATE TABLE crew_escape_retention_grants (
    escape_id TEXT PRIMARY KEY,
    audit_id INTEGER NOT NULL REFERENCES audit_events(id)
);
CREATE TRIGGER crew_escape_retention_grant BEFORE INSERT ON crew_escape_retention_grants
BEGIN
    SELECT CASE WHEN orbyn_escape_writer_version()!=57 OR NOT EXISTS (
        SELECT 1 FROM crew_escapes e JOIN crew_escape_tombstones t ON t.merge_task_id=e.merge_task_id
        JOIN audit_events a ON a.id=NEW.audit_id
        WHERE e.id=NEW.escape_id AND e.state IN ('possible','dismissed')
          AND t.project_id=e.project_id AND t.state=e.state AND t.audit_id=NEW.audit_id
          AND a.action='crew.escape.retain' AND json_extract(a.detail,'$.escape')=e.id
          AND json_extract(a.detail,'$.project')=e.project_id AND json_extract(a.detail,'$.merge')=e.merge_task_id
          AND NOT EXISTS(SELECT 1 FROM crew_escape_checks c WHERE c.escape_id=e.id)
          AND NOT EXISTS(SELECT 1 FROM crew_escape_check_tasks c WHERE c.escape_id=e.id)
          AND NOT EXISTS(SELECT 1 FROM crew_escape_events v WHERE v.escape_id=e.id AND v.project_id!=e.project_id)
    ) THEN RAISE(ABORT,'escape retention requires an audited eligible row') END;
END;
CREATE TRIGGER crew_escape_retention_grants_no_update BEFORE UPDATE ON crew_escape_retention_grants
BEGIN SELECT RAISE(ABORT,'escape retention grants are immutable'); END;
DROP TRIGGER crew_escape_events_no_delete;
CREATE TRIGGER crew_escape_events_no_delete BEFORE DELETE ON crew_escape_events
WHEN orbyn_escape_writer_version()!=57 OR NOT EXISTS (
    SELECT 1 FROM crew_escape_retention_grants g JOIN crew_escapes e ON e.id=g.escape_id
    WHERE e.id=OLD.escape_id AND e.project_id=OLD.project_id AND e.state IN ('possible','dismissed')
      AND NOT EXISTS(SELECT 1 FROM crew_escape_checks c WHERE c.escape_id=e.id)
      AND NOT EXISTS(SELECT 1 FROM crew_escape_check_tasks c WHERE c.escape_id=e.id)
)
BEGIN SELECT RAISE(ABORT,'crew objects are immutable outside audited retention'); END;
CREATE TRIGGER crew_escapes_retention_delete BEFORE DELETE ON crew_escapes
WHEN orbyn_escape_writer_version()!=57 OR (EXISTS(SELECT 1 FROM crew_merges m WHERE m.task_id=OLD.merge_task_id) AND NOT EXISTS (
    SELECT 1 FROM crew_escape_retention_grants g WHERE g.escape_id=OLD.id
      AND OLD.state IN ('possible','dismissed')
      AND NOT EXISTS(SELECT 1 FROM crew_escape_checks c WHERE c.escape_id=OLD.id)
      AND NOT EXISTS(SELECT 1 FROM crew_escape_check_tasks c WHERE c.escape_id=OLD.id)
      AND NOT EXISTS(SELECT 1 FROM crew_escape_events v WHERE v.escape_id=OLD.id)
))
BEGIN SELECT RAISE(ABORT,'escape removal requires audited retention'); END;
CREATE TRIGGER crew_escape_check_tasks_no_update BEFORE UPDATE ON crew_escape_check_tasks
BEGIN SELECT RAISE(ABORT,'escape check task records are permanent'); END;
CREATE TRIGGER crew_escape_check_tasks_no_delete BEFORE DELETE ON crew_escape_check_tasks
BEGIN SELECT RAISE(ABORT,'escape check task records are permanent'); END;

CREATE TRIGGER crew_escapes_writer57_insert BEFORE INSERT ON crew_escapes
WHEN orbyn_escape_writer_version()!=57
BEGIN SELECT RAISE(ABORT,'escape writer version 57 is required'); END;

CREATE TRIGGER crew_escapes_writer57_update BEFORE UPDATE ON crew_escapes
WHEN orbyn_escape_writer_version()!=57
BEGIN SELECT RAISE(ABORT,'escape writer version 57 is required'); END;

CREATE TRIGGER crew_escape_events_writer57_insert BEFORE INSERT ON crew_escape_events
WHEN orbyn_escape_writer_version()!=57
BEGIN SELECT RAISE(ABORT,'escape writer version 57 is required'); END;

CREATE TRIGGER crew_escape_checks_writer57_insert BEFORE INSERT ON crew_escape_checks
WHEN orbyn_escape_writer_version()!=57
BEGIN SELECT RAISE(ABORT,'escape writer version 57 is required'); END;

CREATE TRIGGER crew_escape_checks_writer57_update BEFORE UPDATE ON crew_escape_checks
WHEN orbyn_escape_writer_version()!=57
BEGIN SELECT RAISE(ABORT,'escape writer version 57 is required'); END;

CREATE TRIGGER crew_escape_checks_writer57_delete BEFORE DELETE ON crew_escape_checks
WHEN orbyn_escape_writer_version()!=57
BEGIN SELECT RAISE(ABORT,'escape writer version 57 is required'); END;

CREATE TRIGGER crew_escape_check_tasks_writer57_insert BEFORE INSERT ON crew_escape_check_tasks
WHEN orbyn_escape_writer_version()!=57
BEGIN SELECT RAISE(ABORT,'escape writer version 57 is required'); END;

CREATE TRIGGER crew_escape_tombstones_writer57_insert BEFORE INSERT ON crew_escape_tombstones
WHEN orbyn_escape_writer_version()!=57
BEGIN SELECT RAISE(ABORT,'escape writer version 57 is required'); END;

CREATE TRIGGER crew_escapes_suppression BEFORE INSERT ON crew_escapes
WHEN EXISTS(SELECT 1 FROM crew_escape_tombstones t WHERE t.merge_task_id=NEW.merge_task_id
    AND (t.state='dismissed' OR NEW.source!='person_marked' OR NEW.state!='confirmed'))
BEGIN SELECT RAISE(IGNORE); END;

-- SQL-only reverts remain valid when logging is full; automatic detection never revives a tombstone.
DROP TRIGGER crew_merges_revert_escape;
CREATE TRIGGER crew_merges_revert_escape AFTER UPDATE OF review_state ON crew_merges
WHEN NEW.review_state = 'reverted'
BEGIN
    INSERT OR IGNORE INTO crew_escapes (id, project_id, merge_task_id, source, state, engine, areas, reason, found_unix)
    SELECT 'esc_' || lower(hex(randomblob(12))), t.project_id, NEW.task_id, 'revert', 'possible', NEW.engine, NEW.areas,
           'The merge was reverted.', COALESCE(NEW.reviewed_unix, NEW.merged_unix)
    FROM crew_runs r JOIN threads t ON t.id = r.lead_thread_id
    WHERE r.task_id = NEW.task_id
      AND NOT EXISTS(SELECT 1 FROM crew_escape_tombstones x WHERE x.merge_task_id=NEW.task_id)
      AND (SELECT COUNT(*) FROM crew_escapes e WHERE e.project_id=t.project_id)<5000
      AND (SELECT COUNT(*) FROM crew_escape_events v WHERE v.project_id=t.project_id)<20000;
    INSERT OR IGNORE INTO crew_escape_events (escape_id, project_id, kind, from_state, to_state, decided_by, cause, at)
    SELECT e.id, e.project_id, 'detected', NULL, 'possible', 'core', 'revert:' || substr(COALESCE(NEW.reverted_commit, 'unknown'), 1, 100), e.found_unix
    FROM crew_escapes e WHERE e.merge_task_id = NEW.task_id AND e.source = 'revert'
      AND NOT EXISTS(SELECT 1 FROM crew_escape_tombstones x WHERE x.merge_task_id=NEW.task_id)
      AND (SELECT COUNT(*) FROM crew_escape_events v WHERE v.project_id=e.project_id)<20000
      AND NOT EXISTS (SELECT 1 FROM crew_escape_events d WHERE d.escape_id = e.id AND d.kind = 'detected');
END;
