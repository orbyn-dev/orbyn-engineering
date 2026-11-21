//! Capacity-pressure retention: plan first, audit, tombstone, then remove eligible history in one immediate transaction.
use super::crew_escape_person::EVENTS_PER_PROJECT_MAX;
use super::crew_escapes::ESCAPES_PER_PROJECT_MAX;
use super::{AuditRecord, Store, insert_audit};
use crate::error::{CoreError, Result};
use rusqlite::{Connection, Transaction, params};
use serde_json::json;

pub(super) const WRITER_VERSION: i64 = 57;

struct Eviction {
    id: String,
    merge: String,
    state: String,
    events: u32,
}
fn full() -> CoreError {
    CoreError::Invalid("This project's escape log is full of protected history. Confirmed escapes, check history and unclassified orphan events cannot be cleared automatically.".into())
}

/// A read-only plan; refusing a request cannot partially clear the log, including when the revert owner accepts that refusal.
fn plan(
    conn: &Connection,
    project: &str,
    escapes: u32,
    events: u32,
    exclude: Option<&str>,
) -> Result<Option<Vec<Eviction>>> {
    let mut held_escapes: u32 = conn.query_row(
        "SELECT COUNT(*) FROM crew_escapes WHERE project_id=?1",
        [project],
        |r| r.get(0),
    )?;
    let mut held_events: u32 = conn.query_row(
        "SELECT COUNT(*) FROM crew_escape_events WHERE project_id=?1",
        [project],
        |r| r.get(0),
    )?;
    let fits = |a: u32, b: u32| {
        a.saturating_add(escapes) <= ESCAPES_PER_PROJECT_MAX
            && b.saturating_add(events) <= EVENTS_PER_PROJECT_MAX
    };
    if fits(held_escapes, held_events) || (escapes == 0 && events == 0) {
        return Ok(Some(Vec::new()));
    }
    let mut statement=conn.prepare(
        "SELECT e.id,e.merge_task_id,e.state,COUNT(v.id) FROM crew_escapes e
         LEFT JOIN crew_escape_events v ON v.escape_id=e.id AND v.project_id=e.project_id
         WHERE e.project_id=?1 AND e.id IS NOT ?2 AND e.state IN ('dismissed','possible')
           AND NOT EXISTS(SELECT 1 FROM crew_escape_checks c WHERE c.escape_id=e.id)
           AND NOT EXISTS(SELECT 1 FROM crew_escape_check_tasks c WHERE c.escape_id=e.id)
           AND NOT EXISTS(SELECT 1 FROM crew_escape_events x WHERE x.escape_id=e.id AND x.project_id!=e.project_id)
         GROUP BY e.id ORDER BY CASE e.state WHEN 'dismissed' THEN 0 ELSE 1 END,e.found_unix,e.id")?;
    let rows = statement.query_map(params![project, exclude], |r| {
        Ok(Eviction {
            id: r.get(0)?,
            merge: r.get(1)?,
            state: r.get(2)?,
            events: r.get(3)?,
        })
    })?;
    let mut selected = Vec::new();
    for row in rows {
        let row = row?;
        // Removing a row with no events cannot relieve event-only pressure.
        if row.events == 0 && held_escapes.saturating_add(escapes) <= ESCAPES_PER_PROJECT_MAX {
            continue;
        }
        held_escapes = held_escapes.saturating_sub(1);
        held_events = held_events.saturating_sub(row.events);
        selected.push(row);
        if fits(held_escapes, held_events) {
            return Ok(Some(selected));
        }
    }
    Ok(None)
}

pub(super) fn room(
    tx: &Transaction<'_>,
    project: &str,
    escapes: u32,
    events: u32,
    exclude: Option<&str>,
    at: i64,
) -> Result<()> {
    let selected = plan(tx, project, escapes, events, exclude)?.ok_or_else(full)?;
    apply(tx, project, selected, at)
}

/// Reverts remain valid when history is unprunable; only that typed outcome may skip logging.
pub(super) fn room_for_revert(tx: &Transaction<'_>, project: &str, at: i64) -> Result<bool> {
    let Some(selected) = plan(tx, project, 1, 1, None)? else {
        return Ok(false);
    };
    apply(tx, project, selected, at)?;
    Ok(true)
}

fn apply(tx: &Transaction<'_>, project: &str, selected: Vec<Eviction>, at: i64) -> Result<()> {
    for row in selected {
        let audit = insert_audit(
            tx,
            &AuditRecord {
                actor: "core".into(),
                action: "crew.escape.retain".into(),
                target: format!("{project}:{}", row.id),
                decision: None,
                source: "core".into(),
                thread_id: None,
                run_id: None,
                detail: json!({"project":project,"escape":row.id,"merge":row.merge,"state":row.state,"events":row.events,"at":at}),
            },
        )?;
        tx.execute("INSERT INTO crew_escape_tombstones(merge_task_id,project_id,state,evicted_unix,audit_id) VALUES(?1,?2,?3,?4,?5)",params![row.merge,project,row.state,at,audit])?;
        tx.execute(
            "INSERT INTO crew_escape_retention_grants(escape_id,audit_id) VALUES(?1,?2)",
            params![row.id, audit],
        )?;
        tx.execute(
            "DELETE FROM crew_escape_events WHERE escape_id=?1 AND project_id=?2",
            params![row.id, project],
        )?;
        tx.execute(
            "DELETE FROM crew_escapes WHERE id=?1 AND project_id=?2",
            params![row.id, project],
        )?;
        tx.execute(
            "DELETE FROM crew_escape_retention_grants WHERE escape_id=?1",
            [row.id],
        )?;
    }
    Ok(())
}

impl Store {
    /// Scanner preflight: full logs with enough eligible history may proceed; this read never performs retention.
    pub fn crew_escape_can_record(&self, project: &str) -> Result<bool> {
        Ok(plan(&self.reader(), project, 1, 1, None)?.is_some())
    }
}
