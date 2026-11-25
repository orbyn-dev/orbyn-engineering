//! The CI counts a GitHub issue's linked pull requests carry, read against a scripted `gh`: one
//! `gh pr view --json statusCheckRollup` per link, in that link's own repository. No real GitHub, no network. The
//! detail answer is the JSON shape GitHub documents (UNVERIFIED against a real repository).

use super::*;
use crate::source_control::forge::{Forge, with_crew_input_fixture};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// One scripted `gh` call: its arguments and, when it reads one, the JSON body it sent.
type GhAnswer = Arc<dyn Fn(Forge, &Path, &[&str], Option<&str>) -> Result<String> + Send + Sync>;
/// The `(number, repository)` of every linked pull request that was read.
type Reads = Arc<Mutex<Vec<(String, String)>>>;

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

/// What `gh issue view --json closedByPullRequestsReferences` answers with.
fn links(links: Value) -> String {
    json!({ "closedByPullRequestsReferences": links }).to_string()
}

/// One completed check in a `gh pr view --json statusCheckRollup` answer.
fn rollup(conclusion: &str) -> String {
    json!({ "statusCheckRollup": [
        { "name": "ci", "status": "COMPLETED", "conclusion": conclusion }] })
    .to_string()
}

/// The `(number, repository)` a `gh pr view` call names, read from its arguments.
fn target(args: &[&str]) -> Option<(String, String)> {
    let at = args.iter().position(|arg| *arg == "view")?;
    let number = (*args.get(at + 1)?).to_owned();
    let repo = args.iter().position(|arg| *arg == "--repo")?;
    Some((number, (*args.get(repo + 1)?).to_owned()))
}

#[tokio::test]
async fn each_linked_pull_request_is_read_in_its_own_repository() {
    let reads: Reads = Arc::new(Mutex::new(Vec::new()));
    let record = reads.clone();
    let fixture: GhAnswer = Arc::new(
        move |_forge: Forge, _cwd: &Path, args: &[&str], _input: Option<&str>| -> Result<String> {
            if args.contains(&"closedByPullRequestsReferences") {
                return Ok(links(json!([
                    { "number": 12, "title": "Fix", "url": "u12", "state": "OPEN", "isDraft": false,
                      "repository": { "nameWithOwner": "acme/app" } },
                    { "number": 13, "title": "Also", "url": "u13", "state": "OPEN", "isDraft": false,
                      "repository": { "nameWithOwner": "acme/other" } }])));
            }
            if args.contains(&"statusCheckRollup") {
                let (number, repository) =
                    target(args).expect("a pr view names a number and its repository");
                record
                    .lock()
                    .unwrap()
                    .push((number.clone(), repository.clone()));
                return match (number.as_str(), repository.as_str()) {
                    ("12", "acme/app") => Ok(rollup("FAILURE")),
                    ("13", "acme/other") => Ok(rollup("SUCCESS")),
                    // Any other pair is a link read in the wrong repository: the case this test exists for.
                    other => panic!("a link was read in the wrong repository: {other:?}"),
                };
            }
            Ok(detail_json().to_string())
        },
    );
    let detail = with_crew_input_fixture(fixture, async { github().get("acme/app", "7").await })
        .await
        .unwrap();
    let pulls = &detail.linked_pull_requests;
    assert_eq!(pulls.len(), 2);
    let failing = pulls[0].checks.as_ref().expect("acme/app's counts");
    assert_eq!((failing.total, failing.failing), (1, 1));
    assert_eq!(failing.failing_names, ["ci"]);
    let passing = pulls[1].checks.as_ref().expect("acme/other's counts");
    assert_eq!(
        (passing.total, passing.failing),
        (1, 0),
        "one repository's checks never land on the other repository's link"
    );
    let mut seen = reads.lock().unwrap().clone();
    seen.sort();
    assert_eq!(
        seen,
        [
            ("12".to_owned(), "acme/app".to_owned()),
            ("13".to_owned(), "acme/other".to_owned())
        ],
        "each link was read once, in its own repository"
    );
}

