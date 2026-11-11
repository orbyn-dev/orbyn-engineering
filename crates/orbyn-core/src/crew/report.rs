//! `crew_report`: a worker's result at the end of an attempt. The core stamps author, id and
//! run (H9); a draft that names any of them is refused. The verifier then runs after every
//! report, in the background, and the worker is told the reason if a check fails.

use std::sync::Arc;

use orbyn_contracts::{CrewError, CrewId, CrewMember, CrewObject, CrewToken, ErrorCode};
use serde_json::{Value, json};

use super::guard::refuse_secret;
use super::lifecycle::is_final;
use super::stamp::Stamper;
use super::start::Caller;
use super::tool::{invalid, stamp_now};
use crate::app::Core;
use crate::tools::mcp::McpBinding;

impl Core {
    /// `crew_report`. The checks run when the worker's turn has ended, never during it. The arguments are the report's draft: `summary`, and optionally
    /// `branch`, `commit`, `claims` and `not_verified`.
    pub async fn crew_report_tool(
        self: &Arc<Self>,
        binding: &McpBinding,
        args: &Value,
    ) -> Result<Value, CrewError> {
        if !self.crew_enabled() {
            return Err(CrewError::new(ErrorCode::UnknownTool, "crew_report"));
        }
        let Caller::Worker { task, run } = self.crew_caller(binding)? else {
            return Err(CrewError::new(
                ErrorCode::NotAuthorized,
                "only a crew worker reports",
            ));
        };
        // A member tool refuses once the run has ended (H7).
        if is_final(&task.state) {
            return Err(CrewError::new(ErrorCode::RunEnded, "this run has ended"));
        }
        let stamper = Stamper {
            crew_id: CrewId::new(task.crew_id.clone()).map_err(invalid)?,
            engine: CrewToken::new(binding.scope.family.clone())
                .map_err(|why| invalid(format!("engine: {why}")))?,
            member: CrewMember::new(run.member.clone()).map_err(invalid)?,
            now: stamp_now()?,
        };
        let run_id = CrewId::new(task.run_id.clone()).map_err(invalid)?;
        let brief = orbyn_contracts::CrewHash::new(task.id.clone()).map_err(invalid)?;
        let report = stamper.report(args, &run_id, &brief)?;
        // H2, after the shape.
        refuse_secret(&CrewObject::Report(report.clone()).to_value())?;
        if task.state != "running" {
            return Err(CrewError::new(
                ErrorCode::InvalidTransition,
                format!(
                    "the task is {}; a report is taken while it is running",
                    task.state
                ),
            ));
        }
        self.store
            .begin_crew_verification(&task.id, &binding.scope.run_id, Some(&report))
            .map_err(|e| CrewError::new(ErrorCode::InvalidTransition, e.public_message()))?;
        Ok(json!({
            "report": report.id,
            "state": "verifying",
            "note": "Finish your turn. The core checks your work once it ends; if a check fails, the reason arrives as your next message.",
        }))
    }

    /// Runs the key for a task that is `verifying` and applies the result. Any failure to
    /// verify at all hands the task to a person rather than guessing.
    pub(super) async fn verify_crew_task(self: &Arc<Self>, task_id: &str) {
        let (Ok(Some(task)), Ok(Some(run))) =
            (self.store.crew_task(task_id), self.store.crew_run(task_id))
        else {
            return;
        };
        match self.crew_verify(&task, &run).await {
            Ok(judged) => self.settle_crew_task(task_id, &judged),
            Err(error) => {
                tracing::warn!(%error, "a crew task could not be verified");
                self.crew_needs_person(task_id, "The checks could not be run.", true);
            }
        }
    }
}
