//! The state changes on a GitHub issue's own timeline, read beside a detail: one bounded GraphQL page of the events
//! that closed and reopened it, best effort. No real GitHub, no network: `gh` is scripted, and the answer is the shape
//! GitHub's schema documents (UNVERIFIED against a real repository).

use super::*;
use crate::source_control::forge::{Forge, with_crew_input_fixture};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// One scripted `gh` call: its arguments and, when it reads one, the JSON body it sent.
type GhAnswer = Arc<dyn Fn(Forge, &Path, &[&str], Option<&str>) -> Result<String> + Send + Sync>;
/// The `query` argument of every GraphQL call that was made.
type Queries = Arc<Mutex<Vec<String>>>;

fn github() -> GitHub {
    GitHub {
        cwd: std::env::temp_dir(),
    }
}

/// A `gh issue view --json DETAIL_FIELDS` answer for issue 7.
fn detail_json() -> Value {
    json!({
        "number": 7,
        "title": "Crash on start",
        "url": "https://github.com/acme/app/issues/7",
        "author": { "login": "ann" },
        "state": "OPEN",
        "createdAt": "2026-11-01T00:00:00Z",
        "updatedAt": "2026-11-02T00:00:00Z",
        "closedAt": "",
        "assignees": [],
        "labels": [],
        "comments": [],
        "body": "It breaks.",
    })
}

/// What `gh issue view --json closedByPullRequestsReferences` answers with no links.
fn no_links() -> String {
    json!({ "closedByPullRequestsReferences": [] }).to_string()
}

/// One timeline answer: the nodes GraphQL puts under `data.repository.issue.timelineItems`.
fn timeline(nodes: Value) -> Value {
    json!({ "data": { "repository": { "issue": { "timelineItems": { "nodes": nodes } } } } })
}

/// The `query` argument of a `gh api graphql` call, read from its arguments.
fn query_of(args: &[&str]) -> Option<String> {
    args.iter()
        .find_map(|arg| arg.strip_prefix("query=").map(str::to_owned))
}

/// The one state change a closed issue carries: GitHub's `ClosedEvent`.
fn closed(login: &str, at: &str, reason: &str) -> Value {
    json!({ "__typename": "ClosedEvent", "createdAt": at, "actor": { "login": login },
        "stateReason": reason })
}

#[tokio::test]
async fn the_events_that_closed_and_reopened_an_issue_are_read_from_its_timeline() {
    let queries: Queries = Arc::new(Mutex::new(Vec::new()));
    let seen = queries.clone();
    let fixture: GhAnswer = Arc::new(
        move |_forge: Forge, _cwd: &Path, args: &[&str], _input: Option<&str>| -> Result<String> {
            if args.contains(&"closedByPullRequestsReferences") {
                return Ok(no_links());
            }
            if let Some(query) = query_of(args) {
                seen.lock().unwrap().push(query);
                return Ok(timeline(json!([
                    closed("ann", "2026-11-02T00:00:00Z", "NOT_PLANNED"),
                    { "__typename": "ReopenedEvent", "createdAt": "2026-11-03T00:00:00Z",
                      "actor": { "login": "bo" } }
                ]))
                .to_string());
            }
            Ok(detail_json().to_string())
        },
    );
    let detail = with_crew_input_fixture(fixture, async { github().get("acme/app", "7").await })
        .await
        .unwrap();
    assert_eq!(
        detail.activity,
        vec![
            IssueActivity {
                kind: IssueActivityKind::Closed,
                actor: Some(IssueActor {
                    login: "ann".into()
                }),
                at: "2026-11-02T00:00:00Z".into(),
                close_reason: Some(IssueCloseReason::NotPlanned),
            },
            IssueActivity {
                kind: IssueActivityKind::Reopened,
                actor: Some(IssueActor { login: "bo".into() }),
                at: "2026-11-03T00:00:00Z".into(),
                close_reason: None,
            },
        ],
        "the closed event keeps how it closed, oldest first, and the reopen after it carries no reason"
    );
    let queries = queries.lock().unwrap().clone();
    assert_eq!(
        queries.len(),
        1,
        "the timeline is one read, not one per event"
    );
    assert!(
        queries[0].contains("timelineItems(last:"),
        "the read asks for one bounded page, newest last: {:?}",
        queries[0]
    );
    assert!(
        queries[0].contains("itemTypes:[CLOSED_EVENT,REOPENED_EVENT]"),
        "the read asks the tracker for the state changes only: {:?}",
        queries[0]
    );
}

