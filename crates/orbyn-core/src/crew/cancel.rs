//! A person's cancel of a Crew task: the stop button (docs/crew.md, "As built (cancel)"). Person only: the route is local only and
//! listed in `changes::STATE_CHANGING` (so Autopilot's N1 refuses an agent's request to it), and no MCP tool calls this.
//!
//! Order, with every step failing closed:
//! 1. one transaction checks the task and writes the intent row `crew.task.cancel` (architecture rule 6: the row comes before any
//!    effect). Cancelled already: nothing is written or done. `passed` and `stopped` are over: `TaskEnded`.
//! 2. the worker's engine is stopped (`RunManager::force_stop`, which kills the engine's process tree and closes a run that will not
//!    stop) with its queue emptied so no new turn starts, and the core checks the worker has no run left. If it cannot confirm that,
//!    the task keeps its state, a `crew.task.cancel_failed` row records the attempt and the caller gets an error.
//! 3. one transaction moves the task to `cancelled` (the lifecycle table, which clears a verification claim) with the row
//!    `crew.task.cancelled`; then the existing end of a task runs: its leases are released, a final `cancelled` result is written and
//!    the lead is told.
//! 4. the worker is read again and stopped once more. A start or a retry does not take the cancel's lock, so one may have landed
//!    between steps 1 and 3; the state is final now, so a later one meets its own guard and an earlier one is seen here. If this
//!    pass fails the task stays `cancelled` (the state change stands), a `crew.task.cancel_failed` row flags the live worker and the
//!    caller gets the 502.

use std::future::Future;
use std::sync::Arc;

use orbyn_contracts as wire;
use orbyn_contracts::ResultState;

use super::valid_id;
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::store::{CancelStart, CrewRun};

/// The longest reason, in characters.
pub const REASON_MAX: usize = 200;
/// How many times the stop is checked again when a message slipped into the worker's queue meanwhile.
const STOP_ROUNDS: usize = 3;

impl Core {
    /// Cancels a task. See the module for the order and what each failure leaves behind.
    pub async fn crew_cancel_task(
        self: &Arc<Self>,
        task_id: &str,
        reason: Option<&str>,
        actor: &str,
    ) -> Result<wire::CrewTaskCancelled> {
        self.crew_cancel_with(task_id, reason, actor, self.forcing(actor), || {})
            .await
    }

    /// `crew_cancel_task`, with how a worker's run is forced to stop given, so a test can make the stop not take.
    pub(super) async fn crew_cancel_with<F: Future<Output = Result<()>> + Send>(
        self: &Arc<Self>,
        task_id: &str,
        reason: Option<&str>,
        actor: &str,
        force: impl Fn(String) -> F,
        between: impl FnOnce(),
    ) -> Result<wire::CrewTaskCancelled> {
        self.crew_preview_only()?;
        valid_id(task_id)?;
        let reason = match reason.map(str::trim).filter(|r| !r.is_empty()) {
            Some(text) => Some(super::background::text("reason", text, REASON_MAX)?),
            None => None,
        };
        let lock = self.store.crew_cancel_lock(task_id);
        let held = lock.lock().await;
        let answer = self
            .cancel_locked(task_id, reason, actor, &force, between)
            .await;
        drop(held);
        drop(lock);
        self.store.crew_cancel_unlock(task_id);
        answer
    }

