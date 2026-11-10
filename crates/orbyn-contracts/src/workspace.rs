//! The bundled skills library and the workspace it is applied to.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire! {
pub struct IncomingCommit {
    pub sha: String,
    pub subject: String,
}
}

wire! {
pub struct LibraryStatus {
    pub path: String,
    /// True when the user chose an existing checkout, which is never moved.
    pub adopted: bool,
    /// True while this is the snapshot the app shipped with, not yet a clone. Only a
    /// clone can receive updates.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(optional, as = "Option<bool>")]
    pub snapshot: bool,
    pub revision: String,
    pub branch: Option<String>,
    pub dirty: bool,
    pub dirty_files: Vec<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// The revision a rollback would return to.
    pub previous_revision: Option<String>,
    /// False when `.claude/skills` or `.agents/skills` did not materialize as
    /// links, which sends this session to the MCP skill route.
    pub native_skills: bool,
    pub problem: Option<String>,
}
}

wire! {
pub struct TargetWorktree {
    pub path: String,
    pub branch: Option<String>,
    pub revision: String,
    pub dirty: bool,
}
}

wire! {
pub struct TargetStatus {
    pub project_id: Option<String>,
    pub path: String,
    pub git_root: Option<String>,
    pub revision: Option<String>,
    pub branch: Option<String>,
    pub dirty: bool,
    pub worktrees: Vec<TargetWorktree>,
    pub instructions: Vec<String>,
    /// Target skills sharing a library skill's name. Both stay; neither is renamed.
    pub same_name_skills: Vec<String>,
    pub problem: Option<String>,
}
}

wire! {
pub struct WorkspaceStatus {
    /// True once the library is in place, which startup sees to. Only a failure to
    /// write the data folder leaves it false.
    pub configured: bool,
    pub default_url: String,
    pub library: Option<LibraryStatus>,
    pub targets: Vec<TargetStatus>,
}
}

wire! {
/// Nothing needs this. `adopt_path` uses an existing library; `url`, or neither, asks
/// for the clone now instead of in the background.
pub struct WorkspaceSetup {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub adopt_path: Option<String>,
}
}

wire! {
pub struct LibraryUpdateRequest {
    /// False only fetches and reports incoming commits.
    pub apply: bool,
}
}

wire! {
pub struct LibraryUpdateResult {
    pub applied: bool,
    pub previous_revision: Option<String>,
    pub revision: String,
    pub incoming: Vec<IncomingCommit>,
}
}
