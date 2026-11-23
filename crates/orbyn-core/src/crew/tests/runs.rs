//! A task through a real `Core` with a fake worker: verification, retries with the reason, the
//! loop guard and the end of the run. The worker's engine is a closure; the checks run for real
//! in the project's checkout.

use std::sync::Arc;

use orbyn_contracts::{CheckOutcome, ResultState, RunState, StopReason};
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

/// A crew task whose lead is `w.lead`: the lead thread is the person's own chat.
fn lead_of_a_task(w: &World, label: &str) -> String {
    use orbyn_contracts as wire;
    let stamper = crate::crew::Stamper {
        crew_id: wire::CrewId::new("crew-lead-reply").unwrap(),
        engine: wire::CrewToken::new("alpha").unwrap(),
        member: wire::CrewMember::new("lead-01").unwrap(),
        now: wire::CrewTimestamp::new("2026-11-12T10:00:00Z").unwrap(),
    };
    let brief = stamper
        .brief(&brief(
            json!({ "outcome": format!("The lead's own chat, {label}.") }),
        ))
        .unwrap();
    let key = stamper.answer_key(&key(), &brief.id).unwrap();
    let task = w
        .core
        .store
        .create_crew_task(&brief, &key, &format!("run-{label}"))
        .unwrap();
    w.core
        .store
        .add_crew_run(&task.id, &w.lead, "member-01")
        .unwrap();
    task.id
}

/// The lead's thread is also the person's own chat, so a turn the person started there that the
/// engine ends with nothing fails as "No reply" like any other answer owed to the person.
#[tokio::test]
async fn a_persons_turn_on_a_crew_lead_thread_with_no_reply_fails() {
    let w = world(true, Arc::new(|_, _| Turn::Silent)).await;
    let _task = lead_of_a_task(&w, "reply");
    let run = w
        .core
        .runs
        .admit(&w.lead, "what is the plan?", "user")
        .unwrap();
    w.until("the run ends", |w| {
        w.core.store.run(&run.id).unwrap().state.is_terminal()
    })
    .await;
    let ended = w.core.store.run(&run.id).unwrap();
    assert_eq!(ended.state, RunState::Failed, "{ended:?}");
    assert!(
        ended
            .error
            .as_deref()
            .is_some_and(|message| message.starts_with("No reply")),
        "{ended:?}"
    );
}

/// Crew's own note on the lead's thread is judged by what the lead leaves behind: the person did
/// not write it, so a silent answer is not a failure.
#[tokio::test]
async fn a_crew_note_left_for_the_lead_is_judged_by_its_work() {
    let w = world(true, Arc::new(|_, _| Turn::Silent)).await;
    let id = lead_of_a_task(&w, "note");
    w.core
        .crew_needs_person(&id, "A person should look at this.", true);
    w.until("the note is sent", |w| {
        w.sent_to(&w.lead)
            .iter()
            .any(|prompt| prompt.contains("<crew-result"))
    })
    .await;
    w.until("the note's run ends", |w| {
        let thread = w.core.store.thread(&w.lead).unwrap();
        thread.active_run.is_none() && thread.last_run_state.is_some()
    })
    .await;
    let thread = w.core.store.thread(&w.lead).unwrap();
    assert_eq!(thread.last_run_state, Some(RunState::Completed));
}

/// A person's message waits in the same queue when the lead is busy; it still owes an answer.
#[tokio::test]
async fn a_persons_queued_message_on_a_crew_lead_thread_still_needs_a_reply() {
    let w = world(true, Arc::new(|_, _| Turn::Silent)).await;
    let _task = lead_of_a_task(&w, "queued");
    w.core.runs.enqueue(&w.lead, "hello again", &[]).unwrap();
    w.until("the run ends", |w| {
        let thread = w.core.store.thread(&w.lead).unwrap();
        thread.active_run.is_none() && thread.last_run_state.is_some()
    })
    .await;
    let thread = w.core.store.thread(&w.lead).unwrap();
    assert_eq!(thread.last_run_state, Some(RunState::Failed));
}

/// The notices Crew queues for the lead on its own (a refused start, a review) are Crew's notes too.
#[tokio::test]
async fn a_notice_the_core_queues_for_the_lead_is_a_crew_note() {
    let w = world(true, Arc::new(|_, _| Turn::Silent)).await;
    let _task = lead_of_a_task(&w, "notice");
    w.core
        .enqueue_for_lead(&w.lead, "Task t could not start: no engine.")
        .unwrap();
    w.until("the notice's run ends", |w| {
        let thread = w.core.store.thread(&w.lead).unwrap();
        thread.active_run.is_none() && thread.last_run_state.is_some()
    })
    .await;
    let thread = w.core.store.thread(&w.lead).unwrap();
    assert_eq!(thread.last_run_state, Some(RunState::Completed));
}

