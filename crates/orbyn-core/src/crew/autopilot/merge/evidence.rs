//! Code-written merge Evidence and Gate. A model's approval is never merge evidence.

use orbyn_contracts::{
    CheckOutcome, CrewCommit, CrewHash, CrewId, CrewObject, Evidence, Gate, GateDecider,
    GateDecision, GateStage,
};
use sha2::{Digest, Sha256};

use crate::app::Core;
use crate::crew::checks::{Ran, tail};
use crate::crew::guard::{redact, refuse_secret};
use crate::crew::leases::{core_author, timestamp};
use crate::crew::verify::short;
use crate::error::{CoreError, Result};

fn invalid(error: orbyn_contracts::CrewError) -> CoreError {
    CoreError::Invalid(error.message)
}

impl Core {
    pub(super) fn merge_evidence(
        &self,
        task_id: &str,
        commit: &str,
        number: usize,
        ran: &Ran,
        now: i64,
    ) -> Result<String> {
        let task = self
            .store
            .crew_task(task_id)?
            .ok_or_else(|| CoreError::NotFound("merge task".into()))?;
        let mut evidence = Evidence {
            v: 0,
            id: CrewHash::new(format!("sha256:{}", "0".repeat(64))).map_err(CoreError::Invalid)?,
            crew_id: CrewId::new(task.crew_id).map_err(CoreError::Invalid)?,
            author: core_author(),
            created_at: timestamp(now).map_err(invalid)?,
            run_id: CrewId::new(task.run_id).map_err(CoreError::Invalid)?,
            check: CrewId::new(format!("merge-check-{number}")).map_err(CoreError::Invalid)?,
            claim: None,
            command: short(&ran.command).map_err(invalid)?,
            outcome: ran.outcome,
            exit_code: ran.exit_code,
            duration_ms: ran.duration_ms,
            commit: CrewCommit::new(commit.to_owned()).map_err(CoreError::Invalid)?,
            log_hash: CrewHash::new(format!(
                "sha256:{}",
                crate::util::hex(&Sha256::digest(ran.log.as_bytes()))
            ))
            .map_err(CoreError::Invalid)?,
            excerpt: redact(&tail(&ran.log, 4000)),
            artifacts: Vec::new(),
            proves: short("The named check's observed outcome at the recorded commit.")
                .map_err(invalid)?,
            does_not_prove: short(
                "Coverage, future branch state, or checks that were not observed.",
            )
            .map_err(invalid)?,
        };
        evidence.id = CrewObject::Evidence(evidence.clone())
            .computed_id()
            .map_err(invalid)?;
        let object = CrewObject::Evidence(evidence.clone());
        refuse_secret(&object.to_value()).map_err(invalid)?;
        self.store.add_crew_object(task_id, &object)?;
        Ok(evidence.id.as_str().to_owned())
    }

    /// The evidence IDs must resolve to this task's passed combined and CI checks.
    pub(in crate::crew) fn merge_gate_from_evidence(
        &self,
        task_id: &str,
        ids: &[String],
        combined: &str,
        head: &str,
        now: i64,
    ) -> Result<()> {
        let evidence = self.store.crew_evidence(task_id)?;
        let selected: Vec<_> = evidence
            .iter()
            .filter(|e| ids.iter().any(|id| e.id.as_str() == id))
            .collect();
        if ids.is_empty()
            || selected.len() != ids.len()
            || selected.iter().any(|e| {
                e.outcome != CheckOutcome::Pass || ![combined, head].contains(&e.commit.as_str())
            })
            || !selected.iter().any(|e| e.commit.as_str() == combined)
            || !selected
                .iter()
                .any(|e| e.commit.as_str() == head && e.command.as_str() == "forge CI receipt")
        {
            return Err(CoreError::Invalid(
                "a merge Gate needs passed combined-state and current-head CI Evidence".into(),
            ));
        }
        let task = self
            .store
            .crew_task(task_id)?
            .ok_or_else(|| CoreError::NotFound("merge task".into()))?;
        let mut gate = Gate {
            v: 0,
            id: selected[0].id.clone(),
            crew_id: selected[0].crew_id.clone(),
            author: core_author(),
            created_at: timestamp(now).map_err(invalid)?,
            run_id: CrewId::new(task.run_id).map_err(CoreError::Invalid)?,
            stage: GateStage::Merge,
            subject: CrewHash::new(task_id.to_owned()).map_err(CoreError::Invalid)?,
            decision: GateDecision::Pass,
            decided_by: GateDecider::Code,
            evidence: selected.iter().map(|e| e.id.clone()).collect(),
            reason: short("Combined-state checks and configured current-head forge CI passed.")
                .map_err(invalid)?,
            lane: None,
        };
        gate.validate().map_err(invalid)?;
        gate.id = CrewObject::Gate(gate.clone())
            .computed_id()
            .map_err(invalid)?;
        self.store.add_crew_object(task_id, &CrewObject::Gate(gate))
    }
}
