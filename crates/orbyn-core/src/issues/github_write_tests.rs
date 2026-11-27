//! The GitHub issue writes against a scripted `gh`: no real GitHub, no network, every call recorded. The fixture
//! answers each `gh` invocation the write makes, so the exact arguments and the exact JSON field values can be
//! asserted, and the read-backs are the JSON shapes GitHub documents (UNVERIFIED against a real repository).

use super::*;
use crate::source_control::forge::{Forge, with_crew_input_fixture};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::{Arc, Mutex};

type Calls = Arc<Mutex<Vec<(Vec<String>, Option<String>)>>>;
/// One scripted `gh` call: its arguments and, when it reads one, the JSON body it sent.
type GhAnswer = Arc<dyn Fn(Forge, &Path, &[&str], Option<&str>) -> Result<String> + Send + Sync>;

fn calls() -> Calls {
    Arc::new(Mutex::new(Vec::new()))
}

fn seen(calls: &Calls) -> Vec<(Vec<String>, Option<String>)> {
    calls.lock().unwrap().clone()
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_owned()).collect()
}

fn github() -> GitHub {
    GitHub {
        cwd: std::env::temp_dir(),
    }
}

/// The REST issue `POST /repos/{owner}/{repo}/issues` answers with.
fn rest_issue() -> Value {
    json!({
        "number": 42,
        "html_url": "https://github.com/acme/app/issues/42",
        "title": "Crash on start",
        "body": "It breaks.",
        "state": "open",
        "user": { "login": "ann" },
        "created_at": "2026-11-24T00:00:00Z",
        "updated_at": "2026-11-24T00:00:00Z",
        "closed_at": null,
        "assignees": [{ "login": "octocat" }],
        "labels": [{ "name": "bug" }],
        "comments": 0,
    })
}

/// A `gh issue view --json DETAIL_FIELDS` answer for issue 7. The detail read names no reaction field: counts are
/// the react answer's own optional read.
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

#[tokio::test]
async fn a_create_sends_the_persons_text_as_json_values_and_answers_the_new_issue() {
    let calls = calls();
    let record = calls.clone();
    let fixture = Arc::new(
        move |_forge: Forge, _cwd: &Path, args: &[&str], input: Option<&str>| -> Result<String> {
            let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
            record
                .lock()
                .unwrap()
                .push((args.clone(), input.map(str::to_owned)));
            assert_eq!(
                args,
                strings(&["api", "-X", "POST", "repos/acme/app/issues", "--input", "-"]),
                "the create is one REST call with a JSON body on stdin"
            );
            let body: Value = serde_json::from_str(input.expect("a JSON body on stdin")).unwrap();
            assert_eq!(
                body,
                json!({
                    "title": "Crash on start",
                    "body": "It breaks.",
                    "labels": ["bug"],
                    "assignees": ["octocat"],
                }),
                "the person's text travels as field values, never as an argument"
            );
            Ok(rest_issue().to_string())
        },
    );
    let labels = vec!["bug".to_owned()];
    let assignees = vec!["octocat".to_owned()];
    let new = NewIssue {
        title: "Crash on start",
        body: "It breaks.",
        labels: &labels,
        assignees: &assignees,
    };
    let detail =
        with_crew_input_fixture(fixture, async { github().create("acme/app", &new).await })
            .await
            .unwrap();
    assert_eq!(
        detail.summary.key, "42",
        "the answer is built from the create's own reply"
    );
    assert_eq!(detail.summary.display_key, "#42");
    assert_eq!(detail.summary.title, "Crash on start");
    assert_eq!(detail.summary.url, "https://github.com/acme/app/issues/42");
    assert_eq!(detail.summary.author.unwrap().login, "ann");
    assert_eq!(detail.summary.labels[0].name, "bug");
    assert_eq!(detail.summary.assignees[0].login, "octocat");
    assert_eq!(detail.comments, vec![], "a new issue has no comments");
    assert_eq!(detail.reactions, None);
    assert_eq!(
        seen(&calls).len(),
        1,
        "a create reads nothing back: a create's success cannot be reported as a failure"
    );
}

#[tokio::test]
async fn a_create_that_gets_no_answer_says_the_issue_was_possibly_created() {
    let fixture = Arc::new(
        |_forge: Forge, _cwd: &Path, _args: &[&str], _input: Option<&str>| -> Result<String> {
            Err(CoreError::Git("gh did not finish in time.".into()))
        },
    );
    let error = with_crew_input_fixture(fixture, async {
        let new = NewIssue {
            title: "T",
            body: "",
            labels: &[],
            assignees: &[],
        };
        github().create("acme/app", &new).await.map(|_| ())
    })
    .await
    .unwrap_err();
    match error {
        CoreError::TrackerUnavailable(message) => {
            assert!(
                message.contains("possibly created"),
                "a create's unknown outcome says the issue may exist: {message}"
            );
            assert!(
                !message.contains("did not finish"),
                "the raw gh text is never passed on: {message}"
            );
        }
        other => panic!("a create with no answer is tracker_unavailable, got {other:?}"),
    }
}

