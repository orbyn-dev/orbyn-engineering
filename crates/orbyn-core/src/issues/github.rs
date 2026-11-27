//! GitHub issues through the person's own `gh` (docs/issues.md). Orbyn never reads gh's token. The JSON
//! `gh` prints is the tracker's text and is read defensively: a field that is missing or the wrong type
//! becomes empty, never an error that hides the other issues.

use super::query::{AssigneeFilter, IssueQuery, IssueSort, StateFilter};
use super::{
    DependencyFeed, DependencyUpdate, IssueEdit, IssueSource, LabelEvent, LabelFeed, NewIssue,
    ScopeList,
};
use crate::error::{CoreError, Result};
use crate::source_control::forge::{Forge, call, call_input, github_checks};
use async_trait::async_trait;
use orbyn_contracts::{
    IssueActivity, IssueActivityKind, IssueActor, IssueAssigneeCandidate, IssueCandidateLabel,
    IssueCandidates, IssueCloseReason, IssueComment, IssueCounts, IssueDetail, IssueLabel,
    IssueLinkedPull, IssueReaction, IssueReactionCount, IssueState, IssueStateCategory,
    IssueSummary, LinkedPullState, PullChecks, TrackerKind,
};
use serde_json::Value;
use std::path::PathBuf;

const LIST_FIELDS: &str = "number,title,url,author,state,stateReason,createdAt,updatedAt,closedAt,assignees,labels,comments";
/// Open Dependabot alerts per `gh api` page, and the pages one read takes (1000 alerts).
const DEPENDENCY_PAGE_SIZE: usize = 100;
const DEPENDENCY_PAGES: u32 = 10;
const DETAIL_FIELDS: &str = "number,title,url,author,state,stateReason,createdAt,updatedAt,closedAt,assignees,labels,comments,body";
/// The one issue a reaction is added to, by its GraphQL node id: `addReaction` takes a node id, never a number.
const ISSUE_NODE_QUERY: &str = "query OrbynIssueNode($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){issue(number:$number){id}}}";
/// Proves a comment node id belongs to one issue before a reaction is written to it: node ids are global on GitHub,
/// and a Person-authorized caller must not have a reaction written outside the scope the guard checked.
const COMMENT_SCOPE_QUERY: &str = "query OrbynCommentScope($id:ID!){node(id:$id){__typename ... on IssueComment{issue{number repository{nameWithOwner}}}}}";
/// Both directions are idempotent in GitHub: adding a reaction that is already there answers the stored one, and
/// removing one that is not there answers no reaction without an error.
const ADD_REACTION_MUTATION: &str = "mutation OrbynAddReaction($subject:ID!,$content:ReactionContent!){addReaction(input:{subjectId:$subject,content:$content}){reaction{content}}}";
const REMOVE_REACTION_MUTATION: &str = "mutation OrbynRemoveReaction($subject:ID!,$content:ReactionContent!){removeReaction(input:{subjectId:$subject,content:$content}){reaction{content}}}";
const MAX_COMMENTS: usize = 50;
/// The most labels and the most people one candidates read answers (docs/issues.md, "Issue writes and list
/// controls"); the route caps the answer again, so a reader cannot exceed it.
const MAX_CANDIDATES: usize = 200;

