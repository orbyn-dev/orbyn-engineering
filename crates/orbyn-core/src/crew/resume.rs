//! `crew_resume`: the lead resumes a held Crew task (issue #438). A task waits in `needs_person` when its checks could
//! not be run or could not decide it; this tool takes it back to `running` and sends its worker another turn, so the same
//! thread, answer key and budget carry on. It is the lead's own action, as `crew_stop` is: a worker's token is refused
//! before the task is read, and only the thread that leads the task may call it.
//!
//! Not every held task is the lead's to resume. A task whose worker never ran (a start was refused) is started again with
//! `crew_start`, or retried on another engine by the person; a task waiting on a `manual` check is the person's to answer;
//! a review lane is asked for again (KC8's rule); a merge correction finishes on its own. Those, a busy worker and a task
//! in any other state are refused, and a refused request writes nothing but the `tool.call` row every Crew tool gets.
//!
//! The move is the claim: one transaction takes the task from `needs_person` to `running`, so two windows or a racing stop
//! cannot both resume it, and a caller that loses the race is told what moved the task rather than that it ended. Then the
//! worker is told through the same guarded path every retry uses (H1, H2), and a prompt that cannot be queued puts the task
//! back in `needs_person` rather than leaving a `running` task with no worker. The task's minutes keep running through the
//! wait: a restart credits a held task's off time (see `recover::excuse_held_downtime`), the run is read again here so that
//! credit is counted, a task past its budget is refused, and a resumed one gets its timer again with what is left.

use std::sync::Arc;

use orbyn_contracts::{CrewError, ErrorCode};
use serde::Deserialize;
use serde_json::{Value, json};

use super::leases::internal;
use super::start::Caller;
use super::tool::invalid;
use super::{lifecycle, valid_id};
use crate::app::Core;
use crate::error::CoreError;
use crate::store::{CrewTask, Lane, lane};
use crate::tools::mcp::McpBinding;

/// What the worker is told when its lead resumes it. Code-owned like every retry prompt: it names no check, no part of the
/// key and no hint, and it keeps the worker on the files its brief gave it (H1, H11).
fn resume_prompt() -> String {
    "Your lead resumed this task after it waited for a person. Read the repository state, finish the work that is still \
     missing, and call crew_report again when you are done. The core runs the checks; you do not see them. Do not widen \
     your changes beyond the paths you were given."
        .to_owned()
}

/// `crew_resume`'s arguments: the task, and nothing else.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResumeRequest {
    task: String,
}

fn no_task_in_crew(id: &str) -> CrewError {
    invalid(format!("{id}: no such task in this crew"))
}

impl Core {
    /// `crew_resume`: the lead resumes a held task it leads. See the module for what is refused and what a resume leaves.
    pub async fn crew_resume_tool(
        self: &Arc<Self>,
        binding: &McpBinding,
        args: &Value,
    ) -> Result<Value, CrewError> {
        self.crew_resume_with(binding, args, |task, text| self.send_to_worker(task, text))
    }

