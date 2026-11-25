//! The lead's stop of a Crew task: `crew_stop` ends a task the lead thread leads as `stopped` with the reason
//! `lead_stopped` (Crew Protocol, `crew.stop`). It takes the path the core's own stops take: the state and its reason move in
//! one transaction, then the task's end releases its leases, requests its worker's run to stop and writes its final result.
//!
//! Only the lead thread that leads the task may call it: a worker's token, and a thread that leads another crew, are refused,
//! and the requested task is not read for a worker. A task that already ended is refused, and so is one the lifecycle cannot
//! stop from (`briefed` or `routed`). The person's cancel is not this: it ends `cancelled` and stays theirs.

use std::sync::Arc;

use orbyn_contracts::{CrewError, ErrorCode, ResultState, StopReason};
use serde::Deserialize;
use serde_json::{Value, json};

use super::leases::internal;
use super::settle::stop_name;
use super::start::Caller;
use super::tool::invalid;
use super::{lifecycle, valid_id};
use crate::app::Core;
use crate::error::CoreError;
use crate::tools::mcp::McpBinding;

/// What a lead-stopped task's result says. Code-owned, like the sentence every other stop writes.
const STOPPED: &str = "The lead stopped the task.";

/// `crew_stop`'s arguments: the task, and nothing else.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StopRequest {
    task: String,
}

fn no_task_in_crew(id: &str) -> CrewError {
    invalid(format!("{id}: no such task in this crew"))
}

impl Core {
    /// `crew_stop`: the lead stops a task it leads. A worker's token, a thread that does not lead the task, an ended task and
    /// the two states the lifecycle cannot stop from are all refused; nothing is read for a refused caller and nothing is written
    /// for a refused request.
    pub async fn crew_stop_tool(
        self: &Arc<Self>,
        binding: &McpBinding,
        args: &Value,
    ) -> Result<Value, CrewError> {
        if !self.crew_enabled() {
            return Err(CrewError::new(ErrorCode::UnknownTool, "crew_stop"));
        }
        // Only the lead stops a task. A worker's token is refused before the requested task is read: the caller's own run row
        // is what tells them apart, as it does for the report tool, which refuses a caller that is not a worker.
        if !matches!(self.crew_caller(binding)?, Caller::Lead) {
            return Err(CrewError::new(
                ErrorCode::NotAuthorized,
                "only the lead stops a task",
            ));
        }
        let request: StopRequest =
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
        // Defence in depth: the store writes a task's `crew_id` and its run's `lead_thread_id` from the same lead thread, so a
        // task of this crew is this thread's to stop. The check stays in case those two ever come from different threads.
        if run.lead_thread_id != owner {
            return Err(CrewError::new(
                ErrorCode::NotAuthorized,
                "only the lead that leads this task stops it",
            ));
        }
        // A member tool refuses once the run has ended (H7); a stopped or cancelled task has none left to stop.
        if lifecycle::is_final(&task.state) {
            return Err(CrewError::new(ErrorCode::RunEnded, "this task has ended"));
        }
        if !lifecycle::allowed(&task.state, "stopped") {
            return Err(CrewError::new(
                ErrorCode::InvalidTransition,
                format!(
                    "a {} task cannot be stopped; only the person cancels it",
                    task.state
                ),
            ));
        }
        // The same move the core's own stops make (a spent budget, a repeated failure): the state and the stored reason change
        // in one transaction. A task that moves between the read and this write refuses the move instead of overwriting the state
        // it moved to; where it moved decides the answer, so it is read again.
        let stopped = match self.store.move_crew_task(
            &task.id,
            "stopped",
            Some(stop_name(StopReason::LeadStopped)),
        ) {
            Ok(stopped) => stopped,
            Err(CoreError::InvalidTransition { .. }) => {
                // `run_ended` only for a task that ended. One still live moved to a state the lifecycle will not stop from
                // (a `needs_person` task routed again, say), so the refusal is that transition and never claims it ended.
                return Err(match self.store.crew_task(&task.id).map_err(internal)? {
                    Some(now) if lifecycle::is_final(&now.state) => {
                        CrewError::new(ErrorCode::RunEnded, "this task has ended")
                    }
                    Some(now) => CrewError::new(
                        ErrorCode::InvalidTransition,
                        format!("the task moved to {} while it was being stopped", now.state),
                    ),
                    None => CrewError::new(ErrorCode::RunEnded, "this task has ended"),
                });
            }
            Err(error) => return Err(internal(error)),
        };
        self.end_crew_task(
            &stopped,
            ResultState::Stopped,
            Some(StopReason::LeadStopped),
            STOPPED,
            None,
        );
        Ok(json!({
            "task": stopped.id,
            "run": stopped.run_id,
            "state": stopped.state,
            "stop_reason": super::wire_name(&StopReason::LeadStopped),
        }))
    }
}
