//! A person's re-run of a task's checks through a real `Core`, a fake worker and real git checkouts: what it adds, what it never
//! changes, the order of the audit rows, the one-at-a-time claim, the refusals before a command runs, the command screen and what
//! the answer never holds. Nonces and secrets are not needed here.

use std::sync::Arc;
use std::time::Duration;

use orbyn_contracts::{ActionKind, CrewRerunGate};
use serde_json::{Value, json};

use super::autopilot_support::{request, words};
use super::harness::{KEY_MARKER, Turn, World, brief, key, report, world};
use crate::crew::fixtures::audit_actions;
use crate::crew::rerun::fixtures::task_in;
use crate::error::CoreError;

const INTENT: &str = "crew.task.rerun_checks";
const DONE: &str = "crew.task.rerun_checks_done";

fn rerun_audit(w: &World) -> Vec<String> {
    audit_actions(&w.core, "crew.task.rerun")
}

fn key_with(commands: &[&str]) -> Value {
    let mut checks = vec![json!({ "id": "chk-scope", "kind": "file_scope",
        "params": { "only": ["src/**"] }, "proves": "Only leased paths changed." })];
    for (n, run) in commands.iter().enumerate() {
        checks.push(json!({ "id": format!("chk-run-{n}"), "kind": "command",
            "params": { "run": run }, "proves": "The command exits 0." }));
    }
    json!({ "checks": checks, "protected_paths": ["tests/**"] })
}

/// A task that passed with `key`, its worker's turn over.
async fn passed_with(key: Value, script: super::harness::Script) -> (World, String) {
    let w = world(true, script).await;
    let (answer, error) = w.start(brief(json!({})), key).await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    w.until_state(&id, "passed").await;
    let worker = w.worker(&id);
    w.until("the worker is idle", |w| {
        w.core
            .store
            .thread(&worker)
            .is_ok_and(|t| t.active_run.is_none())
    })
    .await;
    (w, id)
}

fn good_script() -> super::harness::Script {
    Arc::new(|_, root| {
        std::fs::write(root.join("src/lib.txt"), "fn parse_duration() {}\n").unwrap();
        std::fs::write(root.join("src/go"), "go\n").unwrap();
        Turn::Report(report("done"))
    })
}

fn code(error: &CoreError) -> (&'static str, u16) {
    (error.code(), error.status())
}

/// A repository with one commit, as a task's checkout.
fn checkout(w: &World, name: &str) -> std::path::PathBuf {
    let dir = w._dir.path().join(name);
    std::fs::create_dir_all(&dir).unwrap();
    crate::testing::run_git(&dir, &["init", "-q"]);
    std::fs::write(dir.join("a.txt"), "a\n").unwrap();
    crate::testing::run_git(&dir, &["add", "."]);
    crate::testing::run_git(
        &dir,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.test",
            "commit",
            "-q",
            "-m",
            "base",
        ],
    );
    dir
}

#[tokio::test]
async fn a_passed_task_is_checked_again_and_its_state_is_left_alone() {
    let (w, id) = passed_with(key(), good_script()).await;
    let before = (
        w.core.store.crew_evidence(&id).unwrap().len(),
        w.core.store.crew_gates(&id).unwrap().len(),
    );
    let done = w.core.crew_rerun_checks(&id, None, "user").await.unwrap();

    assert_eq!(
        (done.state.as_str(), done.task.as_str()),
        ("passed", id.as_str())
    );
    // scope and the command ran, the probe is a kind this version does not run.
    assert_eq!(
        (
            done.total,
            done.ran,
            done.passed,
            done.refused,
            done.skipped
        ),
        (3, 2, 2, 0, 1)
    );
    assert_eq!(done.gate, CrewRerunGate::Pass);
    assert_eq!(done.refused_checks, Some(Vec::new()));
    assert_eq!(w.task(&id).state, "passed", "a re-run changes no state");
    // Review counts every Evidence of a task as its own verification and reads the commit of the last: a re-run stores none.
    assert_eq!(w.core.store.crew_evidence(&id).unwrap().len(), before.0);
    assert_eq!(w.core.store.crew_gates(&id).unwrap().len(), before.1);
    assert_eq!(rerun_audit(&w), [INTENT, DONE]);
    assert!(
        w.core
            .store
            .crew_run(&id)
            .unwrap()
            .unwrap()
            .verifying_since
            .is_none(),
        "the claim is let go"
    );
}

