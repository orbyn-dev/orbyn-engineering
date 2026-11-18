//! Device-local person-owned ACP tier Settings. Model expectations are
//! preconditions, never authority to select or promote an arbitrary model.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{CrewToken, RouteTier};

wire! {
#[serde(deny_unknown_fields)]
pub struct CrewAcpTierChange {
    pub engine: CrewToken,
    pub tier: RouteTier,
    // The exact default the person reviewed. Core independently resolves the
    // unique current visible default and refuses a mismatch.
    pub expected_default_model: String,
}
}

wire! {
#[serde(deny_unknown_fields)]
pub struct CrewAcpTierSave {
    // Saved-map revision, not a provider discovery revision.
    #[ts(type = "number")]
    pub seen: i64,
    pub assignments: Vec<CrewAcpTierChange>,
}
}

wire! {
#[serde(deny_unknown_fields)]
pub struct CrewAcpTierAssignment {
    pub tier: RouteTier,
    // Authoritative saved pin; distinct from current engine discovery.
    pub model: String,
}
}

wire! {
pub struct CrewAcpTierSettings {
    #[ts(type = "number")]
    pub revision: i64,
    pub assignments: BTreeMap<String, CrewAcpTierAssignment>,
}
}

wire! {
pub struct CrewAcpTierEngine {
    pub id: String, pub name: String, pub native: bool, pub available: bool,
    pub unavailable_reason: Option<String>, pub defaults: Vec<String>,
}
}
wire! {
pub struct CrewAcpTierView { pub settings: CrewAcpTierSettings, pub engines: Vec<CrewAcpTierEngine>, pub writable: bool }
}

wire_enum! {
/// Where the router looks for a worker's engine when the brief names none (the per-project "Worker provider" setting).
/// A value this version does not know reads as the default, `same_as_lead`.
#[derive(Default)]
pub enum CrewWorkerProvider {
    /// The lead's own provider first; another one only when that one is refused or not available. Pass rate and headroom
    /// choose only inside the provider.
    #[default]
    SameAsLead,
    /// Every allowed provider competes on score. Refusal and platform checks still apply.
    Any,
}
}

wire! {
/// A project's stored worker-provider choice (`GET /api/crew/projects/{id}/worker-provider`).
pub struct CrewWorkerProviderView {
    pub project: String,
    pub worker_provider: CrewWorkerProvider,
}
}

wire! {
#[serde(deny_unknown_fields)]
/// A person's choice for one project (`PUT /api/crew/projects/{id}/worker-provider`).
pub struct CrewWorkerProviderSet {
    pub worker_provider: CrewWorkerProvider,
}
}
