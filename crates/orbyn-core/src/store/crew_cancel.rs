//! A person's cancel of a Crew task, at rest. The audit row comes before every effect and the state moves only through the
//! lifecycle table (`crew_runs::step`), in the same transaction as the row that records it. Three steps, each its own transaction:
//! `begin_crew_cancel` (the intent row; nothing else changes), then the core stops the worker's engine, then `finish_crew_cancel`
//! (the state and its row) or `fail_crew_cancel` (a row that says the attempt failed, the state as it was).

use std::sync::Arc;

use serde_json::{Value, json};

use super::crew::{CrewTask, find_task};
use super::crew_runs::{CrewRun, find_run, step};
use super::{AuditRecord, Store, insert_audit};
use crate::error::{CoreError, Result};

/// What the start of a cancel found.
pub enum CancelStart {
    /// The task is already `cancelled`: no row, no effect.
    Already,
    /// The cancel is recorded and may go on, with the task's worker side if it has one.
    Begun(Option<Box<CrewRun>>),
}

fn audit(
    tx: &rusqlite::Transaction<'_>,
    actor: &str,
    action: &str,
    task: &str,
    detail: Value,
) -> Result<()> {
    insert_audit(
        tx,
        &AuditRecord {
            actor: actor.to_owned(),
            action: action.to_owned(),
            target: task.to_owned(),
            decision: None,
            source: if actor == "user" { "user" } else { "core" }.to_owned(),
            thread_id: None,
            run_id: None,
            detail,
        },
    )?;
    Ok(())
}

impl Store {
    /// The lock a cancel of `task_id` holds from its first check to its last row: a second cancel of the task waits for it, then
    /// finds the task cancelled (`Already`) or, if the first failed, tries for itself.
    pub fn crew_cancel_lock(&self, task_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.cancels.lock().unwrap_or_else(|e| e.into_inner());
        locks.entry(task_id.to_owned()).or_default().clone()
    }

    /// Drops the lock of a cancel that is over, when nobody else waits on it.
    pub fn crew_cancel_unlock(&self, task_id: &str) {
        let mut locks = self.cancels.lock().unwrap_or_else(|e| e.into_inner());
        if locks
            .get(task_id)
            .is_some_and(|l| Arc::strong_count(l) <= 1)
        {
            locks.remove(task_id);
        }
    }

    /// True when a person's cancel of the task was recorded (its intent row exists).
    pub fn crew_cancel_intended(&self, task_id: &str) -> Result<bool> {
        Ok(self.reader().query_row(
            "SELECT EXISTS (SELECT 1 FROM audit_events WHERE action = 'crew.task.cancel' AND target = ?1)",
            [task_id],
            |row| row.get(0),
        )?)
    }

    /// Checks the task and writes the intent row (`crew.task.cancel`) in one transaction. `cancelled` answers `Already` and writes
    /// nothing; `passed` and `stopped` are `TaskEnded`.
    pub fn begin_crew_cancel(
        &self,
        task_id: &str,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<CancelStart> {
        self.commit(|tx, _| {
            let task = find_task(tx, task_id)?;
            if task.state == "cancelled" {
                return Ok(CancelStart::Already);
            }
            if crate::crew::lifecycle::is_final(&task.state) {
                return Err(CoreError::TaskEnded);
            }
            let run = find_run(tx, task_id).ok().map(Box::new);
            audit(
                tx,
                actor,
                "crew.task.cancel",
                task_id,
                json!({
                    "previous_state": task.state,
                    "verification_claimed": run.as_ref().is_some_and(|r| r.verifying_since.is_some()),
                    "worker_thread": run.as_ref().and_then(|r| r.worker_thread_id.clone()),
                    "reason": reason,
                }),
            )?;
            Ok(CancelStart::Begun(run))
        })
    }

    /// The engine is confirmed stopped: the task moves to `cancelled` and its row is written together. A task a worker's own run end
    /// already cancelled (its run was the one stopped) gets the row and no second move. One that ended some other way meanwhile is
    /// `TaskEnded`, with a row saying so.
    pub fn finish_crew_cancel(&self, task_id: &str, actor: &str) -> Result<(CrewTask, bool)> {
        self.commit(|tx, _| {
            let task = find_task(tx, task_id)?;
            if task.state == "cancelled" {
                audit(
                    tx,
                    actor,
                    "crew.task.cancelled",
                    task_id,
                    json!({ "moved_by": "run_end" }),
                )?;
                return Ok((task, false));
            }
            if crate::crew::lifecycle::is_final(&task.state) {
                audit(
                    tx,
                    actor,
                    "crew.task.cancel_failed",
                    task_id,
                    json!({ "error": "the task ended first", "state": task.state }),
                )?;
                return Ok((task, false));
            }
            audit(
                tx,
                actor,
                "crew.task.cancelled",
                task_id,
                json!({ "previous_state": task.state }),
            )?;
            Ok((step(tx, &task, "cancelled", None, &[])?, true))
        })
    }

    /// Records that the attempt failed; the task keeps its state.
    pub fn fail_crew_cancel(&self, task_id: &str, actor: &str, error: &str) -> Result<()> {
        self.commit(|tx, _| {
            let task = find_task(tx, task_id)?;
            audit(
                tx,
                actor,
                "crew.task.cancel_failed",
                task_id,
                json!({ "error": error, "state": task.state }),
            )
        })
    }
}
