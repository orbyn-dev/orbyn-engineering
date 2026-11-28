//! Pull requests: lists, details, reviews, guides and forge status.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::Thread;

wire! {
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub url: String,
    /// `open`, `merged` or `closed`.
    pub state: String,
    pub head: String,
    pub base: String,
    pub author: Option<String>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    /// `approved`, `changes_requested`, `review_required` or `none`.
    #[serde(default)]
    pub review_state: Option<String>,
    /// `pass`, `fail`, `pending` or `none`.
    #[serde(default)]
    pub checks_state: Option<String>,
    /// Check counts, read from the check data the list already carries; `None` when
    /// the forge's list has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub checks: Option<PullChecks>,
}
}

wire! {
/// A branch of a project's repository, as its forge sees it: whether the remote already has
/// it, and the pull request that already comes from it or none. This is the read a pull request
/// page makes before a pull request exists (issue 506, frame `pr-detail-none`), so `pull_request`
/// being `None` is an answer, not an error.
pub struct BranchDetail {
    /// The repository of the project the branch was read in: the one asked for, else the project's own folder.
    pub repo_id: String,
    pub branch: String,
    /// The `origin` remote-tracking ref that holds the branch, such as `origin/feat/login`. `None`
    /// means the branch is not pushed; it is as fresh as the last fetch.
    pub remote_ref: Option<String>,
    /// The pull request whose head is this branch: the open one with the highest number, else the
    /// latest of any state. `None` means none was opened from it: a pull request needs a branch the
    /// remote has, so a branch that is not pushed is answered without asking the forge.
    pub pull_request: Option<PullRequest>,
}
}

wire! {
pub struct RequestedReviewer {
    /// A login, or a team's slug.
    pub name: String,
    /// `user` or `team`.
    pub kind: String,
}
}

wire! {
/// Per-state counts over a pull request's checks. Skipped and neutral checks count
/// only in `total`.
pub struct PullChecks {
    pub total: u32,
    pub passing: u32,
    pub failing: u32,
    pub running: u32,
    /// Names of failing checks, at most 20.
    pub failing_names: Vec<String>,
}
}

wire! {
/// Asks for the details of listed pull requests, all projects in one call.
pub struct PullDetailsRequest {
    pub pulls: Vec<PullDetailsRef>,
    /// A refresh the person started: recount instead of reusing known details.
    #[serde(default)]
    pub fresh: bool,
}
}

wire! {
pub struct PullDetailsRef {
    pub project_id: String,
    pub number: u64,
    /// The row's `updated_at`, which says whether known details still hold.
    #[serde(default)]
    pub updated_at: Option<String>,
}
}

wire! {
/// What the base list leaves out, for one pull request. A field is absent when it is
/// unknown, never zero or partial.
pub struct PullDetails {
    pub project_id: String,
    pub number: u64,
    /// When the pull request was opened, as the forge writes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub created_at: Option<String>,
    /// Users and teams whose review is still asked for; empty when nobody is asked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub requested_reviewers: Option<Vec<RequestedReviewer>>,
    /// Conversation comments plus inline review comments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub comment_count: Option<u32>,
    /// Approved latest opinionated reviews, only when the host's connection is complete.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub approval_count: Option<u32>,
    /// The signed-in viewer's latest review: `approved`, `changes_requested`,
    /// `commented`, `dismissed`, `pending` or `none`. Absent when the read could not
    /// see every review, so the viewer's may be past it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub viewer_review_state: Option<String>,
    /// Whether the forge can merge the pull request: `mergeable`, `conflicting` or
    /// `unknown`. Absent when the forge does not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mergeable: Option<String>,
}
}

wire! {
pub struct PullDetailsResponse {
    pub details: Vec<PullDetails>,
    /// Detail fields withheld because GitHub's rate limit is spent, named as they
    /// appear on `PullDetails`; empty otherwise. No row carries a paused field.
    pub paused: Vec<String>,
    /// When the pause ends, as an RFC 3339 time, while `paused` is not empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub resume_at: Option<String>,
}
}