#[tokio::test]
async fn a_create_that_is_refused_keeps_the_refusals_meaning() {
    let fixture = Arc::new(
        |_forge: Forge, _cwd: &Path, _args: &[&str], _input: Option<&str>| -> Result<String> {
            Err(CoreError::Git(
                "HTTP 403: Resource not accessible by integration secret-detail".into(),
            ))
        },
    );
    let error = with_crew_input_fixture(fixture, async {
        let new = NewIssue {
            title: "T",
            body: "",
            labels: &[],
            assignees: &[],
        };
        github().create("acme/app", &new).await.map(|_| ())
    })
    .await
    .unwrap_err();
    match error {
        CoreError::TrackerForbidden(message) => {
            assert!(
                !message.contains("secret-detail"),
                "the raw gh text is never passed on: {message}"
            );
        }
        other => panic!("a 403 is forbidden, got {other:?}"),
    }
}

#[tokio::test]
async fn a_comment_answers_the_comments_node_id_so_a_client_can_react_to_it() {
    let calls = calls();
    let record = calls.clone();
    let fixture = Arc::new(
        move |_forge: Forge, _cwd: &Path, args: &[&str], input: Option<&str>| -> Result<String> {
            let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
            record
                .lock()
                .unwrap()
                .push((args.clone(), input.map(str::to_owned)));
            assert_eq!(
                args,
                strings(&[
                    "api",
                    "-X",
                    "POST",
                    "repos/acme/app/issues/7/comments",
                    "--input",
                    "-"
                ])
            );
            let body: Value = serde_json::from_str(input.expect("a JSON body on stdin")).unwrap();
            assert_eq!(body, json!({ "body": "Looks good." }));
            Ok(json!({
                "id": 99,
                "node_id": "IC_kw99",
                "user": { "login": "ann" },
                "body": "Looks good.",
                "created_at": "2026-11-24T01:00:00Z",
            })
            .to_string())
        },
    );
    let comment = with_crew_input_fixture(fixture, async {
        github().comment("acme/app", "7", "Looks good.").await
    })
    .await
    .unwrap();
    assert_eq!(
        (comment.id.as_str(), comment.body.as_str()),
        ("IC_kw99", "Looks good."),
        "the answer carries the node id a reaction takes, not the numeric id"
    );
    assert_eq!(comment.author.unwrap().login, "ann");
    assert_eq!(comment.created_at, "2026-11-24T01:00:00Z");
    assert_eq!(seen(&calls).len(), 1);
}

#[tokio::test]
async fn a_comment_answer_without_a_node_id_falls_back_to_its_numeric_id() {
    let fixture = Arc::new(
        |_forge: Forge, _cwd: &Path, _args: &[&str], _input: Option<&str>| -> Result<String> {
            Ok(json!({
                "id": 99,
                "user": { "login": "ann" },
                "body": "Looks good.",
                "created_at": "2026-11-24T01:00:00Z",
            })
            .to_string())
        },
    );
    let comment = with_crew_input_fixture(fixture, async {
        github().comment("acme/app", "7", "Looks good.").await
    })
    .await
    .unwrap();
    assert_eq!(comment.id, "99");
}

/// The answer to a react: the node query, the mutation, the detail's two reads and the counts' optional read.
fn react_fixture(
    calls: Calls,
    on: bool,
    content: &'static str,
    subject: &'static str,
    reactions: Value,
) -> GhAnswer {
    let record = calls;
    Arc::new(
        move |_forge: Forge, _cwd: &Path, args: &[&str], input: Option<&str>| -> Result<String> {
            let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
            record
                .lock()
                .unwrap()
                .push((args.clone(), input.map(str::to_owned)));
            if args.first().is_some_and(|arg| arg == "api") {
                let query = args
                    .iter()
                    .find(|arg| arg.starts_with("query="))
                    .map(String::as_str)
                    .unwrap_or_default();
                if query.contains("OrbynIssueNode") {
                    assert!(query.contains("query OrbynIssueNode"), "{query}");
                    return Ok(
                        json!({ "data": { "repository": { "issue": { "id": "I_issue_7" } } } })
                            .to_string(),
                    );
                }
                if query.contains("OrbynCommentScope") {
                    assert!(query.contains("node(id:$id)"), "{query}");
                    assert!(
                        args.iter().any(|arg| arg == &format!("id={subject}")),
                        "the comment id is resolved as a value: {args:?}"
                    );
                    return Ok(json!({ "data": { "node": {
                        "__typename": "IssueComment",
                        "issue": { "number": 7, "repository": { "nameWithOwner": "acme/app" } },
                    } } })
                    .to_string());
                }
                let wanted = if on {
                    "OrbynAddReaction"
                } else {
                    "OrbynRemoveReaction"
                };
                assert!(query.contains(wanted), "a {wanted} mutation, got {query}");
                assert!(
                    args.iter().any(|arg| arg == &format!("subject={subject}")),
                    "the subject is the node id: {args:?}"
                );
                assert!(
                    args.iter().any(|arg| arg == &format!("content={content}")),
                    "the tracker's own word is sent: {args:?}"
                );
                return Ok(
                    json!({ "data": { if on { "addReaction" } else { "removeReaction" }: { "reaction": { "content": content } } } })
                        .to_string(),
                );
            }
            if args
                .iter()
                .any(|arg| arg == "closedByPullRequestsReferences")
            {
                return Ok(json!({ "closedByPullRequestsReferences": [] }).to_string());
            }
            if names_field(&args, "reactionGroups") {
                return Ok(json!({ "reactionGroups": reactions.clone() }).to_string());
            }
            assert_eq!(args[0], "issue", "{args:?}");
            Ok(detail_json().to_string())
        },
    )
}

