//! Issue trackers beside the code (docs/issues.md): the issues a tracker reports, the connections that read
//! them, and a thread's links to them. Field names follow the T3 Code issue schema (MIT, 9168ca73d).
//! Everything a tracker returns (titles, bodies, labels, names) is untrusted text.

use serde::{Deserialize, Deserializer, Serialize};
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

/// Connects a tracker. `token` is absent for GitHub, which reads through the person's own `gh`.
#[derive(Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../web/src/generated/")]
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

/// The token is a secret: this type never prints it, as `SetEnvValue` does. The host and scope are shown,
/// because they are the person's own words and carry no secret.
impl std::fmt::Debug for ConnectTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectTracker")
            .field("host", &self.host)
            .field("scope", &self.scope)
            .finish_non_exhaustive()
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

wire_enum! {
/// The reaction a person adds or takes back (docs/issues.md, "Create, comment, react"). Every tracker that
/// writes issues has this set or a smaller one; a smaller one refuses a value it lacks and names the ones it has.
pub enum IssueReaction {
    ThumbsUp,
    ThumbsDown,
    Laugh,
    Hooray,
    Confused,
    Heart,
    Rocket,
    Eyes,
}
}

wire! {
/// One reaction the tracker reports on an issue, as a count. `content` is the tracker's own word for it
/// (`HEART` on GitHub), so a reader that does not recognize one still shows the tracker's word and its count.
pub struct IssueReactionCount {
    pub content: String,
    pub count: u32,
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
    /// The reactions on the issue as the tracker reports them, in the tracker's own words. Absent when the
    /// tracker does not report them; a write route answers them for a tracker that does (docs/issues.md).
    #[serde(default)]
    #[ts(optional = nullable)]
    pub reactions: Option<Vec<IssueReactionCount>>,
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

wire! {
/// A new issue (docs/issues.md, "Create, comment, react"). `title` is one line, 1 to 256 characters and `body`
/// at most 64 KiB, both the person's own text; `labels` (at most 50) are names and `assignees` (at most 25) are
/// the `id`s a tracker's candidates give, never a login typed by hand. `template` names a template from the
/// templates read, which fills a missing body (R15 PR2); `title` and `body` are the new issue's text and are
/// untrusted as every issue's text is.
pub struct CreateIssue {
    pub tracker: TrackerKind,
    pub scope: String,
    pub title: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub body: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub assignees: Vec<String>,
    /// The id of one of the tracker's templates for this scope; an id the tracker does not report is `400 invalid`.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub template: Option<String>,
}
}

wire! {
/// One comment on one issue (docs/issues.md, "Create, comment, react"): `body` is 1 to 64 KiB and not blank.
pub struct CommentIssue {
    pub tracker: TrackerKind,
    pub scope: String,
    pub key: String,
    pub body: String,
}
}

wire! {
/// Adds (`on: true`) or takes back (`on: false`) one reaction, on a comment when `comment_id` is present and on
/// the issue itself when it is absent. Both directions are idempotent (docs/issues.md, "Create, comment, react").
pub struct ReactIssue {
    pub tracker: TrackerKind,
    pub scope: String,
    pub key: String,
    /// A comment id as `IssueDetail.comments` carried it.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub comment_id: Option<String>,
    pub reaction: IssueReaction,
    pub on: bool,
}
}

wire! {
/// One starting point the tracker offers for a new issue (docs/issues.md, "Create, comment, react, templates and
/// project binding"). The text is the tracker's own and as untrusted as any issue text: it only ever fills the
/// person's own create form.
pub struct IssueTemplate {
    pub id: String,
    pub name: String,
    /// The one-line description the tracker gives the template, when it gives one.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub description: Option<String>,
    pub body: String,
    pub labels: Vec<String>,
}
}

/// A required field that may be `null`: a present `null` is `None`, a missing field is refused (serde would
/// otherwise read a missing `Option` as `None`).
fn required<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

wire! {
/// Binds a project to the connected tracker and scope its issues come from, or unbinds it (docs/issues.md,
/// "Create, comment, react, templates and project binding"). `tracker: null` unbinds; a binding names one scope.
pub struct SetProjectIssueTracker {
    /// The tracker to bind; `null` unbinds. The field is required: a body without it is refused rather than read as
    /// an unbind a client did not ask for.
    #[serde(deserialize_with = "required")]
    pub tracker: Option<TrackerKind>,
    /// The one scope of the connection the project's issues come from; required when binding.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub scope: Option<String>,
}
}

wire! {
/// The tracker and scope a project's issues now come from.
pub struct ProjectIssueTracker {
    pub tracker: TrackerKind,
    pub scope: String,
}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn react(body: &str) -> Result<ReactIssue, serde_json::Error> {
        serde_json::from_str(body)
    }

    #[test]
    fn an_unknown_reaction_is_refused_when_the_body_is_parsed() {
        let error = react(
            r#"{"tracker":"github","scope":"acme/app","key":"7","reaction":"facepalm","on":true}"#,
        )
        .unwrap_err();
        assert!(error.to_string().contains("unknown variant"), "{error}");
        let accepted = react(
            r#"{"tracker":"github","scope":"acme/app","key":"7","reaction":"thumbs_up","on":true}"#,
        )
        .unwrap();
        assert_eq!(accepted.reaction, IssueReaction::ThumbsUp);
        assert_eq!(accepted.comment_id, None);
        let removed = react(
            r#"{"tracker":"github","scope":"acme/app","key":"7","comment_id":"IC_1","reaction":"heart","on":false}"#,
        )
        .unwrap();
        assert_eq!(removed.comment_id.as_deref(), Some("IC_1"));
        assert!(!removed.on);
    }

    #[test]
    fn a_project_tracker_body_must_name_the_tracker_to_unbind_explicitly() {
        let explicit: SetProjectIssueTracker = serde_json::from_str(r#"{"tracker":null}"#).unwrap();
        assert_eq!(explicit.tracker, None);
        assert_eq!(explicit.scope, None);
        let missing = serde_json::from_str::<SetProjectIssueTracker>(r#"{"scope":"o/r"}"#);
        assert!(
            missing.is_err(),
            "a body that does not name the tracker is refused, not read as an unbind"
        );
        let bound: SetProjectIssueTracker =
            serde_json::from_str(r#"{"tracker":"github","scope":"o/r"}"#).unwrap();
        assert_eq!(bound.tracker, Some(TrackerKind::Github));
        assert_eq!(bound.scope.as_deref(), Some("o/r"));
    }

    #[test]
    fn a_template_may_leave_its_description_out_and_round_trips() {
        let template: IssueTemplate = serde_json::from_str(
            r#"{"id":"bug","name":"Bug report","body":"Steps:","labels":["bug"]}"#,
        )
        .unwrap();
        assert_eq!(template.description, None);
        assert_eq!(template.id, "bug");
        let described: IssueTemplate = serde_json::from_str(
            r#"{"id":"bug","name":"Bug report","description":"A crash","body":"Steps:","labels":[]}"#,
        )
        .unwrap();
        assert_eq!(described.description.as_deref(), Some("A crash"));
    }

    #[test]
    fn a_connect_request_never_prints_its_token() {
        let connect = ConnectTracker {
            host: Some("https://dev.azure.com".into()),
            scope: Some("org/project".into()),
            token: Some("SECRET-tracker-token".into()),
        };
        let printed = format!("{connect:?}");
        assert!(!printed.contains("SECRET"), "{printed}");
        assert!(!printed.to_ascii_lowercase().contains("token"), "{printed}");
        // The non-secret fields stay visible, so a failure that names what was refused is readable.
        assert!(
            printed.contains("dev.azure.com") && printed.contains("org/project"),
            "{printed}"
        );
        let parsed: ConnectTracker = serde_json::from_str(
            r#"{"host":"https://dev.azure.com","scope":"org","token":"SECRET-2"}"#,
        )
        .unwrap();
        assert!(!format!("{parsed:?}").contains("SECRET-2"));
    }

    #[test]
    fn a_create_body_may_leave_the_optional_fields_out() {
        let input: CreateIssue = serde_json::from_str(
            r#"{"tracker":"gitlab","scope":"group/app","title":"Crash on start"}"#,
        )
        .unwrap();
        assert_eq!(input.title, "Crash on start");
        assert_eq!(input.body, None);
        assert!(input.labels.is_empty() && input.assignees.is_empty());
        assert_eq!(input.template, None);
    }

    #[test]
    fn an_issue_detail_reactions_are_absent_unless_they_are_reported() {
        let plain: IssueDetail = serde_json::from_str(
            r##"{"tracker":"github","scope":"acme/app","key":"7","display_key":"#7","title":"T",
                "url":"u","author":null,"state":{"category":"open","name":"Open"},
                "created_at":"c","updated_at":"u","assignees":[],"labels":[],"comment_count":0,
                "body":"b","comments":[],"linked_pull_requests":[],"linked_threads":[]}"##,
        )
        .unwrap();
        assert_eq!(plain.reactions, None);
        let reported: IssueDetail = serde_json::from_str(
            r##"{"tracker":"github","scope":"acme/app","key":"7","display_key":"#7","title":"T",
                "url":"u","author":null,"state":{"category":"open","name":"Open"},
                "created_at":"c","updated_at":"u","assignees":[],"labels":[],"comment_count":0,
                "body":"b","comments":[],"linked_pull_requests":[],"linked_threads":[],
                "reactions":[{"content":"HEART","count":2}]}"##,
        )
        .unwrap();
        assert_eq!(
            reported.reactions,
            Some(vec![IssueReactionCount {
                content: "HEART".into(),
                count: 2,
            }])
        );
    }
}
