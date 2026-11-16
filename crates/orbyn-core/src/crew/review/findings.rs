//! `crew_finding`: a reviewer files one typed finding. The core stamps the author, the lane, the reviewed
//! commit, the run and the id (H9); a draft that names any of them is refused. The finding has no state:
//! it blocks nothing until a Gate at `finding_check` confirms it. The tool is a member tool, so it refuses
//! once the reviewer's run has ended (H7); the Gate that decides it is a core write and does not.

use orbyn_contracts::{CrewError, CrewId, CrewMember, CrewObject, CrewToken, ErrorCode};
use serde_json::{Value, json};

use super::{MOST_FINDINGS, MOST_SECOND_FINDINGS, fingerprint};
use crate::app::Core;
use crate::crew::guard::refuse_secret;
use crate::crew::lifecycle::is_final;
use crate::crew::stamp::Stamper;
use crate::crew::start::Caller;
use crate::crew::tool::{invalid, stamp_now};
use crate::error::CoreError;
use crate::tools::mcp::McpBinding;

impl Core {
    pub(super) fn crew_finding_tool(
        &self,
        binding: &McpBinding,
        args: &Value,
    ) -> Result<Value, CrewError> {
        let Caller::Worker { task, run } = self.crew_caller(binding)? else {
            return Err(CrewError::new(
                ErrorCode::NotAuthorized,
                "only a reviewer files a finding",
            ));
        };
        // H7: a member tool refuses once the run has ended.
        if is_final(&task.state) {
            return Err(CrewError::new(ErrorCode::RunEnded, "this run has ended"));
        }
        let internal = crate::crew::leases::internal;
        let review = self
            .store
            .crew_review(&task.id)
            .map_err(internal)?
            .ok_or_else(|| {
                CrewError::new(
                    ErrorCode::NotAuthorized,
                    "only a reviewer started as a review lane files a finding",
                )
            })?;
        let stamper = Stamper {
            crew_id: CrewId::new(task.crew_id.clone()).map_err(invalid)?,
            engine: CrewToken::new(binding.scope.family.clone())
                .map_err(|why| invalid(format!("engine: {why}")))?,
            member: CrewMember::new(run.member.clone()).map_err(invalid)?,
            now: stamp_now()?,
        };
        let run_id = CrewId::new(task.run_id.clone()).map_err(invalid)?;
        let lane = CrewToken::new(review.lane.clone()).map_err(invalid)?;
        let finding = stamper.finding(args, &run_id, &lane, &review.commit)?;
        // H2, after the shape.
        refuse_secret(&CrewObject::Finding(finding.clone()).to_value())?;
        let print = fingerprint(&review.lane, finding.path.as_str(), finding.claim.as_str());
        let stored = self
            .store
            .add_crew_finding(
                &task.id,
                &finding,
                &print,
                if review.role == "finding_check" {
                    MOST_SECOND_FINDINGS
                } else {
                    MOST_FINDINGS
                },
            )
            .map_err(|error| match error {
                CoreError::RunClosed => CrewError::new(ErrorCode::RunEnded, "this run has ended"),
                other => invalid(other.public_message()),
            })?;
        Ok(json!({ "finding": finding.id, "duplicate": !stored }))
    }
}