/// Whether a `gh --json` argument names one field, alone or in the detail's comma-separated list.
fn names_field<S: AsRef<str>>(args: &[S], field: &str) -> bool {
    args.iter()
        .any(|arg| arg.as_ref().split(',').any(|name| name.trim() == field))
}

#[tokio::test]
async fn an_issue_reaction_is_added_by_node_id_and_answers_the_trackers_counts() {
    let calls = calls();
    let fixture = react_fixture(
        calls.clone(),
        true,
        "HEART",
        "I_issue_7",
        json!([{ "content": "HEART", "users": { "totalCount": 2 } }]),
    );
    let detail = with_crew_input_fixture(fixture, async {
        github()
            .react("acme/app", "7", None, IssueReaction::Heart, true)
            .await
    })
    .await
    .unwrap();
    assert_eq!(
        detail.reactions,
        Some(vec![IssueReactionCount {
            content: "HEART".into(),
            count: 2,
        }]),
        "the answer is the issue with the tracker's counts, in its own word"
    );
    assert_eq!(
        seen(&calls).len(),
        5,
        "the issue's node id, one mutation, the detail's two reads and the counts read"
    );
}

#[tokio::test]
async fn a_comment_reaction_uses_the_comments_node_id_and_removes_the_same_content() {
    let calls = calls();
    let fixture = react_fixture(
        calls.clone(),
        false,
        "THUMBS_UP",
        "IC_comment_1",
        json!([{ "content": "THUMBS_UP", "users": { "totalCount": 1 } }]),
    );
    let detail = with_crew_input_fixture(fixture, async {
        github()
            .react(
                "acme/app",
                "7",
                Some("IC_comment_1"),
                IssueReaction::ThumbsUp,
                false,
            )
            .await
    })
    .await
    .unwrap();
    assert_eq!(detail.reactions.unwrap()[0].content, "THUMBS_UP");
    let calls = seen(&calls);
    assert_eq!(
        calls.len(),
        5,
        "the comment's ownership read, the mutation, the detail's two reads and the counts read"
    );
    let scope_read = &calls[0].0;
    assert!(
        scope_read
            .iter()
            .any(|arg| arg.contains("OrbynCommentScope")),
        "a comment react proves the comment belongs to the issue first: {scope_read:?}"
    );
    assert!(
        calls
            .iter()
            .all(|(args, _)| !args.iter().any(|arg| arg.contains("OrbynIssueNode"))),
        "a comment react never looks the issue's node id up"
    );
}

/// A `gh` whose comment-ownership read says the comment is on another repository or issue. Its node id is global,
/// so without that read a Person-authorized caller could write a reaction outside the scope the route checked.
fn foreign_comment_fixture(calls: Calls, number: u64, repository: &'static str) -> GhAnswer {
    let record = calls;
    Arc::new(
        move |_forge: Forge, _cwd: &Path, args: &[&str], _input: Option<&str>| -> Result<String> {
            let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
            record.lock().unwrap().push((args.clone(), None));
            let query = args
                .iter()
                .find(|arg| arg.starts_with("query="))
                .map(String::as_str)
                .unwrap_or_default();
            assert!(
                query.contains("OrbynCommentScope"),
                "only the ownership read runs: {args:?}"
            );
            Ok(json!({ "data": { "node": {
                "__typename": "IssueComment",
                "issue": { "number": number, "repository": { "nameWithOwner": repository } },
            } } })
            .to_string())
        },
    )
}

#[tokio::test]
async fn a_comment_reaction_outside_the_issue_is_refused_before_the_mutation() {
    for (what, number, repository) in [
        ("another repository", 7, "other/repo"),
        ("another issue", 8, "acme/app"),
    ] {
        let calls = calls();
        let fixture = foreign_comment_fixture(calls.clone(), number, repository);
        let error = with_crew_input_fixture(fixture, async {
            github()
                .react(
                    "acme/app",
                    "7",
                    Some("IC_elsewhere"),
                    IssueReaction::Heart,
                    true,
                )
                .await
                .map(|_| ())
        })
        .await
        .unwrap_err();
        assert!(
            matches!(error, CoreError::NotFound(_)),
            "{what} is refused as not_found: {error:?}"
        );
        let calls = seen(&calls);
        assert_eq!(calls.len(), 1, "{what}: only the ownership read ran");
        assert!(
            calls.iter().all(|(args, _)| {
                !args.iter().any(|arg| {
                    arg.contains("OrbynAddReaction") || arg.contains("OrbynRemoveReaction")
                })
            }),
            "{what}: no reaction mutation is sent: {calls:?}"
        );
    }
}