/// A delegated run is judged by the work it leaves behind, whatever Crew thread its parent is.
#[tokio::test]
async fn a_delegated_run_that_says_nothing_is_judged_by_its_work() {
    let w = world(true, Arc::new(|_, _| Turn::Silent)).await;
    let lead = w.core.store.thread(&w.lead).unwrap();
    let child = w
        .core
        .store
        .create_thread(crate::store::NewThread {
            project_id: lead.project_id.clone(),
            title: "delegated".into(),
            engine: lead.engine.clone(),
            model: None,
            cwd: lead.cwd.clone(),
            worktree: None,
            skill_profile: None,
            parent_thread_id: Some(lead.id.clone()),
            relation: Some("subagent".into()),
            scratch: false,
        })
        .unwrap();
    let run = w.core.runs.admit(&child.id, "do the work", "crew").unwrap();
    w.until("the run ends", |w| {
        w.core.store.run(&run.id).unwrap().state.is_terminal()
    })
    .await;
    assert_eq!(
        w.core.store.run(&run.id).unwrap().state,
        RunState::Completed
    );
}

/// A slash command or a goal may have nothing to say, even on the lead's own thread.
#[tokio::test]
async fn a_slash_command_or_goal_on_a_crew_lead_thread_may_say_nothing() {
    let w = world(true, Arc::new(|_, _| Turn::Silent)).await;
    let _task = lead_of_a_task(&w, "commands");
    for prompt in ["/compact", "/goal ship the release"] {
        let run = w.core.runs.admit(&w.lead, prompt, "user").unwrap();
        w.until("the run ends", |w| {
            w.core.store.run(&run.id).unwrap().state.is_terminal()
        })
        .await;
        assert_eq!(
            w.core.store.run(&run.id).unwrap().state,
            RunState::Completed,
            "{prompt}"
        );
    }
}

/// The brief's turns are a budget like its minutes: a failure with every turn spent stops the
/// task with the same `budget` reason and the same plain result, not the loop guard's stop.
#[tokio::test]
async fn a_task_stops_when_the_briefs_turns_are_spent() {
    // Every turn leaves the same file outside the lease, so the check fails the same way and
    // the loop guard would only act after the turns are gone.
    let w = world(
        true,
        Arc::new(|_, root| {
            bad(root);
            Turn::Report(report("done"))
        }),
    )
    .await;
    let (answer, error) = w
        .start(
            brief(json!({ "budget": { "turns": 2, "minutes": 30 } })),
            key(),
        )
        .await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    w.until_state(&id, "stopped").await;

    let task = w.task(&id);
    assert_eq!(
        (task.state.as_str(), task.stop_reason.as_deref()),
        ("stopped", Some("budget"))
    );
    let run = w.core.store.crew_run(&id).unwrap().unwrap();
    assert_eq!(
        (run.attempts, run.failures),
        (2, 1),
        "the second failure spent the last turn, before the loop guard counted a second failure"
    );
    let results = w.core.store.crew_results(&id).unwrap();
    let last = results.last().unwrap();
    assert_eq!(last.stop_reason, Some(StopReason::Budget));
    assert!(
        last.summary.contains("turn budget"),
        "the result says what ran out: {}",
        last.summary
    );
    assert_eq!(
        last.usage.turns,
        Some(2),
        "the result counts the turns spent"
    );
    assert_eq!(
        w.sent_to(&w.worker(&id)).len(),
        2,
        "the brief and one retry, and no new turn once the turns are spent"
    );
}

/// A turn that passes on the last allowed one is not stopped by the budget: the stop is for a
/// task with no turn left, not for one that finished inside it.
#[tokio::test]
async fn a_pass_on_the_last_turn_is_not_stopped_by_the_turn_budget() {
    let w = world(
        true,
        Arc::new(|_, root| {
            good(root);
            Turn::Report(report("done"))
        }),
    )
    .await;
    let (answer, error) = w
        .start(
            brief(json!({ "budget": { "turns": 1, "minutes": 30 } })),
            key(),
        )
        .await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    w.until_state(&id, "passed").await;
    assert_eq!(w.core.store.crew_run(&id).unwrap().unwrap().attempts, 1);
}

/// A key with one more check after the shared three, so a kind can be driven end to end.
fn key_with(check: serde_json::Value) -> serde_json::Value {
    let mut key = key();
    key["checks"].as_array_mut().unwrap().push(check);
    key
}

async fn started_with(w: &World, key: serde_json::Value) -> String {
    let (answer, error) = w.start(brief(json!({})), key).await;
    assert!(!error, "{answer}");
    assert_eq!(answer["state"], "running", "{answer}");
    answer["task"].as_str().unwrap().to_owned()
}