    /// `crew_resume_tool`, with how the worker is told given, so a test can make the prompt fail and see the task put back.
    pub(super) fn crew_resume_with(
        self: &Arc<Self>,
        binding: &McpBinding,
        args: &Value,
        send: impl Fn(&CrewTask, &str) -> Result<(), CrewError>,
    ) -> Result<Value, CrewError> {
        if !self.crew_enabled() {
            return Err(CrewError::new(ErrorCode::UnknownTool, "crew_resume"));
        }
        // Only the lead resumes a task. A worker's token is refused before the task is read.
        if !matches!(self.crew_caller(binding)?, Caller::Lead) {
            return Err(CrewError::new(
                ErrorCode::NotAuthorized,
                "only the lead resumes a task",
            ));
        }
        let request: ResumeRequest =
            serde_json::from_value(args.clone()).map_err(|why| invalid(why.to_string()))?;
        valid_id(&request.task).map_err(internal)?;
        let owner = binding.scope.thread_id.as_str();
        // The task must be this crew's, and this thread must be the one that leads it.
        let task = self
            .store
            .crew_task(&request.task)
            .map_err(internal)?
            .filter(|task| task.crew_id == owner)
            .ok_or_else(|| no_task_in_crew(&request.task))?;
        let run = self
            .store
            .crew_run(&task.id)
            .map_err(internal)?
            .ok_or_else(|| no_task_in_crew(&task.id))?;
        // Defence in depth: a task's crew id and its run's lead thread are written from the same lead thread, so this cannot
        // fire today, but the lead that resumes a task is the lead the run names, not merely a thread of the crew.
        if run.lead_thread_id != owner {
            return Err(CrewError::new(
                ErrorCode::NotAuthorized,
                "only the lead that leads this task resumes it",
            ));
        }
        // A member tool refuses once the run has ended (H7).
        if lifecycle::is_final(&task.state) {
            return Err(CrewError::new(ErrorCode::RunEnded, "this task has ended"));
        }
        if task.state != "needs_person" {
            return Err(CrewError::new(
                ErrorCode::InvalidTransition,
                format!(
                    "a {} task is not waiting for a person, so there is nothing to resume",
                    task.state
                ),
            ));
        }
        // A task whose worker never ran has no work to resume: its start was refused, so it is started again (or the person
        // retries it on another engine), rather than resumed as if it had run.
        if self
            .flow_facts(task.clone())
            .map_err(internal)?
            .could_not_start()
        {
            return Err(invalid(
                "this task never started, so there is no worker to resume; start it again with crew_start, or the \
                 person can retry it on another engine",
            ));
        }
        // KC8's rule, for the same reason: only the builder or fix lane resumes. A review lane is asked for again by the review
        // machinery, whose own prompt and state a resume would bypass.
        match lane(&run.member) {
            Lane::Builder => {}
            Lane::Reviewer => {
                return Err(invalid(
                    "this task is a review lane; a review is asked for again, not resumed",
                ));
            }
            Lane::Other => {
                return Err(invalid(
                    "this task is not a builder or fix task, so the lead does not resume it",
                ));
            }
        }
        // A manual check is the person's to answer, not the lead's to skip.
        if !self
            .store
            .crew_manual_checks_waiting(&task.id)
            .map_err(internal)?
            .is_empty()
        {
            return Err(invalid(
                "this task waits on a check a person makes; the person answers it, not the lead",
            ));
        }
        // A merge correction finishes on its own, without a worker turn.
        if self
            .store
            .crew_correction_for_task(&task.id)
            .map_err(internal)?
            .is_some()
        {
            return Err(invalid(
                "this task belongs to a merge correction and finishes on its own; it cannot be resumed",
            ));
        }
        let Some(worker) = run.worker_thread_id.clone() else {
            return Err(invalid(
                "this task has no worker to resume; start it again with crew_start",
            ));
        };
        // A worker still running, or with a prompt queued, is not between turns. Fail closed.
        let busy = match self.store.thread(&worker) {
            Ok(thread) => thread.active_run.is_some(),
            Err(CoreError::NotFound(_)) => false,
            Err(error) => return Err(internal(error)),
        } || !self.store.list_queue(&worker).map_err(internal)?.is_empty();
        if busy {
            return Err(invalid(
                "this task's worker is still running; wait for its turn to end before resuming it",
            ));
        }
        // A restart credits a held task's off time in the background; read the run again for the minutes so the credit is
        // counted even when startup and this call overlap.
        let run = self
            .store
            .crew_run(&task.id)
            .map_err(internal)?
            .ok_or_else(|| invalid("this task has no run"))?;
        // The minutes keep running through the wait the way the live timer counts them; a task past its budget cannot be
        // resumed, and a resumed one gets the rest of its timer back (a task held across a restart would otherwise have none).
        let left = match self.store.crew_brief(&task.id).map_err(internal)? {
            Some(brief) => {
                let left = super::verify::budget_left_after_downtime(
                    &task.created_at,
                    u64::from(brief.budget.minutes),
                    crate::util::unix_now(),
                    run.downtime_secs,
                );
                if left.is_zero() {
                    return Err(invalid(
                        "this task ran past its time budget, so it cannot be resumed",
                    ));
                }
                Some(left)
            }
            None => None,
        };
        // The move is the claim: a stop or another resume that lands first makes this one fail instead of overwriting it.
        let moved = match self.store.move_crew_task(&task.id, "running", None) {
            Ok(moved) => moved,
            Err(CoreError::InvalidTransition { .. }) => {
                // Name what is there now: an ending is an ending, but another resume or a move is not one.
                let now = self
                    .store
                    .crew_task(&task.id)
                    .map_err(internal)?
                    .map(|task| task.state);
                return Err(match now.as_deref() {
                    Some(state) if lifecycle::is_final(state) => CrewError::new(
                        ErrorCode::RunEnded,
                        "this task ended before it could be resumed",
                    ),
                    Some("running") => CrewError::new(
                        ErrorCode::InvalidTransition,
                        "another action resumed this task a moment ago",
                    ),
                    Some(state) => CrewError::new(
                        ErrorCode::InvalidTransition,
                        format!("this task moved to {state} before it could be resumed"),
                    ),
                    None => CrewError::new(
                        ErrorCode::RunEnded,
                        "this task ended before it could be resumed",
                    ),
                });
            }
            Err(error) => return Err(internal(error)),
        };
        if let Err(error) = send(&moved, &resume_prompt()) {
            // No prompt, no resume: put the task back the way it was rather than leave it running with no worker.
            if let Err(back) = self.store.move_crew_task(&task.id, "needs_person", None) {
                tracing::warn!(
                    %back, task = %task.id,
                    "a resumed task could not be put back after its worker was not told"
                );
            }
            return Err(invalid(format!(
                "the worker could not be told the task resumed: {error}"
            )));
        }
        if let Some(left) = left {
            self.watch_crew_budget(&moved.id, left);
        }
        Ok(json!({
            "task": moved.id,
            "run": moved.run_id,
            "state": moved.state,
            "worker": worker,
        }))
    }
}