#[tokio::test]
async fn a_reaction_can_be_added_and_taken_back_twice() {
    let mutations: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));
    let record = mutations.clone();
    let fixture = Arc::new(
        move |_forge: Forge, _cwd: &Path, args: &[&str], _input: Option<&str>| -> Result<String> {
            if args.first().is_some_and(|arg| *arg == "api") {
                let query = args
                    .iter()
                    .find(|arg| arg.starts_with("query="))
                    .map(|arg| (*arg).to_owned())
                    .unwrap_or_default();
                if query.contains("OrbynIssueNode") {
                    return Ok(
                        json!({ "data": { "repository": { "issue": { "id": "I_issue_7" } } } })
                            .to_string(),
                    );
                }
                let added = query.contains("OrbynAddReaction");
                assert!(added || query.contains("OrbynRemoveReaction"), "{query}");
                record
                    .lock()
                    .unwrap()
                    .push(if added { "add" } else { "remove" });
                return Ok(if added {
                    json!({ "data": { "addReaction": { "reaction": { "content": "HEART" } } } })
                } else {
                    json!({ "data": { "removeReaction": { "reaction": { "content": "HEART" } } } })
                }
                .to_string());
            }
            if args.contains(&"closedByPullRequestsReferences") {
                return Ok(json!({ "closedByPullRequestsReferences": [] }).to_string());
            }
            if names_field(args, "reactionGroups") {
                return Ok(json!({ "reactionGroups": [{ "content": "HEART", "users": { "totalCount": 1 } }] })
                    .to_string());
            }
            Ok(detail_json().to_string())
        },
    );
    for on in [true, true, false, false] {
        let detail = with_crew_input_fixture(fixture.clone(), async {
            github()
                .react("acme/app", "7", None, IssueReaction::Heart, on)
                .await
        })
        .await
        .unwrap();
        assert_eq!(detail.reactions.as_ref().unwrap()[0].content, "HEART");
    }
    assert_eq!(
        *mutations.lock().unwrap(),
        ["add", "add", "remove", "remove"],
        "a duplicate add and an absent remove are both answered without an error, as GitHub does"
    );
}

/// A `gh` older than the `reactionGroups` field: the read that names it fails, as the real one answers "Unknown
/// JSON field". Everything else answers, so the test proves that field is not in the mandatory detail read.
fn older_gh_fixture() -> GhAnswer {
    Arc::new(
        |_forge: Forge, _cwd: &Path, args: &[&str], _input: Option<&str>| -> Result<String> {
            if names_field(args, "reactionGroups") {
                return Err(CoreError::Git(
                    "Unknown JSON field: \"reactionGroups\"".into(),
                ));
            }
            if args.contains(&"closedByPullRequestsReferences") {
                return Ok(json!({ "closedByPullRequestsReferences": [] }).to_string());
            }
            Ok(detail_json().to_string())
        },
    )
}

#[tokio::test]
async fn a_gh_without_the_reactions_field_still_answers_the_issue() {
    let detail = with_crew_input_fixture(older_gh_fixture(), async {
        github().get("acme/app", "7").await
    })
    .await
    .unwrap();
    assert_eq!(detail.summary.key, "7");
    assert_eq!(
        detail.reactions, None,
        "a detail read does not ask for counts, so an old gh answers the issue"
    );
}

#[tokio::test]
async fn a_react_on_a_gh_without_the_reactions_field_answers_without_counts() {
    let calls = calls();
    let record = calls.clone();
    let older = older_gh_fixture();
    let fixture = Arc::new(
        move |forge: Forge, cwd: &Path, args: &[&str], input: Option<&str>| -> Result<String> {
            let args_vec: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
            record
                .lock()
                .unwrap()
                .push((args_vec.clone(), input.map(str::to_owned)));
            if args.first().is_some_and(|arg| *arg == "api") {
                let query = args
                    .iter()
                    .find(|arg| arg.starts_with("query="))
                    .map(|arg| (*arg).to_owned())
                    .unwrap_or_default();
                if query.contains("OrbynIssueNode") {
                    return Ok(
                        json!({ "data": { "repository": { "issue": { "id": "I_issue_7" } } } })
                            .to_string(),
                    );
                }
                return Ok(
                    json!({ "data": { "addReaction": { "reaction": { "content": "HEART" } } } })
                        .to_string(),
                );
            }
            older(forge, cwd, args, input)
        },
    );
    let detail = with_crew_input_fixture(fixture, async {
        github()
            .react("acme/app", "7", None, IssueReaction::Heart, true)
            .await
    })
    .await
    .expect("a gh without the counts field still answers the issue it changed");
    assert_eq!(detail.summary.key, "7");
    assert_eq!(
        detail.reactions, None,
        "the optional counts read failed, so the answer says unknown, not zero"
    );
    assert_eq!(
        seen(&calls).len(),
        5,
        "the counts read ran and answered nothing: node, mutation, detail, links, counts"
    );
}

#[tokio::test]
async fn a_graphql_error_in_a_zero_exit_answer_is_not_a_success() {
    let fixture = Arc::new(
        |_forge: Forge, _cwd: &Path, args: &[&str], _input: Option<&str>| -> Result<String> {
            let query = args
                .iter()
                .find(|arg| arg.starts_with("query="))
                .map(|arg| (*arg).to_owned())
                .unwrap_or_default();
            if query.contains("OrbynIssueNode") {
                return Ok(
                    json!({ "data": { "repository": { "issue": { "id": "I_issue_7" } } } })
                        .to_string(),
                );
            }
            Ok(json!({
                "data": null,
                "errors": [{ "message": "Could not resolve to a node with the global id of 'nope'." }],
            })
            .to_string())
        },
    );
    let error = with_crew_input_fixture(fixture, async {
        github()
            .react("acme/app", "7", None, IssueReaction::Heart, true)
            .await
            .map(|_| ())
    })
    .await
    .unwrap_err();
    assert!(
        matches!(error, CoreError::NotFound(_)),
        "a refused subject is not_found, never a success: {error:?}"
    );
}

