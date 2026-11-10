use super::parse::{check_state, github_checks, github_rollup, parse_pull};
use super::process::{no_such_pull, run_owned};
use std::process::Stdio;
use std::time::Duration;

use orbyn_contracts::{MergePull, PullChecks, ReviewLineComment};
use serde_json::{Value, json};
use tokio::process::Command;

use super::create::number_from_url;
use super::details::{
    DetailsOutcome, classify_details, details_query, graphql_reset, parse_details,
};
use super::github::{github_threads, parse_github_event};
use super::gitlab::gitlab_conversation;
use super::merge::{
    check_head, head_changed, head_in, head_moved, head_precheck_needed, merge_args,
    strip_agent_credits,
};
use super::review::review_payload;
use super::status::auth_accounts;
use super::*;
use crate::error::CoreError;

#[tokio::test]
async fn codeql_14_pull_creation_validates_branch_operands_before_spawning() {
    let dir = crate::testing::TempDir::new();
    // An absent cwd prevents even the unchanged implementation from starting a CLI.
    let absent = dir.path().join("absent");
    for forge in [Forge::GitHub, Forge::GitLab] {
        for (branch, base) in [
            ("--help", Some("main")),
            ("feature/safe", Some("--help")),
            ("feature/safe", Some("")),
            ("feature/safe", Some("main\nother")),
        ] {
            let error = create_pull(forge, &absent, branch, "Title", "Body", base)
                .await
                .unwrap_err();
            assert!(matches!(error, CoreError::Invalid(_)), "{forge:?}: {error}");
        }
    }
    // A valid operand reaches the forge dispatch, rather than the invalid-input guard.
    let error = create_pull(
        Forge::Bitbucket,
        &absent,
        "feature/safe",
        "--help; title",
        "$(literal) body",
        Some("main"),
    )
    .await
    .unwrap_err();
    assert!(matches!(error, CoreError::Unsupported(_)), "{error}");
}

#[tokio::test]
async fn codeql_14_pull_updates_use_the_same_base_operand_boundary() {
    let dir = crate::testing::TempDir::new();
    let absent = dir.path().join("absent");
    for forge in [Forge::GitHub, Forge::GitLab] {
        for base in ["--help", "", "main\nother", "main\0other"] {
            let update: orbyn_contracts::UpdatePull =
                serde_json::from_value(json!({ "base": base })).unwrap();
            let error = update_pull(forge, &absent, 1, &update).await.unwrap_err();
            assert!(matches!(error, CoreError::Invalid(_)), "{forge:?}: {error}");
        }
    }
}

#[test]
fn a_committed_timeline_event_takes_its_author_date_message_and_sha() {
    let commit = serde_json::json!({
        "event": "committed",
        "sha": "abc123",
        "author": { "name": "Ada", "email": "ada@example.com", "date": "2026-11-05T01:02:03Z" },
        "committer": { "name": "GitHub", "date": "2026-11-05T01:05:00Z" },
        "message": "Fix the redirect\n\nLong body."
    });
    let parsed = parse_github_event(&commit);
    assert_eq!(parsed.id, "abc123");
    assert_eq!(parsed.actor.as_deref(), Some("Ada"));
    assert_eq!(parsed.created_at.as_deref(), Some("2026-11-05T01:05:00Z"));
    assert_eq!(parsed.text.as_deref(), Some("Fix the redirect"));
    // Other events keep their own fields.
    let other = serde_json::json!({ "event": "commented", "id": 7, "actor": { "login": "sam" }, "created_at": "2026-11-05T02:00:00Z", "body": "Looks good" });
    let parsed = parse_github_event(&other);
    assert_eq!(parsed.id, "7");
    assert_eq!(parsed.actor.as_deref(), Some("sam"));
    assert_eq!(parsed.text.as_deref(), Some("Looks good"));
}

