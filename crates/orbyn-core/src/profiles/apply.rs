//! Applying a profile to a thread or a crew. The values are copied once: nothing refers to the profile afterwards, so editing or
//! deleting it never touches work that already used it. A profile that grants full access or Autopilot Full applies only when the
//! request carries the grant codes the person saw; otherwise nothing is applied and the answer is `409 confirm_required`.

use orbyn_contracts::{
    AppliedProfileValues, ApplyCrewProfile, ApplyProfile, ApplyProfileAnswer, ModelSelection,
    ProfileGrant, Thread,
};
use serde_json::{Value, json};

use super::{Machine, grants_of};
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::store::{CrewApply, EngineSwitch, ProfileRow, ThreadApply};

fn refused(code: &'static str, message: String, detail: Value) -> CoreError {
    CoreError::Refused {
        status: 409,
        code,
        message,
        detail,
    }
}

/// A `seen_revision` that is present must be the profile's current one: the person sees what they are about to apply.
fn check_seen(profile: &ProfileRow, seen: Option<u32>) -> Result<()> {
    match seen {
        Some(seen) if seen != profile.revision => Err(refused(
            "revision_conflict",
            "This profile changed. Review it again before applying.".into(),
            json!({ "revision": profile.revision }),
        )),
        _ => Ok(()),
    }
}

/// The grants the profile carries that the person did not acknowledge, or `confirm_required` listing them.
fn check_grants(profile: &ProfileRow, ack: &[ProfileGrant]) -> Result<Vec<ProfileGrant>> {
    let carried = grants_of(&profile.settings);
    let missing: Vec<&ProfileGrant> = carried
        .iter()
        .filter(|grant| !ack.contains(grant))
        .collect();
    if missing.is_empty() {
        return Ok(carried);
    }
    Err(refused(
        "confirm_required",
        "Confirm what this profile grants before applying it.".into(),
        json!({ "grants": missing }),
    ))
}

/// Why the profile's engine, mode or tier does not fit this machine now. Nothing is swapped for the nearest value.
fn fit(profile: &ProfileRow, engine: Option<&str>, machine: &Machine) -> Result<Option<String>> {
    if profile.settings.engine.is_some() {
        let availability = machine.availability(engine);
        if availability.blocked() {
            let reason = availability.reason.unwrap_or_default();
            return Err(refused(
                "engine_unavailable",
                format!(
                    "{} is not usable on this computer: {reason}. Orbyn does not fall back to another engine.",
                    engine.unwrap_or_default()
                ),
                json!({ "engine": engine, "reason": reason }),
            ));
        }
    }
    let Some(engine) = engine else {
        return Ok(None);
    };
    if !machine
        .modes(engine)
        .contains(&profile.settings.permission_mode)
    {
        return Err(refused(
            "mode_not_offered",
            format!("{engine} does not offer this permission mode."),
            json!({ "engine": engine }),
        ));
    }
    machine
        .tier_model(engine, profile.settings.model_tier)
        .map(Some)
        .map_err(|()| {
            refused(
                "tier_unmapped",
                format!("{engine} has no model for this tier."),
                json!({ "engine": engine }),
            )
        })
}

impl Core {
    pub(super) async fn apply_thread(
        &self,
        thread_id: &str,
        body: &ApplyProfile,
    ) -> Result<ApplyProfileAnswer> {
        let thread = self.store.thread(thread_id)?;
        let Some(profile_id) = &body.profile_id else {
            self.store.clear_thread_profile(thread_id)?;
            return Ok(ApplyProfileAnswer {
                applied: AppliedProfileValues {
                    engine: None,
                    model: None,
                    permission_mode: None,
                    autopilot_mode: None,
                },
                ignored: Vec::new(),
                application: None,
            });
        };
        let profile = self.profile_row(profile_id)?;
        check_seen(&profile, body.seen_revision)?;
        let grants = check_grants(&profile, &body.ack_grants)?;
        let mut wanted = vec![thread.engine.clone()];
        wanted.extend(profile.settings.engine.clone());
        let machine = self
            .profile_machine_in(&wanted, true, Some(&thread.project_id))
            .await?;
        self.apply_thread_with(&thread, &profile, grants, &machine)
    }