pub(crate) struct GitHub {
    /// Where `gh` runs. With `--repo` it needs no repository, so any folder will do.
    pub(crate) cwd: PathBuf,
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn actor(value: &Value) -> Option<IssueActor> {
    value
        .get("login")
        .and_then(Value::as_str)
        .filter(|l| !l.is_empty())
        .map(|login| IssueActor {
            login: login.to_owned(),
        })
}

/// One state from the words the tracker gave: `gh`'s JSON spells the reason `stateReason` and the REST API
/// `state_reason`, so both callers share this.
fn named_state(closed: bool, reason: &str) -> IssueState {
    let close_reason = closed
        .then(|| match reason.to_ascii_uppercase().as_str() {
            "NOT_PLANNED" => Some(IssueCloseReason::NotPlanned),
            "COMPLETED" => Some(IssueCloseReason::Completed),
            _ => None,
        })
        .flatten();
    let name = match (closed, close_reason) {
        (false, _) => "Open",
        (true, Some(IssueCloseReason::NotPlanned)) => "Closed as not planned",
        (true, _) => "Closed",
    };
    IssueState {
        category: if closed {
            IssueStateCategory::Closed
        } else {
            IssueStateCategory::Open
        },
        name: name.to_owned(),
        close_reason,
    }
}

fn state(value: &Value) -> IssueState {
    named_state(
        text(value, "state").eq_ignore_ascii_case("closed"),
        &text(value, "stateReason"),
    )
}

/// The states a GitHub issue can move to, in GitHub's own words (docs/issues.md, "Issue writes and list
/// controls"). GitHub has one closed state with two reasons, and the two are named apart here so a closed issue's
/// row reads as the state it is in: `Closed` for a completed close, `Closed as not planned` for the other.
fn github_states() -> Vec<IssueState> {
    vec![
        IssueState {
            category: IssueStateCategory::Open,
            name: "Open".into(),
            close_reason: None,
        },
        IssueState {
            category: IssueStateCategory::Closed,
            name: "Closed".into(),
            close_reason: Some(IssueCloseReason::Completed),
        },
        IssueState {
            category: IssueStateCategory::Closed,
            name: "Closed as not planned".into(),
            close_reason: Some(IssueCloseReason::NotPlanned),
        },
    ]
}

/// The close GitHub is asked for, from the state name the issue offered it (`None` reopens). A name GitHub does
/// not offer is refused rather than guessed at.
fn closing(
    state: &str,
    close_reason: Option<IssueCloseReason>,
) -> Result<Option<IssueCloseReason>> {
    match state {
        "Open" => Ok(None),
        "Closed as not planned" => Ok(Some(IssueCloseReason::NotPlanned)),
        "Closed" => Ok(Some(close_reason.unwrap_or(IssueCloseReason::Completed))),
        _ => Err(CoreError::Invalid(
            "That state is not one GitHub offers.".into(),
        )),
    }
}

fn rest_state(value: &Value) -> IssueState {
    named_state(
        text(value, "state").eq_ignore_ascii_case("closed"),
        &text(value, "state_reason"),
    )
}

fn labels_of(value: &Value) -> Vec<IssueLabel> {
    value
        .get("labels")
        .and_then(Value::as_array)
        .map(|labels| {
            labels
                .iter()
                .filter_map(|label| label.get("name").and_then(Value::as_str))
                .map(|name| IssueLabel {
                    name: name.to_owned(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn assignees_of(value: &Value) -> Vec<IssueActor> {
    value
        .get("assignees")
        .and_then(Value::as_array)
        .map(|assignees| assignees.iter().filter_map(actor).collect())
        .unwrap_or_default()
}

/// The reactions `gh issue view --json reactionGroups` reports: each group's own word (`THUMBS_UP`) and how many
/// people used it. A group nobody used is dropped, so the answer is the counts the person can see, not every enum
/// value. `None` when the answer carries no groups at all, so a reader that cannot report them says so rather than
/// guessing zero.
fn reaction_counts(value: &Value) -> Option<Vec<IssueReactionCount>> {
    let groups = value.get("reactionGroups").and_then(Value::as_array)?;
    Some(
        groups
            .iter()
            .filter_map(|group| {
                let content = group
                    .get("content")
                    .and_then(Value::as_str)?
                    .trim()
                    .to_owned();
                if content.is_empty() {
                    return None;
                }
                let count = group
                    .get("users")
                    .and_then(|users| users.get("totalCount"))
                    .and_then(Value::as_u64)
                    .or_else(|| {
                        group
                            .get("users")
                            .and_then(|users| users.get("nodes"))
                            .and_then(Value::as_array)
                            .map(|nodes| nodes.len() as u64)
                    })
                    .unwrap_or(0);
                (count > 0).then(|| IssueReactionCount {
                    content,
                    count: u32::try_from(count).unwrap_or(u32::MAX),
                })
            })
            .collect(),
    )
}

pub(crate) fn summary(scope: &str, value: &Value) -> Option<IssueSummary> {
    let number = value.get("number").and_then(Value::as_u64)?;
    Some(IssueSummary {
        tracker: TrackerKind::Github,
        scope: scope.to_owned(),
        key: number.to_string(),
        display_key: format!("#{number}"),
        title: text(value, "title"),
        url: text(value, "url"),
        author: value.get("author").and_then(actor),
        state: state(value),
        created_at: text(value, "createdAt"),
        updated_at: text(value, "updatedAt"),
        closed_at: Some(text(value, "closedAt")).filter(|t| !t.is_empty()),
        assignees: assignees_of(value),
        labels: labels_of(value),
        comment_count: value
            .get("comments")
            .and_then(Value::as_array)
            .map_or(0, |c| u32::try_from(c.len()).unwrap_or(u32::MAX)),
    })
}

/// The ecosystem as a crew stores it: GitHub names Rust `rust`, a crew (and the Dependabot config) names it `cargo`.
/// Every other name is GitHub's own, lowercased.
fn ecosystem_name(name: &str) -> String {
    let name = name.to_ascii_lowercase();
    if name == "rust" { "cargo".into() } else { name }
}

/// The open Dependabot alerts in `gh api`'s answer: one update per alert, keyed by the alert's number. Only the
/// number and the package ecosystem are read; no package name, advisory text or title is kept, so no alert text
/// can reach a prompt. A missing or wrong-typed field is skipped or none.
pub(crate) fn dependabot_updates(value: &Value) -> Vec<DependencyUpdate> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|alert| {
            let number = alert.get("number").and_then(Value::as_u64)?;
            let ecosystem = alert
                .get("dependency")
                .and_then(|dependency| dependency.get("package"))
                .and_then(|package| package.get("ecosystem"))
                .and_then(Value::as_str)
                .map(ecosystem_name);
            Some(DependencyUpdate {
                key: number.to_string(),
                ecosystem,
            })
        })
        .collect()
}

/// The label events for one stored label in `gh api`'s answer: one event per issue whose label was added, newest
/// first as GitHub answers it. Only the event id and the issue number are read; no actor, title or url is kept, so no
/// event text can reach a prompt. A missing or wrong-typed field is skipped.
pub(crate) fn label_events_of(value: &Value, label: &str) -> Vec<LabelEvent> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|event| {
            if event.get("event").and_then(Value::as_str) != Some("labeled") {
                return None;
            }
            let named = event
                .get("label")
                .and_then(|label| label.get("name"))
                .and_then(Value::as_str)?;
            if !named.eq_ignore_ascii_case(label) {
                return None;
            }
            let id = event.get("id").and_then(Value::as_u64)?;
            let issue = event.get("issue")?;
            // The feed also carries pull requests' events, and an issue that is already closed is no new work.
            if issue.get("pull_request").is_some()
                || issue.get("state").and_then(Value::as_str) == Some("closed")
            {
                return None;
            }
            let number = issue.get("number").and_then(Value::as_u64)?;
            Some(LabelEvent {
                id: id.to_string(),
                issue: number.to_string(),
            })
        })
        .collect()
}

fn comments(value: &Value) -> Vec<IssueComment> {
    value
        .get("comments")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(MAX_COMMENTS)
        .map(|c| IssueComment {
            id: text(c, "id"),
            author: c.get("author").and_then(actor),
            body: text(c, "body"),
            created_at: text(c, "createdAt"),
        })
        .collect()
}

fn linked_pulls(value: &Value) -> Vec<IssueLinkedPull> {
    value
        .get("closedByPullRequestsReferences")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let number = u32::try_from(p.get("number").and_then(Value::as_u64)?).ok()?;
            let repo = p.get("repository");
            let repository = repo
                .and_then(|r| {
                    r.get("nameWithOwner")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .or_else(|| {
                            let owner = r
                                .get("owner")
                                .and_then(|o| o.get("login"))
                                .and_then(Value::as_str)?;
                            Some(format!(
                                "{owner}/{}",
                                r.get("name").and_then(Value::as_str)?
                            ))
                        })
                })
                .unwrap_or_default();
            let state = match text(p, "state").to_ascii_uppercase().as_str() {
                "MERGED" => LinkedPullState::Merged,
                "CLOSED" => LinkedPullState::Closed,
                _ => LinkedPullState::Open,
            };
            Some(IssueLinkedPull {
                repository,
                number,
                title: text(p, "title"),
                url: text(p, "url"),
                state,
                is_draft: p.get("isDraft").and_then(Value::as_bool).unwrap_or(false),
                closes_issue: true,
                // The link list carries no CI: `linked_checks` reads it beside the link, and absent is unknown.
                checks: None,
            })
        })
        .collect()
}

/// How many linked pull requests one issue read asks `gh` about. A link past it is still shown, with no `checks`
/// (unknown, over the read limit): each read is an API call against the account's shared hourly limit.
pub(crate) const LINKED_CHECKS_READ_LIMIT: usize = 20;

/// Where one linked pull request's checks stand, read beside the link: the same rollup counts the pull request
/// list carries. One `gh pr view` per link, best effort — a pull request that is gone, a repository the person
/// cannot read or a `gh` that does not answer leaves `checks` absent (unknown, never zero checks), and the other
/// links are still shown. Nothing here fails the detail read (docs/issues.md).
async fn linked_checks(cwd: &std::path::Path, repository: &str, number: u32) -> Option<PullChecks> {
    // The repository is the tracker's text: it is checked like a scope before it becomes a `--repo` argument, so a
    // link that names none, or names something that is not `owner/repo`, is not read at all.
    scope_ok(repository).ok()?;
    let number = number.to_string();
    let args: Vec<String> = [
        "pr",
        "view",
        &number,
        "--repo",
        repository,
        "--json",
        "statusCheckRollup",
    ]
    .map(str::to_owned)
    .into();
    let answer = run(cwd, &args).await.ok()?;
    github_checks(&answer)
}

/// How many events one issue read keeps. The timeline read asks for one bounded page (the state changes, newest last),
/// and a hostile answer is cut here again (docs/issues.md).
const MAX_ACTIVITY: usize = 50;
/// The state changes on one issue's own timeline: the closed event the activity shows and a reopen after it. `last:`
/// keeps the newest page, so an issue with a long history still answers its state changes in one bounded read, and the
/// item types are named so the answer is these events only — never the labels, assignments and comments GitHub keeps on
/// the same timeline.
const ISSUE_ACTIVITY_QUERY: &str = "query OrbynIssueActivity($owner:String!,$name:String!,$number:Int!,$last:Int!){repository(owner:$owner,name:$name){issue(number:$number){timelineItems(last:$last,itemTypes:[CLOSED_EVENT,REOPENED_EVENT]){nodes{__typename ... on ClosedEvent{createdAt actor{login} stateReason} ... on ReopenedEvent{createdAt actor{login}}}}}}}";

/// The events in a GraphQL timeline answer, oldest first. A node that is not a state change, or that the tracker did not
/// date, is not an event. A close whose reason this GitHub does not have keeps the event and leaves the reason off, and a
/// reopen never carries one (docs/issues.md).
fn activity(value: &Value) -> Vec<IssueActivity> {
    value
        .pointer("/data/repository/issue/timelineItems/nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|node| {
            let kind = match node.get("__typename").and_then(Value::as_str) {
                Some("ClosedEvent") => IssueActivityKind::Closed,
                Some("ReopenedEvent") => IssueActivityKind::Reopened,
                _ => return None,
            };
            let at = text(node, "createdAt");
            // An event nothing dates could not be placed in the activity, so it is not shown.
            if at.is_empty() {
                return None;
            }
            let reason = match node.get("stateReason").and_then(Value::as_str) {
                Some("COMPLETED") => Some(IssueCloseReason::Completed),
                Some("NOT_PLANNED") => Some(IssueCloseReason::NotPlanned),
                _ => None,
            };
            Some(IssueActivity {
                kind,
                actor: node.get("actor").and_then(actor),
                at,
                close_reason: match kind {
                    IssueActivityKind::Closed => reason,
                    IssueActivityKind::Reopened => None,
                },
            })
        })
        .take(MAX_ACTIVITY)
        .collect()
}

