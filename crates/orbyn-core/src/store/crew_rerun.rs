//! A person's re-run of a task's checks, at rest. Two transactions bracket the run, so a crash between them leaves the intent row
//! and nothing else: `begin_crew_rerun` checks the task, takes the claim and writes the intent row (`crew.task.rerun_checks`)
//! together, before any command runs (architecture rule 6); `finish_crew_rerun` writes the outcome row and lets the claim go.
//!
//! The claim is `crew_runs.verifying_since`, the column a live verification uses, so a re-run and a verification can never run
//! together, and a second re-run finds the claim taken. A start of the service clears every claim (`clear_crew_claims`), so a
//! crash never leaves a task locked. The task's state is never changed here (SPEC.md H7).

use rusqlite::params;
use serde_json::{Value, json};

use super::crew::{CrewTask, find_task};
use super::crew_runs::{CrewRun, find_run};
use super::{AuditRecord, Store, insert_audit};
use crate::error::{CoreError, Result};
use crate::util::now;

/// The states whose checks may be run again. `failed` is never at rest (the guard moves a task through it in one transaction),
/// and is here only so the rule reads as the spec's.
const ENDED: [&str; 3] = ["passed", "stopped", "failed"];

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

fn busy(why: &str) -> CoreError {
    CoreError::CrewTaskBusy(why.to_owned())
}

impl Store {
    /// Checks the task, takes the claim and writes the intent row in one transaction. A task with no key or no checkout is
    /// `Unsupported`; one that is not ended, is cancelled, is being verified or is being re-run already is `CrewTaskBusy`.
    pub fn begin_crew_rerun(
        &self,
        task_id: &str,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<(CrewTask, CrewRun)> {
        self.commit(|tx, _| {
            let task = find_task(tx, task_id)?;
            let run = find_run(tx, task_id)
                .map_err(|_| CoreError::Unsupported("The task has no checkout.".into()))?;
            let keyed: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM crew_answer_keys WHERE task_id = ?1)",
                [task_id],
                |row| row.get(0),
            )?;
            if !keyed {
                return Err(CoreError::Unsupported("The task has no key.".into()));
            }
            if run.worktree.is_none() {
                return Err(CoreError::Unsupported("The task has no checkout.".into()));
            }
            match task.state.as_str() {
                "verifying" => return Err(busy("The task's checks are being run now.")),
                "cancelled" => {
                    return Err(busy(
                        "The task was cancelled; its checkout is not checked again.",
                    ));
                }
                "running" => return Err(busy("The task's worker is still running.")),
                state if !ENDED.contains(&state) => {
                    return Err(busy(&format!(
                        "The task is {state}. Its checks can be run again once it has ended."
                    )));
                }
                _ => {}
            }
            if run.verifying_since.is_some() {
                return Err(busy("The task's checks are being run now."));
            }
            audit(
                tx,
                actor,
                "crew.task.rerun_checks",
                task_id,
                json!({ "state": task.state, "reason": reason }),
            )?;
            tx.execute(
                "UPDATE crew_runs SET verifying_since = ?2
                 WHERE task_id = ?1 AND verifying_since IS NULL",
                params![task_id, now()],
            )?;
            Ok((task, run))
        })
    }

    /// Writes the outcome row (`action`, with counts only in `detail`) and lets the claim go, in one transaction.
    pub fn finish_crew_rerun(
        &self,
        task_id: &str,
        actor: &str,
        action: &str,
        detail: Value,
    ) -> Result<()> {
        self.commit(|tx, _| {
            audit(tx, actor, action, task_id, detail)?;
            tx.execute(
                "UPDATE crew_runs SET verifying_since = NULL
                 WHERE task_id = ?1
                   AND (SELECT state FROM crew_tasks WHERE id = ?1) <> 'verifying'",
                [task_id],
            )?;
            Ok(())
        })
    }
}
