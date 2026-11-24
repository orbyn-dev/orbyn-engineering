//! A person's re-run of a task's checks, at rest. Two transactions bracket the run, so a crash between them leaves the intent row
//! and nothing else: `begin_crew_rerun` checks the task, takes the claim and writes the intent row (`crew.task.rerun_checks`)
//! together, before any command runs (architecture rule 6); `finish_crew_rerun` writes the outcome row and lets the claim go.
//!
//! The claim is `crew_runs.verifying_since`, the column a live verification uses, so a re-run and a verification can never run
//! together, and a second re-run finds the claim taken. A claim on a task that is `passed` or `stopped` can only be a re-run's, so
//! "one re-run at a time on this device" is one query over all claims. A start of the service clears every claim
//! (`clear_crew_claims`), so a crash never leaves a task locked. The task's state is never changed here (SPEC.md H7).
//!
//! Which tasks: `passed` ones, and `stopped` ones whose reason is on [`BENIGN_STOPS`] and whose checks ran live at least once. A task
//! Autopilot stopped for hostile behaviour (`injection_suspected`, `too_many_denials`) never had the key run in its checkout, and a
//! reason this code cannot name is read as hostile: the key's commands would run the worker's files, for the first time, on a person's
//! click.

use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

use super::crew::{CrewTask, find_task};
use super::crew_runs::{CrewRun, find_run};
use super::{AuditRecord, Store, insert_audit};
use crate::error::{CoreError, Result};
use crate::util::now;

/// The stop reasons that are not a safety stop. Everything else, `injection_suspected`, `too_many_denials`, `lead_stopped` (the
/// lead's reason is not known) and a reason this list does not name, is refused: allow-list, never block-list.
const BENIGN_STOPS: [&str; 3] = ["repeated_failure", "budget", "engine_error"];

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
        min_gap_secs: i64,
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
                "passed" => {}
                "stopped" => {
                    let benign = task
                        .stop_reason
                        .as_deref()
                        .is_some_and(|r| BENIGN_STOPS.contains(&r));
                    if !benign {
                        return Err(busy(
                            "The task was stopped for a safety reason. Its checks are not run again.",
                        ));
                    }
                    let verified: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM crew_evidence WHERE task_id = ?1)",
                        [task_id],
                        |row| row.get(0),
                    )?;
                    if !verified {
                        return Err(busy(
                            "The task was stopped before its checks ever ran. They are not run now.",
                        ));
                    }
                }
                state => {
                    return Err(busy(&format!(
                        "The task is {state}. Its checks can be run again once it has ended."
                    )));
                }
            }
            if run.verifying_since.is_some() {
                return Err(busy("The task's checks are being run now."));
            }
            // One re-run at a time on this device: a claim on a task that has ended is a re-run's.
            let running: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM crew_runs r JOIN crew_tasks t ON t.id = r.task_id
                 WHERE r.verifying_since IS NOT NULL AND t.state IN ('passed', 'stopped'))",
                [],
                |row| row.get(0),
            )?;
            if running {
                return Err(busy("Another task's checks are being run again. Wait for it."));
            }
            // A gap between two re-runs of one task, counted from the end of the last.
            let last: Option<String> = tx
                .query_row(
                    "SELECT MAX(ts) FROM audit_events
                     WHERE target = ?1 AND action LIKE 'crew.task.rerun_checks%'",
                    [task_id],
                    |row| row.get(0),
                )
                .optional()?
                .flatten();
            if let Some(last) = last.as_deref().and_then(crate::util::parse_time) {
                let wait = last + min_gap_secs - crate::util::unix_now();
                if wait > 0 {
                    return Err(busy(&format!(
                        "This task's checks were run again just now. Try again in {wait} seconds."
                    )));
                }
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