/// The state changes on an issue's own timeline, read beside the detail: one bounded GraphQL page through the person's
/// own `gh`, best effort. A timeline the account cannot read, a `gh` that does not answer and an issue that is gone all
/// leave no event known — never "nothing happened" — and none of them fails the detail read (docs/issues.md).
async fn activity_events(cwd: &std::path::Path, scope: &str, key: &str) -> Vec<IssueActivity> {
    // The scope is checked like every other one before it becomes a GraphQL argument; a scope that is not owner/repo
    // names no repository, so there is no timeline to read.
    let Some((owner, name)) = scope.split_once('/') else {
        return Vec::new();
    };
    let args = vec![
        "api".to_owned(),
        "graphql".to_owned(),
        "-f".to_owned(),
        format!("query={ISSUE_ACTIVITY_QUERY}"),
        "-f".to_owned(),
        format!("owner={owner}"),
        "-f".to_owned(),
        format!("name={name}"),
        "-F".to_owned(),
        format!("number={key}"),
        "-F".to_owned(),
        format!("last={MAX_ACTIVITY}"),
    ];
    match graphql(cwd, &args).await {
        Ok(answer) => activity(&answer),
        Err(_) => Vec::new(),
    }
}

/// A failure of `gh`, in the service's own words. Raw output is never passed on.
pub(crate) fn classify(error: CoreError) -> CoreError {
    let message = match &error {
        CoreError::Unsupported(_) => {
            return CoreError::TrackerNotConnected(
                "Install gh and sign in to it to read GitHub issues.".into(),
            );
        }
        CoreError::Git(message) => message.to_lowercase(),
        _ => return CoreError::TrackerUnavailable("GitHub did not answer.".into()),
    };
    let any = |phrases: &[&str]| phrases.iter().any(|p| message.contains(p));
    if any(&[
        "gh auth login",
        "not logged in",
        "authentication required",
        "bad credentials",
        "http 401",
    ]) {
        CoreError::TrackerNotConnected("Sign in to gh to read GitHub issues.".into())
    } else if any(&["rate limit", "http 429", "secondary rate"]) {
        CoreError::TrackerRateLimited("GitHub asked us to slow down. Try again in a minute.".into())
    } else if any(&[
        "could not resolve to an issue",
        "could not resolve to a node",
        "http 404",
        "not found",
    ]) {
        CoreError::NotFound("issue".into())
    } else if any(&[
        "http 403",
        "resource not accessible",
        "forbidden",
        "must have admin",
    ]) {
        CoreError::TrackerForbidden("GitHub refused access to this repository.".into())
    } else {
        CoreError::TrackerUnavailable("GitHub did not answer.".into())
    }
}

/// The arguments of `gh issue list` for one scope. No shell reads them.
pub(crate) fn list_args(scope: &str, query: &IssueQuery) -> Vec<String> {
    let mut args: Vec<String> = [
        "issue",
        "list",
        "--repo",
        scope,
        "--json",
        LIST_FIELDS,
        "--limit",
    ]
    .map(str::to_owned)
    .into();
    args.push(query.limit.to_string());
    args.push("--state".into());
    args.push(
        match query.state {
            StateFilter::Open => "open",
            StateFilter::Closed => "closed",
            StateFilter::All => "all",
        }
        .into(),
    );
    for label in &query.labels {
        args.push("--label".into());
        args.push(label.clone());
    }
    // `gh issue list` has no order flag: the order is a search qualifier, so it is spelled out rather than left
    // to gh's own default. The person's own text follows it, and is still one value of `--search`.
    let mut search = format!(
        "sort:{}",
        match query.sort {
            IssueSort::Updated => "updated-desc",
            IssueSort::Created => "created-desc",
        }
    );
    if !query.text.is_empty() {
        search.push(' ');
        search.push_str(&query.text);
    }
    match &query.assignee {
        AssigneeFilter::Any => {}
        AssigneeFilter::Me => args.extend(["--assignee".to_owned(), "@me".to_owned()]),
        AssigneeFilter::Login(login) => args.extend(["--assignee".to_owned(), login.clone()]),
        AssigneeFilter::NoOne => {
            search = format!("{search} no:assignee").trim().to_owned();
        }
    }
    args.push("--search".into());
    args.push(search);
    args
}

