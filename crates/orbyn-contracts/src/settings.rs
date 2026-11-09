//! Server settings, preferences, keybindings and scoped preference edits.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::{ModelAlias, ModelPrice, ProjectScopeCatalog, ProjectScopesQuery, ProjectScript};

wire_enum! {
pub enum AutoSettleChoice {
    Inherit,
    Enabled,
    Disabled,
}
}

wire! {
pub struct AutoSettleSettings {
    /// Idle days before settling; `None` turns the idle rule off.
    pub after_days: Option<u32>,
    pub on_merge: bool,
    pub keep_pull_requests: bool,
}
}

wire! {
pub struct SnoozePreset {
    pub label: String,
    /// Minutes from now, up to 30 days.
    pub minutes: u32,
}
}

wire! {
/// Service-wide settings. Fields only ever get added, with defaults.
pub struct ServerSettings {
    pub auto_settle: AutoSettleSettings,
    pub snooze_presets: Vec<SnoozePreset>,
    pub title_instructions: Option<String>,
    /// `false` turns background update checks off. Absent means on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub background_update_checks: Option<bool>,
    /// Per-model rates. A model with no rate has an unknown cost.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<ModelPrice>>")]
    pub model_prices: Vec<ModelPrice>,
    /// Counts one model as another when pricing and grouping.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<ModelAlias>>")]
    pub model_aliases: Vec<ModelAlias>,
}
}

wire! {
/// Effective preference value, built-in default and winning source (`default`,
/// `file`, `global` or `project`). The optional project_file is the actual
/// read-only file layer; project_scoped says whether project overrides are allowed.
pub struct PreferenceView {
    pub key: String,
    pub value: Value,
    pub default: Value,
    pub source: String,
    pub project_scoped: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub project_file: Option<PreferenceFileValue>,
}
}

wire! {
pub struct PreferenceFileValue {
    pub path: String,
    pub value: Value,
}
}

wire! {
/// `null` for a key goes back to what it would inherit.
pub struct SetPreferences {
    pub values: std::collections::BTreeMap<String, Value>,
}
}

wire! {
pub struct KeybindingRule {
    pub key: String,
    pub command: String,
    #[serde(default)]
    pub when: Option<String>,
    /// In the file, turns off a default with this key and command.
    #[serde(default)]
    pub remove: bool,
}
}

wire! {
pub struct KeybindingConflict {
    pub key: String,
    pub when: Option<String>,
    pub commands: Vec<String>,
}
}

wire! {
pub struct KeybindingsView {
    /// The file, which the person may also edit by hand.
    pub path: String,
    pub rules: Vec<KeybindingRule>,
    /// What could not be read. The rest of the file still applies.
    pub issues: Vec<String>,
    pub conflicts: Vec<KeybindingConflict>,
}
}

wire! {
/// `default_theme` and when it was last set, in Unix milliseconds. A viewer applies
/// it when `set_at` is newer than the one it last applied, then keeps its own choice.
pub struct EnvironmentTheme {
    pub name: String,
    #[ts(type = "number")]
    pub set_at: i64,
}
}

wire! {
#[serde(deny_unknown_fields)]
pub struct ScopedPreferencesQuery {
    pub scope: ProjectScopesQuery,
    #[serde(default)]
    #[ts(optional, as = "Option<Vec<String>>")]
    pub keys: Vec<String>,
}
}

wire! {
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreferenceEdit {
    Set { value: serde_json::Value },
    Inherit {},
    ResetOverrides { project_ids: Vec<String> },
    AddProjectScript { script: ProjectScript },
    UpdateProjectScript { name: String, script: ProjectScript },
    RemoveProjectScript { name: String },
}
}

wire! {
#[serde(deny_unknown_fields)]
pub struct ScopedPreferencesChange {
    pub scope: ProjectScopesQuery,
    pub membership_version: String,
    pub if_match: std::collections::BTreeMap<String, String>,
    pub edits: std::collections::BTreeMap<String, PreferenceEdit>,
}
}

wire! {
pub struct ScopedPreferenceMember {
    pub project_id: Option<String>,
    pub name: Option<String>,
    pub view: PreferenceView,
}
}

wire! {
pub struct ScopedPreferenceOverride {
    pub project_id: String,
    pub name: String,
    pub value: serde_json::Value,
}
}

wire! {
pub struct ScopedPreferenceRow {
    pub key: String,
    pub value: serde_json::Value,
    pub mixed: bool,
    pub default: serde_json::Value,
    pub project_scoped: bool,
    pub revision: String,
    pub environment: PreferenceView,
    pub members: Vec<ScopedPreferenceMember>,
    pub overrides: Vec<ScopedPreferenceOverride>,
}
}

wire! {
pub struct ScopedPreferencesResult {
    pub catalog: ProjectScopeCatalog,
    pub rows: Vec<ScopedPreferenceRow>,
}
}
