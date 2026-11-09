//! Projects, their icons and the checkout scopes settings can target.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

fn initial_project_metadata_revision() -> String {
    "0".into()
}

wire_enum! {
pub enum ProjectIconColor {
    Gray, Red, Orange, Amber, Yellow, Lime, Green, Emerald, Teal,
    Cyan, Sky, Blue, Indigo, Violet, Purple, Fuchsia, Pink, Rose,
}
}

wire! {
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectIconOverride {
    Lucide { name: String, color: ProjectIconColor },
    Emoji { emoji: String },
    Monogram { text: String, color: ProjectIconColor },
}
}

wire! {
pub struct Project {
    pub id: String,
    pub name: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub project_icon: Option<ProjectIconOverride>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub favicon_path: Option<String>,
    /// Decimal storage revision, never a JavaScript floating-point counter.
    #[serde(default = "initial_project_metadata_revision")]
    #[ts(optional, as = "Option<String>")]
    pub metadata_revision: String,
    pub path: String,
    /// Engines refuse to start until a person trusts this folder.
    pub trusted: bool,
    /// False for a folder that is not in Git, which works without Git's features.
    pub is_git: bool,
    pub created_at: String,
    /// Orbyn cloned or created this folder. Only such a folder can ever be deleted.
    #[serde(default)]
    pub managed: bool,
    /// A thread's dated folder under app data, hidden from project lists.
    #[serde(default)]
    pub scratch: bool,
    /// The bundled skills library, which the service registers by itself and
    /// project lists leave out, from the first list on.
    #[serde(default)]
    #[ts(as = "Option<bool>", optional)]
    pub library: bool,
}
}

wire_enum! {
#[derive(Default)]
pub enum ProjectGroupingMode {
    #[default]
    Repository,
    RepositoryPath,
    Separate,
}
}

wire! {
#[serde(deny_unknown_fields)]
pub struct ProjectGroupingOverride {
    pub project_id: String,
    pub mode: ProjectGroupingMode,
}
}

wire! {
#[derive(Default)]
#[serde(deny_unknown_fields)]
pub struct ProjectGroupingRecipe {
    #[serde(default)]
    #[ts(optional, as = "Option<ProjectGroupingMode>")]
    pub mode: ProjectGroupingMode,
    #[serde(default)]
    #[ts(optional, as = "Option<Vec<ProjectGroupingOverride>>")]
    pub overrides: Vec<ProjectGroupingOverride>,
}
}

wire! {
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectSettingsSelection {
    AllProjects {},
    ProjectGroup { group_key: String },
    Checkout { group_key: String, project_id: String },
}
}

fn present_project_selection<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<ProjectSettingsSelection>, D::Error> {
    ProjectSettingsSelection::deserialize(deserializer).map(Some)
}

wire! {
#[derive(Default)]
#[serde(deny_unknown_fields)]
/// `selection`: Only an absent selection defaults to All projects; null or malformed selections are rejected.
pub struct ProjectScopesQuery {
    #[serde(default)]
    #[ts(optional, as = "Option<ProjectGroupingRecipe>")]
    pub recipe: ProjectGroupingRecipe,
    #[serde(default, deserialize_with = "present_project_selection")]
    #[ts(optional)]
    pub selection: Option<ProjectSettingsSelection>,
}
}

wire! {
#[serde(deny_unknown_fields)]
/// Names are compared raw for every selected physical member before any update.
pub struct RenameSelectedProjects {
    pub scope: ProjectScopesQuery,
    pub membership_version: String,
    pub expected_names: BTreeMap<String, String>,
    pub name: String,
}
}

fn explicit_project_icon<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<ProjectIconOverride>, D::Error> {
    Option::<ProjectIconOverride>::deserialize(deserializer)
}

wire! {
#[serde(deny_unknown_fields)]
/// Explicit null resets both structured and saved-image overrides.
pub struct ChangeProjectIcon {
    pub scope: ProjectScopesQuery,
    pub membership_version: String,
    pub expected_revisions: BTreeMap<String, String>,
    #[serde(deserialize_with = "explicit_project_icon")]
    pub icon: Option<ProjectIconOverride>,
}
}

wire! {
#[serde(deny_unknown_fields)]
pub struct SearchProjectImages {
    pub scope: ProjectScopesQuery,
    pub membership_version: String,
    #[serde(default)]
    pub query: String,
}
}

wire! {
pub struct ProjectImageCandidate {
    pub path: String,
    pub name: String,
}
}

wire! {
pub struct ProjectImageSearchResult {
    pub project_id: String,
    pub project_path: String,
    pub membership_version: String,
    pub entries: Vec<ProjectImageCandidate>,
    pub truncated: bool,
}
}

wire! {
#[serde(deny_unknown_fields)]
/// A workspace-relative image hint, applied to the observed selected members.
pub struct ChangeProjectImage {
    pub scope: ProjectScopesQuery,
    pub membership_version: String,
    pub expected_revisions: BTreeMap<String, String>,
    pub path: String,
}
}

wire! {
#[serde(deny_unknown_fields)]
/// Asks the desktop app's own file dialog for an image outside the project. The
/// service, not the caller, receives the chosen path and copies the image into its
/// data directory before applying it to the observed selected members.
pub struct ChooseProjectImage {
    pub scope: ProjectScopesQuery,
    pub membership_version: String,
    pub expected_revisions: BTreeMap<String, String>,
}
}

wire! {
pub struct ProjectIconResourceData {
    pub source: ProjectIconSource,
    pub relative_path: String,
    pub revision: String,
    pub media_type: ProjectIconMediaType,
    #[ts(type = "number")]
    pub size: u64,
}
}

