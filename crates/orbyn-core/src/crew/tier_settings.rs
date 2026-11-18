//! Person-owned device-local ACP assignments. Native model-family words keep
//! their existing contract; ACP tier admission never guesses a model name.

use std::collections::BTreeMap;

use orbyn_contracts::{CrewAcpTierSave, CrewAcpTierSettings, CrewError, EngineTier, RouteTier};
use serde_json::{Value, json};

use super::tiers::{AcpTierAssignment, RouterConfig, explicit_default};
use crate::app::Core;
use crate::crew::guard::refuse_secret;
use crate::crew::leases::internal;
use crate::error::{CoreError, Result};

pub(super) fn resolve_acp_assignment(
    view: &super::EngineView,
    tier: RouteTier,
) -> Result<AcpTierAssignment> {
    if view.tier != EngineTier::Acp {
        return Err(CoreError::Invalid(
            "native engines use their model-family tier map".into(),
        ));
    }
    if !view.worth_reading() {
        return Err(CoreError::Invalid(
            "an assignment names an unavailable engine".into(),
        ));
    }
    let models = view.models.as_deref().unwrap_or(&[]);
    let model = explicit_default(models).ok_or_else(|| {
        CoreError::Invalid(
            "the engine must have exactly one visible explicitly marked default model".into(),
        )
    })?;
    Ok(AcpTierAssignment {
        tier,
        model: model.id.clone(),
    })
}

impl Core {
    pub async fn crew_engine_tier_view(&self) -> Result<orbyn_contracts::CrewAcpTierView> {
        self.crew_preview_only()?;
        let settings = serde_json::from_value(self.crew_engine_tiers()?)?;
        let machine = self.crew_machine(None).await?;
        let engines = machine
            .views
            .into_iter()
            .map(|view| {
                use crate::agents::provider_facts::Unavailable;
                let unavailable_reason = if view.turned_off {
                    Some("Disabled".to_owned())
                } else {
                    view.facts().unavailable().map(|why| match why {
                        Unavailable::NotInstalled => "Not installed".into(),
                        Unavailable::SignedOut => "Sign in required".into(),
                        Unavailable::SignInUnconfirmed => {
                            "Sign-in status could not be confirmed".into()
                        }
                        Unavailable::Broken(_) => "Engine unavailable".into(),
                    })
                };
                orbyn_contracts::CrewAcpTierEngine {
                    id: view.id.as_str().into(),
                    name: view.name.clone(),
                    native: view.tier != EngineTier::Acp,
                    available: view.worth_reading(),
                    unavailable_reason,
                    defaults: view
                        .models
                        .as_deref()
                        .unwrap_or(&[])
                        .iter()
                        .filter(|m| m.is_default)
                        .map(|m| m.id.clone())
                        .collect(),
                }
            })
            .collect();
        let shown = orbyn_contracts::CrewAcpTierView {
            settings,
            engines,
            writable: true,
        };
        refuse_secret(&serde_json::to_value(&shown)?).map_err(|e| CoreError::Invalid(e.message))?;
        Ok(shown)
    }
    /// Fallible so a storage failure cannot silently restore higher-tier defaults.
    /// Revision zero uses the explicit CoreConfig seed, never name-derived ACP tiers.
    /// `project_id` is the project of the thread being routed: its stored worker provider fills
    /// `worker_provider` (absent or unknown reads as the lead's provider).
    pub(in crate::crew) fn crew_router_config_with_mappings(
        &self,
        project_id: &str,
    ) -> std::result::Result<RouterConfig, CrewError> {
        let mut config = self.crew_router_config();
        let saved = self.store.crew_acp_tiers().map_err(internal)?;
        if saved.revision > 0 {
            config.acp_tiers = saved.assignments;
        }
        config.worker_provider = self
            .store
            .crew_worker_provider(project_id)
            .map_err(internal)?;
        Ok(config)
    }