#[tokio::test]
async fn a_task_stopped_for_a_benign_reason_after_a_live_verification_may_be_checked_again() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let dir = checkout(&w, "stopped");
    let id = task_in(&w.core, &dir, &["exit 3"], "stopped").await;
    let done = w.core.crew_rerun_checks(&id, None, "user").await.unwrap();
    assert_eq!(done.state, "stopped");
    assert_eq!(
        (done.ran, done.passed, done.gate),
        (2, 1, CrewRerunGate::Fail)
    );
    assert_eq!(w.task(&id).state, "stopped");
    assert_eq!(w.task(&id).stop_reason.as_deref(), Some("repeated_failure"));
    for reason in ["budget", "engine_error"] {
        let id = task_in(&w.core, &dir, &["exit 0"], &format!("stopped:{reason}")).await;
        assert_eq!(
            again(&w, &id).await.unwrap().gate,
            CrewRerunGate::Pass,
            "{reason}"
        );
    }
}

/// A re-run with no gap, for tests that run one task twice.
async fn again(w: &World, id: &str) -> Result<orbyn_contracts::CrewTaskRerunDone, CoreError> {
    w.core
        .rerun_within(id, None, "user", crate::crew::rerun::RERUN_MAX, 0)
        .await
}

/// A task Autopilot stopped for hostile behaviour never had the key run in its checkout, and the worker may have planted what the key's
/// commands would run. The table admits six reasons, so a reason this code cannot name cannot be stored today; the allow-list reads one the same way if a seventh is added. A task never verified live is refused too.
#[tokio::test]
async fn a_task_stopped_for_a_safety_reason_or_never_verified_is_refused_and_nothing_runs() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let dir = checkout(&w, "hostile");
    for state in [
        "stopped:injection_suspected",
        "stopped:too_many_denials",
        "stopped:lead_stopped",
        "stopped:never",
    ] {
        let id = task_in(&w.core, &dir, &["touch ran"], state).await;
        let error = w
            .core
            .crew_rerun_checks(&id, None, "user")
            .await
            .unwrap_err();
        assert_eq!(code(&error), ("task_busy", 409), "{state}: {error}");
        assert!(!dir.join("ran").exists(), "{state}: a command ran");
    }
    assert!(rerun_audit(&w).is_empty(), "a refused request wrote a row");
}

/// One re-run at a time on the device, not per task, and a gap between two of one task.
#[tokio::test]
async fn only_one_rerun_runs_on_the_device_and_a_task_waits_between_two() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let dir = checkout(&w, "device");
    let wait =
        "touch started; i=0; while [ ! -f go ] && [ $i -lt 400 ]; do sleep 0.05; i=$((i+1)); done";
    let slow = task_in(&w.core, &dir, &[wait], "passed").await;
    let other = task_in(&w.core, &dir, &["exit 0"], "passed").await;
    let core = w.core.clone();
    let first = {
        let slow = slow.clone();
        tokio::spawn(async move { core.crew_rerun_checks(&slow, None, "user").await })
    };
    w.until("the first command started", |_| {
        dir.join("started").exists()
    })
    .await;
    let error = w
        .core
        .crew_rerun_checks(&other, None, "user")
        .await
        .unwrap_err();
    assert_eq!(code(&error), ("task_busy", 409), "{error}");
    assert!(error.to_string().contains("Another task"));
    assert_eq!(rerun_audit(&w), [INTENT]);
    std::fs::write(dir.join("go"), "go").unwrap();
    first.await.unwrap().unwrap();

    // The device is free again, so the other task runs; the first waits for its gap.
    w.core
        .crew_rerun_checks(&other, None, "user")
        .await
        .unwrap();
    let error = w
        .core
        .crew_rerun_checks(&other, None, "user")
        .await
        .unwrap_err();
    assert_eq!(code(&error), ("task_busy", 409), "{error}");
    assert!(error.to_string().contains("Try again in"), "{error}");
    assert_eq!(rerun_audit(&w).len(), 4, "a refused request wrote a row");
    again(&w, &other).await.unwrap();
}

