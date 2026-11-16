//! A person's re-run of a task's checks: the key's checks only, again, in the task's own checkout, through the verifier
//! (`Core::judge`). It changes no state (SPEC.md H7) and **stores no Evidence or Gate**: Review and the merge proof count every
//! Evidence of a task as its own verification and the protocol's Evidence has no provenance field, so a re-run's record is its two
//! audit rows (counts only). Person only: the route is local only and listed in
//! `changes::STATE_CHANGING` (so Autopilot's N1 refuses an agent's request to it), and no Crew tool calls this.
//!
//! Order, each step failing closed:
//! 1. the task must exist, have a key and a checkout, and be `passed`, or `stopped` for a benign reason after its checks ran live at
//!    least once (a safety stop, such as an injection or too many denials, never had the key run in its checkout, and is refused). A
//!    task still being worked on, being verified, cancelled, or with a worker whose run is still open is a conflict: the key's
//!    commands never run in a checkout a worker is writing, or beside the live verification. One re-run at a time on the device, and
//!    a gap of [`MIN_GAP_SECS`] between two re-runs of one task.
//! 2. one transaction takes the claim and writes the intent row `crew.task.rerun_checks`, before any command runs.
//! 3. every command of the key is screened with the always-denied list (N1 to N10) and the blast-radius guard, the same reading a
//!    worker's command gets. A command that fails the screen, or cannot be read, is not run; its check is recorded as an error, so
//!    the Gate cannot pass on the others.
//! 4. the verifier runs the rest inside the shorter of the task's time budget and [`RERUN_MAX`].
//! 5. one transaction writes the outcome row (counts only) and lets the claim go.
//!
//! The work runs on its own task, so a caller that goes away mid-run does not leave the claim set or the outcome row unwritten.
//! The answer has counts and outcomes. It has no command, output or claim of a check (SPEC.md H1).

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use orbyn_contracts::{CheckOutcome, CrewRerunGate as Gate, CrewTaskRerunDone};
use serde_json::json;

use super::autopilot::Screened;
use super::lifecycle::is_final;
use super::valid_id;
use super::verify::Rerun;
use crate::app::Core;
use crate::error::{CoreError, Result};

#[cfg(any(test, feature = "test-support"))]
pub mod fixtures;

/// The longest a re-run may take in all, whatever the task's budget was.
pub const RERUN_MAX: Duration = Duration::from_secs(30 * 60);
/// The longest reason, in characters.
pub const REASON_MAX: usize = 200;
/// The least time between the end of one re-run of a task and the next.
pub const MIN_GAP_SECS: i64 = 10;

impl Core {
    /// Runs the key's checks again for the person. See the module for the order and what each failure leaves behind.
    pub async fn crew_rerun_checks(
        self: &Arc<Self>,
        task_id: &str,
        reason: Option<&str>,
        actor: &str,
    ) -> Result<CrewTaskRerunDone> {
        self.rerun_within(task_id, reason, actor, RERUN_MAX, MIN_GAP_SECS)
            .await
    }

    /// [`Self::crew_rerun_checks`] with its own upper bound on the whole run and gap, so a test can use short ones.
    pub(super) async fn rerun_within(
        self: &Arc<Self>,
        task_id: &str,
        reason: Option<&str>,
        actor: &str,
        cap: Duration,
        gap: i64,
    ) -> Result<CrewTaskRerunDone> {
        self.crew_preview_only()?;
        valid_id(task_id)?;
        let reason = match reason.map(str::trim).filter(|r| !r.is_empty()) {
            Some(text) => Some(super::background::text("reason", text, REASON_MAX)?),
            None => None,
        };
        let task = self
            .store
            .crew_task(task_id)?
            .ok_or_else(|| CoreError::NotFound(format!("crew task {task_id}")))?;
        self.refuse_open_worker(&task.id)?;
        let (task, run) = self
            .store
            .begin_crew_rerun(task_id, actor, reason.as_deref(), gap)?;
        let core = self.clone();
        let (id, who) = (task.id.clone(), actor.to_owned());
        let work = tokio::spawn(async move {
            // The run is a task of its own, so a panic in it is a `JoinError` here and the claim is still let go.
            let inner = core.clone();
            let ran = tokio::spawn(async move { inner.rerun_claimed(&task, &run, cap).await });
            let done = match ran.await {
                Ok(done) => done,
                Err(_) => Err(CoreError::Internal(
                    "The re-run stopped unexpectedly.".into(),
                )),
            };
            let (action, detail) = match &done {
                Ok(done) => (
                    "crew.task.rerun_checks_done",
                    json!({ "total": done.total, "ran": done.ran, "passed": done.passed,
                        "refused": done.refused, "skipped": done.skipped, "gate": done.gate }),
                ),
                Err(error) => (
                    "crew.task.rerun_checks_failed",
                    json!({ "error": error.code() }),
                ),
            };
            if let Err(write) = core.store.finish_crew_rerun(&id, &who, action, detail) {
                tracing::warn!(%write, "could not record the end of a crew re-run");
            }
            done
        });
        work.await
            .map_err(|_| CoreError::Internal("The re-run did not finish.".into()))?
    }