#[tokio::test]
async fn a_timeline_the_read_cannot_answer_leaves_the_issue_shown_with_no_activity() {
    let fixture: GhAnswer = Arc::new(
        move |_forge: Forge, _cwd: &Path, args: &[&str], _input: Option<&str>| -> Result<String> {
            if args.contains(&"closedByPullRequestsReferences") {
                return Ok(no_links());
            }
            if query_of(args).is_some() {
                // `gh` exits zero and prints an `errors` array for a timeline this account cannot read.
                return Ok(
                    json!({ "errors": [{ "message": "Resource not accessible by integration" }] })
                        .to_string(),
                );
            }
            Ok(detail_json().to_string())
        },
    );
    let detail = with_crew_input_fixture(fixture, async { github().get("acme/app", "7").await })
        .await
        .expect("an unreadable timeline does not fail the issue");
    assert_eq!(
        (detail.summary.key.as_str(), detail.body.as_str()),
        ("7", "It breaks.")
    );
    assert!(
        detail.activity.is_empty(),
        "no event known, never an invented one"
    );
}

#[tokio::test]
async fn a_timeline_gh_refuses_to_read_leaves_the_issue_shown_with_no_activity() {
    let fixture: GhAnswer = Arc::new(
        move |_forge: Forge, _cwd: &Path, args: &[&str], _input: Option<&str>| -> Result<String> {
            if args.contains(&"closedByPullRequestsReferences") {
                return Ok(no_links());
            }
            if query_of(args).is_some() {
                return Err(CoreError::Git("HTTP 401: Bad credentials".into()));
            }
            Ok(detail_json().to_string())
        },
    );
    let detail = with_crew_input_fixture(fixture, async { github().get("acme/app", "7").await })
        .await
        .expect("a refused timeline does not fail the issue");
    assert!(detail.activity.is_empty());
}

#[test]
fn only_a_described_state_change_becomes_an_event() {
    let nodes = json!([
        // A label, an assignment and a mention are on the same timeline and are not state changes.
        { "__typename": "LabeledEvent", "createdAt": "2026-11-01T00:00:00Z" },
        { "__typename": "AssignedEvent", "createdAt": "2026-11-01T00:00:00Z" },
        // A closed event whose reason this GitHub does not have: the event stands, its reason is unknown.
        closed("ann", "2026-11-02T00:00:00Z", "REOPENED"),
        // An event the tracker did not date is not shown: nothing could say when it happened.
        { "__typename": "ReopenedEvent" },
        // A deleted account is no actor, and the event still stands.
        { "__typename": "ReopenedEvent", "createdAt": "2026-11-04T00:00:00Z", "actor": null },
    ]);
    assert_eq!(
        activity(&timeline(nodes)),
        vec![
            IssueActivity {
                kind: IssueActivityKind::Closed,
                actor: Some(IssueActor {
                    login: "ann".into()
                }),
                at: "2026-11-02T00:00:00Z".into(),
                close_reason: None,
            },
            IssueActivity {
                kind: IssueActivityKind::Reopened,
                actor: None,
                at: "2026-11-04T00:00:00Z".into(),
                close_reason: None,
            },
        ],
        "a reason this GitHub does not have is absent, never guessed; an undated event is dropped"
    );
}

#[test]
fn a_hostile_timeline_is_cut_at_the_read_limit() {
    let many: Vec<Value> = (0..MAX_ACTIVITY + 20)
        .map(|n| {
            json!({ "__typename": "ClosedEvent", "createdAt": format!("2026-11-01T00:{n:02}:00Z"),
                "actor": { "login": "ann" }, "stateReason": "COMPLETED" })
        })
        .collect();
    assert_eq!(
        activity(&timeline(Value::Array(many))).len(),
        MAX_ACTIVITY,
        "no timeline answers more events than the read limit"
    );
}
