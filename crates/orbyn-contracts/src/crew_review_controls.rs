//! Person-owned same-engine review consent; no agent tool accepts these writes.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire_enum! {
pub enum CrewReviewConsentChoice { Once, Wait }
}
wire! {
#[serde(deny_unknown_fields)]
pub struct CrewReviewConsentInput {
    pub request: String,
    pub choice: CrewReviewConsentChoice,
}
}
wire_enum! {
pub enum CrewReviewConsentState { Waiting, Wait, Allowed, Expired }
}
wire_enum! {
pub enum CrewSameEngineAllowedBy { PersonOnce, AutopilotFull }
}
wire! {
/// A pending request, distinct from a reviewer task: nothing has launched yet.
pub struct FlowReviewConsent {
    pub id: String,
    pub task: String,
    pub lane: String,
    pub commit: String,
    pub round: u32,
    pub engine: String,
    pub model: String,
    pub state: CrewReviewConsentState,
    #[ts(type = "number")]
    pub expires_unix: i64,
    // Advisory state eligibility; the local-person HTTP guard and launch
    // admission independently decide the write.
    pub can_answer: bool,
}
}
wire! {
/// Stable fields in the existing launch/wait answer. `task` keeps its launch
/// meaning; subject/request bind the receipt to the invocation that produced it.
pub struct CrewReviewConsentAnswer {
    pub task: String,
    pub subject: String,
    pub request: String,
    pub state: String,
    pub started: bool,
}
}

wire! {
pub struct CrewLaneRemovalReceipt { pub id: String, pub lane: String, pub reason: String }
}
wire! {
pub struct CrewRequiredReviewLane {
    pub lane: String, pub source: String, pub locked: bool, pub can_remove: bool,
    pub removal: Option<CrewLaneRemovalReceipt>,
}
}
wire! {
pub struct CrewReviewControlView {
    pub task: String, pub lanes: Vec<CrewRequiredReviewLane>, pub error: Option<String>,
}
}
wire! {
pub struct CrewLaneRemovalAnswer { pub task: String, pub receipt: CrewLaneRemovalReceipt }
}
