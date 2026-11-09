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
    /// The signed-in viewer's latest review: `approved`, `changes_requested`,
    /// `commented`, `dismissed`, `pending` or `none`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub viewer_review_state: Option<String>,
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
    /// Marked viewed in Orbyn. Marks are kept by this service, per project.
    pub viewed: bool,
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