#[test]
fn a_reaction_counts_answer_keeps_the_trackers_own_word_and_drops_empty_groups() {
    let value = json!({
        "reactionGroups": [
            { "content": "THUMBS_UP", "users": { "totalCount": 3 } },
            { "content": "HEART", "users": { "totalCount": 0 } },
            { "content": "  ", "users": { "totalCount": 1 } },
            { "content": "EYES", "users": { "nodes": [{ "login": "ann" }, { "login": "bo" }] } },
        ]
    });
    assert_eq!(
        reaction_counts(&value),
        Some(vec![
            IssueReactionCount {
                content: "THUMBS_UP".into(),
                count: 3,
            },
            IssueReactionCount {
                content: "EYES".into(),
                count: 2,
            },
        ])
    );
    assert_eq!(
        reaction_counts(&json!({ "number": 7 })),
        None,
        "an answer without groups says the tracker does not report reactions"
    );
    assert_eq!(
        reaction_counts(&json!({ "reactionGroups": [] })),
        Some(vec![]),
        "what shares no reaction is an empty list, not an unknown one"
    );
    assert_eq!(
        reaction_counts(&json!({ "reactionGroups": [
            { "content": "HEART", "users": { "totalCount": 0 } },
        ] })),
        Some(vec![]),
        "a group nobody used is dropped: the answer is the counts the person can see"
    );
}

#[test]
fn every_contract_reaction_has_one_tracker_word() {
    for (reaction, word) in [
        (IssueReaction::ThumbsUp, "THUMBS_UP"),
        (IssueReaction::ThumbsDown, "THUMBS_DOWN"),
        (IssueReaction::Laugh, "LAUGH"),
        (IssueReaction::Hooray, "HOORAY"),
        (IssueReaction::Confused, "CONFUSED"),
        (IssueReaction::Heart, "HEART"),
        (IssueReaction::Rocket, "ROCKET"),
        (IssueReaction::Eyes, "EYES"),
    ] {
        assert_eq!(reaction_content(reaction), word);
    }
}

/// One contents answer: a file body, base64 and wrapped the way GitHub wraps it.
fn contents_body(body: &str) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(body.as_bytes());
    encoded
        .as_bytes()
        .chunks(60)
        .map(|chunk| format!("{}\n", str::from_utf8(chunk).unwrap_or_default()))
        .collect()
}

/// A contents answer for one template file.
fn contents_file(path: &str, body: &str) -> Value {
    json!({
        "name": path.rsplit('/').next().unwrap_or(path),
        "path": path,
        "type": "file",
        "encoding": "base64",
        "content": contents_body(body),
    })
}

/// A contents listing of the templates folder: `(name, type)` pairs, the file's path built from its name.
fn contents_listing(entries: &[(&str, &str)]) -> Value {
    Value::Array(
        entries
            .iter()
            .map(|(name, kind)| {
                json!({
                    "name": name,
                    "path": format!(".github/ISSUE_TEMPLATE/{name}"),
                    "type": kind,
                })
            })
            .collect(),
    )
}

const TEMPLATE_DIR_ARGS: [&str; 2] = ["api", "repos/acme/app/contents/.github/ISSUE_TEMPLATE"];

#[tokio::test]
async fn a_markdown_template_carries_its_front_matter_and_its_body() {
    // GitHub has no issue-template endpoint, so the read lists `.github/ISSUE_TEMPLATE` and then reads one markdown
    // file's body (docs/issues.md, "As built"). A `.yml` issue form needs a YAML reader Orbyn does not carry, so it
    // is skipped rather than half-read (#1548).
    let calls = calls();
    let record = calls.clone();
    let fixture = Arc::new(
        move |_forge: Forge, _cwd: &Path, args: &[&str], _input: Option<&str>| -> Result<String> {
            let args = strings(args);
            record.lock().unwrap().push((args.clone(), None));
            if args == strings(&TEMPLATE_DIR_ARGS) {
                return Ok(contents_listing(&[
                    ("bug_report.md", "file"),
                    ("feature.yml", "file"),
                    ("config.yml", "file"),
                    ("legacy.md", "file"),
                    ("old", "dir"),
                ])
                .to_string());
            }
            match args.get(1).map(String::as_str) {
                Some("repos/acme/app/contents/.github/ISSUE_TEMPLATE/bug_report.md") => Ok(
                    contents_file(
                        ".github/ISSUE_TEMPLATE/bug_report.md",
                        "---\nname: Bug report\nabout: Something is broken\ntitle: ''\nlabels: 'bug, ui'\nassignees: ''\n---\n\n**What happened?**\n",
                    )
                    .to_string(),
                ),
                // A body GitHub did not send as base64 (a file over 1 MB) is skipped, not guessed at.
                other => {
                    assert_eq!(
                        other,
                        Some("repos/acme/app/contents/.github/ISSUE_TEMPLATE/legacy.md"),
                        "only a markdown file is read, in the list's order, and a form is never read"
                    );
                    Ok(json!({ "name": "legacy.md", "type": "file", "encoding": "none" }).to_string())
                }
            }
        },
    );
    let templates =
        with_crew_input_fixture(fixture, async { github().templates("acme/app").await })
            .await
            .unwrap();
    assert_eq!(
        templates.len(),
        1,
        "a form, the config, a folder and a body GitHub did not send are not templates"
    );
    let one = &templates[0];
    assert_eq!(
        one.id, "bug_report.md",
        "the id is the file name, which is what a create names"
    );
    assert_eq!(
        one.name, "Bug report",
        "the front matter's name is the name"
    );
    assert_eq!(one.description.as_deref(), Some("Something is broken"));
    assert_eq!(one.labels, vec!["bug".to_owned(), "ui".to_owned()]);
    assert_eq!(
        one.body, "\n**What happened?**\n",
        "the front matter is not part of the body and the wrapping of the base64 is not either"
    );
    assert_eq!(
        seen(&calls).len(),
        3,
        "one listing and one body per markdown file"
    );
}

