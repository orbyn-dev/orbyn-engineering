//! The personal library (docs/execution-profiles-and-library.md, Part 2): workflows, prompts and environment sets, all on this
//! device. This module owns the validation and the reads and writes; `params` owns placeholders and rendering, `suggest` reads a
//! repository's committed `.orbyn/` files, `exchange` imports and exports, and `run` starts a workflow in a terminal.
//!
//! An environment value is a secret. It enters only through [`Core::set_env_key`], goes to the keychain and nowhere else, and leaves
//! only as a spawn-time value for one workflow run. No type here that a route answers can hold one.

mod exchange;
pub(crate) mod params;
mod run;
mod suggest;
#[cfg(test)]
mod tests;

use orbyn_contracts::{
    EnvKey, EnvKeychain, EnvSet, EnvSetDraft, EnvSetStatus, LibraryLimits, LibrarySource,
    LibrarySummary, Param, Prompt, PromptDraft, RenderValues, RenderedPrompt, RenderedWorkflow,
    UpdateEnvSet, UpdatePrompt, UpdateWorkflow, Workflow, WorkflowDraft,
};

use crate::app::Core;
use crate::credentials::{EnvError, KeyProblem, MAX_VALUE_BYTES, check_key_name};
use crate::error::{CoreError, Result};
use crate::profiles::clean;
use crate::store::{
    EnvSetFields, EnvSetRow, MAX_ENV_SETS, MAX_PROMPTS, MAX_WORKFLOWS, PromptFields, PromptRow,
    WorkflowFields, WorkflowRow,
};

pub use params::Shell;

pub const NAME_MAX: usize = 60;
pub const DESCRIPTION_MAX: usize = 200;
pub const COMMAND_MAX: usize = 4000;
pub const BODY_MAX: usize = 32 * 1024;
pub const ENV_KEYS_PER_SET: usize = 64;
pub const ENV_SETS_PER_WORKFLOW: usize = 4;
/// One committed `.orbyn/` file.
pub const SUGGESTIONS_FILE_BYTES: usize = 64 * 1024;
/// An import document.
pub const IMPORT_BYTES: usize = 2 * 1024 * 1024;

/// The numbers of "Limits", as `GET /api/library` answers them.
pub fn limits() -> LibraryLimits {
    let n = |value: usize| u32::try_from(value).unwrap_or(u32::MAX);
    LibraryLimits {
        workflows: n(MAX_WORKFLOWS),
        prompts: n(MAX_PROMPTS),
        env_sets: n(MAX_ENV_SETS),
        env_keys_per_set: n(ENV_KEYS_PER_SET),
        env_sets_per_workflow: n(ENV_SETS_PER_WORKFLOW),
        params_per_item: n(params::PARAMS_PER_ITEM),
        workflow_command_bytes: n(COMMAND_MAX),
        prompt_body_bytes: n(BODY_MAX),
        env_value_bytes: n(MAX_VALUE_BYTES),
        suggestions_file_bytes: n(SUGGESTIONS_FILE_BYTES),
        import_bytes: n(IMPORT_BYTES),
    }
}

fn refused(status: u16, code: &'static str, message: &str) -> CoreError {
    CoreError::Refused {
        status,
        code,
        message: message.to_owned(),
        detail: serde_json::Value::Null,
    }
}

/// A keychain failure as the person is told it. Never a value.
pub(crate) fn keychain_error(error: EnvError) -> CoreError {
    match error {
        EnvError::InvalidValue => CoreError::Invalid(format!(
            "value: use 1 to {MAX_VALUE_BYTES} bytes, with no NUL."
        )),
        EnvError::Unavailable => refused(
            503,
            "keychain_unavailable",
            "There is no usable keychain on this computer yet, so values cannot be saved.",
        ),
        EnvError::Blocked => refused(
            503,
            "keychain_blocked",
            "The keychain refused for now (locked, or a prompt was cancelled). Try again.",
        ),
        EnvError::Failed => refused(
            502,
            "keychain_failed",
            "The keychain could not finish. Try again.",
        ),
    }
}

/// Text a command or a prompt body may hold: anything but NUL and control characters other than line breaks and tabs.
fn text_ok(text: &str) -> bool {
    !text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
}

