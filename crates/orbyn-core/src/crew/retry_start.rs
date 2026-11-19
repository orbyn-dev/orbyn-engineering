//! A person's retry of a task that could not start (Crew Preview): the worker never ran because the start was refused, and the card
//! offers "Retry on another engine". Person only: the route is local only and listed in `changes::STATE_CHANGING`, and no Crew tool
//! calls this.
//!
//! The task's brief and key stay with the lead, and a new task is the lead's `crew_start`, whose router skips an engine that cannot
//! run a worker. So the retry does what a person cannot do from here: it checks that another engine could take the task, cancels the
//! refused one through the person's cancel (its intent row first, its leases released, a final result), and tells the lead through
//! its queue to start it again. Nothing is routed or started here.
//!
//! Order, each step failing closed: the task must exist and derive as `could_not_start` (else `task_busy`); an engine other than the
//! one that refused must be installed, on, signed in and allowed to run crew workers (else `task_busy`, with the sentence the card
//! shows); then the cancel; then the message, only if this call made the cancel (a task already cancelled by another window is
//! `task_busy` and the lead is not asked).

use std::sync::Arc;

use orbyn_contracts::{CrewTaskRetried, ResultState};

use super::valid_id;
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::host::confine::engine_refusal;

/// What the card says when no other engine can take the task.
pub const NO_OTHER_ENGINE: &str =
    "No other engine is set up to run crew workers. Turn one on in Settings, or dismiss this task.";

impl Core {
    /// Cancels a task that could not start and asks its lead to start it again. See the module for the order.
    pub async fn crew_retry_elsewhere(
        self: &Arc<Self>,
        task_id: &str,
        actor: &str,
    ) -> Result<CrewTaskRetried> {
        self.crew_retry_with(task_id, actor, || {}).await
    }

    /// `crew_retry_elsewhere`, with a hook between the checks and the cancel, so a test can make another window act there.
    pub(super) async fn crew_retry_with(
        self: &Arc<Self>,
        task_id: &str,
        actor: &str,
        between: impl FnOnce(),
    ) -> Result<CrewTaskRetried> {
        self.crew_preview_only()?;
        valid_id(task_id)?;
        let task = self
            .store
            .crew_task(task_id)?
            .ok_or_else(|| CoreError::NotFound(format!("crew task {task_id}")))?;
        let facts = self.flow_facts(task)?;
        if !facts.could_not_start() {
            return Err(CoreError::CrewTaskBusy(
                "This task did start, so it cannot be retried on another engine.".into(),
            ));
        }
        let Some(run) = facts.run.clone() else {
            return Err(CoreError::NotFound(format!("crew run {task_id}")));
        };
        let refused = facts.route.as_ref().map(|r| r.engine.as_str().to_owned());
        let project = self.store.thread(&run.lead_thread_id)?.project_id;
        let machine = self.crew_machine(Some(&project)).await?;
        let another = machine.views.iter().any(|view| {
            view.worth_reading()
                && engine_refusal(view.tier, &view.family).is_none()
                && refused.as_deref() != Some(view.id.as_str())
        });
        if !another {
            return Err(CoreError::CrewTaskBusy(NO_OTHER_ENGINE.into()));
        }
        let why = self
            .store
            .crew_results(task_id)?
            .into_iter()
            .rev()
            .find(|r| r.state == ResultState::NeedsPerson)
            .map(|r| super::redact(&r.summary))
            .unwrap_or_default();
        between();
        let cancelled = self
            .crew_cancel_task(
                task_id,
                Some("Could not start; retried on another engine."),
                actor,
            )
            .await?;
        self.retry_after_cancel(
            cancelled.already,
            task_id,
            &run.lead_thread_id,
            refused,
            &why,
        )
    }

    /// The lead is asked only when this call made the cancel. A task already cancelled was dismissed or retried by someone else
    /// meanwhile (another window): asking again would override the dismiss or start the task twice.
    fn retry_after_cancel(
        self: &Arc<Self>,
        already: bool,
        task_id: &str,
        lead_thread_id: &str,
        refused: Option<String>,
        why: &str,
    ) -> Result<CrewTaskRetried> {
        if already {
            return Err(CoreError::CrewTaskBusy(
                "This task was already cancelled, so nothing was started again.".into(),
            ));
        }
        let on = refused.map_or_else(String::new, |engine| format!(" on {engine}"));
        let note = format!(
            "Task {task_id} could not start{on}: {why} The person cancelled it and asks you to start it again with crew_start; \
             the router now picks an engine that can run a crew worker."
        );
        let lead_told = match self.runs.enqueue_crew_note(lead_thread_id, &note, &[]) {
            Ok(_) => true,
            Err(error) => {
                tracing::warn!(%error, "could not ask the lead to start a task again");
                false
            }
        };
        Ok(CrewTaskRetried {
            task: task_id.to_owned(),
            lead_told,
        })
    }
}