#[tokio::test]
async fn a_markdown_template_below_fifty_forms_and_the_config_is_still_read() {
    // The 50 places are for templates, not for listed entries: a folder that starts with its forms, its config and a
    // folder still answers the markdown templates below them, and one file is read per markdown template (#1548).
    let calls = calls();
    let record = calls.clone();
    let fixture = Arc::new(
        move |_forge: Forge, _cwd: &Path, args: &[&str], _input: Option<&str>| -> Result<String> {
            let args = strings(args);
            record.lock().unwrap().push((args.clone(), None));
            if args == strings(&TEMPLATE_DIR_ARGS) {
                let mut entries: Vec<(String, String)> = (0..50)
                    .map(|n| (format!("form_{n}.yml"), "file".to_owned()))
                    .collect();
                entries.push(("config.yml".to_owned(), "file".to_owned()));
                entries.push(("idle".to_owned(), "dir".to_owned()));
                entries.push(("bug_report.md".to_owned(), "file".to_owned()));
                let listed: Vec<(&str, &str)> = entries
                    .iter()
                    .map(|(name, kind)| (name.as_str(), kind.as_str()))
                    .collect();
                return Ok(contents_listing(&listed).to_string());
            }
            assert_eq!(
                args.get(1).map(String::as_str),
                Some("repos/acme/app/contents/.github/ISSUE_TEMPLATE/bug_report.md"),
                "only the markdown file is read"
            );
            Ok(contents_file(
                ".github/ISSUE_TEMPLATE/bug_report.md",
                "---\nname: Bug report\n---\n\nThe body\n",
            )
            .to_string())
        },
    );
    let templates =
        with_crew_input_fixture(fixture, async { github().templates("acme/app").await })
            .await
            .unwrap();
    assert_eq!(
        templates.len(),
        1,
        "forms, the config and a folder above the markdown template do not hide it"
    );
    assert_eq!(templates[0].id, "bug_report.md");
    assert_eq!(
        seen(&calls).len(),
        2,
        "one listing and one body per template, never one per listed entry"
    );
}

#[tokio::test]
async fn a_template_file_whose_name_is_url_syntax_is_percent_encoded_in_its_read() {
    // `gh api` joins the endpoint into the request URL as given, so a name holding a space, a `%`, a `#` or a `?`
    // would read another path or fail before the request. Each `/`-separated segment is escaped and the id stays the
    // file's own name, which is what a create names.
    let calls = calls();
    let record = calls.clone();
    let fixture = Arc::new(
        move |_forge: Forge, _cwd: &Path, args: &[&str], _input: Option<&str>| -> Result<String> {
            let args = strings(args);
            record.lock().unwrap().push((args.clone(), None));
            if args == strings(&TEMPLATE_DIR_ARGS) {
                return Ok(contents_listing(&[("100% #1 bug?.md", "file")]).to_string());
            }
            assert_eq!(
                args.get(1).map(String::as_str),
                Some("repos/acme/app/contents/.github/ISSUE_TEMPLATE/100%25%20%231%20bug%3F.md"),
                "the one segment's URL syntax is escaped and no `/` is added or dropped"
            );
            Ok(contents_file(
                ".github/ISSUE_TEMPLATE/100% #1 bug?.md",
                "---\nname: Bug\n---\n\nBody\n",
            )
            .to_string())
        },
    );
    let templates =
        with_crew_input_fixture(fixture, async { github().templates("acme/app").await })
            .await
            .unwrap();
    assert_eq!(templates.len(), 1, "the file is still a template");
    assert_eq!(
        templates[0].id, "100% #1 bug?.md",
        "the id is the file's own name"
    );
}

#[tokio::test]
async fn a_repository_without_a_templates_folder_answers_none() {
    // A repository that never made templates is the common case: GitHub answers 404 for the path it does not have,
    // and that is "no templates", never an error.
    let fixture = Arc::new(
        |_forge: Forge, _cwd: &Path, _args: &[&str], _input: Option<&str>| -> Result<String> {
            Err(CoreError::Git("gh: Not Found (HTTP 404)".into()))
        },
    );
    let templates =
        with_crew_input_fixture(fixture, async { github().templates("acme/app").await })
            .await
            .unwrap();
    assert!(templates.is_empty());
}

