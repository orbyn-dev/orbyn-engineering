//! The verifier's pieces on their own: what a worker may read about a failure, the fingerprint
//! the guard compares, and the checks that run in a checkout.

use orbyn_contracts::CheckOutcome;
use serde_json::json;

use crate::crew::checks::{Ran, Site, covers, run_check, tail};
use crate::crew::{redact, withhold};

fn check(value: serde_json::Value) -> orbyn_contracts::CrewCheck {
    serde_json::from_value(value).unwrap()
}

fn integrity_repo() -> crate::testing::TempDir {
    let dir = crate::testing::TempDir::new();
    crate::testing::run_git(dir.path(), &["init", "-q"]);
    crate::testing::run_git(dir.path(), &["config", "core.autocrlf", "false"]);
    std::fs::write(dir.path().join("base.txt"), "base\n").unwrap();
    super::review_support::commit_all(dir.path(), "base");
    dir
}

/// Accepted remaining limit: ignore rules still hide newly created files.
/// This characterizes blindness, not a passing security mitigation (#451/#436).
#[tokio::test]
async fn ignored_new_files_remain_an_accepted_blind_spot() {
    for source in [".gitignore", ".git/info/exclude", "external"] {
        let dir = integrity_repo();
        let root = dir.path();
        let ignore = if source == "external" {
            let file = root.join(".git/global-ignore");
            crate::testing::run_git(
                root,
                &["config", "core.excludesFile", file.to_str().unwrap()],
            );
            file
        } else {
            root.join(source)
        };
        std::fs::write(ignore, "hidden.txt\n").unwrap();
        if source == ".gitignore" {
            super::review_support::commit_all(root, "ignore");
        }
        crate::testing::run_git(root, &["config", "core.untrackedCache", "true"]);
        std::fs::write(root.join("hidden.txt"), "one\ntwo\n").unwrap();
        let site = Site {
            worktree: root,
            deadline: std::time::Instant::now() + std::time::Duration::from_secs(60),
            base: None,
        };
        let scope = check(
            json!({ "id": "scope", "kind": "file_scope", "proves": "scope", "params": { "only": ["base.txt"] } }),
        );
        let size = check(
            json!({ "id": "size", "kind": "diff_size", "proves": "size", "params": { "max_lines": 1 } }),
        );
        assert_eq!(
            run_check(&scope, &site).await.outcome,
            CheckOutcome::Pass,
            "accepted blind spot: {source}"
        );
        assert_eq!(
            run_check(&size, &site).await.outcome,
            CheckOutcome::Pass,
            "accepted blind spot: {source}"
        );
        std::fs::remove_file(root.join("hidden.txt")).unwrap();
        assert_eq!(
            run_check(&scope, &site).await.outcome,
            CheckOutcome::Pass,
            "{source}"
        );
        assert_eq!(
            run_check(&size, &site).await.outcome,
            CheckOutcome::Pass,
            "{source}"
        );
        // Ordinary visible new paths still fail both checks under the same key.
        std::fs::write(root.join("visible.txt"), "one\ntwo\n").unwrap();
        assert_eq!(
            run_check(&scope, &site).await.outcome,
            CheckOutcome::Fail,
            "{source}"
        );
        assert_eq!(
            run_check(&size, &site).await.outcome,
            CheckOutcome::Fail,
            "{source}"
        );
    }
}

/// Existing ignored caches must not require data deletion or broader task scope.
#[tokio::test]
async fn preexisting_ignored_artifacts_do_not_reject_an_ordinary_narrow_task() {
    let dir = integrity_repo();
    let root = dir.path();
    std::fs::write(root.join(".gitignore"), "target/\nnode_modules/\n.env\n").unwrap();
    super::review_support::commit_all(root, "ignore generated and local files");
    std::fs::create_dir_all(root.join("target")).unwrap();
    std::fs::create_dir_all(root.join("node_modules")).unwrap();
    // Synthetic data only. All three artifacts exist before the task's Site is made.
    std::fs::write(root.join("target/cache.txt"), "cache\n").unwrap();
    std::fs::write(root.join("node_modules/cache.txt"), "dependency\n").unwrap();
    std::fs::write(root.join(".env"), "SYNTHETIC_FIXTURE=1\n").unwrap();
    let site = Site {
        worktree: root,
        deadline: std::time::Instant::now() + std::time::Duration::from_secs(60),
        base: None,
    };
    std::fs::write(root.join("base.txt"), "changed\n").unwrap();
    let narrow_scope = check(
        json!({ "id": "scope", "kind": "file_scope", "proves": "scope", "params": { "only": ["base.txt"] } }),
    );
    let narrow_size = check(
        json!({ "id": "size", "kind": "diff_size", "proves": "size", "params": { "max_files": 1, "max_lines": 2 } }),
    );
    assert_eq!(
        run_check(&narrow_scope, &site).await.outcome,
        CheckOutcome::Pass
    );
    assert_eq!(
        run_check(&narrow_size, &site).await.outcome,
        CheckOutcome::Pass
    );
    assert_eq!(
        std::fs::read_to_string(root.join("target/cache.txt")).unwrap(),
        "cache\n"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("node_modules/cache.txt")).unwrap(),
        "dependency\n"
    );
    assert_eq!(
        std::fs::read_to_string(root.join(".env")).unwrap(),
        "SYNTHETIC_FIXTURE=1\n"
    );
}

