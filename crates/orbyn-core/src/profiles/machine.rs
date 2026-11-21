//! What this computer offers a profile: which engines can start, which permission modes each offers and which model a tier means on
//! each. Read once per request from the same facts the Providers list and the crew router read ([`ProviderFacts`]), then handed to
//! plain functions, so the rules of a save and of an apply are decided on one snapshot and tests can build one by hand.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use orbyn_contracts::{
    EngineStatus, EngineTier, ModelInfo, PermissionMode, ProfileAvailability, ProfileEngineState,
    ProfileTier, RouteTier,
};

use crate::agents::permission_modes;
use crate::agents::provider_facts::{ProviderFacts, Unavailable, WriteBlock};
use crate::app::Core;
use crate::crew::RouterConfig;
use crate::error::Result;

/// A model list that does not arrive in this long counts as "no model for the tier" for this request only.
const MODELS_WAIT: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Availability {
    pub state: ProfileEngineState,
    pub reason: Option<String>,
}

impl Availability {
    fn of(state: ProfileEngineState, reason: &str) -> Self {
        Self {
            state,
            reason: Some(reason.to_owned()),
        }
    }

    /// An apply is refused for these. An unmanaged engine only carries a label.
    pub fn blocked(&self) -> bool {
        matches!(
            self.state,
            ProfileEngineState::NotFound
                | ProfileEngineState::CannotStart
                | ProfileEngineState::Disabled
        )
    }

    pub fn wire(&self) -> ProfileAvailability {
        ProfileAvailability {
            engine: self.state,
            reason: self.reason.clone(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Machine {
    statuses: Vec<EngineStatus>,
    models: BTreeMap<String, Vec<ModelInfo>>,
    /// Engines turned off for the whole app or for the project the work is in.
    disabled: BTreeSet<String>,
    config: RouterConfig,
}

impl Machine {
    pub fn new(
        statuses: Vec<EngineStatus>,
        models: BTreeMap<String, Vec<ModelInfo>>,
        disabled: BTreeSet<String>,
        config: RouterConfig,
    ) -> Self {
        Self {
            statuses,
            models,
            disabled,
            config,
        }
    }

    fn status(&self, engine: &str) -> Option<&EngineStatus> {
        self.statuses.iter().find(|status| status.id == engine)
    }

    /// The modes the engine offers now; for an engine this computer has no status for, the fixed matrix by id.
    pub fn modes(&self, engine: &str) -> Vec<PermissionMode> {
        self.status(engine).map_or_else(
            || permission_modes::offered(engine),
            |status| status.capabilities.permission_modes.clone(),
        )
    }

    pub fn availability(&self, engine: Option<&str>) -> Availability {
        let Some(engine) = engine else {
            return Availability {
                state: ProfileEngineState::NoEngine,
                reason: None,
            };
        };
        let Some(status) = self.status(engine) else {
            return Availability::of(ProfileEngineState::NotFound, "Engine not found");
        };
        if self.disabled.contains(engine) {
            return Availability::of(ProfileEngineState::Disabled, "Turned off");
        }
        let facts = ProviderFacts::of(status);
        match facts.unavailable() {
            Some(Unavailable::NotInstalled) => {
                return Availability::of(ProfileEngineState::NotFound, "Engine not found");
            }
            Some(Unavailable::Broken(_)) => {
                return Availability::of(ProfileEngineState::CannotStart, "Can't start");
            }
            Some(Unavailable::SignedOut) => {
                return Availability::of(ProfileEngineState::CannotStart, "Signed out");
            }
            // A sign-in that could not be confirmed has no designed state; the run itself finds out.
            Some(Unavailable::SignInUnconfirmed) | None => {}
        }
        if facts.write_block() == Some(WriteBlock::NotManaged) {
            return Availability::of(
                ProfileEngineState::Unmanaged,
                "Orbyn cannot gate this engine's actions",
            );
        }
        Availability {
            state: ProfileEngineState::Ready,
            reason: None,
        }
    }

    /// The model id the tier means on this engine, or nothing when it has none now.
    pub fn tier_model(&self, engine: &str, tier: ProfileTier) -> std::result::Result<String, ()> {
        let status = self.status(engine).ok_or(())?;
        let models = self.models.get(engine).ok_or(())?;
        let family = status.instance_of.as_deref().unwrap_or(engine);
        let route = match tier {
            ProfileTier::Lead => RouteTier::Lead,
            ProfileTier::Builder => RouteTier::Worker,
            ProfileTier::Scout => RouteTier::Scout,
        };
        crate::crew::tier_model(
            &self.config,
            engine,
            family,
            status.tier == EngineTier::Acp,
            models,
            route,
        )
        .map(|model| model.id.clone())
        .map_err(|_| ())
    }
}

impl Core {
    /// The machine's facts for the engines a request names. Model lists are read only when asked for (a list of profiles needs none),
    /// and a list that is slow or fails is left out rather than waited on. `project` adds the engines that project turned off.
    pub(crate) async fn profile_machine(
        &self,
        engines: &[String],
        with_models: bool,
    ) -> Result<Machine> {
        self.profile_machine_in(engines, with_models, None).await
    }

    pub(crate) async fn profile_machine_in(
        &self,
        engines: &[String],
        with_models: bool,
        project: Option<&str>,
    ) -> Result<Machine> {
        let statuses = self.engines.statuses(false).await?;
        let mut disabled = BTreeSet::new();
        for status in &statuses {
            if self.engines.check_enabled(&status.id).is_err() {
                disabled.insert(status.id.clone());
            }
        }
        if let Some(project) = project
            && let Ok(list) = self.preference("disabled_engines", Some(project))
        {
            disabled.extend(
                list.as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|item| item.as_str().map(str::to_owned)),
            );
        }
        let mut models = BTreeMap::new();
        if with_models {
            for engine in engines {
                let installed = statuses
                    .iter()
                    .any(|status| &status.id == engine && status.installed);
                if !installed || models.contains_key(engine) {
                    continue;
                }
                if let Ok(Ok(list)) =
                    tokio::time::timeout(MODELS_WAIT, self.engine_models(engine, false)).await
                {
                    models.insert(engine.clone(), list.models);
                }
            }
        }
        let mut config = self.config.crew_tiers.clone().unwrap_or_default();
        let saved = self.store.crew_acp_tiers()?;
        if saved.revision > 0 {
            config.acp_tiers = saved.assignments;
        }
        Ok(Machine::new(statuses, models, disabled, config))
    }
}
