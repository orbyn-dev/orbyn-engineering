//! Issue trackers beside the code (docs/issues.md): the issues a tracker reports, the connections that read
//! them, and a thread's links to them. Field names follow the T3 Code issue schema (MIT, 9168ca73d).
//! Everything a tracker returns (titles, bodies, labels, names) is untrusted text.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

wire_enum! {
pub enum TrackerKind {
    Github,
    Gitlab,
    AzureDevops,
    Forgejo,
    Linear,
}
}

wire! {
/// One tracker as the picker lists it. A connection never carries its token.
pub struct TrackerRow {
    pub tracker: TrackerKind,
    pub connected: bool,
    /// What it is connected to: a repository, a team, a project.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub scope: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub host: Option<String>,
    /// Why it is not connected, in the service's words ("gh is not installed").
    #[serde(default)]
    #[ts(optional = nullable)]
    pub detail: Option<String>,
}
}

wire! {
/// Connects a tracker. `token` is absent for GitHub, which reads through the person's own `gh`.
pub struct ConnectTracker {
    #[serde(default)]
    #[ts(optional = nullable)]
    pub host: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub scope: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub token: Option<String>,
}
}

wire_enum! {
pub enum IssueStateCategory {
    Open,
    Closed,
}
}

wire_enum! {
pub enum IssueCloseReason {
    Completed,
    NotPlanned,
}
}

wire! {
/// The tracker's own word for where an issue stands, and the category it falls in.
pub struct IssueState {
    pub category: IssueStateCategory,
    pub name: String,
    /// How it closed, when the tracker says.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub close_reason: Option<IssueCloseReason>,
}
}

wire! {
pub struct IssueActor {
    pub login: String,
}
}

wire! {
pub struct IssueLabel {
    pub name: String,
}
}

wire! {
/// Addresses one issue: `scope` is the tracker's container (`owner/repo`, a team key) and `key` its native id.
pub struct IssueKey {
    pub tracker: TrackerKind,
    pub scope: String,
    pub key: String,
}
}

wire! {
pub struct IssueSummary {
    pub tracker: TrackerKind,
    pub scope: String,
    pub key: String,
    /// How the tracker writes the key: `#412`, `ORB-87`.
    pub display_key: String,
    pub title: String,
    pub url: String,
    pub author: Option<IssueActor>,
    pub state: IssueState,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub closed_at: Option<String>,
    pub assignees: Vec<IssueActor>,
    pub labels: Vec<IssueLabel>,
    pub comment_count: u32,
}
}

wire! {
pub struct IssueComment {
    pub id: String,
    pub author: Option<IssueActor>,
    pub body: String,
    pub created_at: String,
}
}

wire_enum! {
pub enum LinkedPullState {
    Open,
    Closed,
    Merged,
}
}

wire! {
/// A change request the tracker says closes or mentions the issue. Read only.
pub struct IssueLinkedPull {
    pub repository: String,
    pub number: u32,
    pub title: String,
    pub url: String,
    pub state: LinkedPullState,
    pub is_draft: bool,
    pub closes_issue: bool,
}
}

wire! {
pub struct IssueDetail {
    #[serde(flatten)]
    #[ts(flatten)]
    pub summary: IssueSummary,
    pub body: String,
    /// The first 50, oldest first.
    pub comments: Vec<IssueComment>,
    pub linked_pull_requests: Vec<IssueLinkedPull>,
    /// Threads that link this issue.
    pub linked_threads: Vec<String>,
}
}

wire! {
/// What a thread's link keeps of an issue: enough for a chip, a list and a row marker.
pub struct IssueRef {
    pub tracker: TrackerKind,
    pub scope: String,
    pub key: String,
    pub display_key: String,
    pub title: String,
    pub state: IssueState,
    pub labels: Vec<IssueLabel>,
}
}

wire! {
pub struct IssueCounts {
    pub open: u32,
    pub closed: u32,
}
}

wire! {
/// A tracker that did not answer while others did.
pub struct IssueListFailure {
    pub tracker: TrackerKind,
    pub message: String,
}
}

wire! {
pub struct IssueList {
    pub issues: Vec<IssueSummary>,
    /// Totals before label, assignee and text filters; absent when the tracker cannot say.
    pub counts: Option<IssueCounts>,
    pub failed: Vec<IssueListFailure>,
    /// Where each tracker and scope carries on, keyed `"<tracker> <scope>"`.
    pub next_cursors: BTreeMap<String, String>,
}
}

wire! {
/// The first message of a thread started from an issue, framed so the issue's text is data (docs/issues.md).
pub struct IssueDraft {
    pub message: String,
}
}

wire! {
/// Creates a thread, links it to the issue and queues `message`, in one write.
pub struct StartIssueThread {
    pub tracker: TrackerKind,
    pub scope: String,
    pub key: String,
    pub message: String,
    pub project_id: String,
}
}