/// Issue #437: a `schema` check runs on the instance the worker wrote in its checkout and stores
/// its Evidence: the failing attempt retries with the reason, and the fixed one passes.
#[tokio::test]
async fn a_schema_check_runs_on_the_workers_instance_and_records_evidence() {
    let w = world(
        true,
        Arc::new(|n, root| {
            let instance = if n == 0 {
                r#"{"name":7}"#
            } else {
                r#"{"name":"orbyn"}"#
            };
            std::fs::write(root.join("src/instance.json"), instance).unwrap();
            Turn::Report(report("done"))
        }),
    )
    .await;
    std::fs::write(
        w.repo.join("schema.json"),
        br#"{"type":"object","required":["name"],"properties":{"name":{"type":"string"}}}"#,
    )
    .unwrap();
    let schema = json!({ "id": "chk-schema", "kind": "schema",
        "proves": "The instance matches its schema.",
        "params": { "schema": "schema.json", "instance": "src/instance.json" } });
    let id = started_with(&w, json!({ "checks": [schema], "protected_paths": [] })).await;
    w.until_state(&id, "passed").await;
    let evidence = w.core.store.crew_evidence(&id).unwrap();
    let schema: Vec<_> = evidence
        .iter()
        .filter(|e| e.check.as_str() == "chk-schema")
        .collect();
    assert_eq!(schema.len(), 2, "{evidence:?}");
    assert_eq!(
        (schema[0].outcome, schema[1].outcome),
        (CheckOutcome::Fail, CheckOutcome::Pass)
    );
    assert_eq!(schema[0].command.as_str(), "schema: chk-schema");
    // The failing Evidence names the place and never the value the worker wrote.
    assert!(schema[0].excerpt.contains("/name"), "{}", schema[0].excerpt);
    assert!(!schema[0].excerpt.contains('7'), "{}", schema[0].excerpt);
}

/// Issue #437: a `secret_scan` fails a changed file that holds a recognised secret, the worker
/// retries without it, and the secret is never stored or handed to the worker.
#[tokio::test]
async fn a_secret_scan_fails_the_change_and_passes_after_the_retry() {
    let secret = format!("ghp_{}", "x".repeat(36));
    let leak = secret.clone();
    let w = world(
        true,
        Arc::new(move |n, root| {
            let body = if n == 0 {
                format!("token = {leak}\n")
            } else {
                "clean\n".to_owned()
            };
            std::fs::write(root.join("src/lib.txt"), body).unwrap();
            Turn::Report(report("done"))
        }),
    )
    .await;
    let scan = json!({ "id": "chk-secret", "kind": "secret_scan",
        "proves": "The change holds no recognised secret.", "params": {} });
    let id = started_with(&w, key_with(scan)).await;
    w.until_state(&id, "passed").await;
    let evidence = w.core.store.crew_evidence(&id).unwrap();
    let scans: Vec<_> = evidence
        .iter()
        .filter(|e| e.check.as_str() == "chk-secret")
        .collect();
    assert_eq!(scans.len(), 2, "{evidence:?}");
    assert_eq!(
        (scans[0].outcome, scans[1].outcome),
        (CheckOutcome::Fail, CheckOutcome::Pass)
    );
    assert_eq!(scans[0].command.as_str(), "secret_scan: chk-secret");
    for stored in &scans {
        assert!(!stored.excerpt.contains(&secret), "the secret was stored");
    }
    let prompts = w.sent_to(&w.worker(&id));
    assert!(
        prompts.iter().all(|prompt| !prompt.contains(&secret)),
        "the secret reached the worker"
    );
}

/// Issue #437: a `repo_required` check runs the repository's own checks from `.orbyn/crew.json`
/// and stores their Evidence.
#[tokio::test]
async fn a_repo_required_check_runs_the_repositorys_checks_and_records_evidence() {
    let w = world(
        true,
        Arc::new(|_, root| {
            good(root);
            Turn::Report(report("done"))
        }),
    )
    .await;
    std::fs::create_dir_all(w.repo.join(".orbyn")).unwrap();
    std::fs::write(
        w.repo.join(".orbyn/crew.json"),
        r#"{"checks":[{"run":"exit 0"}]}"#,
    )
    .unwrap();
    super::review_support::commit_all(&w.repo, "repository check");
    let repo = json!({ "id": "chk-repo", "kind": "repo_required",
        "proves": "The repository's own checks pass.", "params": {} });
    let id = started_with(&w, key_with(repo)).await;
    w.until_state(&id, "passed").await;
    let evidence = w.core.store.crew_evidence(&id).unwrap();
    let repo: Vec<_> = evidence
        .iter()
        .filter(|e| e.check.as_str() == "chk-repo")
        .collect();
    assert_eq!(repo.len(), 1, "{evidence:?}");
    assert_eq!(repo[0].outcome, CheckOutcome::Pass);
    assert_eq!(repo[0].command.as_str(), "repo_required: chk-repo");
}