wire! {
/// Passive image identity; streaming delivery does not place bytes in project rows.
pub struct ProjectIconResource {
    pub project_id: String,
    pub project_path: String,
    pub favicon_path: Option<String>,
    pub image: Option<ProjectIconResourceData>,
}
}

wire_enum! {
pub enum ProjectIconSource { SavedImage, ProjectFile, Automatic }
}

wire_enum! {
pub enum ProjectIconMediaType { Png, Jpeg, Gif, Webp, Avif, Bmp, Ico, Svg, Heic, Heif, Tiff }
}

wire! {
/// Authenticated, bounded local image bytes. The client maps the closed media type
/// to an image data URL; no caller-supplied URL or absolute asset path is returned.
pub struct ProjectIconImageData {
    pub source: ProjectIconSource,
    pub relative_path: String,
    pub revision: String,
    pub media_type: ProjectIconMediaType,
    pub data_base64: String,
}
}

wire! {
pub struct ProjectIconImage {
    pub project_id: String,
    pub project_path: String,
    pub image: Option<ProjectIconImageData>,
}
}

wire_enum! {
pub enum ProjectScopeUnavailableReason {
    ProjectMissing,
    CheckoutMissing,
    CheckoutChanged,
    InvalidPath,
    MetadataUnavailable,
}
}

wire! {
/// `physical_key`: Opaque identity of the registered local physical path, never a project name.
/// `repository_key`: Opaque, sanitized repository identity. Local/file remotes do not establish one.
/// `repository_relative_path`: Actual path relative to the Git root; empty for a registered Git root.
/// `metadata_complete`: False only when local metadata could not be observed, not for a known physical fallback.
pub struct ProjectScopeCheckout {
    pub project_id: String,
    pub physical_key: String,
    pub name: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub project_icon: Option<ProjectIconOverride>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub favicon_path: Option<String>,
    #[serde(default = "initial_project_metadata_revision")]
    #[ts(optional, as = "Option<String>")]
    pub metadata_revision: String,
    pub path: Option<String>,
    pub repository_key: Option<String>,
    pub repository_label: Option<String>,
    pub repository_relative_path: Option<String>,
    pub grouping_mode: ProjectGroupingMode,
    pub metadata_complete: bool,
    pub unavailable: Option<ProjectScopeUnavailableReason>,
}
}

wire! {
pub struct ProjectScopeGroup {
    pub key: String,
    pub label: String,
    pub members: Vec<ProjectScopeCheckout>,
}
}

wire! {
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResolvedProjectSettingsScope {
    /// Environment defaults, not a fan-out to project overrides.
    AllProjects,
    ProjectGroup { group_key: String, project_ids: Vec<String> },
    Checkout { group_key: String, project_id: String },
    Unavailable { reason: ProjectScopeUnavailableReason },
}
}

wire! {
/// `version`: Membership/recipe digest for a later mutation to fence; this read changes nothing.
pub struct ProjectScopeCatalog {
    pub version: String,
    pub groups: Vec<ProjectScopeGroup>,
    pub metadata_complete: bool,
    pub selection: ResolvedProjectSettingsScope,
}
}

wire! {
pub struct RenameProject {
    pub name: String,
}
}

wire! {
pub struct CloneProject {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    /// Absolute folder to clone into; absent means `<workspace>/repo/<slug>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub path: Option<String>,
}
}

wire! {
pub struct NewProject {
    pub name: String,
    /// Adds a README and makes the first commit.
    #[serde(default)]
    pub readme: bool,
    /// Absolute folder to create; absent means `<workspace>/repo/<slug>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub path: Option<String>,
}
}

wire_enum! {
pub enum CloneState {
    Running,
    Done,
    Failed,
    Cancelled,
}
}

wire! {
/// A clone running in the background. Jobs live until the service stops.
pub struct CloneJob {
    pub id: String,
    pub url: String,
    pub name: Option<String>,
    pub path: Option<String>,
    pub state: CloneState,
    pub project: Option<Project>,
    pub error: Option<String>,
    /// True when an SSH URL was refused and HTTPS worked instead.
    pub used_https: bool,
    pub started_at: String,
}
}

wire! {
/// Registers an existing folder where it is. Nothing is cloned, moved or written.
pub struct AddProject {
    pub path: String,
}
}

wire! {
/// Turns a scratch folder into a project, in place, with its threads.
pub struct AdoptScratch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
}
}

wire! {
pub struct ProjectRemoval {
    pub files_removed: bool,
    /// Set when the folder stays on disk because it still holds work.
    pub kept_path: Option<String>,
}
}

wire! {
pub struct CreateProject {
    pub path: String,
}
}

#[cfg(test)]
mod project_icon_request_tests {
    use super::*;
    use serde_json::Value;
    #[test]
    fn icon_reset_requires_explicit_null_and_a_typed_revision_map() {
        let base = serde_json::json!({"scope":{},"membership_version":"v","expected_revisions":{"project":"0"}});
        assert!(serde_json::from_value::<ChangeProjectIcon>(base.clone()).is_err());
        let mut reset = base.clone();
        reset["icon"] = Value::Null;
        assert!(
            serde_json::from_value::<ChangeProjectIcon>(reset.clone())
                .unwrap()
                .icon
                .is_none()
        );
        reset["expected_revisions"]["project"] = serde_json::json!(0);
        assert!(serde_json::from_value::<ChangeProjectIcon>(reset).is_err());
        let mut unknown = base;
        unknown["icon"] = serde_json::json!({"kind":"future"});
        assert!(serde_json::from_value::<ChangeProjectIcon>(unknown).is_err());
    }
}
