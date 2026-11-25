//! The Crew "needs you" inbox at rest (migration 78): the person's dismissals, and the small reads the inbox joins.
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

/// One task whose last cancel attempt failed, with the project of its lead thread and when that attempt was recorded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CouldNotStopTask {
    pub task: CrewTask,
    pub project_id: Option<String>,
    /// When the failed attempt was recorded, as RFC 3339.
    pub since: String,
}

/// Whether `one` is a later RFC 3339 time than `other`. A pair that does not parse falls back to the stored strings, which are
/// written in one format.
fn later(one: &str, other: &str) -> bool {
    parse_time(one)
        .zip(parse_time(other))
        .map_or_else(|| one > other, |(one, other)| one > other)
}

impl Store {
    /// Every task in one of `states`, newest first, with the project of its lead thread. `crew` and `project`
    /// narrow the query itself, so a read scoped to one crew or one repository never loads the other records.
    pub fn crew_tasks_in_states(
        &self,
        states: &[&str],
        crew: Option<&str>,
        project: Option<&str>,
    ) -> Result<Vec<NeedsYouTask>> {
        if states.is_empty() {
            return Ok(Vec::new());
        }
        let marks = vec!["?"; states.len()].join(",");
        let mut args: Vec<rusqlite::types::Value> = states
            .iter()
            .map(|state| rusqlite::types::Value::from((*state).to_owned()))
            .collect();
        let mut sql = format!(
            "SELECT t.*, th.project_id AS lead_project
             FROM crew_tasks t
             LEFT JOIN crew_runs r ON r.task_id = t.id
             LEFT JOIN threads th ON th.id = r.lead_thread_id
             WHERE t.state IN ({marks})"
        );
        if let Some(crew) = crew {
            sql.push_str(" AND t.crew_id = ?");
            args.push(rusqlite::types::Value::Text(crew.to_owned()));
        }
        if let Some(project) = project {
            sql.push_str(" AND th.project_id = ?");
            args.push(rusqlite::types::Value::Text(project.to_owned()));
        }
        sql.push_str(" ORDER BY t.updated_at DESC, t.id");
        let conn = self.reader();
        let mut statement = conn.prepare(&sql)?;
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

    /// The tasks whose last cancel attempt could not stop the worker, newest first, with the project of their lead thread and
    /// when that attempt failed. The try is the last `crew.task.cancel_failed` row of the task, with no `crew.task.cancelled` or
    /// `crew.task.stop_retried` row after it. A task that passed or stopped is not one. A `cancelled` task is: its second stop
    /// failed after the state moved, and a cancel of it tries the stop again (#1400). Reads the audit rows the cancel already
    /// writes and adds no storage; `crew` and `project` narrow the query itself.
    pub fn crew_could_not_stop(
        &self,
        crew: Option<&str>,
        project: Option<&str>,
    ) -> Result<Vec<CouldNotStopTask>> {
        let mut sql = String::from(
            "SELECT t.*, th.project_id AS lead_project, a.ts AS failed_at
             FROM crew_tasks t
             JOIN audit_events a ON a.id = (
                 SELECT MAX(f.id) FROM audit_events f
                 WHERE f.target = t.id AND f.action = 'crew.task.cancel_failed'
             )
             LEFT JOIN crew_runs r ON r.task_id = t.id
             LEFT JOIN threads th ON th.id = r.lead_thread_id
             WHERE t.state NOT IN ('passed', 'stopped')
               AND NOT EXISTS (
                   SELECT 1 FROM audit_events c
                   WHERE c.target = t.id AND c.id > a.id
                     AND c.action IN ('crew.task.cancelled', 'crew.task.stop_retried')
               )",
        );
        let mut args: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(crew) = crew {
            sql.push_str(" AND t.crew_id = ?");
            args.push(rusqlite::types::Value::Text(crew.to_owned()));
        }
        if let Some(project) = project {
            sql.push_str(" AND th.project_id = ?");
            args.push(rusqlite::types::Value::Text(project.to_owned()));
        }
        sql.push_str(" ORDER BY t.updated_at DESC, t.id");
        let conn = self.reader();
        let mut statement = conn.prepare(&sql)?;
        let rows = statement
            .query_map(rusqlite::params_from_iter(args.iter()), |row| {
                Ok(CouldNotStopTask {
                    task: super::crew::task_row(row)?,
                    project_id: row.get("lead_project")?,
                    since: row.get("failed_at")?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// The tasks that have a ledger row of this kind, in one query. `crew` and `project` narrow the query itself, so a
    /// read scoped to one crew or one repository never reads the whole ledger.
    pub fn crew_tasks_with_autopilot_event(
        &self,
        kind: &str,
        crew: Option<&str>,
        project: Option<&str>,
    ) -> Result<BTreeSet<String>> {
        let mut sql = String::from("SELECT DISTINCT e.task_id FROM crew_autopilot_events e");
        let mut args = vec![rusqlite::types::Value::Text(kind.to_owned())];
        if crew.is_some() || project.is_some() {
            sql.push_str(
                " JOIN crew_tasks t ON t.id = e.task_id
                  LEFT JOIN crew_runs r ON r.task_id = t.id
                  LEFT JOIN threads th ON th.id = r.lead_thread_id",
            );
        }
        sql.push_str(" WHERE e.kind = ?1");
        if let Some(crew) = crew {
            sql.push_str(" AND t.crew_id = ?");
            args.push(rusqlite::types::Value::Text(crew.to_owned()));
        }
        if let Some(project) = project {
            sql.push_str(" AND th.project_id = ?");
            args.push(rusqlite::types::Value::Text(project.to_owned()));
        }
        let conn = self.reader();
        let mut statement = conn.prepare(&sql)?;
        Ok(statement
            .query_map(rusqlite::params_from_iter(args.iter()), |row| row.get(0))?
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

    /// Claims one item for a desktop notification: true for the first caller at this `since`, false for every later one. A later
    /// `since` for the same kind and id is a new wait and is claimed again. Written in one statement, so two windows asking at once
    /// are told apart by the database. Nothing behind the item is touched.
    pub fn claim_crew_needs_you_notice(&self, kind: &str, id: &str, since: &str) -> Result<bool> {
        self.commit(|tx, _| {
            let stored: Option<String> = tx
                .query_row(
                    "SELECT since FROM crew_needs_you_notified WHERE kind = ?1 AND id = ?2",
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
            tx.execute(
                "INSERT INTO crew_needs_you_notified (kind, id, since, notified_unix)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(kind, id) DO UPDATE SET since = ?3, notified_unix = ?4",
                params![kind, id, since, unix_now()],
            )?;
            Ok(true)
        })
    }
}