#[test]
fn the_graphql_reset_is_read_from_the_rate_limit_answer() {
    assert_eq!(
        graphql_reset(r#"{"resources":{"core":{"reset":1},"graphql":{"reset":1762600000}}}"#),
        Some(1_762_600_000)
    );
    assert_eq!(graphql_reset("not json"), None);
    assert_eq!(graphql_reset(r#"{"resources":{}}"#), None);
}

#[test]
fn the_head_the_person_saw_goes_to_the_forge() {
    let options = MergePull {
        method: "squash".into(),
        auto: false,
        delete_branch: false,
        subject: None,
        body: None,
        strip_credits: false,
        expected_head: Some("abc123".into()),
    };
    let github = merge_args(Forge::GitHub, "7", &options, None);
    assert_eq!(
        github,
        [
            "pr",
            "merge",
            "7",
            "--squash",
            "--match-head-commit",
            "abc123"
        ]
    );
    let gitlab = merge_args(Forge::GitLab, "7", &options, None);
    assert_eq!(
        gitlab,
        ["mr", "merge", "7", "--yes", "--squash", "--sha", "abc123"]
    );
    let rebase = MergePull {
        method: "rebase".into(),
        ..options.clone()
    };
    assert!(
        !merge_args(Forge::GitLab, "7", &rebase, None)
            .iter()
            .any(|arg| arg == "--sha")
    );
    assert!(
        merge_args(Forge::GitHub, "7", &rebase, None)
            .iter()
            .any(|arg| arg == "--match-head-commit")
    );
    let none = MergePull {
        expected_head: None,
        ..options
    };
    assert!(
        !merge_args(Forge::GitHub, "7", &none, None)
            .iter()
            .any(|arg| arg.contains("head"))
    );
}

#[test]
fn only_forges_without_a_head_flag_and_a_gitlab_rebase_read_the_head_first() {
    assert!(!head_precheck_needed(Forge::GitHub, "squash"));
    assert!(!head_precheck_needed(Forge::GitHub, "rebase"));
    assert!(!head_precheck_needed(Forge::GitLab, "squash"));
    assert!(!head_precheck_needed(Forge::GitLab, "merge"));
    assert!(head_precheck_needed(Forge::GitLab, "rebase"));
    for forge in [Forge::Azure, Forge::Forgejo, Forge::Bitbucket] {
        assert!(head_precheck_needed(forge, "squash"));
    }
}

#[test]
fn each_forge_names_its_head_in_its_own_record() {
    assert_eq!(
        head_in(Forge::GitLab, &json!({ "sha": "g1" })).as_deref(),
        Some("g1")
    );
    assert_eq!(
        head_in(
            Forge::Azure,
            &json!({ "lastMergeSourceCommit": { "commitId": "a1" } })
        )
        .as_deref(),
        Some("a1")
    );
    assert_eq!(
        head_in(Forge::Forgejo, &json!({ "head": { "sha": "f1" } })).as_deref(),
        Some("f1")
    );
    assert_eq!(head_in(Forge::GitLab, &json!({ "sha": "" })), None);
    assert_eq!(head_in(Forge::Azure, &json!({})), None);
    assert_eq!(head_in(Forge::Bitbucket, &json!({ "sha": "x" })), None);
}

#[test]
fn the_head_check_merges_on_a_match_refuses_a_mismatch_and_fails_closed() {
    assert!(check_head("abc", Ok("abc".into())).is_ok());
    assert!(matches!(
        check_head("abc", Ok("def".into())),
        Err(CoreError::PullHeadChanged)
    ));
    let unreadable = check_head("abc", Err(CoreError::Git("boom".into())));
    assert!(matches!(&unreadable, Err(CoreError::Git(message)) if message.contains("not merged")));
    assert!(matches!(
        check_head("abc", Err(CoreError::Unsupported("no".into()))),
        Err(CoreError::Unsupported(_))
    ));
}

#[test]
fn a_refusal_for_a_moved_head_is_its_own_error() {
    let github = Forge::GitHub;
    assert!(head_changed(
        github,
        "X Pull request #4 is not mergeable: Head branch was modified. Review and try the merge again."
    ));
    assert!(matches!(
        head_moved(
            github,
            CoreError::Git(
                "GraphQL: Head branch was modified. Review and try the merge again.".into()
            )
        ),
        CoreError::PullHeadChanged
    ));
    // Other failures keep their own words.
    assert!(matches!(
        head_moved(github, CoreError::Git("merge conflict".into())),
        CoreError::Git(_)
    ));
    assert!(head_changed(
        Forge::GitLab,
        "405: SHA does not match HEAD of source branch"
    ));
    // A message that only mentions a head commit is not a moved head.
    assert!(!head_changed(
        Forge::GitHub,
        "could not find the head commit"
    ));
    assert!(!head_changed(Forge::Forgejo, "Head branch was modified"));
    assert_eq!(CoreError::PullHeadChanged.code(), "pull_head_changed");
    assert_eq!(CoreError::PullHeadChanged.status(), 409);
}

#[test]
fn only_a_forges_no_such_pull_request_answer_is_matched() {
    let github = Forge::GitHub;
    assert!(no_such_pull(
        github,
        "GraphQL: Could not resolve to a PullRequest with the number of 5. (repository.pullRequest)"
    ));
    // The branch form is not a numbered answer, and a proxy's bare 404 page is not GitLab's.
    assert!(!no_such_pull(
        github,
        "no pull requests found for branch \"x\""
    ));
    assert!(!no_such_pull(
        Forge::GitLab,
        "<html>404 Not Found</html> from a proxy"
    ));
    assert!(no_such_pull(
        Forge::GitLab,
        "GET https://x/api/v4/projects/1/merge_requests/9: 404 Not Found"
    ));
    assert!(no_such_pull(
        Forge::Azure,
        "TF401180: The requested pull request was not found."
    ));
    // Auth, rate limit and server failures keep their own wording.
    assert!(!no_such_pull(
        github,
        "HTTP 401: Bad credentials (https://api.github.com/graphql)"
    ));
    assert!(!no_such_pull(github, "API rate limit exceeded"));
    assert!(!no_such_pull(github, "HTTP 502: Bad Gateway"));
    assert!(!no_such_pull(Forge::GitLab, "401 Unauthorized"));
    assert!(!no_such_pull(
        Forge::Azure,
        "Please run 'az login' to setup account."
    ));
    assert!(!no_such_pull(Forge::Forgejo, "404 Not Found"));
}

#[tokio::test]
async fn a_finished_forge_cli_does_not_leave_helpers_running() {
    let dir = crate::testing::TempDir::new();
    // Git runs ! aliases through its shell on both supported platforms, so it
    // stands in for a CLI whose helper outlives it.
    let mut builder = Command::new("git");
    builder
        .args([
            "-c",
            "alias.detach=!(sleep 1; echo escaped > escaped) &",
            "detach",
        ])
        .current_dir(dir.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = tokio::time::timeout(Duration::from_secs(5), run_owned(builder, None))
        .await
        .unwrap()
        .unwrap();
    assert!(output.status.success());
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(
        !dir.path().join("escaped").exists(),
        "the CLI's helper outlived it"
    );
}

#[test]
fn forge_comes_from_the_host_alone() {
    assert_eq!(Forge::of_remote("git@github.com:a/b.git"), Forge::GitHub);
    assert_eq!(Forge::of_remote("git@gitlab.com:a/b.git"), Forge::GitLab);
    assert_eq!(
        Forge::of_remote("https://gitlab.acme.dev/a/b"),
        Forge::GitLab
    );
    assert_eq!(
        Forge::of_remote("https://github.com/gitlab/b"),
        Forge::GitHub
    );
    assert_eq!(
        Forge::of_remote("ssh://git@gitlab.x.io:2222/a/b"),
        Forge::GitLab
    );
    assert_eq!(
        Forge::of_remote("https://dev.azure.com/o/p/_git/r"),
        Forge::Azure
    );
    assert_eq!(
        Forge::of_remote("https://o.visualstudio.com/p/_git/r"),
        Forge::Azure
    );
    assert_eq!(
        Forge::of_remote("git@bitbucket.org:a/b.git"),
        Forge::Bitbucket
    );
    assert_eq!(Forge::of_remote("https://codeberg.org/a/b"), Forge::Forgejo);
    assert_eq!(
        Forge::of_remote("https://git.corp.example/a/b"),
        Forge::GitHub
    );
}

#[test]
fn a_github_row_carries_check_counts_from_its_own_rollup() {
    let row = json!({
        "number": 7, "title": "T", "url": "u", "state": "OPEN",
        "headRefName": "a", "baseRefName": "b",
        "statusCheckRollup": [
            { "name": "ci", "status": "COMPLETED", "conclusion": "SUCCESS" },
            { "name": "lint", "status": "COMPLETED", "conclusion": "FAILURE" },
            { "name": "e2e", "status": "IN_PROGRESS", "conclusion": "" },
            { "context": "ci/legacy", "state": "ERROR" },
        ],
    });
    let pull = parse_pull(&row).unwrap();
    let checks = pull.checks.unwrap();
    assert_eq!(
        (checks.total, checks.passing, checks.failing, checks.running),
        (4, 1, 2, 1)
    );
    assert_eq!(checks.failing_names, ["lint", "ci/legacy"]);
    assert_eq!(pull.checks_state.as_deref(), Some("fail"));
    let bare = json!({ "number": 1, "title": "T", "url": "u", "state": "OPEN" });
    assert!(parse_pull(&bare).unwrap().checks.is_none());
}

#[test]
fn every_forge_reads_into_one_shape() {
    let hub = json!({
        "number": 7, "title": "T", "url": "https://x/7", "state": "MERGED",
        "headRefName": "feat", "baseRefName": "main", "author": { "login": "a" },
        "isDraft": true, "reviewDecision": "APPROVED", "labels": [{ "name": "bug" }],
        "statusCheckRollup": [{ "status": "COMPLETED", "conclusion": "FAILURE" }],
    });
    let pull = parse_pull(&hub).unwrap();
    assert_eq!(pull.state, "merged");
    assert_eq!(pull.author.as_deref(), Some("a"));
    assert!(pull.draft);
    assert_eq!(pull.labels, ["bug"]);
    assert_eq!(pull.review_state.as_deref(), Some("approved"));
    assert_eq!(pull.checks_state.as_deref(), Some("fail"));
    let lab = json!({
        "iid": 9, "title": "T", "web_url": "https://x/9", "state": "opened",
        "source_branch": "feat", "target_branch": "main", "author": { "username": "b" },
        "labels": ["ui"],
    });
    let pull = parse_pull(&lab).unwrap();
    assert_eq!((pull.number, pull.state.as_str()), (9, "open"));
    assert_eq!(pull.head, "feat");
    assert_eq!(pull.labels, ["ui"]);
    let azure = json!({
        "pullRequestId": 4, "title": "T", "status": "completed",
        "sourceRefName": "refs/heads/feat", "targetRefName": "refs/heads/main",
        "createdBy": { "displayName": "C" },
    });
    let pull = parse_pull(&azure).unwrap();
    assert_eq!(
        (pull.state.as_str(), pull.head.as_str(), pull.base.as_str()),
        ("merged", "feat", "main")
    );
    assert_eq!(pull.author.as_deref(), Some("C"));
    let forgejo = json!({ "index": 5, "title": "T", "state": "open", "head": "h", "base": "b", "poster": "d" });
    assert_eq!(parse_pull(&forgejo).unwrap().number, 5);
    assert!(parse_pull(&json!({ "title": "no number" })).is_none());
}

#[test]
fn pull_numbers_come_from_the_url() {
    assert_eq!(
        number_from_url("https://github.com/a/b/pull/12\n".trim()),
        Some(12)
    );
    assert_eq!(number_from_url("https://x/a/-/merge_requests/3/"), Some(3));
    assert_eq!(number_from_url("azure:44"), Some(44));
    assert_eq!(number_from_url("not a url"), None);
}

#[test]
fn credit_lines_leave_the_message_and_other_lines_stay() {
    let message = "Fix the thing\n\nCo-Authored-By: Claude Opus <noreply@anthropic.com>\n\
                   Co-authored-by: Sam <sam@example.com>\n\n🤖 Generated with [Claude Code](x)\n";
    assert_eq!(
        strip_agent_credits(message),
        "Fix the thing\n\nCo-authored-by: Sam <sam@example.com>"
    );
}

#[test]
fn check_words_come_from_status_and_conclusion() {
    assert_eq!(check_state("COMPLETED", "SUCCESS"), "pass");
    assert_eq!(check_state("COMPLETED", "TIMED_OUT"), "fail");
    assert_eq!(check_state("IN_PROGRESS", ""), "pending");
    assert_eq!(check_state("SUCCESS", ""), "pass");
    assert_eq!(check_state("failed", ""), "fail");
}

fn rolled(entries: Value) -> (PullChecks, String) {
    let row = json!({ "statusCheckRollup": entries });
    (github_checks(&row).unwrap(), github_rollup(&row).unwrap())
}

#[test]
fn rollup_entries_count_by_completion_then_conclusion() {
    let run = |status: &str, conclusion: &str| json!({ "name": "c", "status": status, "conclusion": conclusion });
    let context = |state: &str| json!({ "context": "c", "state": state });
    // (entry, passing, failing, running) with a total of one each.
    let table = [
        (run("COMPLETED", "SUCCESS"), 1, 0, 0),
        (run("COMPLETED", "FAILURE"), 0, 1, 0),
        (run("COMPLETED", "TIMED_OUT"), 0, 1, 0),
        (run("COMPLETED", "CANCELLED"), 0, 1, 0),
        (run("COMPLETED", "ACTION_REQUIRED"), 0, 1, 0),
        (run("COMPLETED", "STARTUP_FAILURE"), 0, 1, 0),
        (run("COMPLETED", "STALE"), 0, 0, 0),
        (run("COMPLETED", "NEUTRAL"), 0, 0, 0),
        (run("COMPLETED", "SKIPPED"), 0, 0, 0),
        (run("IN_PROGRESS", ""), 0, 0, 1),
        (run("QUEUED", ""), 0, 0, 1),
        (run("WAITING", ""), 0, 0, 1),
        // Not completed, so a stale conclusion does not count.
        (run("IN_PROGRESS", "FAILURE"), 0, 0, 1),
        (context("SUCCESS"), 1, 0, 0),
        (context("FAILURE"), 0, 1, 0),
        (context("ERROR"), 0, 1, 0),
        (context("PENDING"), 0, 0, 1),
        (context("EXPECTED"), 0, 0, 1),
    ];
    for (entry, passing, failing, running) in table {
        let (checks, _) = rolled(json!([entry.clone()]));
        assert_eq!(
            (checks.total, checks.passing, checks.failing, checks.running),
            (1, passing, failing, running),
            "{entry}"
        );
    }
}

#[test]
fn the_rollup_word_follows_the_counts() {
    let run = |status: &str, conclusion: &str| json!({ "name": "c", "status": status, "conclusion": conclusion });
    assert_eq!(rolled(json!([])).1, "none");
    assert_eq!(rolled(json!([run("COMPLETED", "SKIPPED")])).1, "pass");
    assert_eq!(
        rolled(json!([run("COMPLETED", "SUCCESS"), run("QUEUED", "")])).1,
        "pending"
    );
    assert_eq!(
        rolled(json!([
            run("COMPLETED", "ACTION_REQUIRED"),
            run("QUEUED", "")
        ]))
        .1,
        "fail"
    );
}

#[test]
fn failing_names_are_capped() {
    let many: Vec<Value> = (0..30)
        .map(|n| json!({ "name": format!("c{n}"), "status": "COMPLETED", "conclusion": "FAILURE" }))
        .collect();
    let (checks, _) = rolled(json!(many));
    assert_eq!((checks.failing, checks.failing_names.len()), (30, 20));
}

#[test]
fn a_review_with_lines_keeps_every_line() {
    let payload = review_payload(
        "request_changes",
        "See notes",
        &[ReviewLineComment {
            path: "a.rs".into(),
            line: 3,
            body: "Hm".into(),
        }],
    );
    assert_eq!(payload["event"], "REQUEST_CHANGES");
    assert_eq!(payload["comments"][0]["side"], "RIGHT");
}

#[test]
fn gitlab_discussions_split_into_comments_and_threads() {
    let discussions = vec![
        json!({ "id": "d1", "notes": [{
            "id": 1, "body": "fix", "resolvable": true, "resolved": false,
            "author": { "username": "a" },
            "position": { "new_path": "x.rs", "new_line": 4 },
        }]}),
        json!({ "id": "d2", "notes": [{ "id": 2, "body": "hello", "resolvable": false }]}),
        json!({ "id": "d3", "notes": [{ "id": 3, "body": "merged", "system": true }]}),
    ];
    let conversation = gitlab_conversation(&discussions);
    assert_eq!(conversation.threads.len(), 1);
    assert_eq!(conversation.threads[0].line, Some(4));
    assert_eq!(conversation.comments.len(), 1);
}

#[test]
fn github_threads_are_read_from_graphql() {
    let reply = json!({ "data": { "repository": { "pullRequest": { "reviewThreads": { "nodes": [{
        "id": "T1", "isResolved": true, "path": "a.rs", "line": 2,
        "comments": { "nodes": [{ "databaseId": 11, "body": "b", "author": { "login": "z" } }] },
    }]}}}}});
    let threads = github_threads(&reply);
    assert!(threads[0].resolved);
    assert_eq!(threads[0].comments[0].id, "11");
    assert_eq!(threads[0].comments[0].author.as_deref(), Some("z"));
}

#[test]
fn accounts_are_read_without_any_token() {
    let output =
        "github.com\n  ✓ Logged in to github.com account octo (keyring)\n  - Token: gho_****\n";
    assert_eq!(auth_accounts(output), ["octo"]);
}

fn details_response() -> Value {
    json!({ "data": {
        "viewer": { "login": "Me" },
        "repository": {
            "p3": {
                "createdAt": "2026-01-02T03:04:05Z",
                "reviewRequests": { "nodes": [
                    { "requestedReviewer": { "__typename": "User", "login": "sam" } },
                    { "requestedReviewer": { "__typename": "Team", "slug": "core" } },
                    { "requestedReviewer": { "__typename": "Bot" } },
                ] },
                "latestReviews": { "nodes": [
                    { "author": null, "state": "APPROVED" },
                    { "author": { "login": "me" }, "state": "CHANGES_REQUESTED" },
                ] },
                "comments": { "totalCount": 2 },
                "reviews": { "nodes": [
                    { "comments": { "totalCount": 3 } },
                    { "comments": { "totalCount": 0 } },
                ] },
            },
            "p4": {
                "createdAt": "2026-02-02T00:00:00Z",
                "reviewRequests": { "nodes": [] },
                "latestReviews": { "nodes": [] },
                "comments": { "totalCount": 0 },
                "reviews": { "nodes": [] },
            },
            "p5": null,
            "p6": { "comments": { "totalCount": 1 } },
        },
    } })
}

#[test]
fn the_details_response_merges_per_pull_request() {
    let details = parse_details(&details_response(), "proj", &[3, 4, 5, 6, 7]);
    // A pull request that is missing, null or incomplete has no entry at all.
    assert_eq!(
        details
            .keys()
            .copied()
            .collect::<std::collections::BTreeSet<_>>(),
        [3, 4].into()
    );
    let three = &details[&3];
    assert_eq!(three.project_id, "proj");
    assert_eq!(three.created_at.as_deref(), Some("2026-01-02T03:04:05Z"));
    // Conversation comments plus every review's comments.
    assert_eq!(three.comment_count, Some(5));
    let reviewers = three.requested_reviewers.as_ref().unwrap();
    assert_eq!(
        reviewers
            .iter()
            .map(|r| (r.name.as_str(), r.kind.as_str()))
            .collect::<Vec<_>>(),
        [("sam", "user"), ("core", "team")]
    );
    // The viewer is matched ignoring case, and a null author is skipped.
    assert_eq!(
        three.viewer_review_state.as_deref(),
        Some("changes_requested")
    );
    let four = &details[&4];
    assert_eq!(four.requested_reviewers.as_deref(), Some(&[][..]));
    assert_eq!(four.viewer_review_state.as_deref(), Some("none"));
    assert_eq!(four.comment_count, Some(0));
}

#[test]
fn without_a_viewer_the_review_state_is_absent_not_none() {
    let mut response = details_response();
    response["data"]["viewer"] = Value::Null;
    let details = parse_details(&response, "proj", &[3]);
    assert_eq!(details[&3].viewer_review_state, None);
    assert_eq!(details[&3].comment_count, Some(5));
}

#[test]
fn details_are_absent_when_the_batch_fails_or_the_limit_is_spent() {
    let ok = details_response().to_string();
    assert!(matches!(
        classify_details(true, &ok, "", "p", &[3]),
        DetailsOutcome::Details(found) if found.len() == 1
    ));
    for (success, stdout, stderr) in [
        (false, "", "GraphQL: API rate limit already exceeded"),
        (false, r#"{"errors":[{"type":"RATE_LIMITED"}]}"#, ""),
    ] {
        assert_eq!(
            classify_details(success, stdout, stderr, "p", &[3]),
            DetailsOutcome::RateLimited
        );
    }
    for (success, stdout) in [
        (false, ""),
        (true, "not json"),
        (
            true,
            r#"{"data":{},"errors":[{"message":"Could not resolve"}]}"#,
        ),
    ] {
        assert_eq!(
            classify_details(success, stdout, "", "p", &[3]),
            DetailsOutcome::Unavailable
        );
    }
}

#[test]
fn errors_beside_data_keep_every_row_that_came_back() {
    let mut response = details_response();
    // p5 is null, and an error inside p4 nulled a node of an otherwise whole row.
    response["errors"] = json!([
        { "type": "NOT_FOUND", "path": ["repository", "p5"] },
        { "type": "FORBIDDEN", "path": ["repository", "p4", "reviewRequests", "nodes", 0, "requestedReviewer"] },
    ]);
    let outcome = classify_details(false, &response.to_string(), "", "p", &[3, 4, 5]);
    let DetailsOutcome::Details(found) = outcome else {
        panic!("rows with data are kept: {outcome:?}");
    };
    assert_eq!(found.keys().copied().collect::<Vec<_>>(), [3]);
}

#[test]
fn the_details_query_names_one_alias_per_pull_request() {
    let query = details_query(&[3, 40]);
    assert!(query.contains("viewer{login}"));
    assert!(
        query.contains("p3:pullRequest(number:3)") && query.contains("p40:pullRequest(number:40)")
    );
    assert_eq!(query.matches("pullRequest(").count(), 2);
}
