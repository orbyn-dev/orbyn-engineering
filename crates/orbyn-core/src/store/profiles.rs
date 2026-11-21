//! Execution profiles at rest (docs/execution-profiles-and-library.md, Part 1). A profile is four settings and a name.
//! An application row records what a thread or crew last got: it is a copy, with no link back to the profile, so
//! editing or deleting a profile never changes work that already used it. Every write adds its audit row first, in the
//! same transaction, with ids, names and grant codes only.

use orbyn_contracts::{
    AutopilotMode, ModelSelection, PermissionMode, ProfileApplication, ProfileGrant, ProfileTier,
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::{Value, json};

use super::threads::{has_active_run, notify};
use super::{AuditRecord, Store, insert_audit, parse_wire, wire_name};
use crate::error::{CoreError, Result};
use crate::util::{new_id, now};

/// At most this many saved profiles (built-ins are not counted).
pub const PROFILE_LIMIT: usize = 50;

/// The four settings, a name and a description: what a create writes and what a change leaves behind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileSettings {
    pub name: String,
    pub description: String,
    pub engine: Option<String>,
    pub model_tier: ProfileTier,
    pub permission_mode: PermissionMode,
    pub autopilot_mode: AutopilotMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileRow {
    pub id: String,
    pub settings: ProfileSettings,
    pub revision: u32,
    pub created_at: String,
    pub updated_at: String,
}

/// An engine switch an apply makes on a thread, built by core the way `switch_engine` builds one.
pub struct EngineSwitch {
    pub engine: String,
    pub selection: ModelSelection,
    pub handoff: String,
}

/// What an apply to a thread writes, all in one transaction.
pub struct ThreadApply<'a> {
    pub thread_id: &'a str,
    pub profile: &'a ProfileRow,
    /// The engine changes: the selection replaces the thread's.
    pub switch: Option<EngineSwitch>,
    /// With no engine switch, the model the tier resolved to, when it did.
    pub model: Option<String>,
    pub permission_mode: PermissionMode,
    /// The grant codes the profile carries, all of them acknowledged by the person.
    pub grants: Vec<ProfileGrant>,
    pub applied: Value,
}

pub struct CrewApply<'a> {
    pub crew_id: &'a str,
    pub profile: &'a ProfileRow,
    pub grants: Vec<ProfileGrant>,
    pub applied: Value,
}

fn refused(status: u16, code: &'static str, message: &str, detail: Value) -> CoreError {
    CoreError::Refused {
        status,
        code,
        message: message.to_owned(),
        detail,
    }
}