#[tokio::test]
async fn a_template_read_failure_is_named_and_never_a_template() {
    // A refusal other than "no such folder" is the usual GitHub refusal, and the read answers it rather than an
    // empty list: a broken connection must not read as a repository without templates.
    let fixture = Arc::new(
        |_forge: Forge, _cwd: &Path, _args: &[&str], _input: Option<&str>| -> Result<String> {
            Err(CoreError::Git("gh: HTTP 401: Bad credentials".into()))
        },
    );
    let error = with_crew_input_fixture(fixture, async { github().templates("acme/app").await })
        .await
        .unwrap_err();
    assert!(
        matches!(error, CoreError::TrackerNotConnected(_)),
        "{error:?}"
    );
}

/// One fixture for an edit or a state write: the PATCH is recorded with the JSON body it sent, and the issue is read
/// back as a detail (its two reads). Nothing else is expected.
fn patch_fixture(calls: Calls) -> GhAnswer {
    let record = calls.clone();
    Arc::new(
        move |_forge: Forge, _cwd: &Path, args: &[&str], input: Option<&str>| -> Result<String> {
            let args = strings(args);
            record
                .lock()
                .unwrap()
                .push((args.clone(), input.map(str::to_owned)));
            if args.first().map(String::as_str) == Some("api")
                && args.get(1).map(String::as_str) == Some("-X")
                && args.get(2).map(String::as_str) == Some("PATCH")
            {
                return Ok("{}".to_owned());
            }
            assert_eq!(args[0], "issue", "{args:?}");
            if names_field(&args, "closedByPullRequestsReferences") {
                return Ok("[]".to_owned());
            }
            Ok(detail_json().to_string())
        },
    )
}

/// The JSON body the one PATCH of a fixture recorded.
fn patched_body(calls: &Calls) -> Value {
    let all = seen(calls);
    let (args, body) = all
        .iter()
        .find(|(args, _)| args.get(2).map(String::as_str) == Some("PATCH"))
        .expect("one PATCH");
    assert_eq!(
        args[3], "repos/acme/app/issues/7",
        "the PATCH addresses the issue by its repo and number: {args:?}"
    );
    serde_json::from_str(body.as_deref().expect("a JSON body")).expect("a JSON body")
}

#[tokio::test]
async fn an_update_writes_only_the_fields_it_was_given_and_answers_the_issue_read_back() {
    let calls = calls();
    let fixture = patch_fixture(calls.clone());
    let edit = IssueEdit {
        title: Some("New title"),
        body: None,
        labels: None,
        assignees: None,
    };
    let detail = with_crew_input_fixture(fixture, async {
        github().update("acme/app", "7", &edit).await
    })
    .await
    .unwrap();
    assert_eq!(
        detail.summary.title, "Crash on start",
        "the answer is the issue as it was read back, not the write's own reply"
    );
    assert_eq!(
        detail.states.as_ref().map(Vec::len),
        Some(3),
        "a detail read carries the states the issue can move to"
    );
    assert_eq!(
        patched_body(&calls),
        json!({ "title": "New title" }),
        "only the present field is written"
    );
    assert_eq!(
        seen(&calls).len(),
        3,
        "one PATCH and the detail's two reads"
    );
}

#[tokio::test]
async fn an_update_sends_labels_and_assignees_as_the_whole_new_set() {
    let first = calls();
    let fixture = patch_fixture(first.clone());
    let labels = vec!["bug".to_owned(), "ui".to_owned()];
    let assignees = vec!["octocat".to_owned()];
    let edit = IssueEdit {
        title: None,
        body: Some("Steps: 1."),
        labels: Some(&labels),
        assignees: Some(&assignees),
    };
    with_crew_input_fixture(fixture, async {
        github().update("acme/app", "7", &edit).await
    })
    .await
    .unwrap();
    assert_eq!(
        patched_body(&first),
        json!({ "body": "Steps: 1.", "labels": ["bug", "ui"], "assignees": ["octocat"] }),
        "labels and assignees are the whole new set, as GitHub writes them"
    );
    // An empty list clears the set rather than being left out.
    let clearing_calls = calls();
    let fixture = patch_fixture(clearing_calls.clone());
    let none: Vec<String> = Vec::new();
    let clearing = IssueEdit {
        title: None,
        body: None,
        labels: Some(&none),
        assignees: None,
    };
    with_crew_input_fixture(fixture, async {
        github().update("acme/app", "7", &clearing).await
    })
    .await
    .unwrap();
    assert_eq!(patched_body(&clearing_calls), json!({ "labels": [] }));
}

#[tokio::test]
async fn a_state_write_closes_with_the_reason_its_name_carries() {
    for (state, reason, expected) in [
        (
            "Closed as not planned",
            None,
            json!({ "state": "closed", "state_reason": "not_planned" }),
        ),
        (
            "Closed",
            Some(IssueCloseReason::Completed),
            json!({ "state": "closed", "state_reason": "completed" }),
        ),
        ("Open", None, json!({ "state": "open" })),
    ] {
        let calls = calls();
        let fixture = patch_fixture(calls.clone());
        let detail = with_crew_input_fixture(fixture, async {
            github().set_state("acme/app", "7", state, reason).await
        })
        .await
        .unwrap();
        assert!(
            detail.states.is_some(),
            "a state write answers the issue with its states"
        );
        assert_eq!(patched_body(&calls), expected, "{state}");
    }
}

