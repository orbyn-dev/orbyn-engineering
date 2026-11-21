//! The personal library (docs/execution-profiles-and-library.md, Part 2): workflows, prompts and environment sets, all on this
//! device. An environment set holds key names; its values live in the OS keychain and are in no type here that a route answers.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{ProfileDraft, ProfileGrant};

wire_enum! {
/// How a parameter is filled and checked.
pub enum ParamKind {
    Text,
    /// One of `choices`.
    Choice,
    /// Relative to the project: never absolute, no `..`.
    Path,
}
}

wire! {
/// A placeholder `{{name}}` of a workflow or prompt.
pub struct Param {
    /// `[a-z][a-z0-9_]{0,31}`, unique in the item, at most 16 per item.
    pub name: String,
    #[serde(default)]
    pub label: String,
    pub kind: ParamKind,
    /// At most 500 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub default: Option<String>,
    #[serde(default)]
    pub required: bool,
    /// 2 to 20 choices of at most 60 characters, for `choice`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub choices: Option<Vec<String>>,
}
}

wire_enum! {
/// Where an entry came from. Only `local` was written on this device.
pub enum LibrarySource {
    Local,
    Import,
    Repo,
    Shared,
}
}

wire! {
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub description: String,
    /// At most 4,000 bytes. Text: saving it runs nothing.
    pub command: String,
    pub params: Vec<Param>,
    /// At most 4 environment sets, by id. They attach to workflow runs only.
    pub env_set_ids: Vec<String>,
    pub source: LibrarySource,
    /// A non-local entry's first run, and the first after its command or parameters change, needs the `untrusted_source` grant.
    pub needs_ack: bool,
    pub revision: u32,
    pub created_at: String,
    pub updated_at: String,
}
}

wire! {
/// `POST /api/library/workflows`.
pub struct WorkflowDraft {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub command: String,
    #[serde(default)]
    pub params: Vec<Param>,
    #[serde(default)]
    pub env_set_ids: Vec<String>,
}
}

wire! {
/// `PATCH /api/library/workflows/{id}`. Absent fields are left alone; a stale `seen_revision` is `409 revision_conflict`.
pub struct UpdateWorkflow {
    pub seen_revision: u32,
    #[serde(default)]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub command: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub params: Option<Vec<Param>>,
    #[serde(default)]
    #[ts(optional)]
    pub env_set_ids: Option<Vec<String>>,
}
}

wire! {
pub struct Prompt {
    pub id: String,
    pub name: String,
    pub description: String,
    /// At most 32 KiB. Stored as written.
    pub body: String,
    pub params: Vec<Param>,
    pub source: LibrarySource,
    pub revision: u32,
    pub created_at: String,
    pub updated_at: String,
}
}

wire! {
/// `POST /api/library/prompts`.
pub struct PromptDraft {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub body: String,
    #[serde(default)]
    pub params: Vec<Param>,
}
}

wire! {
/// `PATCH /api/library/prompts/{id}`.
pub struct UpdatePrompt {
    pub seen_revision: u32,
    #[serde(default)]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub body: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub params: Option<Vec<Param>>,
}
}

wire_enum! {
/// What the keychain said when it was asked.
pub enum EnvKeychain {
    Ok,
    /// No keychain here (Windows and Linux in the beta).
    Unavailable,
    /// Locked, denied or a cancelled prompt; retryable.
    Blocked,
}
}

wire_enum! {
pub enum EnvSetStatus {
    /// Values can be saved (macOS).
    Stored,
    /// Names only, no values (Windows and Linux in the beta).
    Planned,
}
}

wire! {
/// A key name and whether the keychain holds a non-empty value for it. Never the value, its length or any part of it.
pub struct EnvKey {
    pub name: String,
    pub present: bool,
}
}

wire! {
pub struct EnvSet {
    pub id: String,
    pub name: String,
    pub description: String,
    pub keys: Vec<EnvKey>,
    pub keychain: EnvKeychain,
    pub status: EnvSetStatus,
    pub revision: u32,
    pub created_at: String,
    pub updated_at: String,
}
}

wire! {
/// `POST /api/library/env-sets`: a name and key names, no values.
pub struct EnvSetDraft {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub keys: Vec<String>,
}
}

wire! {
/// `PATCH /api/library/env-sets/{id}`. `keys` is the full list of names: one left out is removed, with its keychain item.
pub struct UpdateEnvSet {
    pub seen_revision: u32,
    #[serde(default)]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub keys: Option<Vec<String>>,
}
}

/// `PUT /api/library/env-sets/{id}/keys/{key}`. The value is a secret: this type prints nothing in `Debug`.
#[derive(Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../web/src/generated/")]
pub struct SetEnvValue {
    pub value: String,
}

impl std::fmt::Debug for SetEnvValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SetEnvValue").finish_non_exhaustive()
    }
}

wire! {
/// The numbers of "Limits": integers, a byte limit in bytes.
pub struct LibraryLimits {
    pub workflows: u32,
    pub prompts: u32,
    pub env_sets: u32,
    pub env_keys_per_set: u32,
    pub env_sets_per_workflow: u32,
    pub params_per_item: u32,
    pub workflow_command_bytes: u32,
    pub prompt_body_bytes: u32,
    pub env_value_bytes: u32,
    pub suggestions_file_bytes: u32,
    pub import_bytes: u32,
}
}