wire! {
pub struct PullRequestDetail {
    pub pull: PullRequest,
    pub body: String,
    /// The forge's own records, which differ by forge.
    pub checks: Value,
    pub comments: Value,
    /// `mergeable`, `conflicting` or `unknown`.
    #[serde(default)]
    pub mergeable: Option<String>,
    #[serde(default)]
    pub reviewers: Vec<String>,
    #[serde(default)]
    pub head_sha: Option<String>,
    #[serde(default)]
    pub base_sha: Option<String>,
    #[serde(default)]
    pub additions: Option<u64>,
    #[serde(default)]
    pub deletions: Option<u64>,
    #[serde(default)]
    pub changed_files: Option<u64>,
    #[serde(default)]
    pub maintainer_can_modify: Option<bool>,
    #[serde(default)]
    pub auto_merge: bool,
}
}

wire! {
pub struct PullCheck {
    pub name: String,
    /// `pass`, `fail`, `pending`, `skipped` or `neutral`.
    pub state: String,
    pub url: Option<String>,
    pub workflow: Option<String>,
    pub description: Option<String>,
}
}

wire! {
pub struct PullComment {
    pub id: String,
    /// `issue` or `review` on GitHub, `note` on GitLab.
    pub kind: String,
    pub author: Option<String>,
    pub body: String,
    pub created_at: Option<String>,
    pub path: Option<String>,
    pub line: Option<u64>,
}
}

wire! {
pub struct PullThread {
    pub id: String,
    pub resolved: bool,
    pub path: Option<String>,
    pub line: Option<u64>,
    pub comments: Vec<PullComment>,
}
}

wire! {
pub struct PullConversation {
    pub comments: Vec<PullComment>,
    pub threads: Vec<PullThread>,
}
}

wire! {
pub struct PullFile {
    pub path: String,
    pub previous_path: Option<String>,
    /// `added`, `modified`, `deleted` or `renamed`.
    pub status: String,
    pub additions: u64,
    pub deletions: u64,
    pub patch: Option<String>,
    /// Marked viewed. On a GitHub pull request this is GitHub's own `viewerViewedState` for the
    /// file; on a forge without viewed files, or while GitHub cannot be asked, it is this
    /// service's mark, kept per project.
    pub viewed: bool,
    /// The mark is GitHub's own. False when this service keeps the mark itself: on a forge with no
    /// viewed files, while GitHub's state cannot be read, or when GitHub did not save a mark the
    /// person made here (the mark is then still shown, never dropped).
    pub viewed_synced: bool,
}
}

wire! {
/// Counts by change kind over one pull request's changed files. A kind the forge
/// names that is none of the four below counts only in `files`.
pub struct PullChangeBreakdown {
    pub files: u64,
    pub added: u64,
    pub modified: u64,
    pub deleted: u64,
    pub renamed: u64,
    /// Lines added over every file.
    pub additions: u64,
    /// Lines deleted over every file.
    pub deletions: u64,
}
}

wire! {
/// One commit of a pull request, oldest first.
pub struct PullRevision {
    pub sha: String,
    pub message: String,
    pub author: Option<String>,
    pub committed_at: Option<String>,
}
}

wire! {
pub struct PullTimelineEvent {
    pub id: String,
    /// The forge's own event name, such as `reviewed` or `labeled`.
    pub kind: String,
    pub actor: Option<String>,
    pub created_at: Option<String>,
    pub text: Option<String>,
}
}

wire! {
pub struct PullCandidate {
    pub name: String,
    pub description: Option<String>,
}
}

wire! {
pub struct AddPullComment {
    pub body: String,
    /// With `line`, a comment on that line of the new file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub line: Option<u64>,
    /// A comment or thread id to reply to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reply_to: Option<String>,
}
}

wire! {
pub struct EditPullComment {
    pub kind: String,
    pub body: String,
}
}