pub(crate) fn workflow_fields(
    name: &str,
    description: &str,
    command: &str,
    params: &[Param],
    env_set_ids: &[String],
) -> Result<WorkflowFields> {
    let name = clean("name", name, NAME_MAX, 1, false)?;
    let description = clean("description", description, DESCRIPTION_MAX, 0, true)?;
    if command.trim().is_empty() || command.len() > COMMAND_MAX || !text_ok(command) {
        return Err(CoreError::Invalid(format!(
            "command: use 1 to {COMMAND_MAX} bytes of text."
        )));
    }
    let params = params::clean_params(params)?;
    params::check_template("command", command, &params, true)?;
    let mut ids: Vec<String> = Vec::new();
    for id in env_set_ids {
        if !ids.contains(id) {
            ids.push(id.clone());
        }
    }
    if ids.len() > ENV_SETS_PER_WORKFLOW {
        return Err(CoreError::Invalid(format!(
            "env_set_ids: list at most {ENV_SETS_PER_WORKFLOW} sets."
        )));
    }
    Ok(WorkflowFields {
        name,
        description,
        command: command.to_owned(),
        params,
        env_set_ids: ids,
    })
}

pub(crate) fn prompt_fields(
    name: &str,
    description: &str,
    body: &str,
    params: &[Param],
) -> Result<PromptFields> {
    let name = clean("name", name, NAME_MAX, 1, false)?;
    let description = clean("description", description, DESCRIPTION_MAX, 0, true)?;
    if body.trim().is_empty() || body.len() > BODY_MAX || !text_ok(body) {
        return Err(CoreError::Invalid(format!(
            "body: use 1 to {BODY_MAX} bytes of text."
        )));
    }
    let params = params::clean_params(params)?;
    params::check_template("body", body, &params, false)?;
    Ok(PromptFields {
        name,
        description,
        body: body.to_owned(),
        params,
    })
}

/// Key names for a set: each shaped and allowed, none twice, at most 64. A refused or repeated name is `409 key_refused`.
pub(crate) fn clean_keys(keys: &[String]) -> Result<Vec<String>> {
    if keys.len() > ENV_KEYS_PER_SET {
        return Err(refused(
            409,
            "library_limit",
            &format!("A set holds at most {ENV_KEYS_PER_SET} keys."),
        ));
    }
    let mut out: Vec<String> = Vec::with_capacity(keys.len());
    for key in keys {
        match check_key_name(key) {
            Ok(()) => {}
            Err(KeyProblem::Malformed) => {
                return Err(CoreError::Invalid(
                    "keys: use capital letters, digits and underscores, starting with a letter or underscore, at most 128.".into(),
                ));
            }
            Err(KeyProblem::Refused) => {
                return Err(refused(
                    409,
                    "key_refused",
                    "A key name in this list is one Orbyn does not allow, because it changes what runs.",
                ));
            }
        }
        if out.contains(key) {
            return Err(refused(409, "key_refused", "A key name is listed twice."));
        }
        out.push(key.clone());
    }
    Ok(out)
}

pub(crate) fn env_set_fields(
    name: &str,
    description: &str,
    keys: &[String],
) -> Result<EnvSetFields> {
    Ok(EnvSetFields {
        name: clean("name", name, NAME_MAX, 1, false)?,
        description: clean("description", description, DESCRIPTION_MAX, 0, true)?,
        keys: clean_keys(keys)?,
    })
}

fn source_of(text: &str) -> LibrarySource {
    serde_json::from_value(serde_json::Value::String(text.to_owned()))
        .unwrap_or(LibrarySource::Shared)
}