#[tokio::test]
async fn autocrlf_cannot_hide_a_byte_change_from_scope_or_size() {
    let dir = integrity_repo();
    let root = dir.path();
    crate::testing::run_git(root, &["config", "core.autocrlf", "true"]);
    std::fs::write(root.join("base.txt"), "base\r\n").unwrap();
    let site = Site {
        worktree: root,
        deadline: std::time::Instant::now() + std::time::Duration::from_secs(60),
        base: None,
    };
    for value in [
        json!({ "id": "scope", "kind": "file_scope", "proves": "scope", "params": { "only": ["other.txt"] } }),
        json!({ "id": "size", "kind": "diff_size", "proves": "size", "params": { "max_lines": 0 } }),
    ] {
        assert_eq!(
            run_check(&check(value), &site).await.outcome,
            CheckOutcome::Fail
        );
    }
}

#[tokio::test]
async fn info_attributes_cannot_normalize_a_byte_change_into_a_pass() {
    let dir = integrity_repo();
    let root = dir.path();
    let site = Site {
        worktree: root,
        deadline: std::time::Instant::now() + std::time::Duration::from_secs(60),
        base: None,
    };
    let scope = check(
        json!({ "id": "scope", "kind": "file_scope", "proves": "scope", "params": { "only": ["other.txt"] } }),
    );
    let size = check(
        json!({ "id": "size", "kind": "diff_size", "proves": "size", "params": { "max_lines": 0 } }),
    );
    for bytes in [None, Some("")] {
        if let Some(bytes) = bytes {
            std::fs::write(root.join(".git/info/attributes"), bytes).unwrap();
        }
        for c in [&scope, &size] {
            assert_eq!(run_check(c, &site).await.outcome, CheckOutcome::Pass);
        }
    }
    std::fs::write(root.join(".git/info/attributes"), "base.txt text eol=lf\n").unwrap();
    std::fs::write(root.join("base.txt"), b"base\r\n").unwrap();
    for c in [&scope, &size] {
        assert_eq!(run_check(c, &site).await.outcome, CheckOutcome::Error);
    }
    std::fs::remove_file(root.join(".git/info/attributes")).unwrap();
    std::fs::create_dir(root.join(".git/info/attributes")).unwrap();
    assert_eq!(run_check(&scope, &site).await.outcome, CheckOutcome::Error);
}

#[tokio::test]
async fn byte_fix_guard_handles_windows_names_and_never_passes_on_read_errors() {
    use crate::crew::checks::fix_touched;
    use sha2::{Digest, Sha256};
    let dir = crate::testing::TempDir::new();
    let site = Site {
        worktree: dir.path(),
        deadline: std::time::Instant::now() + std::time::Duration::from_secs(60),
        base: None,
    };
    let path = "file with spaces @plus+.txt";
    let digest = crate::util::hex(&Sha256::digest(b"one\n"));
    std::fs::write(dir.path().join(path), b"one\n").unwrap();
    assert_eq!(
        fix_touched(path, &digest, &site).outcome,
        CheckOutcome::Fail
    );
    std::fs::write(dir.path().join(path), b"one\r\n").unwrap();
    assert_eq!(
        fix_touched(path, &digest, &site).outcome,
        CheckOutcome::Pass
    );
    assert_eq!(fix_touched(path, "", &site).outcome, CheckOutcome::Error);
    // The existing wire path grammar excludes Unicode and shell metacharacters.
    // The byte guard must refuse those, rather than widen that contract.
    assert_eq!(
        fix_touched("unicode é.txt", &digest, &site).outcome,
        CheckOutcome::Error
    );
    assert_eq!(
        fix_touched("shell & name.txt", &digest, &site).outcome,
        CheckOutcome::Error
    );
    std::fs::remove_file(dir.path().join(path)).unwrap();
    assert_eq!(
        fix_touched(path, &digest, &site).outcome,
        CheckOutcome::Error
    );
    std::fs::create_dir(dir.path().join(path)).unwrap();
    assert_eq!(
        fix_touched(path, &digest, &site).outcome,
        CheckOutcome::Error
    );
}

#[test]
fn the_internal_fix_digest_is_withheld_from_worker_reports() {
    let digest = "d".repeat(64);
    let c = check(
        json!({ "id": "chk-fix-touched", "kind": "repo_required", "proves": "p",
        "params": { "name": "crew-fix-touched", "note": digest } }),
    );
    let needles = crate::crew::guard::check_needles(&[c]);
    assert!(crate::crew::guard::refuse_leak(&json!({ "summary": digest }), &needles).is_err());
    assert!(!withhold(&digest, &needles).contains(&digest));
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