    /// The apply, decided on one snapshot of the machine. `grants` are the profile's own, already acknowledged.
    pub(crate) fn apply_thread_with(
        &self,
        thread: &Thread,
        profile: &ProfileRow,
        grants: Vec<ProfileGrant>,
        machine: &Machine,
    ) -> Result<ApplyProfileAnswer> {
        let engine = profile.settings.engine.as_deref().unwrap_or(&thread.engine);
        let model = fit(profile, Some(engine), machine)?;
        let switches = engine != thread.engine;
        // A different engine takes what was asked and what it last used, never the old engine's values.
        let switch = if switches {
            let chosen = crate::agents::selection::merged(
                &self.store.remembered_selection(engine)?,
                &ModelSelection {
                    model: model.clone(),
                    effort: None,
                    options: Default::default(),
                },
            );
            let through = self.store.latest_seq(&thread.id)?;
            Some(EngineSwitch {
                engine: engine.to_owned(),
                selection: chosen,
                handoff: self.store.handoff_text(&thread.id, through)?,
            })
        } else {
            None
        };
        let applied = AppliedProfileValues {
            engine: Some(engine.to_owned()),
            model: model.clone(),
            permission_mode: Some(profile.settings.permission_mode),
            autopilot_mode: None,
        };
        let application = self.store.apply_profile_to_thread(&ThreadApply {
            thread_id: &thread.id,
            profile,
            switch,
            model,
            permission_mode: profile.settings.permission_mode,
            grants,
            applied: serde_json::to_value(&applied)?,
        })?;
        if switches {
            self.store
                .note_thread_event(&thread.id, "engine.switched", json!({ "to": engine }))?;
        }
        Ok(ApplyProfileAnswer {
            applied,
            // There is no per-thread Autopilot: the mode is stored on the profile and shown as "applies to crews only".
            ignored: vec!["autopilot_mode".to_owned()],
            application: Some(application),
        })
    }

    pub(super) async fn apply_crew(&self, body: &ApplyCrewProfile) -> Result<ApplyProfileAnswer> {
        if !self.config.crew_preview {
            return Err(CoreError::NotFound("route".into()));
        }
        let profile = self.profile_row(&body.profile_id)?;
        check_seen(&profile, body.seen_revision)?;
        let grants = check_grants(&profile, &body.ack_grants)?;
        self.crew_mode_target(&body.crew_id)?;
        let machine = self
            .profile_machine(profile.settings.engine.as_slice(), true)
            .await?;
        self.apply_crew_with(&body.crew_id, &profile, grants, &machine)
    }

    /// The crew's engine, permission mode and Autopilot mode, and the model its tier means as the builder default. The router still
    /// picks the model per brief.
    pub(crate) fn apply_crew_with(
        &self,
        crew_id: &str,
        profile: &ProfileRow,
        grants: Vec<ProfileGrant>,
        machine: &Machine,
    ) -> Result<ApplyProfileAnswer> {
        let model = fit(profile, profile.settings.engine.as_deref(), machine)?;
        let applied = AppliedProfileValues {
            engine: profile.settings.engine.clone(),
            model,
            permission_mode: Some(profile.settings.permission_mode),
            autopilot_mode: Some(profile.settings.autopilot_mode),
        };
        let application = self.store.apply_profile_to_crew(&CrewApply {
            crew_id,
            profile,
            grants,
            applied: serde_json::to_value(&applied)?,
        })?;
        Ok(ApplyProfileAnswer {
            applied,
            ignored: Vec::new(),
            application: Some(application),
        })
    }
}
