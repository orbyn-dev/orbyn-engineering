//! The Crew "needs you" inbox at rest (migration 77): the person's dismissals, and the small reads the inbox joins.
//!
//! A dismissal is a device-local choice, never a change to what it hides: the row keeps only the kind, the id and the item's
//! `since`, so a later wait of the same kind and id is a new item and shows again. The table holds no reason, no task content
//! and no path. The audit row is written in the same transaction, before the dismissal.
//!
//! The reads here join a task to the project of its lead thread and collect the tasks a trip wire stopped, so the inbox can
//! answer across every crew without a new stored state.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{OptionalExtension, params};
use serde_json::json;

use super::{AuditRecord, CrewTask, Store, insert_audit};
use crate::error::Result;
use crate::util::{parse_time, unix_now};

/// One dismissal as stored: which item, and the `since` it hid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NeedsYouDismissal {
    pub kind: String,
    pub id: String,
    pub since: String,
}

/// One task and the project of its lead thread, when it still has one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NeedsYouTask {
    pub task: CrewTask,
    pub project_id: Option<String>,
}

/// Whether `one` is a later RFC 3339 time than `other`. A pair that does not parse falls back to the stored strings, which are
/// written in one format.
fn later(one: &str, other: &str) -> bool {
    parse_time(one)
        .zip(parse_time(other))
        .map_or_else(|| one > other, |(one, other)| one > other)
}

impl Store {
    /// Every task in one of `states`, across every crew, newest first, with the project of its lead thread.
    pub fn crew_tasks_in_states(&self, states: &[&str]) -> Result<Vec<NeedsYouTask>> {
        if states.is_empty() {
            return Ok(Vec::new());
        }
        let marks = vec!["?"; states.len()].join(",");
        let args: Vec<rusqlite::types::Value> = states
            .iter()
            .map(|state| rusqlite::types::Value::from((*state).to_owned()))
            .collect();
        let conn = self.reader();
        let mut statement = conn.prepare(&format!(
            "SELECT t.*, th.project_id AS lead_project
             FROM crew_tasks t
             LEFT JOIN crew_runs r ON r.task_id = t.id
             LEFT JOIN threads th ON th.id = r.lead_thread_id
             WHERE t.state IN ({marks})
             ORDER BY t.updated_at DESC, t.id"
        ))?;
        let rows = statement
            .query_map(rusqlite::params_from_iter(args.iter()), |row| {
                Ok(NeedsYouTask {
                    task: super::crew::task_row(row)?,
                    project_id: row.get("lead_project")?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// The project of each of these tasks, for the items whose id is a task id. A task whose lead thread is gone has none.
    pub fn crew_projects_of_tasks(&self, task_ids: &[String]) -> Result<BTreeMap<String, String>> {
        if task_ids.is_empty() {
            return Ok(BTreeMap::new());
        }
        let marks = vec!["?"; task_ids.len()].join(",");
        let args: Vec<rusqlite::types::Value> = task_ids
            .iter()
            .cloned()
            .map(rusqlite::types::Value::Text)
            .collect();
        let conn = self.reader();
        let mut statement = conn.prepare(&format!(
            "SELECT t.id, th.project_id
             FROM crew_tasks t
             JOIN crew_runs r ON r.task_id = t.id
             JOIN threads th ON th.id = r.lead_thread_id
             WHERE t.id IN ({marks})"
        ))?;
        let rows = statement
            .query_map(rusqlite::params_from_iter(args.iter()), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows.into_iter().collect())
    }

    /// The tasks that have a ledger row of this kind, in one query.
    pub fn crew_tasks_with_autopilot_event(&self, kind: &str) -> Result<BTreeSet<String>> {
        let conn = self.reader();
        let mut statement =
            conn.prepare("SELECT DISTINCT task_id FROM crew_autopilot_events WHERE kind = ?1")?;
        Ok(statement
            .query_map([kind], |row| row.get(0))?
            .collect::<rusqlite::Result<BTreeSet<String>>>()?)
    }

    /// Every dismissal on this device, by kind and id.
    pub fn crew_needs_you_dismissals(&self) -> Result<Vec<NeedsYouDismissal>> {
        let conn = self.reader();
        let mut statement =
            conn.prepare("SELECT kind, id, since FROM crew_needs_you_dismissed ORDER BY kind, id")?;
        Ok(statement
            .query_map([], |row| {
                Ok(NeedsYouDismissal {
                    kind: row.get(0)?,
                    id: row.get(1)?,
                    since: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// One dismissal, if this device hid this kind and id.
    pub fn crew_needs_you_dismissal(
        &self,
        kind: &str,
        id: &str,
    ) -> Result<Option<NeedsYouDismissal>> {
        Ok(self
            .reader()
            .query_row(
                "SELECT kind, id, since FROM crew_needs_you_dismissed WHERE kind = ?1 AND id = ?2",
                params![kind, id],
                |row| {
                    Ok(NeedsYouDismissal {
                        kind: row.get(0)?,
                        id: row.get(1)?,
                        since: row.get(2)?,
                    })
                },
            )
            .optional()?)
    }

    /// Hides one item on this device. The audit row comes first, then the dismissal, in one transaction; a repeat at the same or
    /// an older `since` writes nothing. Returns whether this call wrote. Nothing behind the item is touched.
    pub fn dismiss_crew_needs_you(
        &self,
        kind: &str,
        id: &str,
        since: &str,
        actor: &str,
    ) -> Result<bool> {
        self.commit(|tx, _| {
            let stored: Option<String> = tx
                .query_row(
                    "SELECT since FROM crew_needs_you_dismissed WHERE kind = ?1 AND id = ?2",
                    params![kind, id],
                    |row| row.get(0),
                )
                .optional()?;
            if stored
                .as_deref()
                .is_some_and(|stored| !later(since, stored))
            {
                return Ok(false);
            }
            insert_audit(
                tx,
                &AuditRecord {
                    actor: actor.to_owned(),
                    action: "crew.needs_you.dismiss".to_owned(),
                    target: id.to_owned(),
                    decision: None,
                    source: if actor == "user" { "user" } else { "local" }.to_owned(),
                    thread_id: None,
                    run_id: None,
                    detail: json!({ "kind": kind, "id": id, "since": since }),
                },
            )?;
            tx.execute(
                "INSERT INTO crew_needs_you_dismissed (kind, id, since, dismissed_unix)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(kind, id) DO UPDATE SET since = ?3, dismissed_unix = ?4",
                params![kind, id, since, unix_now()],
            )?;
            Ok(true)
        })
    }
}
