//! Autopilot at rest: the person's mode per scope and the approval ledger. A row of the ledger is
//! only ever added. The counters of the trip wires are read from it, so no counter can drift from
//! the rows it counts.

use std::collections::BTreeMap;

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
pub const EVENT_KINDS: [&str; 29] = [
    "approval_started",
    "approval_usage",
    "approval_ended",
    "second_engine_dismissed",
    "second_engine_resolved",
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
    "provenance_failed",
];

impl Store {
    /// Effective model of this exact active lead run, selecting only the scalar.
    /// Missing metadata never falls back to a different or older run.
    pub(crate) fn crew_lead_effective_model(
        &self,
        thread: &str,
        run: &str,
        project: &str,
        engine: &str,
    ) -> Result<Option<String>> {
        // Multiple different init models, or an invalid one, are ambiguous.
        Ok(self
            .reader()
            .query_row(
                "SELECT MIN(json_extract(e.event, '$.value.model')) FROM thread_events e
             JOIN runs r ON r.id=e.run_id AND r.thread_id=e.thread_id
             JOIN threads t ON t.id=r.thread_id
             WHERE e.thread_id=?1 AND e.run_id=?2 AND t.project_id=?3 AND r.engine=?4
               AND json_extract(e.event, '$.type')='CUSTOM'
               AND json_extract(e.event, '$.name')='engine.model'
               AND json_extract(e.event, '$.value.engine')='claude'
             HAVING COUNT(DISTINCT json_extract(e.event, '$.value.model'))=1
               AND COUNT(json_extract(e.event, '$.value.model'))=COUNT(*)",
                params![thread, run, project, engine],
                |row| row.get(0),
            )
            .optional()?
            .flatten())
    }

    /// Approval execution also observes ordinary run-stop intent, including a
    /// parent-family stop, even though its no-tool pump owns its own handle.
    pub(crate) fn crew_approval_live(&self, run: &str) -> Result<bool> {
        Ok(self.reader().query_row("SELECT EXISTS(SELECT 1 FROM runs WHERE id=?1 AND owner_epoch=?2 AND stop_requested=0 AND state IN ('admitted','starting','running'))", params![run,self.epoch()], |row| row.get(0))?)
    }

