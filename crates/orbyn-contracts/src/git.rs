//! Git actions, branches, worktrees and publishing a project, and the app's own
//! record of a repository it already has checkouts of.

use crate::ProjectScopeCheckout;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire! {
pub struct PullRequestDraft {
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base: Option<String>,
}
}

wire! {
/// Any combination of commit, push and open a pull request, in that order.
pub struct GitAction {
    /// Commits every change with this message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub commit_message: Option<String>,
    #[serde(default)]
    pub push: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pull_request: Option<PullRequestDraft>,
}
}

wire! {
pub struct GitPhase {
    /// `commit`, `push` or `pull_request`.
    pub name: String,
    pub ok: bool,
    pub message: Option<String>,
    pub url: Option<String>,
}
}

wire! {
pub struct GitActionResult {
    /// Phases that ran. A failed phase is the last one.
    pub phases: Vec<GitPhase>,
    /// Why blank text was not generated and the fallback was used instead.
    #[serde(default)]
    pub notice: Option<String>,
}
}

wire! {
pub struct BranchInfo {
    pub name: String,
    pub current: bool,
    pub remote: bool,
}
}

wire! {
/// The checked-out state the VCS `refreshStatus` handler reads now, not from a kept
/// answer: the branch or detached HEAD, its tracking ref and distance, and the
/// working-tree counts Git reports.
pub struct VcsStatus {
    /// The branch HEAD points at; `None` when HEAD is detached.
    pub branch: Option<String>,
    pub detached: bool,
    /// The abbreviated checked-out commit.
    pub head: String,
    /// The ref the branch tracks, such as `origin/main`; `None` when it tracks none.
    pub upstream: Option<String>,
    /// Commits this checkout has that its upstream does not, and the reverse.
    pub ahead: u32,
    pub behind: u32,
    /// Files Git reports as changed (tracked), untracked, and conflicted.
    pub changed: u32,
    pub untracked: u32,
    pub conflicted: u32,
}
}

wire! {
pub struct CreateBranch {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub start: Option<String>,
}
}

wire! {
pub struct SwitchBranch {
    pub name: String,
}
}

wire! {
pub struct PublishProject {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    pub private: bool,
    /// `github`, the default, or `gitlab`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub forge: Option<String>,
}
}

wire! {
pub struct Worktree {
    pub path: String,
    pub branch: String,
    pub base: String,
}
}

wire! {
/// The working-copy change of a Jujutsu checkout that is not colocated with Git,
/// as the installed `jj` reports it.
pub struct JjStatus {
    pub change_id: String,
    pub commit_id: String,
    pub bookmarks: Vec<String>,
    /// The first line of the change's description; empty when undescribed.
    pub description: String,
    pub conflict: bool,
    pub changed_files: u32,
    pub additions: u32,
    pub deletions: u32,
}
}

wire! {
/// What the app already knows about one repository, looked up by the `owner/name`
/// its registered checkouts' own Git configs name. Read without a transport, so a
/// repository the person has not added is simply not found and no forge is asked.
pub struct RepositoryLookup {
    /// Every registered project's repository identity could be observed. When false,
    /// empty `checkouts` is not proof that no checkout names the repository.
    pub complete: bool,
    /// The app's records of the checkouts that name the repository, by project id.
    pub checkouts: Vec<ProjectScopeCheckout>,
}
}