fn unreadable() -> CoreError {
    CoreError::TrackerUnavailable("GitHub answered something unreadable.".into())
}

fn parse_json(out: &str) -> Result<Value> {
    serde_json::from_str(out).map_err(|_| unreadable())
}

/// One `gh` read whose answer must be JSON.
async fn run(cwd: &std::path::Path, args: &[String]) -> Result<Value> {
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = call(Forge::GitHub, cwd, &refs).await.map_err(classify)?;
    parse_json(&out)
}

/// One GraphQL call through the person's `gh`. `gh api graphql` can exit zero while its answer holds an `errors`
/// array, so a refused mutation is read there and never mistaken for a success; the words are classified as every
/// other `gh` failure is and never passed on.
async fn graphql(cwd: &std::path::Path, args: &[String]) -> Result<Value> {
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = call(Forge::GitHub, cwd, &refs).await.map_err(classify)?;
    let value = parse_json(&out)?;
    if let Some(errors) = value
        .get("errors")
        .and_then(Value::as_array)
        .filter(|errors| !errors.is_empty())
    {
        let words: Vec<&str> = errors
            .iter()
            .filter_map(|error| error.get("message").and_then(Value::as_str))
            .collect();
        return Err(classify(CoreError::Git(words.join(" "))));
    }
    Ok(value)
}

/// A create whose outcome nobody saw: the issue may exist now, so the answer says so and nothing retries. A refusal
/// keeps its own meaning and changes nothing.
fn possibly_created(error: CoreError) -> CoreError {
    match error {
        CoreError::TrackerUnavailable(_) => CoreError::TrackerUnavailable(
            "GitHub did not answer, so the issue was possibly created. Re-read the list before writing it again."
                .into(),
        ),
        other => other,
    }
}

/// The comment a REST answer stored: its node id as text (the numeric id when the reply has none), its author, its
/// body as GitHub kept it and when.
fn stored_comment(value: &Value) -> Result<IssueComment> {
    // The node id is what a reaction takes; the REST reply also carries its numeric id, which can only name the
    // comment for an edit or a delete. A reply without a node id (an older shape) falls back to the numeric one.
    let id = value
        .get("node_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            value.get("id").and_then(|id| {
                id.as_u64()
                    .map(|id| id.to_string())
                    .or_else(|| id.as_str().map(str::to_owned))
            })
        })
        .filter(|id| !id.is_empty())
        .ok_or_else(unreadable)?;
    Ok(IssueComment {
        id,
        author: value.get("user").and_then(actor),
        body: text(value, "body"),
        created_at: text(value, "created_at"),
    })
}

/// A summary from the REST issue shape `POST /repos/{scope}/issues` answers with: the same fields as a read, in the
/// API's own spelling.
fn rest_summary(scope: &str, value: &Value) -> Option<IssueSummary> {
    let number = value.get("number").and_then(Value::as_u64)?;
    Some(IssueSummary {
        tracker: TrackerKind::Github,
        scope: scope.to_owned(),
        key: number.to_string(),
        display_key: format!("#{number}"),
        title: text(value, "title"),
        url: text(value, "html_url"),
        author: value.get("user").and_then(actor),
        state: rest_state(value),
        created_at: text(value, "created_at"),
        updated_at: text(value, "updated_at"),
        closed_at: Some(text(value, "closed_at")).filter(|t| !t.is_empty()),
        assignees: assignees_of(value),
        labels: labels_of(value),
        comment_count: value
            .get("comments")
            .and_then(Value::as_u64)
            .map_or(0, |count| u32::try_from(count).unwrap_or(u32::MAX)),
    })
}

/// The issue a create just made, from the create's own answer: nothing is read again, so a successful create cannot
/// be reported as a failure. A new issue has no comments.
fn created_detail(scope: &str, value: &Value) -> Result<IssueDetail> {
    let summary = rest_summary(scope, value).ok_or_else(unreadable)?;
    Ok(IssueDetail {
        summary,
        body: text(value, "body"),
        comments: Vec::new(),
        activity: Vec::new(),
        linked_pull_requests: Vec::new(),
        linked_threads: Vec::new(),
        reactions: None,
        states: Some(github_states()),
    })
}

/// One PATCH of the issue's own fields, the person's text in the body's field values and never in an argument. The
/// answer is not read here: the caller reads the issue back for the shape a detail read has.
async fn patch_issue(cwd: &std::path::Path, scope: &str, key: &str, payload: &Value) -> Result<()> {
    let args: Vec<String> = vec![
        "api".to_owned(),
        "-X".to_owned(),
        "PATCH".to_owned(),
        format!("repos/{scope}/issues/{key}"),
        "--input".to_owned(),
        "-".to_owned(),
    ];
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    call_input(Forge::GitHub, cwd, &refs, &payload.to_string())
        .await
        .map_err(classify)?;
    Ok(())
}

/// The tracker's own word for one reaction, as its GraphQL `ReactionContent` spells it.
fn reaction_content(reaction: IssueReaction) -> &'static str {
    match reaction {
        IssueReaction::ThumbsUp => "THUMBS_UP",
        IssueReaction::ThumbsDown => "THUMBS_DOWN",
        IssueReaction::Laugh => "LAUGH",
        IssueReaction::Hooray => "HOORAY",
        IssueReaction::Confused => "CONFUSED",
        IssueReaction::Heart => "HEART",
        IssueReaction::Rocket => "ROCKET",
        IssueReaction::Eyes => "EYES",
    }
}

/// A GitHub issue key is a number, checked before `gh` runs.
fn key_ok(key: &str) -> Result<()> {
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_digit()) || key.len() > 9 {
        return Err(CoreError::Invalid("A GitHub issue key is a number.".into()));
    }
    Ok(())
}

fn scope_ok(scope: &str) -> Result<()> {
    let ok = scope.split('/').count() == 2
        && scope.split('/').all(|p| {
            !p.is_empty()
                && !p.starts_with(['-', '.'])
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
        });
    if ok {
        Ok(())
    } else {
        Err(CoreError::Invalid("A GitHub scope is owner/repo.".into()))
    }
}

/// The reactions `gh` reports for one issue, or `None` when this `gh` cannot say. It is a read of its own because
/// `reactionGroups` is a field an older `gh` does not know: naming it in the detail read would fail that whole call,
/// and the module's rule is that a field a tracker lacks becomes empty, never an error. Only the react answer carries
/// counts (docs/issues.md); a plain detail read does not ask for them.
async fn optional_reactions(
    cwd: &std::path::Path,
    scope: &str,
    key: &str,
) -> Option<Vec<IssueReactionCount>> {
    let args: Vec<String> = [
        "issue",
        "view",
        key,
        "--repo",
        scope,
        "--json",
        "reactionGroups",
    ]
    .map(str::to_owned)
    .into();
    let value = run(cwd, &args).await.ok()?;
    reaction_counts(&value)
}

