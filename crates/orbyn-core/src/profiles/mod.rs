//! Execution profiles (docs/execution-profiles-and-library.md, Part 1): named presets of engine, model tier, permission mode and
//! Autopilot mode, picked per thread or per crew. Built-ins ship in code. A saved profile lives in SQLite. Applying one copies its
//! values; nothing links the thread or crew to it afterwards. This module owns the shapes, the validation and the reads and writes
//! of profiles; `apply` owns what applying one does.

mod apply;
mod machine;
#[cfg(test)]
mod tests;

use orbyn_contracts::{
    ApplyProfileAnswer, AutopilotMode, PermissionMode, Profile, ProfileCheck, ProfileDraft,
    ProfileGrant, ProfileTier, ProfileWarning, UpdateProfile,
};

use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::store::{ProfileRow, ProfileSettings};
pub(crate) use machine::Machine;

const NAME_MAX: usize = 60;
const DESCRIPTION_MAX: usize = 200;
const ENGINE_MAX: usize = 80;
/// What a built-in's timestamps say: they ship in code and never change.
const BUILTIN_STAMP: &str = "1970-01-01T00:00:00Z";

/// The built-ins, listed first. There is no Hands-off and no other full-access preset.
fn builtins() -> Vec<ProfileRow> {
    let make = |slug: &str, name: &str, description: &str, tier, mode, autopilot| ProfileRow {
        id: format!("builtin:{slug}"),
        settings: ProfileSettings {
            name: name.to_owned(),
            description: description.to_owned(),
            engine: None,
            model_tier: tier,
            permission_mode: mode,
            autopilot_mode: autopilot,
        },
        revision: 1,
        created_at: BUILTIN_STAMP.to_owned(),
        updated_at: BUILTIN_STAMP.to_owned(),
    };
    vec![
        make(
            "careful",
            "Careful",
            "Asks before commands and edits. Balanced model.",
            ProfileTier::Builder,
            PermissionMode::Supervised,
            AutopilotMode::Off,
        ),
        make(
            "fast",
            "Fast",
            "Accepts edits inside the checkout and turns Autopilot on. Balanced model.",
            ProfileTier::Builder,
            PermissionMode::AutoEdits,
            AutopilotMode::On,
        ),
        make(
            "cheap",
            "Cheap",
            "Asks before commands and edits. Smallest model.",
            ProfileTier::Scout,
            PermissionMode::Supervised,
            AutopilotMode::Off,
        ),
    ]
}

fn is_builtin(id: &str) -> bool {
    id.starts_with("builtin:")
}

/// The grants a profile carries: what its confirm lists, in a fixed order.
pub(crate) fn grants_of(settings: &ProfileSettings) -> Vec<ProfileGrant> {
    let mut grants = Vec::new();
    if settings.permission_mode == PermissionMode::FullAccess {
        grants.push(ProfileGrant::FullAccess);
    }
    if settings.autopilot_mode == AutopilotMode::Full {
        grants.push(ProfileGrant::AutopilotFull);
    }
    grants
}

fn invalid(field: &str, rule: &str) -> CoreError {
    CoreError::Invalid(format!("{field}: {rule}"))
}

fn builtin_refused() -> CoreError {
    CoreError::Refused {
        status: 403,
        code: "builtin_profile",
        message: "A built-in profile cannot be changed or deleted. Duplicate it to change it."
            .into(),
        detail: serde_json::Value::Null,
    }
}

fn name_taken() -> CoreError {
    CoreError::Refused {
        status: 409,
        code: "name_taken",
        message: "A profile with this name already exists.".into(),
        detail: serde_json::json!({ "field": "name" }),
    }
}

/// A name or description as it is stored, or the rule it broke. The value is never put in the answer.
pub(crate) fn clean(
    field: &str,
    text: &str,
    max: usize,
    min: usize,
    newline_ok: bool,
) -> Result<String> {
    let text = text.trim();
    let count = text.chars().count();
    if count < min || count > max {
        return Err(invalid(field, &format!("use {min} to {max} characters.")));
    }
    // Control characters and the invisible ones that spoof a name (zero width, bidirectional marks and overrides).
    let invisible = |c: char| matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}' | '\u{feff}');
    if text
        .chars()
        .any(|c| (c.is_control() && !(newline_ok && c == '\n')) || invisible(c))
    {
        return Err(invalid(field, "control characters are not allowed."));
    }
    Ok(text.to_owned())
}

