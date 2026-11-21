//! Repositories of a project (docs/multi-repo.md). Names follow the T3 Code workspace folder schema (MIT,
//! 9168ca73d, names only). Every name, path, branch and URL here is untrusted text.

use crate::PermissionMode;
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

wire! {
/// One repository a thread works in, with what changed in its worktree.
pub struct ThreadRepo {
    pub repo_id: String,
    pub name: String,
    /// The thread's own branch in this repository.
    pub branch: String,
    pub added: u32,
    pub deleted: u32,
    pub trusted: bool,
    /// The changed files, relative to the repository root; at most 200.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub files: Option<Vec<String>>,
}
}

wire! {
/// A repository of a new thread and the branch its own branch is cut from.
pub struct StartRepoChoice {
    pub repo_id: String,
    pub base_branch: String,
}
}

wire! {
/// Starts a thread in two or more repositories of a project, each on one new branch name.
pub struct StartThreadInput {
    pub repos: Vec<StartRepoChoice>,
    pub engine: String,
    pub model: Option<String>,
    pub permission_mode: PermissionMode,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub message: Option<String>,
    /// One new branch name for every repository; absent, the service makes one.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub branch_name: Option<String>,
}
}

wire! {
/// Adds a trusted repository of the project to a multi-repository thread.
pub struct AddThreadRepo {
    pub repo_id: String,
}
}

wire! {
/// A multi-repository thread's folder: one worktree per repository, `<root>/<name>`, on one branch.
pub struct TaskFolder {
    pub root: String,
    pub branch: String,
    pub repos: Vec<ThreadRepo>,
    pub max_per_thread: u32,
}
}