#[tokio::test]
async fn a_state_name_github_does_not_offer_is_refused_before_anything_is_sent() {
    let calls = calls();
    let fixture = patch_fixture(calls.clone());
    let error = with_crew_input_fixture(fixture, async {
        github()
            .set_state("acme/app", "7", "Won't fix", None)
            .await
            .map(|_| ())
    })
    .await
    .unwrap_err();
    assert!(matches!(error, CoreError::Invalid(_)), "{error:?}");
    assert!(seen(&calls).is_empty(), "a refused name sends nothing");
}

#[tokio::test]
async fn a_scope_or_a_key_that_is_not_plain_is_refused_before_the_patch() {
    let calls = calls();
    let fixture = patch_fixture(calls.clone());
    let edit = IssueEdit {
        title: Some("T"),
        body: None,
        labels: None,
        assignees: None,
    };
    let bad_scope = with_crew_input_fixture(fixture.clone(), async {
        github()
            .update("acme/app/extra", "7", &edit)
            .await
            .map(|_| ())
    })
    .await
    .unwrap_err();
    assert!(matches!(bad_scope, CoreError::Invalid(_)), "{bad_scope:?}");
    let bad_key = with_crew_input_fixture(fixture, async {
        github()
            .update("acme/app", "--web", &edit)
            .await
            .map(|_| ())
    })
    .await
    .unwrap_err();
    assert!(matches!(bad_key, CoreError::Invalid(_)), "{bad_key:?}");
    assert!(
        seen(&calls).is_empty(),
        "the scope and the key are checked before gh runs"
    );
}

#[test]
fn the_two_closed_states_are_named_apart_and_match_what_a_closed_issue_reports() {
    let states = github_states();
    assert_eq!(
        states.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
        ["Open", "Closed", "Closed as not planned"],
        "GitHub has one closed state with two reasons, and they are named apart"
    );
    assert_eq!(states[0].category, IssueStateCategory::Open);
    assert_eq!(states[1].category, IssueStateCategory::Closed);
    assert_eq!(states[1].close_reason, Some(IssueCloseReason::Completed));
    assert_eq!(states[2].close_reason, Some(IssueCloseReason::NotPlanned));
    // The name a closed issue reports is one of the states it can move to, so its row reads as its own state.
    for reason in ["COMPLETED", "NOT_PLANNED"] {
        let reported = named_state(true, reason);
        assert!(
            states
                .iter()
                .any(|state| state.name == reported.name && state.category == reported.category),
            "{reason} is offered under the name the read reports: {reported:?}"
        );
    }
    assert_eq!(closing("Open", None).unwrap(), None);
    assert_eq!(
        closing("Closed as not planned", Some(IssueCloseReason::Completed)).unwrap(),
        Some(IssueCloseReason::NotPlanned),
        "the name carries the reason it names"
    );
}

#[tokio::test]
async fn candidates_offer_the_repository_labels_and_assignable_people() {
    let calls = calls();
    let record = calls.clone();
    let fixture = Arc::new(
        move |_forge: Forge, _cwd: &Path, args: &[&str], input: Option<&str>| -> Result<String> {
            let args = strings(args);
            record
                .lock()
                .unwrap()
                .push((args.clone(), input.map(str::to_owned)));
            if args[0] == "label" {
                return Ok(json!([
                    { "name": "bug", "color": "d73a4a" },
                    { "name": "ui" }
                ])
                .to_string());
            }
            assert_eq!(args[0], "api", "{args:?}");
            Ok(json!([{ "login": "ann" }, { "login": "bo" }]).to_string())
        },
    );
    let found = with_crew_input_fixture(fixture, async { github().candidates("acme/app").await })
        .await
        .unwrap();
    assert_eq!(
        found
            .labels
            .iter()
            .map(|label| (label.name.as_str(), label.color.as_deref()))
            .collect::<Vec<_>>(),
        [("bug", Some("d73a4a")), ("ui", None)]
    );
    assert_eq!(
        found
            .assignees
            .iter()
            .map(|person| (person.login.as_str(), person.id.as_str()))
            .collect::<Vec<_>>(),
        [("ann", "ann"), ("bo", "bo")],
        "GitHub addresses a person by login, so a candidate's id is the login"
    );
    assert!(!found.truncated);
    let all = seen(&calls);
    assert!(
        all.iter().any(|(args, _)| args[0] == "api"
            && args[1].contains("repos/acme/app/assignees?per_page=100&page=1")),
        "the people are read a page at a time: {all:?}"
    );
}

#[tokio::test]
async fn candidates_say_when_there_were_more_than_the_two_hundred_kept() {
    let calls = calls();
    let record = calls.clone();
    let fixture = Arc::new(
        move |_forge: Forge, _cwd: &Path, args: &[&str], input: Option<&str>| -> Result<String> {
            let args = strings(args);
            record
                .lock()
                .unwrap()
                .push((args.clone(), input.map(str::to_owned)));
            if args[0] == "label" {
                return Ok("[]".to_owned());
            }
            // Two full pages of a hundred: the second may have more behind it.
            let rows: Vec<Value> = (0..100)
                .map(|i| json!({ "login": format!("u{i}") }))
                .collect();
            Ok(Value::Array(rows).to_string())
        },
    );
    let found = with_crew_input_fixture(fixture, async { github().candidates("acme/app").await })
        .await
        .unwrap();
    assert_eq!(found.assignees.len(), 200);
    assert!(
        found.truncated,
        "a full last page may have more, so a missing name is not proof of ineligibility"
    );
}