fn clean_engine(engine: &str) -> Result<String> {
    let engine = engine.trim();
    let fine = !engine.is_empty()
        && engine.len() <= ENGINE_MAX
        && engine
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'));
    if fine {
        Ok(engine.to_owned())
    } else {
        Err(invalid("engine", "name an engine id, or leave it empty."))
    }
}

impl Core {
    /// Validates what the person wrote. The mode is checked against the engine only when one is named.
    fn profile_settings(&self, draft: Settings<'_>, machine: &Machine) -> Result<ProfileSettings> {
        let name = clean("name", draft.name, NAME_MAX, 1, false)?;
        let description = clean("description", draft.description, DESCRIPTION_MAX, 0, true)?;
        let engine = draft.engine.map(clean_engine).transpose()?;
        if let Some(engine) = &engine
            && !machine.modes(engine).contains(&draft.permission_mode)
        {
            return Err(invalid(
                "permission_mode",
                &format!("{engine} does not offer this permission mode."),
            ));
        }
        let key = |text: &str| text.trim().to_lowercase();
        if builtins()
            .iter()
            .any(|one| key(&one.settings.name) == key(&name))
        {
            return Err(name_taken());
        }
        Ok(ProfileSettings {
            name,
            description,
            engine,
            model_tier: draft.model_tier,
            permission_mode: draft.permission_mode,
            autopilot_mode: draft.autopilot_mode,
        })
    }

    /// Notes that never block a save: an engine this machine cannot use now, a tier it has no model for.
    fn profile_warnings(settings: &ProfileSettings, machine: &Machine) -> Vec<ProfileWarning> {
        let Some(engine) = &settings.engine else {
            return Vec::new();
        };
        let mut warnings = Vec::new();
        let availability = machine.availability(Some(engine));
        if availability.blocked() {
            warnings.push(ProfileWarning {
                field: "engine".into(),
                code: "engine_unavailable".into(),
                message: format!(
                    "{engine} is not usable on this computer now: {}",
                    availability.reason.clone().unwrap_or_default()
                ),
            });
        } else if machine.tier_model(engine, settings.model_tier).is_err() {
            warnings.push(ProfileWarning {
                field: "model_tier".into(),
                code: "tier_unmapped".into(),
                message: format!("{engine} has no model for this tier now."),
            });
        }
        warnings
    }

    fn profile_wire(
        row: &ProfileRow,
        machine: &Machine,
        warnings: Option<Vec<ProfileWarning>>,
    ) -> Profile {
        Profile {
            id: row.id.clone(),
            name: row.settings.name.clone(),
            description: row.settings.description.clone(),
            engine: row.settings.engine.clone(),
            model_tier: row.settings.model_tier,
            permission_mode: row.settings.permission_mode,
            autopilot_mode: row.settings.autopilot_mode,
            builtin: is_builtin(&row.id),
            revision: row.revision,
            created_at: row.created_at.clone(),
            updated_at: row.updated_at.clone(),
            availability: machine.availability(row.settings.engine.as_deref()).wire(),
            warnings,
        }
    }

    fn profile_row(&self, id: &str) -> Result<ProfileRow> {
        if is_builtin(id) {
            return builtins()
                .into_iter()
                .find(|one| one.id == id)
                .ok_or_else(|| CoreError::NotFound("profile".into()));
        }
        self.store.execution_profile(id)
    }

    /// `GET /api/profiles`: built-ins first, then the saved ones, each with its availability.
    pub async fn list_profiles(&self) -> Result<Vec<Profile>> {
        let saved = self.store.execution_profiles()?;
        let engines: Vec<String> = saved
            .iter()
            .filter_map(|row| row.settings.engine.clone())
            .collect();
        let machine = self.profile_machine(&engines, false).await?;
        Ok(builtins()
            .iter()
            .chain(saved.iter())
            .map(|row| Self::profile_wire(row, &machine, None))
            .collect())
    }

    /// `GET /api/profiles/{id}`: one profile with its warnings.
    pub async fn get_profile(&self, id: &str) -> Result<Profile> {
        let row = self.profile_row(id)?;
        let machine = self
            .profile_machine(row.settings.engine.as_slice(), true)
            .await?;
        let warnings = Self::profile_warnings(&row.settings, &machine);
        Ok(Self::profile_wire(&row, &machine, Some(warnings)))
    }