/// A panic in the run must not leave the claim set or the outcome row unwritten.
#[tokio::test]
async fn a_panic_in_the_run_lets_the_claim_go_and_writes_the_failed_row() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let dir = checkout(&w, "panic");
    let id = task_in(&w.core, &dir, &["exit 0"], "passed").await;
    crate::crew::rerun::tests_panic::arm(&id);
    let error = w
        .core
        .crew_rerun_checks(&id, None, "user")
        .await
        .unwrap_err();
    assert_eq!(error.status(), 500, "{error}");
    assert_eq!(rerun_audit(&w), [INTENT, "crew.task.rerun_checks_failed"]);
    assert!(
        w.core
            .store
            .crew_run(&id)
            .unwrap()
            .unwrap()
            .verifying_since
            .is_none(),
        "the claim is let go"
    );
    assert_eq!(again(&w, &id).await.unwrap().gate, CrewRerunGate::Pass);
}

/// The intent row exists before the first command runs, and a second request while one runs is refused.
#[tokio::test]
async fn the_intent_is_written_before_the_first_command_and_a_second_request_is_busy() {
    let wait = "touch src/started; i=0; while [ ! -f src/go ] && [ $i -lt 400 ]; do sleep 0.05; i=$((i+1)); done";
    let (w, id) = passed_with(key_with(&[wait]), good_script()).await;
    let worktree = std::path::PathBuf::from(
        w.core
            .store
            .crew_run(&id)
            .unwrap()
            .unwrap()
            .worktree
            .unwrap(),
    );
    std::fs::remove_file(worktree.join("src/go")).unwrap();
    std::fs::remove_file(worktree.join("src/started")).unwrap();
    let before = rerun_audit(&w);
    assert!(before.is_empty());

    let core = w.core.clone();
    let task = id.clone();
    let first = tokio::spawn(async move { core.crew_rerun_checks(&task, None, "user").await });
    w.until("the first command started", |_| {
        worktree.join("src/started").exists()
    })
    .await;

    // The command is running now: the intent row is there, the outcome is not, and the claim is held.
    assert_eq!(rerun_audit(&w), [INTENT]);
    assert!(
        w.core
            .store
            .crew_run(&id)
            .unwrap()
            .unwrap()
            .verifying_since
            .is_some()
    );
    // A second request is busy, writes nothing and runs nothing.
    let second = w
        .core
        .crew_rerun_checks(&id, None, "user")
        .await
        .unwrap_err();
    assert_eq!(code(&second), ("task_busy", 409), "{second}");
    assert_eq!(rerun_audit(&w), [INTENT]);

    std::fs::write(worktree.join("src/go"), "go").unwrap();
    let done = first.await.unwrap().unwrap();
    assert_eq!(done.gate, CrewRerunGate::Pass);
    assert_eq!(rerun_audit(&w), [INTENT, DONE]);
    // The claim is let go; the gap between two re-runs of one task still holds.
    let soon = w
        .core
        .crew_rerun_checks(&id, None, "user")
        .await
        .unwrap_err();
    assert_eq!(code(&soon), ("task_busy", 409), "{soon}");
    std::fs::remove_file(worktree.join("src/started")).unwrap();
    again(&w, &id).await.unwrap();
    assert_eq!(rerun_audit(&w), [INTENT, DONE, INTENT, DONE]);
}