wire! {
pub struct ReactPullComment {
    pub kind: String,
    /// GitHub's names: `+1`, `-1`, `laugh`, `hooray`, `confused`, `heart`,
    /// `rocket` or `eyes`.
    pub content: String,
}
}

wire! {
pub struct ResolvePullThread {
    pub resolved: bool,
}
}

wire! {
pub struct ReviewLineComment {
    pub path: String,
    pub line: u64,
    pub body: String,
}
}

wire! {
pub struct SubmitReview {
    /// `approve`, `request_changes` or `comment`.
    pub event: String,
    #[serde(default)]
    pub body: String,
    /// Line comments sent with the review.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<ReviewLineComment>>")]
    pub comments: Vec<ReviewLineComment>,
}
}

wire! {
pub struct SetPullPeople {
    #[serde(default)]
    pub add: Vec<String>,
    #[serde(default)]
    pub remove: Vec<String>,
}
}

wire! {
/// `PATCH` on a pull request. Absent fields are left alone.
#[derive(Default)]
pub struct UpdatePull {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub draft: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub maintainer_can_modify: Option<bool>,
}
}

wire! {
pub struct PullAction {
    /// `close`, `reopen`, `revert`, `update_branch`, `rebase` or `delete_branch`.
    pub action: String,
}
}

wire! {
pub struct BulkClosePulls {
    pub numbers: Vec<u64>,
}
}

wire! {
pub struct PullBulkResult {
    pub number: u64,
    pub ok: bool,
    pub message: Option<String>,
}
}

wire! {
pub struct SetFilesViewed {
    pub paths: Vec<String>,
    pub viewed: bool,
}
}

wire! {
pub struct PullListEntry {
    pub project_id: String,
    pub pull: PullRequest,
}
}

wire! {
pub struct PullList {
    pub pulls: Vec<PullListEntry>,
    /// Projects that could not be read, so a partial list is never silent.
    pub failed: Vec<PullListFailure>,
}
}

wire! {
pub struct PullListFailure {
    pub project_id: String,
    pub message: String,
}
}

wire! {
pub struct PullStack {
    /// Bottom layer first.
    pub layers: Vec<PullRequest>,
    /// Index of the pull request asked about.
    pub position: usize,
}
}

wire! {
pub struct StackAction {
    /// `merge` or `rebase`.
    pub action: String,
    /// `merge`, `squash` or `rebase`, for `merge`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub method: Option<String>,
}
}

wire! {
pub struct PullWatch {
    pub thread_id: String,
    /// The repository the watch belongs to: the project's id for the primary repository, as every `repo_id` here.
    pub repo_id: String,
    pub number: u64,
    pub created_at: String,
}
}

wire! {
pub struct CheckoutPull {
    /// True makes a worktree under the project, false checks out in place.
    pub worktree: bool,
}
}

wire! {
pub struct PullCheckout {
    pub path: String,
    pub branch: String,
    /// Set when the local head is not the pull request's latest commit.
    pub warning: Option<String>,
}
}

wire! {
pub struct WorkOnPull {
    pub engine: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
}
}

wire! {
/// One file a guide stop covers, with what to look for in it.
pub struct PullGuideFile {
    pub path: String,
    pub note: String,
}
}

wire! {
/// One stop of a review guide, in reading order.
pub struct PullGuideStop {
    pub title: String,
    pub summary: String,
    pub files: Vec<PullGuideFile>,
    pub questions: Vec<String>,
}
}

wire! {
/// A review guide an agent wrote for one pull request. It is this service's own
/// and never reaches the forge.
pub struct PullGuide {
    pub number: u64,
    /// The commit the writing agent says it described.
    pub head_sha: Option<String>,
    pub stops: Vec<PullGuideStop>,
    /// The thread that wrote it.
    pub thread_id: Option<String>,
    pub written_at: String,
}
}

wire! {
/// What the guide tab shows: whether guides are on for the project, and the
/// stored guide if there is one.
pub struct PullGuideState {
    pub enabled: bool,
    pub guide: Option<PullGuide>,
}
}

