//! Review: findings, disagreements and the gates that decide.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::common::{CrewError, CrewRole, Verdict, count, envelope, written_by};
use super::text::{
    CrewCommit, CrewHash, CrewId, CrewLongText, CrewMember, CrewPath, CrewShortText, CrewToken,
};

wire_enum! {
pub enum FindingSeverity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}
}

crew_object! {
/// A reviewer's typed finding. It blocks only after a gate at `finding_check` confirms it.
pub struct Finding {
    pub run_id: CrewId,
    /// The reviewer lane, for example `correctness` or `security`.
    pub lane: CrewToken,
    pub severity: FindingSeverity,
    pub path: CrewPath,
    /// 1-based.
    pub line: u32,
    pub claim: CrewShortText,
    pub evidence: CrewLongText,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub repro: Option<CrewLongText>,
    /// The exact commit that was read.
    pub commit: CrewCommit,
}
}

impl Finding {
    pub fn validate(&self) -> Verdict {
        envelope(self.v, &self.author)?;
        if self.line < 1 {
            return Err(CrewError::invalid("line is 1-based"));
        }
        Ok(())
    }
}

wire! {
#[serde(deny_unknown_fields)]
pub struct DisagreementPosition {
    pub member: CrewMember,
    pub engine: CrewToken,
    pub stance: CrewShortText,
    pub evidence: Vec<CrewHash>,
}
}

crew_object! {
/// Two or more opposing positions on one subject, each with its evidence.
pub struct Disagreement {
    pub run_id: CrewId,
    pub subject: CrewHash,
    pub topic: CrewShortText,
    pub positions: Vec<DisagreementPosition>,
}
}

impl Disagreement {
    pub fn validate(&self) -> Verdict {
        envelope(self.v, &self.author)?;
        count("positions", self.positions.len(), 2, 16)?;
        self.positions
            .iter()
            .try_for_each(|p| count("evidence", p.evidence.len(), 0, 32))
    }
}

wire_enum! {
pub enum GateStage {
    Checks,
    CodeReview,
    SecurityReview,
    Review,
    FindingCheck,
    PrReview,
    Disagreement,
    Merge,
}
}

wire_enum! {
pub enum GateDecision {
    Pass,
    Fail,
    Confirmed,
    Unconfirmed,
}
}

wire_enum! {
pub enum GateDecider {
    Code,
    ConfirmedFinding,
    Person,
}
}

crew_object! {
/// A decision at a stage, by code, a confirmed finding or a person. It needs Evidence
/// or a person (H8). Written only by the core, or by a person for a person's gate.
pub struct Gate {
    pub run_id: CrewId,
    pub stage: GateStage,
    pub subject: CrewHash,
    pub decision: GateDecision,
    pub decided_by: GateDecider,
    pub evidence: Vec<CrewHash>,
    pub reason: CrewShortText,
    /// The reviewer lane when `stage` is `review`, and only then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lane: Option<CrewToken>,
}
}

impl Gate {
    pub fn validate(&self) -> Verdict {
        envelope(self.v, &self.author)?;
        written_by("a gate", &self.author, &[CrewRole::Core, CrewRole::Person])?;
        count("evidence", self.evidence.len(), 0, 64)?;
        let finding_check = self.stage == GateStage::FindingCheck;
        let confirms = matches!(
            self.decision,
            GateDecision::Confirmed | GateDecision::Unconfirmed
        );
        if finding_check != confirms {
            return Err(CrewError::invalid(if finding_check {
                "`decision` of a finding_check gate is confirmed or unconfirmed"
            } else {
                "`decision` is pass or fail; only a finding_check gate confirms or does not"
            }));
        }
        if self.decided_by == GateDecider::Person {
            written_by("a person's gate", &self.author, &[CrewRole::Person])?;
        } else {
            written_by("a gate decided by code", &self.author, &[CrewRole::Core])?;
            if self.evidence.is_empty() {
                return Err(CrewError::invalid(
                    "a gate not decided by a person needs evidence",
                ));
            }
        }
        match (self.stage == GateStage::Review, self.lane.is_some()) {
            (true, false) => Err(CrewError::invalid("a review gate names its lane")),
            (false, true) => Err(CrewError::invalid("only a review gate has a lane")),
            _ => Ok(()),
        }
    }
}