    /// A worker whose engine run is still open is writing the checkout.
    fn refuse_open_worker(&self, task_id: &str) -> Result<()> {
        let Some(thread) = self
            .store
            .crew_run(task_id)?
            .and_then(|run| run.worker_thread_id)
        else {
            return Ok(());
        };
        match self.store.thread(&thread) {
            Ok(thread) if thread.active_run.is_some() => Err(CoreError::CrewTaskBusy(
                "The task's worker is still running.".into(),
            )),
            Ok(_) | Err(CoreError::NotFound(_)) => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// The part that runs commands, with the claim held and the intent row written.
    async fn rerun_claimed(
        &self,
        task: &crate::store::CrewTask,
        run: &crate::store::CrewRun,
        cap: Duration,
    ) -> Result<CrewTaskRerunDone> {
        self.refuse_open_worker(&task.id)?;
        #[cfg(test)]
        assert!(
            !tests_panic::take(&task.id),
            "a re-run that panics, for the test of it"
        );
        let key = self
            .store
            .crew_answer_key(&task.id)?
            .ok_or_else(|| CoreError::Unsupported("The task has no key.".into()))?;
        let worktree = Path::new(
            run.worktree
                .as_deref()
                .ok_or_else(|| CoreError::Unsupported("The task has no checkout.".into()))?,
        );
        let mut refused: Vec<(String, &'static str)> = Vec::new();
        for check in &key.checks {
            let Some(command) = check.command() else {
                continue;
            };
            let screened = self
                .crew_screen_command(
                    worktree,
                    run.base_commit.as_deref(),
                    &key.protected_paths,
                    &command,
                )
                .await;
            if let Screened::Refused(rule) = screened {
                refused.push((check.id.as_str().to_owned(), rule));
            }
        }
        let rerun = Rerun {
            refused: &refused,
            cap,
        };
        let verdict = self
            .judge(task, run, &key, worktree, false, Some(&rerun))
            .await
            .map_err(|error| {
                let why: String = super::redact(&error.message).chars().take(200).collect();
                CoreError::Unsupported(format!("The checks could not be run: {why}"))
            })?;
        let gate = if verdict.evidence.is_empty() || verdict.undecided() {
            Gate::None
        } else if verdict.passed() {
            Gate::Pass
        } else {
            Gate::Fail
        };
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        let ran = verdict
            .checks
            .iter()
            .filter(|c| {
                c.outcome != CheckOutcome::Skipped
                    && !refused.iter().any(|(id, _)| id == c.check.as_str())
            })
            .count();
        Ok(CrewTaskRerunDone {
            task: task.id.clone(),
            state: task.state.clone(),
            total: count(key.checks.len()),
            ran: count(ran),
            passed: count(verdict.passes),
            refused: count(refused.len()),
            skipped: count(verdict.not_verified.len()),
            gate,
            refused_checks: is_final(&task.state)
                .then(|| refused.iter().map(|(check, _)| check.clone()).collect()),
        })
    }
}

/// Makes the re-run of one task panic, so a test can see that the claim is let go.
#[cfg(test)]
pub(super) mod tests_panic {
    use std::sync::Mutex;

    static TASKS: Mutex<Vec<String>> = Mutex::new(Vec::new());

    #[cfg_attr(not(unix), allow(dead_code))]
    pub fn arm(task: &str) {
        TASKS.lock().unwrap().push(task.to_owned());
    }

    /// True once for a task that was armed.
    pub fn take(task: &str) -> bool {
        let mut tasks = TASKS.lock().unwrap();
        let before = tasks.len();
        tasks.retain(|t| t != task);
        tasks.len() < before
    }
}
