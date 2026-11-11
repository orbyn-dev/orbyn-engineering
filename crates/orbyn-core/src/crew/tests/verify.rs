//! The verifier's pieces on their own: what a worker may read about a failure, the fingerprint
//! the guard compares, and the checks that run in a checkout.

use orbyn_contracts::CheckOutcome;
use serde_json::json;

use crate::crew::checks::{Ran, Site, covers, run_check, tail};
use crate::crew::{redact, withhold};

fn check(value: serde_json::Value) -> orbyn_contracts::CrewCheck {
    serde_json::from_value(value).unwrap()
}

#[test]
fn redaction_removes_secrets_and_withholding_removes_the_key() {
    let token = format!("ghp_{}", "x".repeat(36));
    let out = redact(&format!("token was {token} in the log"));
    assert!(
        out.contains("[redacted]") && !out.contains(&token),
        "a secret survived"
    );
    let needles = vec!["cargo test -p crew-demo".to_owned(), String::new()];
    let text = withhold("error: cargo test -p crew-demo failed", &needles);
    assert_eq!(text, "error: [check] failed");
}

#[test]
fn a_reason_never_carries_the_check_or_a_secret() {
    let command = "cargo test -p crew-demo --lib".to_owned();
    let secret = format!("sk-{}", "z".repeat(24));
    let ran = Ran {
        outcome: CheckOutcome::Fail,
        exit_code: Some(101),
        command: command.clone(),
        log: String::new(),
        duration_ms: 1,
        reason: Some(format!(
            "The check exited with 101.\nran: {command}\nkey: {secret}"
        )),
        names: Vec::new(),
    };
    let reason = crate::crew::verify::worker_reason(&ran, std::slice::from_ref(&command));
    assert!(reason.contains("101") && reason.contains("[check]"));
    assert!(!reason.contains(&command), "the command reached a worker");
    assert!(!reason.contains(&secret), "a secret reached a worker");
}

#[test]
fn patterns_cover_whole_segments() {
    let p = |s: &str| orbyn_contracts::CrewPathPattern::new(s).unwrap();
    assert!(covers(&p("src/**"), "src/a/b.rs"));
    assert!(covers(&p("src/a.rs"), "src/a.rs"));
    assert!(!covers(&p("src/**"), "src2/a.rs"));
    assert!(!covers(&p("src/a.rs"), "src/a.rs.bak"));
    assert_eq!(tail("a\nb\nc\nd", 4), "c\nd");
    assert_eq!(tail(" short ", 100), "short");
}

#[tokio::test]
async fn command_checks_pass_on_the_expected_exit_and_fail_with_the_output() {
    let dir = crate::testing::TempDir::new();
    let site = Site {
        worktree: dir.path(),
        deadline: std::time::Instant::now() + std::time::Duration::from_secs(60),
        base: None,
    };
    let one = |run: &str, expect: u32| {
        check(json!({ "id": "c1", "kind": "command", "proves": "p",
            "params": { "run": run, "expect_exit": expect } }))
    };
    let ok = run_check(&one("exit 0", 0), &site).await;
    assert_eq!((ok.outcome, ok.exit_code), (CheckOutcome::Pass, Some(0)));
    let expected = run_check(&one("exit 3", 3), &site).await;
    assert_eq!(expected.outcome, CheckOutcome::Pass);
    let failed = run_check(&one("echo the-visible-failure && exit 2", 0), &site).await;
    assert_eq!(
        (failed.outcome, failed.exit_code),
        (CheckOutcome::Fail, Some(2))
    );
    let reason = failed.reason.unwrap();
    assert!(reason.contains("the-visible-failure") && reason.contains("exited with 2"));
    // A kind this version does not run is skipped, never passed or failed.
    let page = check(json!({ "id": "c2", "kind": "preview", "proves": "p",
        "params": { "url": "http://127.0.0.1:1/" } }));
    assert_eq!(run_check(&page, &site).await.outcome, CheckOutcome::Skipped);
}

fn failed(reason: &str, names: &[&str]) -> crate::crew::verify::Failed {
    crate::crew::verify::Failed {
        id: "chk-a".into(),
        outcome: CheckOutcome::Fail,
        exit_code: Some(101),
        names: names.iter().map(|n| (*n).to_owned()).collect(),
        reason: reason.into(),
    }
}

fn judged(failures: Vec<crate::crew::verify::Failed>) -> crate::crew::Judgement {
    crate::crew::Judgement {
        commit: orbyn_contracts::CrewCommit::new("0".repeat(40)).unwrap(),
        checks: Vec::new(),
        evidence: Vec::new(),
        failures,
        not_verified: Vec::new(),
        passes: 0,
    }
}

/// The guard compares what failed, not the output: output that changes between runs must not
/// make every failure look new.
#[test]
fn the_fingerprint_is_the_check_the_exit_and_the_names_and_not_the_output() {
    let a = judged(vec![failed(
        "ran 12 tests in 0.31s, then 5 lines of noise",
        &["m::t1", "m::t2"],
    )]);
    let b = judged(vec![failed(
        "took 0.97s; a different log entirely",
        &["m::t2", "m::t1"],
    )]);
    assert_eq!(a.fingerprint(), b.fingerprint());
    let other_test = judged(vec![failed("same", &["m::t3"])]);
    assert_ne!(a.fingerprint(), other_test.fingerprint());
    let mut other_exit = failed("same", &["m::t1", "m::t2"]);
    other_exit.exit_code = Some(1);
    assert_ne!(a.fingerprint(), judged(vec![other_exit]).fingerprint());
}

#[test]
fn failing_tests_are_named_from_common_runners() {
    let log = "test a::b ... FAILED\ntest a::c ... ok\n--- FAIL: TestX (0.01s)\nFAILED tests/t.py::test_y - boom\n";
    assert_eq!(
        crate::crew::checks::failing_names(log),
        ["TestX", "a::b", "tests/t.py::test_y"]
    );
    assert!(crate::crew::checks::failing_names("all fine").is_empty());
}

#[tokio::test]
async fn no_check_runs_past_the_tasks_time_budget() {
    let dir = crate::testing::TempDir::new();
    let site = Site {
        worktree: dir.path(),
        deadline: std::time::Instant::now(),
        base: None,
    };
    let run = check(json!({ "id": "c1", "kind": "command", "proves": "p",
        "params": { "run": "exit 0" } }));
    let ran = run_check(&run, &site).await;
    assert_eq!(ran.outcome, CheckOutcome::Error);
    assert!(ran.reason.unwrap().contains("time budget"));
}