wire! {
/// `GET /api/library`.
pub struct LibrarySummary {
    pub workflows: u32,
    pub prompts: u32,
    pub env_sets: u32,
    pub keychain: EnvKeychain,
    pub limits: LibraryLimits,
}
}

wire! {
/// `POST .../render`: a value for each parameter the person filled in.
pub struct RenderValues {
    #[serde(default)]
    pub values: BTreeMap<String, String>,
}
}

wire! {
pub struct RenderedWorkflow {
    /// The exact text that would run. Each value is quoted for the shell; it never holds an environment value.
    pub command: String,
}
}

wire! {
pub struct RenderedPrompt {
    pub text: String,
}
}

wire! {
/// `POST /api/library/workflows/{id}/run`: one of `thread_id` and `project_id`.
pub struct RunWorkflow {
    #[serde(default)]
    pub values: BTreeMap<String, String>,
    #[serde(default)]
    #[ts(optional)]
    pub thread_id: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub project_id: Option<String>,
    /// The grant codes the person saw. They must match exactly what the run needs: `env_values` when sets are attached, else none.
    #[serde(default)]
    pub ack_grants: Vec<ProfileGrant>,
}
}

wire! {
pub struct RunWorkflowAnswer {
    pub terminal_id: String,
}
}

wire_enum! {
pub enum LibraryKind {
    Workflow,
    Prompt,
    EnvSet,
}
}

wire! {
/// A workflow in a committed `.orbyn/workflows.json`: the library shape without `id`, `revision` and the timestamps, with the
/// file and the position in it. Sets are named, since an id means nothing on another device.
pub struct SuggestedWorkflow {
    pub file: String,
    pub index: u32,
    pub name: String,
    pub description: String,
    pub command: String,
    pub params: Vec<Param>,
    pub env_set_names: Vec<String>,
}
}

wire! {
pub struct SuggestedPrompt {
    pub file: String,
    pub index: u32,
    pub name: String,
    pub description: String,
    pub body: String,
    pub params: Vec<Param>,
}
}

wire! {
/// Key names only.
pub struct SuggestedEnvSet {
    pub file: String,
    pub index: u32,
    pub name: String,
    pub description: String,
    pub keys: Vec<String>,
}
}

wire! {
/// One entry or file that was not admitted. `code` is a stable word, never the text that was wrong.
pub struct SuggestionProblem {
    pub file: String,
    pub index: u32,
    pub code: String,
}
}

wire! {
/// `GET /api/projects/{id}/library/suggestions`.
pub struct Suggestions {
    pub workflows: Vec<SuggestedWorkflow>,
    pub prompts: Vec<SuggestedPrompt>,
    pub env_sets: Vec<SuggestedEnvSet>,
    pub problems: Vec<SuggestionProblem>,
}
}

wire! {
/// `POST /api/projects/{id}/library/suggestions/copy`.
pub struct CopySuggestion {
    pub file: String,
    pub index: u32,
}
}

wire! {
/// `201`: the created device-library entry (a `Workflow`, a `Prompt` or an `EnvSet`) beside its kind.
pub struct CopiedSuggestion {
    pub kind: LibraryKind,
    #[ts(type = "import('./Workflow').Workflow | import('./Prompt').Prompt | import('./EnvSet').EnvSet")]
    pub entry: serde_json::Value,
}
}

wire! {
/// A workflow as exported and imported: sets by name.
pub struct WorkflowExport {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub command: String,
    #[serde(default)]
    pub params: Vec<Param>,
    #[serde(default)]
    pub env_set_names: Vec<String>,
}
}

wire! {
pub struct PromptExport {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub body: String,
    #[serde(default)]
    pub params: Vec<Param>,
}
}

wire! {
/// A set as exported: its name and key names. Values are never exported.
pub struct EnvSetExport {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub keys: Vec<String>,
}
}

wire! {
/// The export document, and what an import reads (plus `dry_run`). Unknown fields are dropped.
pub struct LibraryDocument {
    pub format: String,
    pub version: u32,
    pub exported_at: String,
    #[serde(default)]
    pub profiles: Vec<ProfileDraft>,
    #[serde(default)]
    pub workflows: Vec<WorkflowExport>,
    #[serde(default)]
    pub prompts: Vec<PromptExport>,
    #[serde(default)]
    pub env_sets: Vec<EnvSetExport>,
}
}

wire_enum! {
/// What an import did, or with `dry_run` would do, with one entry.
pub enum ImportOutcome {
    Created,
    /// Created under another name, because the name was taken.
    Renamed,
    Refused,
}
}

wire! {
pub struct ImportLine {
    pub kind: String,
    pub index: u32,
    pub outcome: ImportOutcome,
    /// The name the entry has now, or would have. Absent when it was refused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    /// A stable word: `invalid_input`, `library_limit`, `unknown_env_set` and the like. Never the text that was wrong.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub code: Option<String>,
}
}

wire! {
/// Something dropped from an entry that was otherwise admitted (a set name this device has no set for).
pub struct ImportProblem {
    pub kind: String,
    pub index: u32,
    pub code: String,
}
}

wire! {
/// `POST /api/library/import`.
pub struct ImportAnswer {
    pub dry_run: bool,
    pub problems: Vec<ImportProblem>,
    pub created: u32,
    pub renamed: u32,
    pub refused: u32,
    pub lines: Vec<ImportLine>,
}
}
