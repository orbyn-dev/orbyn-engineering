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