wire! {
/// Starts a thread that writes the guide. An absent model uses the project's
/// review guide model, then the engine's default.
pub struct GeneratePullGuide {
    pub engine: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
}
}

wire! {
/// Opens a thread about one guide stop, counted from zero.
pub struct AskAboutStop {
    pub engine: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
    pub stop: u32,
}
}

wire! {
/// The new thread and the composer draft that carries the stop.
pub struct AskedAboutStop {
    pub thread: Thread,
    pub draft: String,
}
}

wire! {
pub struct ForgeStatus {
    /// `github`, `gitlab`, `azure`, `forgejo` or `bitbucket`.
    pub id: String,
    pub name: String,
    /// The command this forge goes through; `None` when none exists.
    pub program: Option<String>,
    pub installed: bool,
    pub version: Option<String>,
    pub signed_in: bool,
    pub accounts: Vec<String>,
    /// The login the CLI is signed in as; `None` when signed out or the forge has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub viewer: Option<String>,
    pub problem: Option<String>,
}
}

wire! {
pub struct MergePull {
    /// `merge`, `squash` or `rebase`.
    pub method: String,
    /// Merge when requirements pass, rather than now.
    #[serde(default)]
    pub auto: bool,
    #[serde(default)]
    pub delete_branch: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub subject: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub body: Option<String>,
    /// Removes agent credit lines from the merge commit body.
    #[serde(default)]
    pub strip_credits: bool,
    /// This merge's own choice to remove agent credit lines. When present it wins over the account setting
    /// "Remove agent credits on merge", in both directions; absent, the setting (or `strip_credits`) applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub remove_agent_credits: Option<bool>,
    /// The head commit the person saw. The forge refuses the merge if the branch has moved on.
    /// Only GitHub and GitLab (not a rebase) are fully guarded: the forge itself refuses a moved head.
    /// For Azure, Forgejo, Bitbucket and a GitLab rebase the service reads the head just before the
    /// merge and refuses a mismatch, which leaves a small window between that read and the merge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub expected_head: Option<String>,
}
}

wire! {
pub struct LinkPull {
    pub number: u64,
}
}

wire! {
pub struct RepositoryStarStatus {
    pub starred: bool,
}
}

wire! {
/// The person's starred GitHub repositories, read through their own `gh` CLI. One
/// bounded page is read; `unavailable` says why none was, and an empty list there is
/// never reported as "you starred nothing".
pub struct StarredRepositories {
    /// As the forge lists them, at most one page of 100.
    pub repositories: Vec<StarredRepository>,
    /// The forge holds more than the one page read.
    pub truncated: bool,
    /// Why no list could be read, in the person's words; `None` when one was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub unavailable: Option<String>,
}
}

wire! {
/// One repository from the person's starred list. The app stores none of it.
pub struct StarredRepository {
    /// The owner's login, from `full_name`.
    pub owner: String,
    /// The repository's name, from `full_name`.
    pub name: String,
    /// `owner/name` as the forge spells it.
    pub full_name: String,
    pub url: String,
    pub description: Option<String>,
    pub private: bool,
    pub archived: bool,
    pub default_branch: Option<String>,
    pub language: Option<String>,
    /// Stars the forge reports.
    pub stars: u32,
}
}

#[cfg(test)]
mod tests {
    use super::PullDetails;
    use serde_json::json;

    #[test]
    fn approval_count_is_additive_and_distinguishes_unknown_from_zero() {
        let old = json!({"project_id":"p", "number":7});
        let unknown: PullDetails = serde_json::from_value(old.clone()).unwrap();
        assert_eq!(serde_json::to_value(unknown).unwrap(), old);
        for count in [0, 2] {
            let known = json!({"project_id":"p", "number":7, "approval_count":count});
            let details: PullDetails = serde_json::from_value(known.clone()).unwrap();
            assert_eq!(serde_json::to_value(details).unwrap(), known);
        }
    }
}