#[tokio::test]
async fn a_task_that_is_not_over_is_refused_and_nothing_runs() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let dir = checkout(&w, "states");
    let cases = [
        ("briefed", "is briefed"),
        ("running", "worker is still running"),
        ("verifying", "being run now"),
        ("needs_person", "is needs_person"),
        ("cancelled", "was cancelled"),
    ];
    for (state, words) in cases {
        let id = task_in(&w.core, &dir, &["touch ran"], state).await;
        let error = w
            .core
            .crew_rerun_checks(&id, None, "user")
            .await
            .unwrap_err();
        assert_eq!(code(&error), ("task_busy", 409), "{state}: {error}");
        assert!(error.to_string().contains(words), "{state}: {error}");
        assert!(!dir.join("ran").exists(), "{state}: a command ran");
        assert!(rerun_audit(&w).is_empty(), "{state}: a row was written");
    }
}

/// A live verification holds the claim: nothing runs beside it. So does another re-run.
#[tokio::test]
async fn a_task_with_a_claim_is_refused() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let dir = checkout(&w, "claimed");
    let id = task_in(&w.core, &dir, &["exit 0"], "verifying").await;
    assert!(w.core.store.claim_crew_verification(&id).unwrap());
    let error = w
        .core
        .crew_rerun_checks(&id, None, "user")
        .await
        .unwrap_err();
    assert_eq!(code(&error), ("task_busy", 409));
    let ended = task_in(&w.core, &dir, &["exit 0"], "stopped").await;
    w.core
        .store
        .begin_crew_rerun(&ended, "user", None, 0)
        .unwrap();
    let error = w
        .core
        .crew_rerun_checks(&ended, None, "user")
        .await
        .unwrap_err();
    assert_eq!(code(&error), ("task_busy", 409));
}

/// A task can end (a limit stop, say) while its worker's engine run is still closing; the worker may still be writing the checkout.
#[tokio::test]
async fn a_task_whose_worker_is_still_running_is_refused() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (answer, _) = w.start(brief(json!({})), key()).await;
    let id = answer["task"].as_str().unwrap().to_owned();
    w.core
        .store
        .move_crew_task(&id, "stopped", Some("budget"))
        .unwrap();
    let worker = w.worker(&id);
    assert!(w.core.store.thread(&worker).unwrap().active_run.is_some());
    let error = w
        .core
        .crew_rerun_checks(&id, None, "user")
        .await
        .unwrap_err();
    assert_eq!(code(&error), ("task_busy", 409), "{error}");
    assert!(error.to_string().contains("worker is still running"));
    assert!(rerun_audit(&w).is_empty());
    assert!(w.core.store.crew_evidence(&id).unwrap().is_empty());
}

#[tokio::test]
async fn an_unknown_task_a_bad_id_and_a_bad_reason_are_refused_before_anything_is_written() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let missing = w
        .core
        .crew_rerun_checks("sha256:nope", None, "user")
        .await
        .unwrap_err();
    assert_eq!(code(&missing), ("not_found", 404));
    let bad = w
        .core
        .crew_rerun_checks("../x", None, "user")
        .await
        .unwrap_err();
    assert_eq!(bad.status(), 400);
    let dir = checkout(&w, "plain");
    let id = task_in(&w.core, &dir, &["exit 0"], "stopped").await;
    let long = "x".repeat(201);
    let error = w
        .core
        .crew_rerun_checks(&id, Some(&long), "user")
        .await
        .unwrap_err();
    assert_eq!(error.status(), 400);
    assert!(rerun_audit(&w).is_empty());
    assert!(w.core.store.crew_gates(&id).unwrap().is_empty());
}

