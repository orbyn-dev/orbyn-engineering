//! What the router knows about one engine: a plain copy of the registry's status, its
//! model list and its usage, so the router itself touches no process and no file.

use crate::agents::provider_facts::ProviderFacts;
use orbyn_contracts::{
    AuthState, CrewToken, EngineCapabilities, EngineLimits, EngineModels, EngineStatus, EngineTier,
    ModelInfo,
};

#[derive(Clone, Debug)]
pub struct EngineView {
    pub id: CrewToken,
    pub name: String,
    /// The engine this one is a setup of, or itself. Two engines of one family are one vendor.
    pub family: String,
    pub tier: EngineTier,
    /// Turned off for this project, or an instance that is disabled.
    pub turned_off: bool,
    pub installed: bool,
    pub auth: AuthState,
    /// The signed manifest's reason, when the installed version is known to be broken.
    pub broken: Option<String>,
    pub capabilities: EngineCapabilities,
    /// The models the picker lists: custom ones added, hidden ones removed. `None` when they
    /// were not read, which only happens for an engine a cheaper filter already refused.
    pub models: Option<Vec<ModelInfo>>,
    /// The most used window of the engine's own limit report, 0 to 100. `None` when the
    /// engine reports none.
    pub used_percent: Option<u32>,
}

impl EngineView {
    /// `None` for an engine whose id is not a protocol token (an instance id may hold
    /// capitals), since a decision cannot name it.
    pub fn from_status(status: &EngineStatus, turned_off: bool) -> Option<Self> {
        Some(Self {
            id: CrewToken::new(status.id.clone()).ok()?,
            name: status.name.clone(),
            family: status
                .instance_of
                .clone()
                .unwrap_or_else(|| status.id.clone()),
            tier: status.tier,
            turned_off,
            installed: status.installed,
            auth: status.auth,
            broken: ProviderFacts::of(status).broken_note().map(str::to_owned),
            capabilities: status.capabilities.clone(),
            models: None,
            used_percent: None,
        })
    }

    /// The state facts the Providers list reads too (`agents::provider_facts`), so the
    /// hard filters and the list cannot disagree about an engine.
    pub fn facts(&self) -> ProviderFacts {
        ProviderFacts::from_parts(
            self.installed,
            self.auth,
            self.broken.clone(),
            None,
            &self.capabilities,
        )
    }

    pub fn with_models(mut self, models: &EngineModels) -> Self {
        self.models = Some(models.models.clone());
        self
    }

    pub fn with_limits(mut self, limits: &EngineLimits) -> Self {
        let used = limits
            .windows
            .iter()
            .map(|w| w.used_percent)
            .fold(None, |top: Option<f64>, one| {
                Some(top.map_or(one, |t| t.max(one)))
            });
        // Whole percent, rounded down, so only a window at 100% counts as exhausted.
        self.used_percent = used.map(|p| p.clamp(0.0, 100.0).floor() as u32);
        self
    }

    /// True for an engine the router may look inside: the checks that need no model list
    /// all pass, so reading its models and limits is worth a call.
    pub fn worth_reading(&self) -> bool {
        !self.turned_off && self.facts().unavailable().is_none()
    }
}
