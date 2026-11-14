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
        self.crew_preview_only()?;
        valid_id(task_id)?;
        let reason = match reason.map(str::trim).filter(|r| !r.is_empty()) {
            Some(text) => Some(super::background::text("reason", text, REASON_MAX)?),
            None => None,
        };
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
        if let Err(error) = self.stop_crew_worker(run.as_deref(), actor).await {
            let why: String = super::redact(&error.to_string())
                .chars()
                .take(300)
                .collect();
            if let Err(write) = self.store.fail_crew_cancel(task_id, actor, &why) {
                tracing::warn!(%write, "could not record a failed crew cancel");
            }
            return Err(CoreError::EngineUnavailable(
                "The task was not cancelled: its worker could not be stopped. Try again.".into(),
            ));
        }
        let (task, moved) = self.store.finish_crew_cancel(task_id, actor)?;
        if task.state != "cancelled" {
            return Err(CoreError::TaskEnded);
        }
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
        Ok(done(false))
    }

    /// Stops the worker's engine run and confirms it, with the worker's queue emptied so no new turn starts. A task with no worker,
    /// a worker thread that was removed and one with no run are all stopped already.
    async fn stop_crew_worker(&self, run: Option<&CrewRun>, actor: &str) -> Result<()> {
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
                self.runs.force_stop(thread_id, actor).await?;
            }
            let now = self.store.thread(thread_id)?;
            if now.active_run.is_none() && self.store.list_queue(thread_id)?.is_empty() {
                return Ok(());
            }
        }
        Err(CoreError::Internal("The worker's run did not stop.".into()))
    }
}