#[tokio::test]
async fn a_task_without_a_checkout_or_without_a_key_is_a_plain_error() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let id = crate::crew::rerun::fixtures::task_without_checkout(&w.core);
    let error = w
        .core
        .crew_rerun_checks(&id, None, "user")
        .await
        .unwrap_err();
    assert_eq!(code(&error), ("unsupported", 422), "{error}");
    assert!(error.to_string().contains("no checkout"));
    assert!(rerun_audit(&w).is_empty());

    // No key: rows cannot be taken away through the store, so one is taken away behind its back.
    let dir = checkout(&w, "keyless");
    let keyless = task_in(&w.core, &dir, &["exit 0"], "stopped").await;
    let removed = w.core.store.reader().execute(
        "DELETE FROM crew_answer_keys WHERE task_id = ?1",
        [&keyless],
    );
    assert_eq!(removed.unwrap(), 1, "the key was taken away");
    {
        let error = w
            .core
            .crew_rerun_checks(&keyless, None, "user")
            .await
            .unwrap_err();
        assert_eq!(code(&error), ("unsupported", 422), "{error}");
        assert!(error.to_string().contains("no key"));
        assert!(rerun_audit(&w).is_empty());
    }
}

/// The reason is redacted before it is kept, and a crash between the rows leaves the intent visible.
#[tokio::test]
async fn the_reason_is_redacted_and_a_crash_leaves_the_intent_row() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let dir = checkout(&w, "reason");
    let id = task_in(&w.core, &dir, &["exit 0"], "stopped").await;
    let token = format!("ghp_{}", "q".repeat(36));
    w.core
        .crew_rerun_checks(&id, Some(&format!("looking at {token}")), "user")
        .await
        .unwrap();
    let rows = w.core.store.list_audit(None, 500).unwrap();
    let intent = rows.iter().find(|r| r.action == INTENT).unwrap();
    let text = intent.detail.to_string();
    assert!(
        text.contains("[redacted]") && !text.contains(&token),
        "{text}"
    );
    assert_eq!(
        (intent.actor.as_str(), intent.target.as_str()),
        ("user", id.as_str())
    );
    assert_eq!(intent.detail["state"], "stopped");
    let outcome = rows.iter().find(|r| r.action == DONE).unwrap();
    assert!(
        !outcome.detail.to_string().contains("exit 0"),
        "no command in the outcome row"
    );

    // A crash after the intent: the row stays, the claim holds the task, and a start of the service lets it go.
    w.core.store.begin_crew_rerun(&id, "user", None, 0).unwrap();
    assert_eq!(rerun_audit(&w), [INTENT, DONE, INTENT]);
    let error = w
        .core
        .crew_rerun_checks(&id, None, "user")
        .await
        .unwrap_err();
    assert_eq!(code(&error), ("task_busy", 409));
    w.core.store.clear_crew_claims().unwrap();
    again(&w, &id).await.unwrap();
}

