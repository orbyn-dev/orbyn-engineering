//! What goes back to the lead about a task. Written only by the core.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::common::{CrewError, CrewRole, Verdict, count, envelope, required, text, written_by};
use super::evidence::CheckOutcome;
use super::text::{CrewBranch, CrewCommit, CrewHash, CrewId, CrewShortText};

wire_enum! {
/// `passed`, `stopped` and `cancelled` are final; `failed` and `needs_person` are interim.
pub enum ResultState {
    Passed,
    Failed,
    Stopped,
    Cancelled,
    NeedsPerson,
}
}

impl ResultState {
    pub fn is_final(self) -> bool {
        matches!(self, Self::Passed | Self::Stopped | Self::Cancelled)
    }
}

wire_enum! {
pub enum StopReason {
    RepeatedFailure,
    Budget,
    TooManyDenials,
    InjectionSuspected,
    EngineError,
    LeadStopped,
}
}

wire! {
#[serde(deny_unknown_fields)]
pub struct ResultCheck {
    pub check: CrewId,
    pub evidence: CrewHash,
    pub outcome: CheckOutcome,
}
}

wire! {
#[serde(deny_unknown_fields)]
pub struct ResultUsage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub turns: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub minutes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub tokens_in: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub tokens_out: Option<u64>,
    /// Millionths of the billing unit; never mixed across engines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub cost_micros: Option<u64>,
}
}

crew_object! {
pub struct CrewResult {
    pub run_id: CrewId,
    pub brief: CrewHash,
    pub state: ResultState,
    pub r#final: bool,
    /// Present exactly when `state` is `stopped`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub stop_reason: Option<StopReason>,
    pub summary: String,
    #[serde(deserialize_with = "required")]
    pub branch: Option<CrewBranch>,
    #[serde(deserialize_with = "required")]
    pub commit: Option<CrewCommit>,
    pub checks: Vec<ResultCheck>,
    /// What no check covered, so a pass never implies more than was checked.
    pub not_verified: Vec<CrewShortText>,
    #[ts(type = "number")]
    pub attempts: u64,
    pub usage: ResultUsage,
    /// Null when the task ended before it was routed.
    #[serde(deserialize_with = "required")]
    pub route: Option<CrewHash>,
    #[serde(deserialize_with = "required")]
    pub report: Option<CrewHash>,
}
}

impl CrewResult {
    pub fn validate(&self) -> Verdict {
        envelope(self.v, &self.author)?;
        written_by("a result", &self.author, &[CrewRole::Core])?;
        text("summary", &self.summary, 2000)?;
        count("not_verified", self.not_verified.len(), 0, 64)?;
        if self.r#final != self.state.is_final() {
            return Err(CrewError::invalid(
                "final: true exactly for passed, stopped and cancelled",
            ));
        }
        match (
            self.state == ResultState::Stopped,
            self.stop_reason.is_some(),
        ) {
            (true, false) => Err(CrewError::invalid("stop_reason: a stopped result names it")),
            (false, true) => Err(CrewError::invalid(
                "stop_reason: only a stopped result has one",
            )),
            _ => Ok(()),
        }
    }
}