/// The key a name is unique by: trimmed and lowercased.
pub fn name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProfileRow> {
    Ok(ProfileRow {
        id: row.get("id")?,
        settings: ProfileSettings {
            name: row.get("name")?,
            description: row.get("description")?,
            engine: row.get("engine")?,
            model_tier: parse_wire(0, row.get("model_tier")?)?,
            permission_mode: parse_wire(0, row.get("permission_mode")?)?,
            autopilot_mode: parse_wire(0, row.get("autopilot_mode")?)?,
        },
        revision: u32::try_from(row.get::<_, i64>("revision")?).unwrap_or(u32::MAX),
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

const COLUMNS: &str = "id, name, description, engine, model_tier, permission_mode, autopilot_mode, revision, created_at, updated_at";

fn load(tx: &Transaction<'_>, id: &str) -> Result<ProfileRow> {
    tx.query_row(
        &format!("SELECT {COLUMNS} FROM profiles WHERE id = ?1"),
        [id],
        read,
    )
    .optional()?
    .ok_or_else(|| CoreError::NotFound("profile".into()))
}

fn name_taken(tx: &Transaction<'_>, key: &str, except: Option<&str>) -> Result<bool> {
    Ok(tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM profiles WHERE name_key = ?1 AND id <> ?2)",
        params![key, except.unwrap_or("")],
        |row| row.get(0),
    )?)
}

fn audit(
    tx: &Transaction<'_>,
    action: &str,
    target: &str,
    thread: Option<&str>,
    detail: Value,
) -> Result<()> {
    insert_audit(
        tx,
        &AuditRecord {
            actor: "user".into(),
            action: action.to_owned(),
            target: target.to_owned(),
            decision: None,
            source: "user".into(),
            thread_id: thread.map(str::to_owned),
            run_id: None,
            detail,
        },
    )?;
    Ok(())
}

fn grant_codes(grants: &[ProfileGrant]) -> Result<Vec<String>> {
    grants.iter().map(|grant| wire_name(*grant)).collect()
}

fn upsert_application(
    tx: &Transaction<'_>,
    kind: &str,
    scope_id: &str,
    profile: &ProfileRow,
    applied: &Value,
    stamp: &str,
) -> Result<()> {
    tx.execute(
        "INSERT INTO profile_applications
            (scope_kind, scope_id, profile_id, name_at_apply, revision_at_apply, customized, applied_json, applied_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7)
         ON CONFLICT(scope_kind, scope_id) DO UPDATE SET
            profile_id = ?3, name_at_apply = ?4, revision_at_apply = ?5, customized = 0, applied_json = ?6, applied_at = ?7",
        params![
            kind,
            scope_id,
            profile.id,
            profile.settings.name,
            i64::from(profile.revision),
            applied.to_string(),
            stamp
        ],
    )?;
    Ok(())
}

/// A person's own change to a thread's model or permission mode after an apply: the chip then reads "edited".
pub(super) fn mark_customized(tx: &Transaction<'_>, thread_id: &str) -> Result<()> {
    tx.execute(
        "UPDATE profile_applications SET customized = 1 WHERE scope_kind = 'thread' AND scope_id = ?1",
        [thread_id],
    )?;
    Ok(())
}

/// A deleted thread or crew keeps no application row.
pub(super) fn forget_applications(tx: &Transaction<'_>, scope_ids: &[String]) -> Result<()> {
    for id in scope_ids {
        tx.execute("DELETE FROM profile_applications WHERE scope_id = ?1", [id])?;
    }
    Ok(())
}

impl Store {
    /// Saved profiles, oldest first. Built-ins are not here.
    pub fn execution_profiles(&self) -> Result<Vec<ProfileRow>> {
        let reader = self.reader();
        let mut statement = reader.prepare(&format!(
            "SELECT {COLUMNS} FROM profiles ORDER BY created_at, id"
        ))?;
        let rows = statement.query_map([], read)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn execution_profile(&self, id: &str) -> Result<ProfileRow> {
        self.reader()
            .query_row(
                &format!("SELECT {COLUMNS} FROM profiles WHERE id = ?1"),
                [id],
                read,
            )
            .optional()?
            .ok_or_else(|| CoreError::NotFound("profile".into()))
    }

    /// Creates a profile, refusing a 51st and a repeated name in the same transaction as the insert.
    pub fn create_execution_profile(&self, settings: &ProfileSettings) -> Result<ProfileRow> {
        self.commit(|tx, _| {
            let count: i64 = tx.query_row("SELECT COUNT(*) FROM profiles", [], |row| row.get(0))?;
            if usize::try_from(count).unwrap_or(usize::MAX) >= PROFILE_LIMIT {
                return Err(refused(
                    409,
                    "profile_limit",
                    "You have saved the most profiles Orbyn keeps. Delete one first.",
                    json!({ "limit": PROFILE_LIMIT }),
                ));
            }
            let key = name_key(&settings.name);
            if name_taken(tx, &key, None)? {
                return Err(refused(
                    409,
                    "name_taken",
                    "A profile with this name already exists.",
                    json!({ "field": "name" }),
                ));
            }
            let id = new_id();
            let stamp = now();
            audit(
                tx,
                "profile.created",
                &id,
                None,
                json!({ "id": id, "name": settings.name }),
            )?;
            tx.execute(
                "INSERT INTO profiles (id, name, name_key, description, engine, model_tier, permission_mode,
                     autopilot_mode, revision, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, ?9, ?9)",
                params![
                    id,
                    settings.name,
                    key,
                    settings.description,
                    settings.engine,
                    wire_name(settings.model_tier)?,
                    wire_name(settings.permission_mode)?,
                    wire_name(settings.autopilot_mode)?,
                    stamp
                ],
            )?;
            load(tx, &id)
        })
    }

    /// Replaces a profile's settings when `seen` is its current revision, and counts the change.
    pub fn update_execution_profile(
        &self,
        id: &str,
        seen: u32,
        settings: &ProfileSettings,
    ) -> Result<ProfileRow> {
        self.commit(|tx, _| {
            let current = load(tx, id)?;
            if current.revision != seen {
                return Err(refused(
                    409,
                    "revision_conflict",
                    "This profile changed. Review it again before saving.",
                    json!({ "revision": current.revision }),
                ));
            }
            let key = name_key(&settings.name);
            if name_taken(tx, &key, Some(id))? {
                return Err(refused(
                    409,
                    "name_taken",
                    "A profile with this name already exists.",
                    json!({ "field": "name" }),
                ));
            }
            let revision = current.revision.saturating_add(1);
            audit(
                tx,
                "profile.changed",
                id,
                None,
                json!({ "id": id, "name": settings.name, "revision": revision }),
            )?;
            tx.execute(
                "UPDATE profiles SET name = ?2, name_key = ?3, description = ?4, engine = ?5, model_tier = ?6,
                     permission_mode = ?7, autopilot_mode = ?8, revision = ?9, updated_at = ?10
                 WHERE id = ?1",
                params![
                    id,
                    settings.name,
                    key,
                    settings.description,
                    settings.engine,
                    wire_name(settings.model_tier)?,
                    wire_name(settings.permission_mode)?,
                    wire_name(settings.autopilot_mode)?,
                    i64::from(revision),
                    now()
                ],
            )?;
            load(tx, id)
        })
    }

    /// Deletes a profile. Application rows stay: the chip shows the stored name with "(removed)".
    pub fn delete_execution_profile(&self, id: &str) -> Result<()> {
        self.commit(|tx, _| {
            let current = load(tx, id)?;
            audit(
                tx,
                "profile.deleted",
                id,
                None,
                json!({ "id": id, "name": current.settings.name }),
            )?;
            tx.execute("DELETE FROM profiles WHERE id = ?1", [id])?;
            Ok(())
        })
    }

    /// What a thread or crew last got, `None` when nothing was applied or it was cleared.
    pub fn profile_application(
        &self,
        kind: &str,
        scope_id: &str,
    ) -> Result<Option<ProfileApplication>> {
        Ok(self
            .reader()
            .query_row(
                "SELECT profile_id, name_at_apply, revision_at_apply, customized, applied_at
                 FROM profile_applications WHERE scope_kind = ?1 AND scope_id = ?2",
                params![kind, scope_id],
                |row| {
                    Ok(ProfileApplication {
                        profile_id: row.get(0)?,
                        name: row.get(1)?,
                        revision: u32::try_from(row.get::<_, i64>(2)?).unwrap_or(u32::MAX),
                        customized: row.get(3)?,
                        applied_at: row.get(4)?,
                    })
                },
            )
            .optional()?)
    }

    /// The values an application copied, as stored.
    pub fn profile_application_values(&self, kind: &str, scope_id: &str) -> Result<Option<Value>> {
        let text: Option<String> = self
            .reader()
            .query_row(
                "SELECT applied_json FROM profile_applications WHERE scope_kind = ?1 AND scope_id = ?2",
                params![kind, scope_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(text.and_then(|text| serde_json::from_str(&text).ok()))
    }

    /// Writes the thread's own engine, model and permission mode once, records the application and the audit row.
    /// Nothing links the thread to the profile afterwards.
    pub fn apply_profile_to_thread(&self, apply: &ThreadApply<'_>) -> Result<ProfileApplication> {
        let mode = wire_name(apply.permission_mode)?;
        let grants = grant_codes(&apply.grants)?;
        self.commit(|tx, appended| {
            let exists: bool = tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM threads WHERE id = ?1)",
                [apply.thread_id],
                |row| row.get(0),
            )?;
            if !exists {
                return Err(CoreError::NotFound("thread".into()));
            }
            if apply.switch.is_some() && has_active_run(tx, apply.thread_id)? {
                return Err(refused(
                    409,
                    "run_active",
                    "This thread is running. Wait for the run to end before changing its engine.",
                    Value::Null,
                ));
            }
            let stamp = now();
            audit(
                tx,
                "profile.applied",
                apply.thread_id,
                Some(apply.thread_id),
                json!({
                    "profile_id": apply.profile.id,
                    "name": apply.profile.settings.name,
                    "revision": apply.profile.revision,
                    "scope": "thread",
                    "grants": grants,
                }),
            )?;
            if let Some(switch) = &apply.switch {
                let options = serde_json::to_string(&switch.selection.options)?;
                tx.execute(
                    "UPDATE threads SET engine = ?2, model = ?3, effort = ?4, options = ?5,
                        engine_session = NULL, handoff = ?6 WHERE id = ?1",
                    params![
                        apply.thread_id,
                        switch.engine,
                        switch.selection.model,
                        switch.selection.effort,
                        options,
                        switch.handoff
                    ],
                )?;
                notify(tx, appended, apply.thread_id, "engine")?;
                notify(tx, appended, apply.thread_id, "model")?;
            } else if let Some(model) = &apply.model {
                tx.execute(
                    "UPDATE threads SET model = ?2 WHERE id = ?1",
                    params![apply.thread_id, model],
                )?;
                notify(tx, appended, apply.thread_id, "model")?;
            }
            tx.execute(
                "UPDATE threads SET permission_mode = ?2 WHERE id = ?1",
                params![apply.thread_id, mode],
            )?;
            notify(tx, appended, apply.thread_id, "permission_mode")?;
            upsert_application(
                tx,
                "thread",
                apply.thread_id,
                apply.profile,
                &apply.applied,
                &stamp,
            )?;
            Ok(ProfileApplication {
                profile_id: apply.profile.id.clone(),
                name: apply.profile.settings.name.clone(),
                revision: apply.profile.revision,
                customized: false,
                applied_at: stamp,
            })
        })
    }

    /// Sets a crew's Autopilot mode, through the same table `POST /api/crew/autopilot/mode` writes, and records what the
    /// crew's engine, permission mode and builder default were copied as. Crews keep no engine or permission mode of
    /// their own yet, so those three live in the application row only.
    pub fn apply_profile_to_crew(&self, apply: &CrewApply<'_>) -> Result<ProfileApplication> {
        let grants = grant_codes(&apply.grants)?;
        let mode = wire_name(apply.profile.settings.autopilot_mode)?;
        self.commit(|tx, _| {
            let stamp = now();
            audit(
                tx,
                "profile.applied",
                apply.crew_id,
                None,
                json!({
                    "profile_id": apply.profile.id,
                    "name": apply.profile.settings.name,
                    "revision": apply.profile.revision,
                    "scope": "crew",
                    "crew_id": apply.crew_id,
                    "grants": grants,
                }),
            )?;
            tx.execute(
                "INSERT INTO crew_autopilot_modes (scope, mode, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(scope) DO UPDATE SET mode = ?2, updated_at = ?3",
                params![format!("crew:{}", apply.crew_id), mode, stamp],
            )?;
            upsert_application(
                tx,
                "crew",
                apply.crew_id,
                apply.profile,
                &apply.applied,
                &stamp,
            )?;
            Ok(ProfileApplication {
                profile_id: apply.profile.id.clone(),
                name: apply.profile.settings.name.clone(),
                revision: apply.profile.revision,
                customized: false,
                applied_at: stamp,
            })
        })
    }

    /// Forgets the chip of a thread and keeps its values. Audited, because it is a Person write.
    pub fn clear_thread_profile(&self, thread_id: &str) -> Result<()> {
        self.commit(|tx, _| {
            let exists: bool = tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM threads WHERE id = ?1)",
                [thread_id],
                |row| row.get(0),
            )?;
            if !exists {
                return Err(CoreError::NotFound("thread".into()));
            }
            audit(
                tx,
                "profile.cleared",
                thread_id,
                Some(thread_id),
                json!({ "scope": "thread" }),
            )?;
            tx.execute(
                "DELETE FROM profile_applications WHERE scope_kind = 'thread' AND scope_id = ?1",
                [thread_id],
            )?;
            Ok(())
        })
    }
}