    /// `POST /api/profiles/check`: what saving this draft would warn about. Saves nothing.
    pub async fn check_profile(&self, draft: &ProfileDraft) -> Result<ProfileCheck> {
        let machine = self.profile_machine(draft.engine.as_slice(), true).await?;
        let settings = self.profile_settings(Settings::of(draft), &machine)?;
        Ok(ProfileCheck {
            warnings: Self::profile_warnings(&settings, &machine),
            availability: machine.availability(settings.engine.as_deref()).wire(),
        })
    }

    /// `POST /api/profiles` (Person).
    pub async fn create_profile(&self, draft: &ProfileDraft) -> Result<Profile> {
        let machine = self.profile_machine(draft.engine.as_slice(), true).await?;
        let settings = self.profile_settings(Settings::of(draft), &machine)?;
        let row = self.store.create_execution_profile(&settings)?;
        let warnings = Self::profile_warnings(&row.settings, &machine);
        Ok(Self::profile_wire(&row, &machine, Some(warnings)))
    }

    /// `PATCH /api/profiles/{id}` (Person). A stale `seen_revision` changes nothing.
    pub async fn update_profile(&self, id: &str, patch: &UpdateProfile) -> Result<Profile> {
        if is_builtin(id) {
            self.profile_row(id)?;
            return Err(builtin_refused());
        }
        let current = self.store.execution_profile(id)?;
        if current.revision != patch.seen_revision {
            return Err(CoreError::Refused {
                status: 409,
                code: "revision_conflict",
                message: "This profile changed. Review it again before saving.".into(),
                detail: serde_json::json!({ "revision": current.revision }),
            });
        }
        let was = &current.settings;
        let engine = patch.engine.clone().unwrap_or_else(|| was.engine.clone());
        let machine = self.profile_machine(engine.as_slice(), true).await?;
        let settings = self.profile_settings(
            Settings {
                name: patch.name.as_deref().unwrap_or(&was.name),
                description: patch.description.as_deref().unwrap_or(&was.description),
                engine: engine.as_deref(),
                model_tier: patch.model_tier.unwrap_or(was.model_tier),
                permission_mode: patch.permission_mode.unwrap_or(was.permission_mode),
                autopilot_mode: patch.autopilot_mode.unwrap_or(was.autopilot_mode),
            },
            &machine,
        )?;
        let row = self
            .store
            .update_execution_profile(id, patch.seen_revision, &settings)?;
        let warnings = Self::profile_warnings(&row.settings, &machine);
        Ok(Self::profile_wire(&row, &machine, Some(warnings)))
    }

    /// `DELETE /api/profiles/{id}` (Person). Threads and crews that used it keep what they were given.
    pub fn delete_profile(&self, id: &str) -> Result<()> {
        if is_builtin(id) {
            self.profile_row(id)?;
            return Err(builtin_refused());
        }
        self.store.delete_execution_profile(id)
    }

    /// `PUT /api/threads/{id}/profile` (Person).
    pub async fn apply_profile_to_thread(
        &self,
        thread_id: &str,
        body: &orbyn_contracts::ApplyProfile,
    ) -> Result<ApplyProfileAnswer> {
        self.apply_thread(thread_id, body).await
    }

    /// `POST /api/crew/profile/apply` (Person).
    pub async fn apply_profile_to_crew(
        &self,
        body: &orbyn_contracts::ApplyCrewProfile,
    ) -> Result<ApplyProfileAnswer> {
        self.apply_crew(body).await
    }
}

/// The values a validation reads, borrowed from a draft or from a patch laid over the stored profile.
struct Settings<'a> {
    name: &'a str,
    description: &'a str,
    engine: Option<&'a str>,
    model_tier: ProfileTier,
    permission_mode: PermissionMode,
    autopilot_mode: AutopilotMode,
}

impl<'a> Settings<'a> {
    fn of(draft: &'a ProfileDraft) -> Self {
        Self {
            name: &draft.name,
            description: &draft.description,
            engine: draft.engine.as_deref(),
            model_tier: draft.model_tier,
            permission_mode: draft.permission_mode,
            autopilot_mode: draft.autopilot_mode,
        }
    }
}