    /// `GET /api/crew/projects/{id}/worker-provider`: the project's stored choice, `same_as_lead` when none.
    pub fn crew_worker_provider_view(
        &self,
        project_id: &str,
    ) -> Result<orbyn_contracts::CrewWorkerProviderView> {
        self.crew_preview_only()?;
        crate::crew::valid_id(project_id)?;
        self.store.project(project_id)?;
        Ok(orbyn_contracts::CrewWorkerProviderView {
            project: project_id.to_owned(),
            worker_provider: self.store.crew_worker_provider(project_id)?,
        })
    }

    /// `POST /api/crew/projects/{id}/worker-provider/set`: a person's choice for one project, audited. It changes where work
    /// runs, so no model tool reaches it. The next start or dry run reads it; a task already routed keeps its engine.
    pub async fn crew_set_worker_provider(
        &self,
        project_id: &str,
        change: &orbyn_contracts::CrewWorkerProviderSet,
    ) -> Result<orbyn_contracts::CrewWorkerProviderView> {
        self.crew_preview_only()?;
        crate::crew::valid_id(project_id)?;
        self.store.project(project_id)?;
        // A start reads the setting while it holds this lock, so a save never lands between routing and its record.
        let _held = self.crew_starts.lock().await;
        self.store
            .set_crew_worker_provider(project_id, change.worker_provider, "user")?;
        drop(_held);
        self.crew_worker_provider_view(project_id)
    }

    pub fn crew_engine_tiers(&self) -> Result<Value> {
        self.crew_preview_only()?;
        let saved = self.store.crew_acp_tiers()?;
        let assignments = if saved.revision == 0 {
            self.crew_router_config().acp_tiers
        } else {
            saved.assignments
        };
        let shown = serde_json::to_value(CrewAcpTierSettings {
            revision: saved.revision,
            assignments,
        })?;
        refuse_secret(&shown).map_err(|e| CoreError::Invalid(e.message))?;
        Ok(shown)
    }

    /// Whole-map replacement with a revision, through a local-person route.
    /// A model expectation is only a precondition: core resolves the unique
    /// current default independently. Untouched rows retain their saved pins in
    /// the request until the person explicitly resolves any default drift.
    pub async fn crew_set_engine_tiers(&self, body: &Value) -> Result<Value> {
        self.crew_preview_only()?;
        refuse_secret(body).map_err(|e| CoreError::Invalid(e.message))?;
        let change: CrewAcpTierSave = serde_json::from_value(body.clone()).map_err(|_| {
            CoreError::Invalid(
                "give seen and assignments with engine, tier and expected_default_model".into(),
            )
        })?;
        let seen = change.seen;
        if seen < 0 || change.assignments.len() > 200 {
            return Err(CoreError::Invalid(
                "seen is nonnegative and assignments has at most 200 entries".into(),
            ));
        }
        let _held = self.crew_starts.lock().await;
        let machine = self.crew_machine(None).await?;
        let mut assignments = BTreeMap::new();
        for entry in &change.assignments {
            let engine = &entry.engine;
            let tier = entry.tier;
            let view = machine
                .views
                .iter()
                .find(|v| &v.id == engine)
                .ok_or_else(|| {
                    CoreError::Invalid("an assignment names an unconfigured engine".into())
                })?;
            let assignment = resolve_acp_assignment(view, tier)?;
            if assignment.model != entry.expected_default_model {
                return Err(CoreError::Invalid(
                    "the engine's default model changed; review the current default before saving"
                        .into(),
                ));
            }
            if assignments
                .insert(engine.as_str().to_owned(), assignment)
                .is_some()
            {
                return Err(CoreError::Invalid("give each engine once".into()));
            }
        }
        refuse_secret(&json!({"assignments":assignments}))
            .map_err(|e| CoreError::Invalid(e.message))?;
        let saved = self.store.save_crew_acp_tiers(seen, &assignments)?;
        Ok(serde_json::to_value(CrewAcpTierSettings {
            revision: saved.revision,
            assignments: saved.assignments,
        })?)
    }
}
