//! A lane Gate records a workflow decision, never a model's opinion of correctness.
//! Reads derive the current decision; lifecycle writes keep append-only snapshots.

use orbyn_contracts::{
    CheckOutcome, CrewError, CrewHash, CrewId, CrewObject, CrewToken, Gate, GateDecider,
    GateDecision, GateStage,
};
use serde_json::Value;

use crate::app::Core;
use crate::crew::leases::{core_author, internal};
use crate::crew::tool::invalid;
use crate::crew::verify::short;
use crate::store::CrewReview;

impl Core {
    /// Current Gate for one lane at the caller's expected head. Missing Evidence is
    /// no decision. Consumers must require Pass here, rather than reuse a stored pass.
    pub fn review_lane_gate(
        &self,
        subject: &str,
        lane: &str,
        expected_commit: &str,
    ) -> Result<Option<Gate>, CrewError> {
        orbyn_contracts::CrewCommit::new(expected_commit).map_err(invalid)?;
        CrewToken::new(lane).map_err(invalid)?;
        let status = self.review_status(subject)?;
        let latest = self
            .store
            .crew_reviews_of(subject)
            .map_err(internal)?
            .into_iter()
            .filter(|r| r.role == "review" && r.lane == lane)
            .max_by_key(|r| r.round);
        match latest {
            Some(review) => self.lane_gate(&review, expected_commit, &status["findings"]),
            None => Ok(None),
        }
    }

    pub(super) fn lane_gate(
        &self,
        review: &CrewReview,
        expected_commit: &str,
        findings: &Value,
    ) -> Result<Option<Gate>, CrewError> {
        let Some(task) = self.store.crew_task(&review.task_id).map_err(internal)? else {
            return Ok(None);
        };
        // Passed-candidate Result citations are the verification boundary. Later
        // review/merge records cannot redefine what this reviewer checked.
        let results = self.store.crew_results(&task.id).map_err(internal)?;
        let Some(passed) = results
            .iter()
            .rfind(|r| r.state == orbyn_contracts::ResultState::Passed)
        else {
            return Ok(None);
        };
        let Some(cited_read_only) = passed
            .checks
            .iter()
            .find(|c| c.check.as_str() == "chk-read-only")
        else {
            return Ok(None);
        };
        let evidence = self.store.crew_evidence(&task.id).map_err(internal)?;
        let Some(read_only) = evidence
            .iter()
            .find(|e| e.id == cited_read_only.evidence && e.check == cited_read_only.check)
        else {
            return Ok(None);
        };
        let valid_run = task.state == "passed"
            && passed.r#final
            && passed.run_id.as_str() == task.run_id
            && passed.brief.as_str() == task.id
            && passed
                .commit
                .as_ref()
                .is_some_and(|c| c.as_str() == review.commit)
            && cited_read_only.outcome == CheckOutcome::Pass
            && read_only.outcome == CheckOutcome::Pass
            && read_only.run_id.as_str() == task.run_id
            && read_only.commit.as_str() == review.commit;
        let held = findings.as_array().into_iter().flatten().any(|f| {
            f["lane"] == review.lane
                && (f["blocks"] == true
                    || matches!(f["check"].as_str(), Some("unchecked" | "pending")))
        });
        let reason = if review.commit != expected_commit {
            "This lane read another commit; the current head still needs review."
        } else if !valid_run {
            "The latest round has no completed valid read-only verification."
        } else if held {
            "This lane has a confirmed blocker or findings still awaiting a code-owned check."
        } else {
            "The latest read-only run completed at this commit and no unresolved finding holds the lane. This is not proof of code correctness."
        };
        let subject = self
            .store
            .crew_task(&review.subject_id)
            .map_err(internal)?
            .ok_or_else(|| invalid("the reviewed task is missing"))?;
        // Include finding-check Evidence so changed adjudication produces a new
        // content-addressed snapshot even though the reviewer run did not change.
        let mut cited = vec![read_only.id.clone()];
        let mut created_at = read_only.created_at.clone();
        for gate in self
            .store
            .crew_gates(&subject.id)
            .map_err(internal)?
            .iter()
            .filter(|g| g.stage == GateStage::FindingCheck || g.stage == GateStage::Checks)
        {
            let same_lane = self
                .store
                .crew_finding(gate.subject.as_str())
                .map_err(internal)?
                .is_some_and(|f| f.lane == review.lane);
            if same_lane {
                for id in &gate.evidence {
                    if !cited.contains(id) && cited.len() < 64 {
                        cited.push(id.clone());
                    }
                }
                if gate.created_at.as_str() > created_at.as_str() {
                    created_at = gate.created_at.clone();
                }
            }
        }
        let mut gate = Gate {
            v: 0,
            id: CrewHash::new(format!("sha256:{}", "0".repeat(64))).map_err(invalid)?,
            crew_id: CrewId::new(subject.crew_id).map_err(invalid)?,
            author: core_author(),
            created_at,
            run_id: CrewId::new(subject.run_id).map_err(invalid)?,
            stage: GateStage::Review,
            subject: CrewHash::new(review.task_id.clone()).map_err(invalid)?,
            decision: if valid_run && !held && review.commit == expected_commit {
                GateDecision::Pass
            } else {
                GateDecision::Fail
            },
            decided_by: GateDecider::Code,
            evidence: cited,
            reason: short(reason)?,
            lane: Some(CrewToken::new(review.lane.clone()).map_err(invalid)?),
        };
        gate.validate()?;
        gate.id = CrewObject::Gate(gate.clone()).computed_id()?;
        Ok(Some(gate))
    }

    /// Lifecycle-only snapshots. The status view and merge admission still derive
    /// fresh state, so a superseded snapshot can never authorize another round.
    pub(super) fn snapshot_review_gates(&self, subject: &str) -> Result<(), CrewError> {
        let status = self.review_status(subject)?;
        for lane in status["lanes"].as_array().into_iter().flatten() {
            if let Some(value) = lane.get("gate").filter(|v| !v.is_null()) {
                let gate: Gate = serde_json::from_value(value.clone())
                    .map_err(|e| invalid(format!("lane Gate: {e}")))?;
                self.store
                    .add_crew_object(subject, &CrewObject::Gate(gate))
                    .map_err(internal)?;
            }
        }
        Ok(())
    }
}
