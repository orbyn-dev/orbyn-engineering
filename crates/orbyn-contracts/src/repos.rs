//! Repositories of a project (docs/multi-repo.md). Names follow the T3 Code workspace folder schema (MIT,
//! 9168ca73d, names only). Every name, path, branch and URL here is untrusted text.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire_enum! {
pub enum RepoCloneState {
    Cloning,
    Failed,
}
}

wire! {
/// A clone in progress, or one that failed.
pub struct RepoClone {
    pub state: RepoCloneState,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub percent: Option<u8>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub message: Option<String>,
}
}

wire! {
/// One folder of the project. The project's own folder is the first, `id` equal to the project id.
pub struct RepoRow {
    pub id: String,
    pub name: String,
    pub path: String,
    pub is_git: bool,
    pub trusted: bool,
    pub branch: Option<String>,
    pub branches: Vec<String>,
    pub has_remote: bool,
    pub primary: bool,
    pub clone: Option<RepoClone>,
}
}

wire! {
pub struct ReposView {
    pub repositories: Vec<RepoRow>,
    pub clone_directory: Option<String>,
    pub max_per_thread: u32,
}
}

wire! {
/// Adds a folder the person picked, in place.
pub struct AddRepoFolder {
    pub path: String,
}
}

wire! {
/// A new empty repository under the clone directory.
pub struct CreateRepo {
    pub name: String,
}
}

wire! {
/// A clone source: `https://`, `ssh://` or scp-style `user@host:path`, nothing else.
pub struct CloneRepo {
    pub source: String,
}
}

wire! {
/// A folder to look into: a directory, or a `.code-workspace` file.
pub struct ScanRepos {
    pub path: String,
}
}

wire_enum! {
pub enum ScanSource {
    Child,
    CodeWorkspace,
}
}

wire_enum! {
pub enum ScanRefusal {
    Outside,
    Missing,
    NotDirectory,
}
}

wire! {
pub struct ScannedFolder {
    pub name: String,
    pub path: String,
    pub is_git: bool,
    pub source: ScanSource,
    /// Already a repository of a project.
    pub in_project: bool,
    pub refused: Option<ScanRefusal>,
}
}

wire! {
pub struct RepoScan {
    pub path: String,
    pub is_git: bool,
    pub code_workspace_file: Option<String>,
    pub folders: Vec<ScannedFolder>,
    pub truncated: bool,
}
}