/// The core's own key commands got no screen before: a re-run gives each the worker's reading, and does not run what fails it.
#[tokio::test]
async fn a_command_the_screen_refuses_is_not_run_and_the_others_are() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let dir = checkout(&w, "screen");
    let outside = w._dir.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("keep"), "keep").unwrap();
    let drop = "psql -h db.prod.internal -c 'DROP TABLE users' && touch ran-bad";
    let rm = format!("rm -rf {}", outside.display());
    let open = "touch ran-bad2 && echo 'oops";
    let id = task_in(&w.core, &dir, &["touch ran-ok", drop, &rm, open], "stopped").await;
    let done = w.core.crew_rerun_checks(&id, None, "user").await.unwrap();

    assert!(dir.join("ran-ok").exists(), "the clean command ran");
    assert!(!dir.join("ran-bad").exists(), "the guarded command ran");
    assert!(!dir.join("ran-bad2").exists(), "the unreadable command ran");
    assert!(
        outside.join("keep").exists(),
        "the command outside the checkout ran"
    );
    assert_eq!(
        (done.total, done.ran, done.passed, done.refused),
        (5, 2, 2, 3)
    );
    assert_eq!(
        done.gate,
        CrewRerunGate::Fail,
        "a refused check keeps the Gate from passing"
    );
    // The ids of the refused checks, and nothing else about them: not the rule, not the command.
    assert_eq!(
        done.refused_checks.as_deref(),
        Some(&["chk-2".to_owned(), "chk-3".to_owned(), "chk-4".to_owned()][..])
    );
    assert_eq!(
        w.core.store.crew_evidence(&id).unwrap().len(),
        1,
        "a re-run stores no Evidence"
    );
    assert_eq!(w.core.store.crew_gates(&id).unwrap().len(), 0, "or Gate");
    let rows = w.core.store.list_audit(None, 500).unwrap();
    let kept = serde_json::to_string(&rows).unwrap();
    for text in ["DROP", "psql", "rm -rf", "oops"] {
        assert!(!kept.contains(text), "{text} was kept");
    }
}

/// The answer has counts, the ids of the refused checks, and nothing else of the key (H1).
#[tokio::test]
async fn the_answer_has_counts_and_refused_ids_and_none_of_the_key() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let dir = checkout(&w, "leak");
    let id = task_in(
        &w.core,
        &dir,
        &[
            "echo orbyn-key-marker-77 && touch ran-ok",
            "psql -h db.prod.internal -c 'DROP TABLE users'",
        ],
        "stopped",
    )
    .await;
    let done = w.core.crew_rerun_checks(&id, None, "user").await.unwrap();
    assert_eq!(done.state, "stopped");
    assert_eq!((done.ran, done.passed, done.refused), (2, 2, 1));
    assert_eq!(
        done.refused_checks.as_deref(),
        Some(&["chk-2".to_owned()][..])
    );
    let text = serde_json::to_string(&done).unwrap();
    for text_of_key in [
        "chk-1",
        "psql",
        "DROP",
        "touch",
        KEY_MARKER,
        "proves",
        "Only leased",
        "exits 0",
        "blast",
        "N4",
        "evidence",
    ] {
        assert!(!text.contains(text_of_key), "{text_of_key}: {text}");
    }
}

#[tokio::test]
async fn the_whole_run_has_an_upper_bound() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let dir = checkout(&w, "slow");
    let id = task_in(&w.core, &dir, &["sleep 30"], "stopped").await;
    let started = std::time::Instant::now();
    let done = w
        .core
        .rerun_within(&id, None, "user", Duration::from_secs(1), 0)
        .await
        .unwrap();
    assert!(started.elapsed() < Duration::from_secs(15));
    assert_eq!(
        (done.ran, done.passed, done.gate),
        (2, 1, CrewRerunGate::Fail)
    );
    assert_eq!(crate::crew::rerun::RERUN_MAX, Duration::from_secs(1800));
}

/// An agent reaches the service as a command, and the always-denied list refuses a request to the route (N1).
#[tokio::test]
async fn an_agents_request_to_the_route_is_refused_by_n1() {
    let (w, id) = super::autopilot_support::running().await;
    for target in [
        format!("curl -X POST http://127.0.0.1:9/api/crew/tasks/{id}/rerun-checks"),
        "curl -X POST http://127.0.0.1:9/api/%63rew/tasks/x/RERUN-checks/ -d '{}'".to_owned(),
    ] {
        let verdict = request(&w, &id, ActionKind::Command, &target).await;
        let said = words(&verdict);
        assert!(said.contains("N1"), "{target}: {said}");
    }
    assert!(crate::crew::changes::changes_crew_state(
        "/api/crew/tasks/t-1/rerun-checks"
    ));
    assert!(
        crate::crew::changes::STATE_CHANGING
            .contains(&("POST", "/api/crew/tasks/{id}/rerun-checks"))
    );
}
