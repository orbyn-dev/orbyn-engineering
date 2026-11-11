//! A task through a real `Core` with a fake worker: verification, retries with the reason, the
//! loop guard and the end of the run. The worker's engine is a closure; the checks run for real
//! in the project's checkout.

use std::sync::Arc;

use orbyn_contracts::{CheckOutcome, ResultState, StopReason};
use serde_json::json;

use super::harness::{KEY_MARKER, Turn, World, brief, key, report, world};

fn bad(root: &std::path::Path) {
    std::fs::create_dir_all(root.join("docs")).unwrap();
    std::fs::write(root.join("docs/bad.txt"), "outside\n").unwrap();
}

fn good(root: &std::path::Path) {
    std::fs::write(root.join("src/lib.txt"), "fn parse_duration() {}\n").unwrap();
}

async fn started(w: &World) -> String {
    let (answer, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "{answer}");
    assert_eq!(answer["state"], "running", "{answer}");
    answer["task"].as_str().unwrap().to_owned()
}

/// Slice 3's "done when": a task fails verification, retries with the reason, hits the same
/// failure twice, changes approach once, and stops on the next repeat. The key never reaches
/// the worker.
#[tokio::test]
async fn the_same_failure_changes_approach_once_and_then_stops() {
    // The first turn leaves a file outside the lease; later turns change nothing, so the
    // check fails the same way each time.
    let w = world(
        true,
        Arc::new(|n, root| {
            if n == 0 {
                bad(root);
            }
            Turn::Report(report("done"))
        }),
    )
    .await;
    let id = started(&w).await;
    w.until_state(&id, "stopped").await;

    let task = w.task(&id);
    assert_eq!(task.stop_reason.as_deref(), Some("repeated_failure"));
    let run = w.core.store.crew_run(&id).unwrap().unwrap();
    assert_eq!((run.attempts, run.failures), (3, 3));
    assert!(run.guard.approach_changed);
    assert_eq!(run.guard.streak, 3);

    // Two interim `failed` results, then the final `stopped` one.
    let results = w.core.store.crew_results(&id).unwrap();
    let states: Vec<(ResultState, bool)> = results.iter().map(|r| (r.state, r.r#final)).collect();
    assert_eq!(
        states,
        [
            (ResultState::Failed, false),
            (ResultState::Failed, false),
            (ResultState::Stopped, true)
        ]
    );
    let last = results.last().unwrap();
    assert_eq!(last.stop_reason, Some(StopReason::RepeatedFailure));
    assert_eq!(last.attempts, 3);
    assert!(last.route.is_some());

    // The worker was told the reason, then to change approach, and nothing of the key.
    let worker = w.worker(&id);
    let prompts = w.sent_to(&worker);
    assert_eq!(prompts.len(), 3, "the brief, a retry and a new approach");
    assert!(prompts[0].contains("Add a parse_duration function."));
    assert!(prompts[1].contains("docs/bad.txt") && prompts[1].contains("Fix this"));
    assert!(prompts[2].contains("different approach"));
    let key_id = w.core.store.crew_answer_key(&id).unwrap().unwrap().id;
    for prompt in &prompts {
        assert!(!prompt.contains(KEY_MARKER), "a prompt held the check text");
        assert!(
            !prompt.contains(key_id.as_str()),
            "a prompt held the key id"
        );
        assert!(!prompt.contains("chk-"), "a prompt held a check id");
        assert!(!prompt.contains("protected_paths"), "a prompt held the key");
    }

    // The lead was told how it ended, and the worker's run is over: member tools refuse.
    w.until("the lead is told", |w| {
        w.sent_to(&w.lead)
            .iter()
            .any(|p| p.contains("<crew-result"))
    })
    .await;
    let told = w.sent_to(&w.lead).join("\n");
    assert!(told.contains("state=\"stopped\"") && told.contains("final=\"true\""));
    let token = super::harness::bind(&w.core, &worker, "run-late");
    let (refusal, error) = w.call(&token, "crew_report", report("again")).await;
    assert!(error);
    assert_eq!(refusal["code"], "run_ended");
    let (refusal, _) = w
        .call(
            &token,
            "crew_start",
            json!({ "brief": brief(json!({})), "answer_key": key() }),
        )
        .await;
    assert_eq!(refusal["code"], "run_ended");

    // The core may still verify again: it adds Evidence and a Gate, and never reopens the run.
    let before = w.core.store.crew_evidence(&id).unwrap().len();
    std::fs::remove_file(w.repo.join("docs/bad.txt")).unwrap();
    let run = w.core.store.crew_run(&id).unwrap().unwrap();
    let judged = w.core.crew_verify(&w.task(&id), &run).await.unwrap();
    assert!(judged.passed());
    assert!(w.core.store.crew_evidence(&id).unwrap().len() > before);
    assert_eq!(w.task(&id).state, "stopped");
    assert!(w.core.store.move_crew_task(&id, "running", None).is_err());
}

/// A different failure starts the count again, so a worker that makes progress is not stopped.
#[tokio::test]
async fn a_different_failure_starts_the_count_again() {
    let w = world(
        true,
        Arc::new(|n, root| {
            match n {
                0 => bad(root),
                1 => {
                    std::fs::remove_file(root.join("docs/bad.txt")).unwrap();
                    std::fs::write(root.join("docs/other.txt"), "x\n").unwrap();
                }
                _ => {
                    std::fs::remove_dir_all(root.join("docs")).unwrap();
                    good(root);
                }
            }
            Turn::Report(report("done"))
        }),
    )
    .await;
    let id = started(&w).await;
    w.until_state(&id, "passed").await;
    let run = w.core.store.crew_run(&id).unwrap().unwrap();
    assert_eq!((run.attempts, run.failures), (3, 2));
    assert!(
        !run.guard.approach_changed,
        "no change of approach was used"
    );
    assert_eq!(run.guard.streak, 1);
}

/// A pass needs Evidence, lists what no check covered, releases the leases and tells the lead.
#[tokio::test]
async fn a_pass_carries_its_evidence_and_what_was_not_verified() {
    let w = world(
        true,
        Arc::new(|_, root| {
            good(root);
            Turn::Report(json!({ "summary": "added", "not_verified": ["Windows paths"] }))
        }),
    )
    .await;
    let id = started(&w).await;
    w.until_state(&id, "passed").await;

    let evidence = w.core.store.crew_evidence(&id).unwrap();
    assert_eq!(evidence.len(), 3);
    let outcomes: Vec<CheckOutcome> = evidence.iter().map(|e| e.outcome).collect();
    assert_eq!(
        outcomes,
        [
            CheckOutcome::Pass,
            CheckOutcome::Pass,
            CheckOutcome::Skipped
        ]
    );
    assert_eq!(w.core.store.crew_gates(&id).unwrap().len(), 1);
    // The checkout had uncommitted work when the checks ran, and the Evidence says so.
    assert!(
        evidence
            .iter()
            .all(|e| e.does_not_prove.as_str().contains("uncommitted"))
    );
    let result = w.core.store.crew_results(&id).unwrap().pop().unwrap();
    assert_eq!(result.state, ResultState::Passed);
    assert!(result.r#final && result.stop_reason.is_none());
    assert_eq!(result.checks.len(), 3);
    let missing: Vec<&str> = result.not_verified.iter().map(|t| t.as_str()).collect();
    assert!(
        missing.iter().any(|t| t.contains("http_probe")),
        "{missing:?}"
    );
    assert!(missing.contains(&"Windows paths"));
    assert!(result.report.is_some() && result.commit.is_some());

    // The worker's lease is released with the run, so it blocks no one.
    let crew = w.core.store.crew_task(&id).unwrap().unwrap().crew_id;
    let live = w
        .core
        .live_crew_leases(&crew, crate::util::unix_now())
        .unwrap();
    assert!(live.is_empty(), "{live:?}");
}

/// A turn that ends without `crew_report` is a failed attempt the guard counts.
#[tokio::test]
async fn a_turn_without_a_report_is_a_failed_attempt() {
    let w = world(
        true,
        Arc::new(|n, root| {
            if n == 0 {
                Turn::Silent
            } else {
                good(root);
                Turn::Report(report("done"))
            }
        }),
    )
    .await;
    let id = started(&w).await;
    w.until_state(&id, "passed").await;
    let prompts = w.sent_to(&w.worker(&id));
    assert!(prompts[1].contains("without calling crew_report"));
    assert_eq!(w.core.store.crew_run(&id).unwrap().unwrap().attempts, 2);
}

#[tokio::test]
async fn an_engine_that_fails_ends_the_task_with_engine_error() {
    let w = world(true, Arc::new(|_, _| Turn::Crash)).await;
    let id = started(&w).await;
    w.until_state(&id, "stopped").await;
    assert_eq!(w.task(&id).stop_reason.as_deref(), Some("engine_error"));
    let result = w.core.store.crew_results(&id).unwrap().pop().unwrap();
    assert_eq!(result.stop_reason, Some(StopReason::EngineError));
}

/// A worker's draft cannot name a field the core stamps (H9), and a secret in a report is
/// refused (H2).
#[tokio::test]
async fn a_report_is_stamped_by_the_core() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let id = started(&w).await;
    let token = super::harness::bind(&w.core, &w.worker(&id), "run-w");
    for field in [
        "id",
        "author",
        "run_id",
        "brief",
        "crew_id",
        "created_at",
        "v",
        "type",
    ] {
        let mut draft = report("done");
        draft[field] = json!("x");
        let (refusal, error) = w.call(&token, "crew_report", draft).await;
        assert!(error);
        assert_eq!(refusal["code"], "invalid_object", "{field}");
    }
    let leaked = format!("sk-{}", "a".repeat(24));
    let (refusal, _) = w.call(&token, "crew_report", report(&leaked)).await;
    assert_eq!(refusal["code"], "secret_detected");
    // Nothing was stored, and the lead cannot report as a worker.
    assert!(w.core.store.crew_reports(&id).unwrap().is_empty());
    assert_eq!(w.task(&id).state, "running");
    let (refusal, _) = w.call(&w.lead_token, "crew_report", report("done")).await;
    assert_eq!(refusal["code"], "not_authorized");
}