    /// Pending ordinary second-engine notices, by immutable ledger cursor.
    /// Resolution rows are excluded before the bound, so old pending items
    /// cannot be crowded out by recent denials or resolved notices.
    pub(crate) fn crew_second_notices(
        &self,
        before: Option<i64>,
        limit: u32,
    ) -> Result<Vec<AutopilotEvent>> {
        let conn = self.reader();
        let mut statement = conn.prepare("SELECT e.id,e.crew_id,e.task_id,e.kind,e.body,e.at FROM crew_autopilot_events e
            WHERE e.kind='denied' AND json_extract(e.body,'$.needs_second_engine')=1
            AND json_extract(e.body,'$.mode')='full' AND json_extract(e.body,'$.guard') IS NULL
            AND (?1 IS NULL OR e.id < ?1)
            AND NOT EXISTS(SELECT 1 FROM crew_autopilot_events r
                WHERE r.kind IN ('second_engine_dismissed','second_engine_resolved') AND json_extract(r.body,'$.notice')=e.id)
            ORDER BY e.id DESC LIMIT ?2")?;
        let rows = statement
            .query_map(params![before, limit], |row| {
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

    /// Idempotently ends one notice; dismissal and sign-in resolution compete
    /// in this transaction and neither grants or retries the denied action.
    pub(crate) fn settle_crew_second_notice(
        &self,
        id: i64,
        resolution: Option<(&str, &str)>,
    ) -> Result<()> {
        if id <= 0 {
            return Err(CoreError::Invalid("that notice id is not valid".into()));
        }
        self.commit(|tx, _| {
            let (crew, task, original): (String, String, String) = tx.query_row(
                "SELECT crew_id,task_id,body FROM crew_autopilot_events WHERE id=?1 AND kind='denied'
                 AND json_extract(body,'$.needs_second_engine')=1 AND json_extract(body,'$.mode')='full'
                 AND json_extract(body,'$.guard') IS NULL", [id],
                |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
            ).optional()?.ok_or_else(|| CoreError::NotFound("second-engine notice".into()))?;
            let done: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM crew_autopilot_events WHERE kind IN ('second_engine_dismissed','second_engine_resolved') AND json_extract(body,'$.notice')=?1)",
                [id], |row| row.get(0),
            )?;
            if done { return Ok(()); }
            let mut body = serde_json::json!({ "notice": id });
            if let Some((engine, family)) = resolution {
                let original: Value = serde_json::from_str(&original)?;
                let first = original.pointer("/action/lead_family").and_then(Value::as_str).ok_or(CoreError::RunClosed)?;
                if first == family || engine.is_empty() || family.is_empty() { return Err(CoreError::RunClosed); }
                body["engine"] = Value::String(engine.to_owned());
                body["family"] = Value::String(family.to_owned());
            }
            let signed_in = resolution.is_some();
            let kind = if signed_in { "second_engine_resolved" } else { "second_engine_dismissed" };
            super::insert_audit(tx, &super::AuditRecord {
                actor: if signed_in { "core" } else { "local_caller" }.into(),
                action: format!("crew.{kind}"), target: id.to_string(), decision: None,
                source: if signed_in { "core" } else { "local" }.into(),
                thread_id: None, run_id: None, detail: body.clone(),
            })?;
            tx.execute("INSERT INTO crew_autopilot_events(crew_id,task_id,kind,body,at) VALUES(?1,?2,?3,?4,?5)", params![crew,task,kind,body.to_string(),now()])?;
            Ok(())
        })
    }

    /// Reserves one of the brief's existing turns before an approval is launched.
    /// Counts actual worker admissions plus prior reservations, atomically with
    /// the new reservation. Missing budgets, liveness or usage never grant time.
    pub(crate) fn reserve_crew_approval(
        &self,
        task_id: &str,
        attempt: u32,
        approval: &str,
        engine: &str,
    ) -> Result<()> {
        self.commit(|tx, _| {
            let task = super::crew::find_task(tx, task_id)?;
            let run = super::crew_runs::find_run(tx, task_id)?;
            let body: String = tx.query_row("SELECT body FROM crew_briefs WHERE id = ?1", [task_id], |row| row.get(0))?;
            let orbyn_contracts::CrewObject::Brief(brief) = super::crew::read_object(&body)? else { return Err(CoreError::RunClosed); };
            let cancelling: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM audit_events WHERE action = 'crew.task.cancel' AND target = ?1)", [task_id], |row| row.get(0))?;
            let start = crate::util::parse_time(&task.created_at).ok_or(CoreError::RunClosed)?;
            if !matches!(task.state.as_str(), "running" | "changed_approach") || cancelling || run.attempts != attempt
                || crate::util::unix_now() >= start.saturating_add(i64::from(brief.budget.minutes) * 60) {
                return Err(CoreError::RunClosed);
            }
            let reserved: Option<String> = tx.query_row("SELECT body FROM crew_autopilot_events WHERE task_id=?1 AND kind='approval_started' AND json_extract(body,'$.approval')=?2", params![task_id, approval], |row| row.get(0)).optional()?;
            if let Some(body) = reserved {
                let body: Value = serde_json::from_str(&body)?;
                return if body["engine"] == engine && body["attempt"] == attempt { Ok(()) } else { Err(CoreError::RunClosed) };
            }
            if super::crew_runs::turns_spent(tx, &run)? >= u64::from(brief.budget.turns) {
                return Err(CoreError::EngineUnavailable("The task has no turn left for an approval.".into()));
            }
            let body = serde_json::json!({ "approval": approval, "engine": engine, "attempt": attempt, "turns": 1 });
            tx.execute("INSERT INTO crew_autopilot_events(crew_id,task_id,kind,body,at) VALUES(?1,?2,'approval_started',?3,?4)", params![task.crew_id, task_id, body.to_string(), now()])?;
            Ok(())
        })
    }

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

    /// The newest breaker failures after a revert that no later row marks resolved, at most `limit`. Resolved rows are filtered out
    /// in SQL, so they never crowd the unresolved ones out of the bound.
    pub fn crew_unresolved_breaker_failures(&self, limit: u32) -> Result<Vec<AutopilotEvent>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT id, crew_id, task_id, kind, body, at FROM crew_autopilot_events f
             WHERE kind = 'area_breaker_failed'
               AND json_extract(body, '$.resolved') = 0
               AND json_extract(body, '$.stage') = 'revert'
               AND NOT EXISTS (
                   SELECT 1 FROM crew_autopilot_events r
                   WHERE r.kind = 'area_breaker_failed' AND json_extract(r.body, '$.resolved') = 1
                     AND json_extract(r.body, '$.revert') = json_extract(f.body, '$.revert'))
             ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = statement
            .query_map([limit], |row| {
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

    /// The ledger rows of these kinds for every task in `task_ids`, oldest first within a task,
    /// read in one query. A dashboard over a window needs the same kinds for all its tasks.
    pub fn crew_autopilot_events_of_tasks(
        &self,
        task_ids: &[String],
        kinds: &[&str],
    ) -> Result<BTreeMap<String, Vec<AutopilotEvent>>> {
        if task_ids.is_empty() || kinds.is_empty() {
            return Ok(BTreeMap::new());
        }
        let task_marks = vec!["?"; task_ids.len()].join(",");
        let kind_marks = vec!["?"; kinds.len()].join(",");
        let mut args: Vec<rusqlite::types::Value> = task_ids
            .iter()
            .cloned()
            .map(rusqlite::types::Value::Text)
            .collect();
        args.extend(
            kinds
                .iter()
                .map(|kind| rusqlite::types::Value::Text((*kind).to_owned())),
        );
        let conn = self.reader();
        let mut statement = conn.prepare(&format!(
            "SELECT id, crew_id, task_id, kind, body, at FROM crew_autopilot_events
             WHERE task_id IN ({task_marks}) AND kind IN ({kind_marks}) ORDER BY id"
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
        let mut events: BTreeMap<String, Vec<AutopilotEvent>> = BTreeMap::new();
        for (id, crew_id, task_id, kind, body, at) in rows {
            let event = AutopilotEvent {
                id,
                crew_id,
                task_id: task_id.clone(),
                kind,
                body: serde_json::from_str(&body)?,
                at,
            };
            events.entry(task_id).or_default().push(event);
        }
        Ok(events)
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
