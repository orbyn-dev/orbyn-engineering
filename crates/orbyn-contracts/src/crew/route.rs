//! Which engine was chosen for a brief and why, with every rejected engine and its
//! reason. Written only by the core.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::common::{CrewError, CrewRole, Verdict, count, envelope, within, written_by};
use super::text::{CrewHash, CrewId, CrewShortText, CrewToken};

wire_enum! {
/// Never above medium in v0.
pub enum RouteEffort {
    Low,
    Medium,
}
}

wire_enum! {
pub enum RouteTier {
    Lead,
    Worker,
    Scout,
}
}

wire_enum! {
pub enum RouteRole {
    Builder,
    CodeReview,
    SecurityReview,
    FindingCheck,
    SecondKey,
    Scout,
    Review,
}
}

wire_enum! {
pub enum RouteAssigner {
    Router,
    Person,
    Lead,
}
}

wire_enum! {
pub enum RejectionWhy {
    NotInstalled,
    NotSignedIn,
    CompatibilityBroken,
    NotManaged,
    NoApprovals,
    MissingCapability,
    QuotaExhausted,
    TierBelowLead,
    SameEngineRefused,
}
}

wire! {
/// What the person or lead asked for. The decision never quietly swaps it.
#[serde(deny_unknown_fields)]
pub struct RouteRequested {
    pub engine: CrewToken,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<CrewShortText>,
}
}

wire! {
#[serde(deny_unknown_fields)]
pub struct RouteReason {
    pub code: CrewToken,
    pub text: CrewShortText,
}
}

wire! {
#[serde(deny_unknown_fields)]
pub struct RouteRejection {
    pub engine: CrewToken,
    pub why: RejectionWhy,
    pub text: CrewShortText,
}
}

wire! {
/// A rate always travels with its sample size.
#[serde(deny_unknown_fields)]
pub struct PassRate {
    /// 0 to 1000.
    pub permille: u32,
    /// At least 1.
    #[ts(type = "number")]
    pub samples: u64,
}
}

wire! {
#[serde(deny_unknown_fields)]
pub struct RouteScore {
    pub engine: CrewToken,
    /// 0 to 1000.
    pub score: u32,
    /// True when no measured result backed this score.
    pub used_defaults: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pass_rate: Option<PassRate>,
}
}

crew_object! {
pub struct RouteDecision {
    pub run_id: CrewId,
    pub brief: CrewHash,
    pub engine: CrewToken,
    pub model: CrewShortText,
    pub effort: RouteEffort,
    pub tier: RouteTier,
    pub role: RouteRole,
    pub assigned_by: RouteAssigner,
    /// Set when a person or the lead assigned the engine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub requested: Option<RouteRequested>,
    pub reasons: Vec<RouteReason>,
    pub rejected: Vec<RouteRejection>,
    pub scores: Vec<RouteScore>,
    /// The reviewer lane when `role` is `review`, and only then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lane: Option<CrewToken>,
}
}

impl RouteDecision {
    pub fn validate(&self) -> Verdict {
        envelope(self.v, &self.author)?;
        written_by("a route decision", &self.author, &[CrewRole::Core])?;
        count("reasons", self.reasons.len(), 1, 32)?;
        count("rejected", self.rejected.len(), 0, 64)?;
        count("scores", self.scores.len(), 0, 64)?;
        for score in &self.scores {
            within("score", score.score.into(), 0, 1000)?;
            if let Some(rate) = &score.pass_rate {
                within("permille", rate.permille.into(), 0, 1000)?;
                within("samples", rate.samples, 1, u64::MAX)?;
            }
        }
        if self.assigned_by != RouteAssigner::Router && self.requested.is_none() {
            return Err(CrewError::invalid(
                "a decision assigned by a person or the lead names what was requested",
            ));
        }
        match (self.role == RouteRole::Review, self.lane.is_some()) {
            (true, false) => Err(CrewError::invalid("a review decision names its lane")),
            (false, true) => Err(CrewError::invalid("only a review decision has a lane")),
            _ => Ok(()),
        }
    }
}