#[tokio::test]
async fn a_link_a_person_cannot_read_leaves_the_other_links_shown() {
    let reads: Reads = Arc::new(Mutex::new(Vec::new()));
    let record = reads.clone();
    let fixture: GhAnswer = Arc::new(
        move |_forge: Forge, _cwd: &Path, args: &[&str], _input: Option<&str>| -> Result<String> {
            if args.contains(&"closedByPullRequestsReferences") {
                return Ok(links(json!([
                    { "number": 12, "state": "OPEN", "repository": { "nameWithOwner": "acme/app" } },
                    { "number": 13, "state": "OPEN", "repository": { "nameWithOwner": "acme/other" } },
                    { "number": 14, "state": "OPEN" }])));
            }
            if args.contains(&"statusCheckRollup") {
                let (number, repository) =
                    target(args).expect("a pr view names a number and its repository");
                record
                    .lock()
                    .unwrap()
                    .push((number.clone(), repository.clone()));
                return match repository.as_str() {
                    "acme/app" => Ok(rollup("SUCCESS")),
                    // A repository the person cannot read, as `gh` answers it.
                    _ => Err(CoreError::Git("HTTP 404: Not Found".into())),
                };
            }
            Ok(detail_json().to_string())
        },
    );
    let detail = with_crew_input_fixture(fixture, async { github().get("acme/app", "7").await })
        .await
        .expect("an unreadable link does not fail the issue");
    let pulls = &detail.linked_pull_requests;
    assert_eq!(pulls.len(), 3, "every link is still shown");
    assert_eq!(
        pulls[0].checks.as_ref().map(|checks| checks.passing),
        Some(1)
    );
    assert_eq!(
        pulls[1].checks, None,
        "a refused read is unknown, never zero checks"
    );
    assert_eq!(
        pulls[2].checks, None,
        "a link GitHub named no repository for is not read at all"
    );
    let mut seen = reads.lock().unwrap().clone();
    seen.sort();
    assert_eq!(
        seen,
        [
            ("12".to_owned(), "acme/app".to_owned()),
            ("13".to_owned(), "acme/other".to_owned())
        ],
        "the link with no repository was never read"
    );
}

#[tokio::test]
async fn only_the_first_links_are_read_and_the_rest_stay_shown_unknown() {
    let reads: Reads = Arc::new(Mutex::new(Vec::new()));
    let record = reads.clone();
    let total = LINKED_CHECKS_READ_LIMIT + 5;
    let fixture: GhAnswer = Arc::new(
        move |_forge: Forge, _cwd: &Path, args: &[&str], _input: Option<&str>| -> Result<String> {
            if args.contains(&"closedByPullRequestsReferences") {
                let many: Vec<Value> = (1..=total)
                    .map(|number| {
                        json!({ "number": number, "state": "OPEN",
                            "repository": { "nameWithOwner": "acme/app" } })
                    })
                    .collect();
                return Ok(links(Value::Array(many)));
            }
            if args.contains(&"statusCheckRollup") {
                let (number, repository) =
                    target(args).expect("a pr view names a number and its repository");
                record.lock().unwrap().push((number, repository));
                return Ok(rollup("SUCCESS"));
            }
            Ok(detail_json().to_string())
        },
    );
    let detail = with_crew_input_fixture(fixture, async { github().get("acme/app", "7").await })
        .await
        .unwrap();
    let pulls = &detail.linked_pull_requests;
    assert_eq!(pulls.len(), total, "every link is still shown");
    assert_eq!(
        reads.lock().unwrap().len(),
        LINKED_CHECKS_READ_LIMIT,
        "only the first links are read"
    );
    assert!(
        pulls[..LINKED_CHECKS_READ_LIMIT]
            .iter()
            .all(|pull| pull.checks.is_some())
    );
    assert!(
        pulls[LINKED_CHECKS_READ_LIMIT..]
            .iter()
            .all(|pull| pull.checks.is_none()),
        "a link over the read limit is unknown, never zero checks"
    );
}