/// Refuses a comment node id that is not on this repository and issue: one read, made only when a reaction names a
/// comment. A node id from another issue or repository is `not_found` (the usual refusal wording) and no mutation is
/// sent, so such a reaction can never be written outside the connection's scope.
async fn comment_belongs(
    cwd: &std::path::Path,
    scope: &str,
    key: &str,
    comment: &str,
) -> Result<()> {
    let args = vec![
        "api".to_owned(),
        "graphql".to_owned(),
        "-f".to_owned(),
        format!("query={COMMENT_SCOPE_QUERY}"),
        "-f".to_owned(),
        format!("id={comment}"),
    ];
    let value = graphql(cwd, &args).await?;
    let refused = || CoreError::NotFound("issue".into());
    let node = value
        .pointer("/data/node")
        .filter(|node| !node.is_null())
        .ok_or_else(refused)?;
    let wants = key.parse::<u64>().map_err(|_| refused())?;
    let repository = node
        .pointer("/issue/repository/nameWithOwner")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let number = node.pointer("/issue/number").and_then(Value::as_u64);
    if node.get("__typename").and_then(Value::as_str) != Some("IssueComment")
        || number != Some(wants)
        || !repository.eq_ignore_ascii_case(scope)
    {
        return Err(refused());
    }
    Ok(())
}