/// What an acknowledgement covers: the command template and the parameters. Editing either invalidates it.
pub(crate) fn command_hash(fields: &WorkflowFields) -> String {
    use sha2::{Digest, Sha256};
    let params = serde_json::to_string(&fields.params).unwrap_or_default();
    let digest = Sha256::new()
        .chain_update(fields.command.as_bytes())
        .chain_update([0u8])
        .chain_update(params.as_bytes())
        .finalize();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// A run of this entry needs the `untrusted_source` grant: it was not written here and its current command is not acknowledged.
pub(crate) fn needs_ack(row: &WorkflowRow) -> bool {
    row.source != "local" && row.acked_hash.as_deref() != Some(command_hash(&row.fields).as_str())
}

fn workflow_wire(row: WorkflowRow) -> Workflow {
    let needs_ack = needs_ack(&row);
    Workflow {
        source: source_of(&row.source),
        needs_ack,
        id: row.id,
        name: row.fields.name,
        description: row.fields.description,
        command: row.fields.command,
        params: row.fields.params,
        env_set_ids: row.fields.env_set_ids,
        revision: row.revision,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn prompt_wire(row: PromptRow) -> Prompt {
    Prompt {
        source: source_of(&row.source),
        id: row.id,
        name: row.fields.name,
        description: row.fields.description,
        body: row.fields.body,
        params: row.fields.params,
        revision: row.revision,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

impl Core {
    /// `GET /api/library`.
    pub fn library_summary(&self) -> Result<LibrarySummary> {
        let (workflows, prompts, env_sets) = self.store.library_counts()?;
        let n = |value: usize| u32::try_from(value).unwrap_or(u32::MAX);
        Ok(LibrarySummary {
            workflows: n(workflows),
            prompts: n(prompts),
            env_sets: n(env_sets),
            keychain: self.env_sets.state(),
            limits: limits(),
        })
    }

    // ---- workflows ----

    pub fn list_workflows(&self) -> Result<Vec<Workflow>> {
        Ok(self
            .store
            .library_workflows()?
            .into_iter()
            .map(workflow_wire)
            .collect())
    }

    pub fn get_workflow(&self, id: &str) -> Result<Workflow> {
        Ok(workflow_wire(self.store.library_workflow(id)?))
    }

    pub fn create_workflow(&self, draft: &WorkflowDraft) -> Result<Workflow> {
        let fields = workflow_fields(
            &draft.name,
            &draft.description,
            &draft.command,
            &draft.params,
            &draft.env_set_ids,
        )?;
        Ok(workflow_wire(self.store.create_library_workflow(&fields)?))
    }

    pub fn update_workflow(&self, id: &str, patch: &UpdateWorkflow) -> Result<Workflow> {
        let was = self.store.library_workflow(id)?.fields;
        let fields = workflow_fields(
            patch.name.as_deref().unwrap_or(&was.name),
            patch.description.as_deref().unwrap_or(&was.description),
            patch.command.as_deref().unwrap_or(&was.command),
            patch.params.as_deref().unwrap_or(&was.params),
            patch.env_set_ids.as_deref().unwrap_or(&was.env_set_ids),
        )?;
        Ok(workflow_wire(self.store.update_library_workflow(
            id,
            patch.seen_revision,
            &fields,
        )?))
    }

    pub fn delete_workflow(&self, id: &str) -> Result<()> {
        self.store.delete_library_workflow(id)
    }

    /// `POST /api/library/workflows/{id}/render`: the exact command, each value quoted for the shell. Runs and saves nothing.
    pub fn render_workflow(&self, id: &str, body: &RenderValues) -> Result<RenderedWorkflow> {
        let row = self.store.library_workflow(id)?;
        Ok(RenderedWorkflow {
            command: params::render_command(
                &row.fields.command,
                &row.fields.params,
                &body.values,
                Shell::native(),
            )?,
        })
    }

    // ---- prompts ----

    pub fn list_prompts(&self) -> Result<Vec<Prompt>> {
        Ok(self
            .store
            .library_prompts()?
            .into_iter()
            .map(prompt_wire)
            .collect())
    }

    pub fn get_prompt(&self, id: &str) -> Result<Prompt> {
        Ok(prompt_wire(self.store.library_prompt(id)?))
    }

    pub fn create_prompt(&self, draft: &PromptDraft) -> Result<Prompt> {
        let fields = prompt_fields(&draft.name, &draft.description, &draft.body, &draft.params)?;
        Ok(prompt_wire(self.store.create_library_prompt(&fields)?))
    }

    pub fn update_prompt(&self, id: &str, patch: &UpdatePrompt) -> Result<Prompt> {
        let was = self.store.library_prompt(id)?.fields;
        let fields = prompt_fields(
            patch.name.as_deref().unwrap_or(&was.name),
            patch.description.as_deref().unwrap_or(&was.description),
            patch.body.as_deref().unwrap_or(&was.body),
            patch.params.as_deref().unwrap_or(&was.params),
        )?;
        Ok(prompt_wire(self.store.update_library_prompt(
            id,
            patch.seen_revision,
            &fields,
        )?))
    }

    pub fn delete_prompt(&self, id: &str) -> Result<()> {
        self.store.delete_library_prompt(id)
    }

    pub fn render_prompt(&self, id: &str, body: &RenderValues) -> Result<RenderedPrompt> {
        let row = self.store.library_prompt(id)?;
        Ok(RenderedPrompt {
            text: params::render_prompt(&row.fields.body, &row.fields.params, &body.values)?,
        })
    }

    // ---- environment sets ----

    /// The sets as a read answers them: names and `present`, one keychain probe for the whole list. When the keychain cannot be
    /// used every key is `present: false` and `keychain` says why; a set then reads `planned` where there is no keychain at all.
    fn env_set_views(&self, rows: Vec<EnvSetRow>) -> Vec<EnvSet> {
        let mut state = self.env_sets.state();
        rows.into_iter()
            .map(|row| {
                let mut keys = Vec::with_capacity(row.fields.keys.len());
                for name in &row.fields.keys {
                    let present = if state == EnvKeychain::Ok {
                        match self.env_sets.present(&row.id, name) {
                            Ok(present) => present,
                            Err(EnvError::Unavailable) => {
                                state = EnvKeychain::Unavailable;
                                false
                            }
                            Err(EnvError::Blocked) => {
                                state = EnvKeychain::Blocked;
                                false
                            }
                            Err(_) => false,
                        }
                    } else {
                        false
                    };
                    keys.push(EnvKey {
                        name: name.clone(),
                        present,
                    });
                }
                EnvSet {
                    id: row.id,
                    name: row.fields.name,
                    description: row.fields.description,
                    keys,
                    keychain: state,
                    status: if state == EnvKeychain::Unavailable {
                        EnvSetStatus::Planned
                    } else {
                        EnvSetStatus::Stored
                    },
                    revision: row.revision,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                }
            })
            .collect()
    }

    pub(crate) fn env_set_view(&self, row: EnvSetRow) -> EnvSet {
        self.env_set_views(vec![row]).remove(0)
    }

    pub fn list_env_sets(&self) -> Result<Vec<EnvSet>> {
        Ok(self.env_set_views(self.store.library_env_sets()?))
    }

    pub fn get_env_set(&self, id: &str) -> Result<EnvSet> {
        Ok(self.env_set_view(self.store.library_env_set(id)?))
    }

    pub fn create_env_set(&self, draft: &EnvSetDraft) -> Result<EnvSet> {
        let fields = env_set_fields(&draft.name, &draft.description, &draft.keys)?;
        Ok(self.env_set_view(self.store.create_library_env_set(&fields)?))
    }

    /// Renames, describes, adds or removes key names. A removed name loses its keychain item first; if that cannot be done the set is
    /// left as it was and the call answers the keychain's error, so it can be tried again.
    pub fn update_env_set(&self, id: &str, patch: &UpdateEnvSet) -> Result<EnvSet> {
        let was = self.store.library_env_set(id)?;
        if was.revision != patch.seen_revision {
            return Err(CoreError::Refused {
                status: 409,
                code: "revision_conflict",
                message: "This environment set changed. Review it again before saving.".into(),
                detail: serde_json::json!({ "revision": was.revision }),
            });
        }
        let fields = env_set_fields(
            patch.name.as_deref().unwrap_or(&was.fields.name),
            patch
                .description
                .as_deref()
                .unwrap_or(&was.fields.description),
            patch.keys.as_deref().unwrap_or(&was.fields.keys),
        )?;
        let gone: Vec<String> = was
            .fields
            .keys
            .iter()
            .filter(|key| !fields.keys.contains(key))
            .cloned()
            .collect();
        self.remove_items(id, &gone)?;
        Ok(self.env_set_view(self.store.update_library_env_set(
            id,
            patch.seen_revision,
            &fields,
        )?))
    }

    /// Removes a set: every keychain item first, then the rows. A failure leaves the set whole.
    pub fn delete_env_set(&self, id: &str) -> Result<()> {
        let was = self.store.library_env_set(id)?;
        self.remove_items(id, &was.fields.keys)?;
        self.store.delete_library_env_set(id)
    }

    /// Deletes keychain items. With no keychain at all there is nothing to remove (a set made where one is missing holds no
    /// values), so that is not a failure; a keychain that is there and says no is.
    fn remove_items(&self, id: &str, keys: &[String]) -> Result<()> {
        if keys.is_empty() {
            return Ok(());
        }
        match self.env_sets.remove_all(id, keys) {
            Ok(()) | Err(EnvError::Unavailable) => Ok(()),
            Err(error) => Err(keychain_error(error)),
        }
    }

    /// `PUT /api/library/env-sets/{id}/keys/{key}`. Write-only: the answer is `{ name, present: true }`.
    pub fn set_env_key(&self, id: &str, key: &str, value: &str) -> Result<EnvKey> {
        if value.is_empty() || value.len() > MAX_VALUE_BYTES || value.contains('\0') {
            return Err(keychain_error(EnvError::InvalidValue));
        }
        self.store
            .library_key_event("library.env_set.key_set", id, key)?;
        self.env_sets.save(id, key, value).map_err(keychain_error)?;
        Ok(EnvKey {
            name: key.to_owned(),
            present: true,
        })
    }

    /// `DELETE /api/library/env-sets/{id}/keys/{key}`: the item goes, the name stays.
    pub fn remove_env_key(&self, id: &str, key: &str) -> Result<EnvKey> {
        self.store
            .library_key_event("library.env_set.key_removed", id, key)?;
        match self.env_sets.remove(id, key) {
            Ok(()) | Err(EnvError::Unavailable) => {}
            Err(error) => return Err(keychain_error(error)),
        }
        Ok(EnvKey {
            name: key.to_owned(),
            present: false,
        })
    }
}
