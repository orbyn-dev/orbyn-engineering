//! Autopilot at rest: the person's mode per scope and the approval ledger. A row of the ledger is
//! only ever added. The counters of the trip wires are read from it, so no counter can drift from
//! the rows it counts.

use rusqlite::{OptionalExtension, params};
use serde_json::Value;

use super::Store;
use crate::error::{CoreError, Result};
use crate::util::now;

/// One row of the approval ledger.
#[derive(Clone, Debug, PartialEq)]
pub struct AutopilotEvent {
    pub id: i64,
    pub crew_id: String,
    pub task_id: String,
    pub kind: String,
    pub body: Value,
    pub at: String,
}

/// The kinds a row may have.
pub const EVENT_KINDS: [&str; 23] = [
    "allowed",
    "denied",
    "person",
    "disagreement",
    "tripwire",
    "merge_queued",
    "merge_waiting",
    "merged",
    "merge_rejected",
    "main_broken",
    "reverted",
    "person_told",
    "held",
    "approved_once",
    "denied_by_person",
    "dismissed_by_person",
    "hold_used",
    "hold_ended",
    "command_exit",
    "route_class",
    "area_demoted",
    "area_read_failed",
    "area_breaker_failed",
];

impl Store {
    /// Sets the person's mode for a scope (`default`, `repo:<id>` or `crew:<id>`).
    pub fn set_crew_autopilot_mode(&self, scope: &str, mode: &str) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "INSERT INTO crew_autopilot_modes (scope, mode, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(scope) DO UPDATE SET mode = ?2, updated_at = ?3",
                params![scope, mode, now()],
            )?;
            Ok(())
        })
    }

    /// The mode stored for exactly this scope.
    pub fn crew_autopilot_mode_of(&self, scope: &str) -> Result<Option<String>> {
        Ok(self
            .reader()
            .query_row(
                "SELECT mode FROM crew_autopilot_modes WHERE scope = ?1",
                [scope],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// Adds one row to the ledger. The caller has already redacted and scanned the body.
    pub fn add_crew_autopilot_event(
        &self,
        task_id: &str,
        crew_id: &str,
        kind: &str,
        body: &Value,
    ) -> Result<i64> {
        if !EVENT_KINDS.contains(&kind) {
            return Err(CoreError::Invalid(format!(
                "no autopilot event kind {kind}"
            )));
        }
        let text = serde_json::to_string(body)?;
        self.commit(|tx, _| {
            tx.execute(
                "INSERT INTO crew_autopilot_events (crew_id, task_id, kind, body, at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![crew_id, task_id, kind, text, now()],
            )?;
            Ok(tx.last_insert_rowid())
        })
    }

    /// Every saved mode, by scope, at most `limit`.
    pub fn crew_autopilot_modes_all(&self, limit: u32) -> Result<Vec<(String, String, String)>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT scope, mode, updated_at FROM crew_autopilot_modes ORDER BY scope LIMIT ?1",
        )?;
        Ok(statement
            .query_map([limit], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// One page of the ledger across tasks, newest first, and how many rows match. `kinds` empty means every kind.
    pub fn crew_autopilot_events_page(
        &self,
        task_id: Option<&str>,
        kinds: &[&str],
        limit: u32,
        offset: u32,
    ) -> Result<(Vec<AutopilotEvent>, u32)> {
        let marks = vec!["?"; kinds.len()].join(",");
        let mut filter = String::from("WHERE 1 = 1");
        let mut args: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(task) = task_id {
            filter.push_str(" AND task_id = ?");
            args.push(task.to_owned().into());
        }
        if !kinds.is_empty() {
            filter.push_str(&format!(" AND kind IN ({marks})"));
            args.extend(
                kinds
                    .iter()
                    .map(|k| rusqlite::types::Value::from((*k).to_owned())),
            );
        }
        let conn = self.reader();
        let total: u32 = conn.query_row(
            &format!("SELECT COUNT(*) FROM crew_autopilot_events {filter}"),
            rusqlite::params_from_iter(args.iter()),
            |row| row.get(0),
        )?;
        args.push(i64::from(limit).into());
        args.push(i64::from(offset).into());
        let mut statement = conn.prepare(&format!(
            "SELECT id, crew_id, task_id, kind, body, at FROM crew_autopilot_events {filter}
             ORDER BY id DESC LIMIT ? OFFSET ?"
        ))?;
        let rows = statement
            .query_map(rusqlite::params_from_iter(args.iter()), |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let events = rows
            .into_iter()
            .map(|(id, crew_id, task_id, kind, body, at)| {
                Ok(AutopilotEvent {
                    id,
                    crew_id,
                    task_id,
                    kind,
                    body: serde_json::from_str(&body)?,
                    at,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok((events, total))
    }

    /// How many rows of each kind were written at or after `since` (RFC 3339, UTC).
    pub fn crew_autopilot_counts_since(&self, since: &str) -> Result<Vec<(String, u32)>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT kind, COUNT(*) FROM crew_autopilot_events WHERE at >= ?1 GROUP BY kind",
        )?;
        Ok(statement
            .query_map([since], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// A task's ledger, oldest first.
    pub fn crew_autopilot_events(&self, task_id: &str) -> Result<Vec<AutopilotEvent>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT id, crew_id, task_id, kind, body, at FROM crew_autopilot_events
             WHERE task_id = ?1 ORDER BY id",
        )?;
        let rows = statement
            .query_map([task_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(id, crew_id, task_id, kind, body, at)| {
                Ok(AutopilotEvent {
                    id,
                    crew_id,
                    task_id,
                    kind,
                    body: serde_json::from_str(&body)?,
                    at,
                })
            })
            .collect()
    }

    /// How a task's approvals stand: the denials since the last allow, and the denials in all.
    pub fn crew_autopilot_denials(&self, task_id: &str) -> Result<(u32, u32)> {
        let (mut streak, mut total) = (0u32, 0u32);
        for event in self.crew_autopilot_events(task_id)? {
            match event.kind.as_str() {
                "denied" => {
                    streak += 1;
                    total += 1;
                }
                // Only an approval by the lead or by two keys breaks a run of denials: a read that needs nobody does not, or a
                // worker could put one between its denials.
                "allowed"
                    if matches!(event.body["decided_by"].as_str(), Some("lead" | "two_keys")) =>
                {
                    streak = 0;
                }
                _ => {}
            }
        }
        Ok((streak, total))
    }
}