    async fn cancel_locked<F: Future<Output = Result<()>> + Send>(
        self: &Arc<Self>,
        task_id: &str,
        reason: Option<String>,
        actor: &str,
        force: &impl Fn(String) -> F,
        between: impl FnOnce(),
    ) -> Result<wire::CrewTaskCancelled> {
        let done = |already| wire::CrewTaskCancelled {
            task: task_id.to_owned(),
            state: wire::CrewCancelState::Cancelled,
            already,
        };
        let run = match self
            .store
            .begin_crew_cancel(task_id, actor, reason.as_deref())?
        {
            CancelStart::Already => return Ok(done(true)),
            CancelStart::Begun(run) => run,
        };
        if let Err(error) = self.stop_crew_worker_by(run.as_deref(), force).await {
            return Err(self.cancel_failed(task_id, actor, &error, "The task was not cancelled"));
        }
        // Start and retry do not take this lock, so a worker may have been recorded or a message queued since the run was read.
        // `between` is where a test makes that happen.
        between();
        let (task, moved) = self.store.finish_crew_cancel(task_id, actor)?;
        if task.state != "cancelled" {
            return Err(CoreError::TaskEnded);
        }
        // The state is final now, so any later start or retry meets its own guard, and any earlier one is visible to this second
        // pass: it reads the worker again and stops what the first pass could not have seen.
        let again = self.store.crew_run(task_id).ok().flatten();
        let stopped = self.stop_crew_worker_by(again.as_ref(), force).await;
        // Exactly one of this call and the worker's run end (which the stop above triggers) moves the task, and that one ends it:
        // leases, the final result, the lead.
        if moved {
            self.end_crew_task(
                &task,
                ResultState::Cancelled,
                None,
                "The person cancelled the task.",
                None,
            );
        }
        if let Err(error) = stopped {
            // The task stays cancelled: the failed row flags a cancelled task whose worker may still run.
            return Err(self.cancel_failed(
                task_id,
                actor,
                &error,
                "The task is cancelled, but its worker could not be stopped",
            ));
        }
        Ok(done(false))
    }

    fn cancel_failed(
        &self,
        task_id: &str,
        actor: &str,
        error: &CoreError,
        lead: &str,
    ) -> CoreError {
        let why: String = super::redact(&error.to_string())
            .chars()
            .take(300)
            .collect();
        if let Err(write) = self.store.fail_crew_cancel(task_id, actor, &why) {
            tracing::warn!(%write, "could not record a failed crew cancel");
        }
        CoreError::EngineUnavailable(format!("{lead}. Try again."))
    }

    /// Stops the worker's engine run and confirms it, with the worker's queue emptied so no new turn starts. A task with no worker,
    /// a worker thread that was removed and one with no run are all stopped already.
    pub(super) async fn stop_crew_worker(&self, run: Option<&CrewRun>, actor: &str) -> Result<()> {
        self.stop_crew_worker_by(run, &self.forcing(actor)).await
    }

    /// The real stop: the run manager's forced stop of a thread's run.
    pub(super) fn forcing(
        &self,
        actor: &str,
    ) -> impl Fn(String) -> std::pin::Pin<Box<dyn Future<Output = Result<()>> + Send>> + use<> {
        let (runs, actor) = (self.runs.clone(), actor.to_owned());
        move |thread| {
            let (runs, actor) = (runs.clone(), actor.clone());
            Box::pin(async move { runs.force_stop(&thread, &actor).await.map(|_| ()) })
        }
    }

    async fn stop_crew_worker_by<F: Future<Output = Result<()>> + Send>(
        &self,
        run: Option<&CrewRun>,
        force: &impl Fn(String) -> F,
    ) -> Result<()> {
        let Some(thread_id) = run.and_then(|r| r.worker_thread_id.as_deref()) else {
            return Ok(());
        };
        for _ in 0..STOP_ROUNDS {
            let thread = match self.store.thread(thread_id) {
                Ok(thread) => thread,
                Err(CoreError::NotFound(_)) => return Ok(()),
                Err(error) => return Err(error),
            };
            for queued in self.store.list_queue(thread_id)? {
                match self.store.remove_queued(&queued.id) {
                    Ok(()) | Err(CoreError::NotFound(_)) => {}
                    Err(error) => return Err(error),
                }
            }
            if thread.active_run.is_some() {
                force(thread_id.to_owned()).await?;
            }
            let now = self.store.thread(thread_id)?;
            if now.active_run.is_none() && self.store.list_queue(thread_id)?.is_empty() {
                return Ok(());
            }
        }
        Err(CoreError::Internal("The worker's run did not stop.".into()))
    }
}