/// The GraphQL node id of one issue: `addReaction` and `removeReaction` take a node id, never a number.
async fn node_id(cwd: &std::path::Path, scope: &str, key: &str) -> Result<String> {
    let (owner, name) = scope
        .split_once('/')
        .ok_or_else(|| CoreError::Invalid("A GitHub scope is owner/repo.".into()))?;
    let args = vec![
        "api".to_owned(),
        "graphql".to_owned(),
        "-f".to_owned(),
        format!("query={ISSUE_NODE_QUERY}"),
        "-f".to_owned(),
        format!("owner={owner}"),
        "-f".to_owned(),
        format!("name={name}"),
        "-F".to_owned(),
        format!("number={key}"),
    ];
    let value = graphql(cwd, &args).await?;
    value
        .pointer("/data/repository/issue/id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| CoreError::NotFound("issue".into()))
}

#[async_trait]
impl IssueSource for GitHub {
    async fn list(&self, scope: &str, query: &IssueQuery) -> Result<ScopeList> {
        scope_ok(scope)?;
        if query.cursors.contains_key(&format!("github {scope}")) {
            // GitHub's answer has one page and issues no cursor, so any that arrives was not ours.
            return Err(CoreError::Invalid(
                "That page cursor was not issued for this repository.".into(),
            ));
        }
        let listed = run(&self.cwd, &list_args(scope, query)).await?;
        let issues = listed
            .as_array()
            .map(|a| a.iter().filter_map(|v| summary(scope, v)).collect())
            .unwrap_or_default();
        let count = |state: &str| {
            let args: Vec<String> = [
                "api",
                "-X",
                "GET",
                "search/issues",
                "-f",
                &format!("q=repo:{scope} is:issue is:{state}"),
                "--jq",
                ".total_count",
            ]
            .map(str::to_owned)
            .into();
            let cwd = self.cwd.clone();
            async move {
                let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                let out = call(Forge::GitHub, &cwd, &refs).await.ok()?;
                out.trim().parse::<u32>().ok()
            }
        };
        let (open, closed) = tokio::join!(count("open"), count("closed"));
        Ok(ScopeList {
            issues,
            counts: open
                .zip(closed)
                .map(|(open, closed)| IssueCounts { open, closed }),
            next: None,
        })
    }

    async fn dependency_updates(&self, scope: &str) -> Result<DependencyFeed> {
        scope_ok(scope)?;
        // Every page of open alerts through the person's own gh. Any failure (no access, alerts disabled, signed
        // out, gh missing) means the tracker cannot answer, never a refusal, so no event is spent. A feed that still
        // has a full last page after DEPENDENCY_PAGES pages is incomplete, and an incomplete feed cannot be a baseline:
        // it answers "cannot" too.
        let mut updates = Vec::new();
        for page in 1..=DEPENDENCY_PAGES {
            let args: Vec<String> = [
                "api",
                "-X",
                "GET",
                &format!("repos/{scope}/dependabot/alerts"),
                "-f",
                "state=open",
                "-f",
                &format!("per_page={DEPENDENCY_PAGE_SIZE}"),
                "-f",
                &format!("page={page}"),
            ]
            .map(str::to_owned)
            .into();
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            let Ok(out) = call(Forge::GitHub, &self.cwd, &refs).await else {
                return Ok(DependencyFeed::Cannot);
            };
            let Ok(value) = serde_json::from_str::<Value>(&out) else {
                return Ok(DependencyFeed::Cannot);
            };
            let Some(alerts) = value.as_array() else {
                return Ok(DependencyFeed::Cannot);
            };
            updates.extend(dependabot_updates(&value));
            if alerts.len() < DEPENDENCY_PAGE_SIZE {
                return Ok(DependencyFeed::Updates(updates));
            }
        }
        Ok(DependencyFeed::Cannot)
    }

    async fn label_events(&self, scope: &str, label: &str) -> Result<LabelFeed> {
        scope_ok(scope)?;
        // One page of the repository's issue events, newest first, through the person's own gh. Any failure (no
        // access, signed out, gh missing) means the tracker cannot answer, never a refusal, so no event is spent.
        let args: Vec<String> = [
            "api",
            "-X",
            "GET",
            &format!("repos/{scope}/issues/events"),
            "-f",
            "per_page=100",
        ]
        .map(str::to_owned)
        .into();
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let Ok(out) = call(Forge::GitHub, &self.cwd, &refs).await else {
            return Ok(LabelFeed::Cannot);
        };
        let Ok(value) = serde_json::from_str::<Value>(&out) else {
            return Ok(LabelFeed::Cannot);
        };
        if !value.is_array() {
            return Ok(LabelFeed::Cannot);
        }
        Ok(LabelFeed::Events(label_events_of(&value, label)))
    }

    fn can_write(&self) -> bool {
        true
    }

    fn can_edit(&self) -> bool {
        true
    }

    /// Writes the present fields in one PATCH and answers the issue as `gh` reads it back, so the reply carries the
    /// comments, the linked pull requests and the states a detail read does (docs/issues.md, "Issue writes and
    /// list controls"). GitHub takes `labels` and `assignees` as the whole new set.
    async fn update(&self, scope: &str, key: &str, edit: &IssueEdit<'_>) -> Result<IssueDetail> {
        scope_ok(scope)?;
        key_ok(key)?;
        let mut payload = serde_json::Map::new();
        if let Some(title) = edit.title {
            payload.insert("title".into(), serde_json::json!(title));
        }
        if let Some(body) = edit.body {
            payload.insert("body".into(), serde_json::json!(body));
        }
        if let Some(labels) = edit.labels {
            payload.insert("labels".into(), serde_json::json!(labels));
        }
        if let Some(assignees) = edit.assignees {
            payload.insert("assignees".into(), serde_json::json!(assignees));
        }
        patch_issue(&self.cwd, scope, key, &Value::Object(payload)).await?;
        self.get(scope, key).await
    }

    /// Closes or reopens the issue with the reason its state name carries, then answers it as it is now stored.
    async fn set_state(
        &self,
        scope: &str,
        key: &str,
        state: &str,
        close_reason: Option<IssueCloseReason>,
    ) -> Result<IssueDetail> {
        scope_ok(scope)?;
        key_ok(key)?;
        let closed = closing(state, close_reason)?;
        let mut payload = serde_json::Map::new();
        payload.insert(
            "state".into(),
            serde_json::json!(if closed.is_some() { "closed" } else { "open" }),
        );
        if let Some(reason) = closed {
            payload.insert(
                "state_reason".into(),
                serde_json::json!(match reason {
                    IssueCloseReason::Completed => "completed",
                    IssueCloseReason::NotPlanned => "not_planned",
                }),
            );
        }
        patch_issue(&self.cwd, scope, key, &Value::Object(payload)).await?;
        self.get(scope, key).await
    }

    /// The repository's labels and assignable people, at most 200 of each through `gh` (docs/issues.md, "Issue
    /// writes and list controls"). GitHub addresses a person by login, so an assignee candidate's `id` is its login.
    async fn candidates(&self, scope: &str) -> Result<IssueCandidates> {
        scope_ok(scope)?;
        let label_args: Vec<String> = [
            "label",
            "list",
            "--repo",
            scope,
            "--json",
            "name,color",
            "--limit",
            "200",
        ]
        .map(str::to_owned)
        .into();
        let listed = run(&self.cwd, &label_args).await?;
        let mut labels = Vec::new();
        let mut truncated = false;
        for row in listed.as_array().into_iter().flatten() {
            let Some(name) = row
                .get("name")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
            else {
                continue;
            };
            if labels.len() >= MAX_CANDIDATES {
                truncated = true;
                break;
            }
            labels.push(IssueCandidateLabel {
                name: name.to_owned(),
                color: row
                    .get("color")
                    .and_then(Value::as_str)
                    .filter(|color| !color.is_empty())
                    .map(str::to_owned),
            });
        }
        // The assignable people, 100 a page: two pages cover the 200 the route keeps, and a full second page may
        // have more behind it.
        let mut assignees: Vec<IssueAssigneeCandidate> = Vec::new();
        for page in 1..=2u32 {
            let args: Vec<String> = vec![
                "api".to_owned(),
                format!("repos/{scope}/assignees?per_page=100&page={page}"),
            ];
            let value = run(&self.cwd, &args).await?;
            let rows = value.as_array().ok_or_else(unreadable)?;
            for row in rows {
                let Some(login) = row
                    .get("login")
                    .and_then(Value::as_str)
                    .filter(|login| !login.is_empty())
                else {
                    continue;
                };
                if assignees.len() >= MAX_CANDIDATES {
                    truncated = true;
                    break;
                }
                assignees.push(IssueAssigneeCandidate {
                    login: login.to_owned(),
                    name: None,
                    id: login.to_owned(),
                });
            }
            if rows.len() < 100 {
                break;
            }
            if page == 2 {
                truncated = true;
            }
        }
        Ok(IssueCandidates {
            labels,
            assignees,
            truncated,
        })
    }

    async fn create(&self, scope: &str, new: &NewIssue<'_>) -> Result<IssueDetail> {
        scope_ok(scope)?;
        // The person's text travels as the JSON body's field values, never as an argument position.
        let payload = serde_json::json!({
            "title": new.title,
            "body": new.body,
            "labels": new.labels,
            "assignees": new.assignees,
        })
        .to_string();
        let args: Vec<String> = vec![
            "api".to_owned(),
            "-X".to_owned(),
            "POST".to_owned(),
            format!("repos/{scope}/issues"),
            "--input".to_owned(),
            "-".to_owned(),
        ];
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let answer = call_input(Forge::GitHub, &self.cwd, &refs, &payload)
            .await
            .map_err(|error| possibly_created(classify(error)))?;
        created_detail(scope, &parse_json(&answer)?)
    }

    async fn comment(&self, scope: &str, key: &str, body: &str) -> Result<IssueComment> {
        scope_ok(scope)?;
        key_ok(key)?;
        let payload = serde_json::json!({ "body": body }).to_string();
        let args: Vec<String> = vec![
            "api".to_owned(),
            "-X".to_owned(),
            "POST".to_owned(),
            format!("repos/{scope}/issues/{key}/comments"),
            "--input".to_owned(),
            "-".to_owned(),
        ];
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let answer = call_input(Forge::GitHub, &self.cwd, &refs, &payload)
            .await
            .map_err(classify)?;
        stored_comment(&parse_json(&answer)?)
    }

    async fn react(
        &self,
        scope: &str,
        key: &str,
        comment: Option<&str>,
        reaction: IssueReaction,
        on: bool,
    ) -> Result<IssueDetail> {
        scope_ok(scope)?;
        key_ok(key)?;
        // A comment id is the subject exactly as the detail carried it, but node ids are global: prove it belongs to
        // this issue before writing to it, or the reaction could land outside the scope the route checked. The issue
        // itself is looked up by its node id.
        let subject = match comment {
            Some(comment) => {
                comment_belongs(&self.cwd, scope, key, comment).await?;
                comment.to_owned()
            }
            None => node_id(&self.cwd, scope, key).await?,
        };
        let mutation = if on {
            ADD_REACTION_MUTATION
        } else {
            REMOVE_REACTION_MUTATION
        };
        let args = vec![
            "api".to_owned(),
            "graphql".to_owned(),
            "-f".to_owned(),
            format!("query={mutation}"),
            "-f".to_owned(),
            format!("subject={subject}"),
            "-f".to_owned(),
            format!("content={}", reaction_content(reaction)),
        ];
        graphql(&self.cwd, &args).await?;
        let mut detail = self.get(scope, key).await?;
        // The counts go in the reaction's answer only. The read is optional so a `gh` without the field still answers
        // the issue it just changed, with no counts rather than a failure.
        detail.reactions = optional_reactions(&self.cwd, scope, key).await;
        Ok(detail)
    }

    async fn get(&self, scope: &str, key: &str) -> Result<IssueDetail> {
        scope_ok(scope)?;
        key_ok(key)?;
        let args: Vec<String> = [
            "issue",
            "view",
            key,
            "--repo",
            scope,
            "--json",
            DETAIL_FIELDS,
        ]
        .map(str::to_owned)
        .into();
        let value = run(&self.cwd, &args).await?;
        let summary = summary(scope, &value).ok_or_else(|| CoreError::NotFound("issue".into()))?;
        // Linked pull requests are a separate field that older `gh` lacks; without it there are none.
        let links: Vec<String> = [
            "issue",
            "view",
            key,
            "--repo",
            scope,
            "--json",
            "closedByPullRequestsReferences",
        ]
        .map(str::to_owned)
        .into();
        let mut linked = run(&self.cwd, &links)
            .await
            .map(|v| linked_pulls(&v))
            .unwrap_or_default();
        // The link list carries no CI, so each linked pull request is read once for it. The reads go together, so
        // one repository that is slow or refused does not hold up the others, and none of them can fail the issue.
        // Each read is one `gh` call against the account's shared hourly API limit, so only the first
        // `LINKED_CHECKS_READ_LIMIT` links are read; the rest stay shown with `checks` absent (unknown).
        let checks = futures_util::future::join_all(
            linked
                .iter()
                .take(LINKED_CHECKS_READ_LIMIT)
                .map(|pull| linked_checks(&self.cwd, &pull.repository, pull.number)),
        )
        .await;
        for (pull, checks) in linked.iter_mut().zip(checks) {
            pull.checks = checks;
        }
        // The state changes on the issue's own timeline come with the detail (docs/issues.md), best effort: an
        // unreadable timeline leaves the issue shown with no events rather than failing the read.
        let activity = activity_events(&self.cwd, scope, key).await;
        Ok(IssueDetail {
            summary,
            body: text(&value, "body"),
            comments: comments(&value),
            activity,
            linked_pull_requests: linked,
            linked_threads: Vec::new(),
            // Counts are the react answer's alone: a detail read keeps them absent (docs/issues.md, "As built").
            reactions: None,
            states: Some(github_states()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_gh_issue_becomes_a_summary_with_its_close_reason() {
        let open = summary("o/r", &json!({ "number": 7, "title": "T", "url": "u", "author": { "login": "ann" }, "state": "OPEN",
            "createdAt": "c", "updatedAt": "u", "closedAt": "", "assignees": [{ "login": "bo" }], "labels": [{ "name": "bug" }], "comments": [1, 2] })).unwrap();
        assert_eq!(
            (
                open.key.as_str(),
                open.display_key.as_str(),
                open.comment_count
            ),
            ("7", "#7", 2)
        );
        assert_eq!(open.state.category, IssueStateCategory::Open);
        assert_eq!(open.closed_at, None);
        assert_eq!(open.labels[0].name, "bug");
        let closed = summary(
            "o/r",
            &json!({ "number": 8, "state": "CLOSED", "stateReason": "NOT_PLANNED" }),
        )
        .unwrap();
        assert_eq!(
            closed.state.close_reason,
            Some(IssueCloseReason::NotPlanned)
        );
        assert_eq!(closed.state.name, "Closed as not planned");
        let done = summary(
            "o/r",
            &json!({ "number": 9, "state": "CLOSED", "stateReason": "COMPLETED" }),
        )
        .unwrap();
        assert_eq!(done.state.close_reason, Some(IssueCloseReason::Completed));
        let plain = summary("o/r", &json!({ "number": 10, "state": "CLOSED" })).unwrap();
        assert_eq!(
            plain.state.close_reason, None,
            "a tracker that does not say reports none"
        );
    }

    #[test]
    fn wrong_types_become_empty_and_a_row_without_a_number_is_dropped() {
        assert!(summary("o/r", &json!({ "title": "no number" })).is_none());
        let odd = summary(
            "o/r",
            &json!({ "number": 1, "title": 5, "labels": "x", "assignees": 3, "author": "ann" }),
        )
        .unwrap();
        assert_eq!(
            (
                odd.title.as_str(),
                odd.labels.len(),
                odd.assignees.len(),
                odd.author
            ),
            ("", 0, 0, None)
        );
    }

    #[test]
    fn list_arguments_carry_only_the_four_filters() {
        let q = IssueQuery {
            state: StateFilter::All,
            labels: vec!["bug".into()],
            assignee: AssigneeFilter::Me,
            text: "crash".into(),
            ..IssueQuery::default()
        };
        let args = list_args("o/r", &q);
        assert_eq!(&args[..4], ["issue", "list", "--repo", "o/r"]);
        let at = |flag: &str| {
            args.iter()
                .position(|a| a == flag)
                .map(|i| args[i + 1].as_str())
        };
        assert_eq!(
            (
                at("--state"),
                at("--label"),
                at("--assignee"),
                at("--search")
            ),
            (
                Some("all"),
                Some("bug"),
                Some("@me"),
                Some("sort:updated-desc crash")
            ),
            "the order is a search qualifier, and the person's text follows it"
        );
        let created = list_args(
            "o/r",
            &IssueQuery {
                sort: IssueSort::Created,
                ..IssueQuery::default()
            },
        );
        let i = created.iter().position(|a| a == "--search").unwrap();
        assert_eq!(created[i + 1], "sort:created-desc");
        let none = list_args(
            "o/r",
            &IssueQuery {
                assignee: AssigneeFilter::NoOne,
                text: "x".into(),
                ..IssueQuery::default()
            },
        );
        assert!(none.contains(&"sort:updated-desc x no:assignee".to_owned()));
        // A text that looks like an option is a value of --search, never an option.
        let odd = list_args(
            "o/r",
            &IssueQuery {
                text: "--web".into(),
                ..IssueQuery::default()
            },
        );
        let i = odd.iter().position(|a| a == "--search").unwrap();
        assert_eq!(odd[i + 1], "sort:updated-desc --web");
    }

    #[test]
    fn linked_pulls_read_state_and_repository() {
        let pulls = linked_pulls(&json!({ "closedByPullRequestsReferences": [
            { "number": 3, "title": "Fix", "url": "u", "state": "MERGED", "isDraft": false, "repository": { "nameWithOwner": "o/r" } },
            { "number": 4, "state": "OPEN", "isDraft": true, "repository": { "owner": { "login": "o" }, "name": "r2" } },
            { "title": "no number" }] }));
        assert_eq!(pulls.len(), 2);
        assert_eq!(
            (pulls[0].state, pulls[0].repository.as_str()),
            (LinkedPullState::Merged, "o/r")
        );
        assert_eq!(
            (pulls[1].is_draft, pulls[1].repository.as_str()),
            (true, "o/r2")
        );
    }

    #[test]
    fn a_link_carries_no_checks_until_its_own_read_answers() {
        let mut pulls = linked_pulls(&json!({ "closedByPullRequestsReferences": [
            { "number": 7, "state": "OPEN", "repository": { "nameWithOwner": "o/r" } },
            { "number": 7, "state": "OPEN", "repository": { "nameWithOwner": "o/other" } }] }));
        assert_eq!(
            pulls.len(),
            2,
            "one number in two repositories is two links"
        );
        assert!(
            pulls.iter().all(|pull| pull.checks.is_none()),
            "the tracker's link list reports no CI"
        );
        // A read is one repository and one number, so one repository's counts never land on another's link.
        pulls[0].checks = github_checks(&json!({ "statusCheckRollup": [
            { "name": "ci", "status": "COMPLETED", "conclusion": "FAILURE" }] }));
        assert_eq!(pulls[0].checks.as_ref().map(|c| c.failing), Some(1));
        assert_eq!(pulls[0].repository, "o/r");
        assert_eq!(pulls[1].checks, None, "o/other's link was not read");
    }

    #[test]
    fn a_linked_pull_reads_its_check_counts_and_one_without_them_still_reads() {
        let with = json!({ "repository": "o/r", "number": 12, "title": "T", "url": "u", "state": "open",
            "is_draft": false, "closes_issue": true,
            "checks": { "total": 3, "passing": 1, "failing": 1, "running": 1, "failing_names": ["ci"] } });
        let read: IssueLinkedPull = serde_json::from_value(with.clone()).expect("a link with CI");
        let checks = read.checks.as_ref().expect("the counts");
        assert_eq!(
            (checks.total, checks.passing, checks.failing, checks.running),
            (3, 1, 1, 1)
        );
        assert_eq!(checks.failing_names, ["ci"]);
        assert_eq!(
            serde_json::to_value(&read).unwrap(),
            with,
            "the counts travel back out"
        );
        // A tracker that reports no CI for a link, and a link read before this field existed: both still read.
        let mut without = with;
        without.as_object_mut().unwrap().remove("checks");
        let read: IssueLinkedPull = serde_json::from_value(without).expect("a link without CI");
        assert_eq!(read.checks, None, "absent is unknown, never zero checks");
    }

    #[test]
    fn a_gh_failure_is_named_without_its_raw_text() {
        let git = |m: &str| classify(CoreError::Git(m.into()));
        assert!(matches!(
            git("To get started with GitHub CLI, please run: gh auth login"),
            CoreError::TrackerNotConnected(_)
        ));
        assert!(matches!(
            git("HTTP 403: API rate limit exceeded for user"),
            CoreError::TrackerRateLimited(_)
        ));
        assert!(matches!(
            git("GraphQL: Could not resolve to an Issue with the number of 99."),
            CoreError::NotFound(_)
        ));
        assert!(matches!(
            git("HTTP 403: Resource not accessible by integration"),
            CoreError::TrackerForbidden(_)
        ));
        let other = git("secret-token-ghp_abc leaked in this line");
        assert!(matches!(&other, CoreError::TrackerUnavailable(m) if !m.contains("ghp_")));
        assert!(matches!(
            classify(CoreError::Unsupported("Install gh".into())),
            CoreError::TrackerNotConnected(_)
        ));
    }

    #[tokio::test]
    async fn a_page_cursor_is_refused_because_github_issues_none() {
        let github = GitHub {
            cwd: std::env::temp_dir(),
        };
        let query = IssueQuery {
            cursors: [("github o/r".to_owned(), "2.abc".to_owned())].into(),
            ..IssueQuery::default()
        };
        let refused = github.list("o/r", &query).await.map(|_| ());
        assert!(matches!(refused, Err(CoreError::Invalid(_))));
    }

    #[test]
    fn a_scope_and_a_key_are_checked_before_gh_runs() {
        assert!(scope_ok("o/r").is_ok() && scope_ok("a-b/c.d_e").is_ok());
        for bad in [
            "o", "o/r/x", "o/ r", "--repo/x", "o/-r", "o/r;rm", "../x", "",
        ] {
            assert!(scope_ok(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn dependabot_alerts_become_updates_with_only_their_number_and_ecosystem() {
        let alerts = json!([
            {
                "number": 12,
                "dependency": { "package": { "ecosystem": "npm", "name": "left-pad" } },
                "security_advisory": { "summary": "never read" }
            },
            {
                "number": 13,
                "dependency": { "package": { "ecosystem": "Rust", "name": "serde" } }
            },
            { "title": "no number" },
            { "number": 14 }
        ]);
        assert_eq!(
            dependabot_updates(&alerts),
            vec![
                DependencyUpdate {
                    key: "12".into(),
                    ecosystem: Some("npm".into()),
                },
                DependencyUpdate {
                    key: "13".into(),
                    ecosystem: Some("cargo".into()),
                },
                DependencyUpdate {
                    key: "14".into(),
                    ecosystem: None,
                },
            ]
        );
        assert!(
            dependabot_updates(&json!({ "not": "an array" })).is_empty(),
            "a non-array answer reports no updates"
        );
    }

    #[test]
    fn labeled_issue_events_keep_only_the_id_and_issue_of_the_stored_label() {
        let events = json!([
            {
                "id": 101,
                "event": "labeled",
                "label": { "name": "bug" },
                "issue": { "number": 7, "title": "never read" },
                "actor": { "login": "never read" }
            },
            {
                "id": 102,
                "event": "labeled",
                "label": { "name": "ui" },
                "issue": { "number": 8 }
            },
            {
                "id": 103,
                "event": "unlabeled",
                "label": { "name": "bug" },
                "issue": { "number": 9 }
            },
            {
                "event": "labeled",
                "label": { "name": "bug" },
                "issue": { "number": 10 }
            },
            {
                "id": 105,
                "event": "labeled",
                "label": { "name": "BUG" },
                "issue": { "number": 11, "state": "open" }
            },
            {
                "id": 106,
                "event": "labeled",
                "label": { "name": "bug" },
                "issue": { "number": 12, "pull_request": { "url": "never read" } }
            },
            {
                "id": 107,
                "event": "labeled",
                "label": { "name": "bug" },
                "issue": { "number": 13, "state": "closed" }
            }
        ]);
        assert_eq!(
            label_events_of(&events, "bug"),
            vec![
                LabelEvent {
                    id: "101".into(),
                    issue: "7".into(),
                },
                LabelEvent {
                    id: "105".into(),
                    issue: "11".into(),
                },
            ],
            "only a labeled event of the stored label on an open issue (not a pull request), with an id and a number, is kept"
        );
    }
}

#[cfg(test)]
#[path = "github_linked_checks_tests.rs"]
mod linked_checks_tests;

#[cfg(test)]
#[path = "github_activity_tests.rs"]
mod activity_tests;

#[cfg(test)]
#[path = "github_write_tests.rs"]
mod write_tests;
