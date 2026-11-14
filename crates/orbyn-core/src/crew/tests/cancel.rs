//! A person's cancel of a task, through a real `Core` and a fake worker that hangs until it is stopped. Each case reads the audit
//! rows as well as the state: the intent row comes before the effect, a repeat writes nothing, a failed stop leaves the state alone.

use std::sync::Arc;

use orbyn_contracts::ResultState;
use serde_json::json;

use super::harness::{Turn, World, brief, key, world};
use crate::crew::MergeTarget;
use crate::error::CoreError;

async fn running() -> (World, String) {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (answer, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    let worker = w.worker(&id);
    w.until("the worker's run", |w| {
        w.core.store.thread(&worker).unwrap().active_run.is_some()
    })
    .await;
    (w, id)
}

/// The audit actions about cancelling or force-stopping, oldest first.
fn actions(w: &World) -> Vec<String> {
    let mut rows = w.core.store.list_audit(None, 500).unwrap();
    rows.reverse();
    rows.into_iter()
        .map(|row| row.action)
        .filter(|a| a.starts_with("crew.task.cancel") || a == "run.force_stop")
        .collect()
}

fn count(w: &World, action: &str) -> usize {
    actions(w).iter().filter(|a| *a == action).count()
}

#[tokio::test]
async fn cancelling_a_running_worker_stops_its_run_and_ends_the_task() {
    let (w, id) = running().await;
    let worker = w.worker(&id);
    // A message waiting for the worker must not start another turn.
    w.core.runs.enqueue(&worker, "later", &[]).ok();
    let prompts = w.sent_to(&worker).len();

    let done = w
        .core
        .crew_cancel_task(&id, Some("wrong task"), "user")
        .await
        .unwrap();
    assert!(!done.already);
    assert_eq!(done.task, id);

    assert_eq!(w.task(&id).state, "cancelled");
    assert!(w.task(&id).stop_reason.is_none());
    assert!(w.core.store.thread(&worker).unwrap().active_run.is_none());
    assert!(w.core.store.list_queue(&worker).unwrap().is_empty());
    assert_eq!(w.sent_to(&worker).len(), prompts, "no new turn started");

    // The intent row comes first, then the engine stop, then the state's row.
    assert_eq!(
        actions(&w),
        ["crew.task.cancel", "run.force_stop", "crew.task.cancelled"]
    );
    let rows = w.core.store.list_audit(None, 500).unwrap();
    let intent = rows
        .iter()
        .find(|r| r.action == "crew.task.cancel")
        .unwrap();
    assert_eq!(
        (intent.actor.as_str(), intent.source.as_str()),
        ("user", "user")
    );
    assert_eq!(intent.target, id);
    assert_eq!(intent.detail["previous_state"], "running");
    assert_eq!(intent.detail["reason"], "wrong task");

    // One final result, and the lead is told.
    let results = w.core.store.crew_results(&id).unwrap();
    let cancelled: Vec<_> = results
        .iter()
        .filter(|r| r.state == ResultState::Cancelled && r.r#final)
        .collect();
    assert_eq!(cancelled.len(), 1, "{results:?}");
    w.until("the lead is told", |w| {
        w.sent_to(&w.lead)
            .iter()
            .any(|p| p.contains("state=\"cancelled\""))
    })
    .await;

    // The worker's own run end finds the task over and changes nothing.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(w.task(&id).state, "cancelled");
    assert_eq!(
        w.core
            .store
            .crew_results(&id)
            .unwrap()
            .iter()
            .filter(|r| r.state == ResultState::Cancelled)
            .count(),
        1
    );
}

#[tokio::test]
async fn a_cancelled_task_never_merges() {
    let (w, id) = running().await;
    w.core.crew_cancel_task(&id, None, "user").await.unwrap();
    let target = MergeTarget {
        repo: w.repo.clone(),
        branch: "main".into(),
        owed: Some(Vec::new()),
    };
    let refused = w.core.crew_queue_merge(&id, &target, 1).await.unwrap_err();
    assert!(
        matches!(&refused, CoreError::Invalid(m) if m.contains("only a task that passed")),
        "{refused}"
    );
    assert!(
        w.core
            .store
            .merge_entries(
                &w.repo.to_string_lossy(),
                "main",
                &["queued", "waiting", "merged"]
            )
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn cancelling_twice_writes_one_row_and_answers_already() {
    let (w, id) = running().await;
    let first = w.core.crew_cancel_task(&id, None, "user").await.unwrap();
    let before = actions(&w);
    let second = w.core.crew_cancel_task(&id, None, "user").await.unwrap();
    assert!(!first.already && second.already);
    assert_eq!(second.state, first.state);
    assert_eq!(
        actions(&w),
        before,
        "a repeat writes no row and does nothing"
    );
    assert_eq!(count(&w, "crew.task.cancel"), 1);
}

#[tokio::test]
async fn an_ended_task_is_a_conflict_and_nothing_changes() {
    let (w, id) = running().await;
    w.core.crew_budget_ended(&id);
    assert_eq!(w.task(&id).state, "stopped");
    let before = actions(&w);
    let error = w
        .core
        .crew_cancel_task(&id, None, "user")
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::TaskEnded), "{error}");
    assert_eq!(error.status(), 409);
    assert_eq!(error.public_message(), "This task already ended.");
    assert_eq!(w.task(&id).state, "stopped");
    assert_eq!(actions(&w), before);
}

#[tokio::test]
async fn an_unknown_task_and_a_bad_id_are_refused() {
    let (w, _) = running().await;
    let error = w
        .core
        .crew_cancel_task("no-such-task", None, "user")
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::NotFound(_)), "{error}");
    let error = w.core.crew_cancel_task("", None, "user").await.unwrap_err();
    assert!(matches!(error, CoreError::Invalid(_)), "{error}");
    let long = "x".repeat(201);
    let id = w.core.store.crew_tasks_in_flight().unwrap()[0].0.id.clone();
    let error = w
        .core
        .crew_cancel_task(&id, Some(&long), "user")
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::Invalid(_)), "{error}");
    assert_eq!(w.task(&id).state, "running");
    assert!(actions(&w).is_empty());
}

#[tokio::test]
async fn the_reason_is_redacted_before_it_is_kept() {
    let (w, id) = running().await;
    // A token-shaped value built at run time.
    let secret = format!("sk-{}", "a1B2c3D4".repeat(5));
    w.core
        .crew_cancel_task(&id, Some(&format!("stop, key {secret}")), "user")
        .await
        .unwrap();
    let rows = serde_json::to_string(&w.core.store.list_audit(None, 50).unwrap()).unwrap();
    assert!(!rows.contains(&secret), "{rows}");
}

#[tokio::test]
async fn cancelling_while_verifying_drops_the_claim_and_never_settles_as_passed() {
    let (w, id) = running().await;
    w.core
        .store
        .begin_crew_verification(&id, "run-x", None)
        .unwrap();
    assert!(w.core.store.claim_crew_verification(&id).unwrap());
    assert!(
        w.core
            .store
            .crew_run(&id)
            .unwrap()
            .unwrap()
            .verifying_since
            .is_some()
    );
    let done = w.core.crew_cancel_task(&id, None, "user").await.unwrap();
    assert!(!done.already);
    assert_eq!(w.task(&id).state, "cancelled");
    let run = w.core.store.crew_run(&id).unwrap().unwrap();
    assert!(run.verifying_since.is_none(), "the claim is cleared");
    let intent = w
        .core
        .store
        .list_audit(None, 50)
        .unwrap()
        .into_iter()
        .find(|r| r.action == "crew.task.cancel")
        .unwrap();
    assert_eq!(intent.detail["previous_state"], "verifying");
    assert_eq!(intent.detail["verification_claimed"], true);
    // A verification that finishes later cannot pass, fail or move it, and a new one is not claimed.
    assert!(w.core.store.pass_crew_task(&id, &[]).is_err());
    assert!(w.core.store.fail_crew_verification(&id, "f").is_err());
    assert!(!w.core.store.claim_crew_verification(&id).unwrap());
    w.core.verify_crew_task(&id).await;
    assert_eq!(w.task(&id).state, "cancelled");
}

/// A run the service cannot stop (it belongs to another process) leaves the task as it was, with a row for the failed attempt; a
/// later cancel, once the run can be stopped, works.
#[tokio::test]
async fn a_worker_that_cannot_be_stopped_leaves_the_task_running_and_the_attempt_audited() {
    let (w, id) = running().await;
    let worker = w.worker(&id);
    let run_id = w.core.store.thread(&worker).unwrap().active_run.unwrap().id;
    let db = rusqlite::Connection::open(w._dir.path().join("data/orbyn.sqlite3")).unwrap();
    db.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
    let own: String = db
        .query_row(
            "SELECT owner_epoch FROM runs WHERE id = ?1",
            [&run_id],
            |r| r.get(0),
        )
        .unwrap();
    db.execute(
        "UPDATE runs SET owner_epoch = 'another-process' WHERE id = ?1",
        [&run_id],
    )
    .unwrap();

    let error = w
        .core
        .crew_cancel_task(&id, None, "user")
        .await
        .unwrap_err();
    assert!(error.public_message().contains("not cancelled"), "{error}");
    assert_eq!(
        w.task(&id).state,
        "running",
        "fail closed: the state is kept"
    );
    assert!(w.core.store.thread(&worker).unwrap().active_run.is_some());
    // The engine stop writes its own row first; the failed attempt is the last.
    assert_eq!(
        actions(&w),
        [
            "crew.task.cancel",
            "run.force_stop",
            "crew.task.cancel_failed"
        ]
    );
    assert_eq!(count(&w, "crew.task.cancelled"), 0);

    db.execute(
        "UPDATE runs SET owner_epoch = ?2 WHERE id = ?1",
        [&run_id, &own],
    )
    .unwrap();
    let done = w.core.crew_cancel_task(&id, None, "user").await.unwrap();
    assert!(!done.already);
    assert_eq!(w.task(&id).state, "cancelled");
    assert!(w.core.store.thread(&worker).unwrap().active_run.is_none());
}

#[tokio::test]
async fn a_cancel_after_the_workers_run_was_stopped_is_a_repeat() {
    // The worker's run is stopped by someone else (not through the cancel): the run's own end cancels the task, so a person's cancel
    // that follows finds it cancelled and answers `already`, with nothing to stop and no rows.
    let (w, id) = running().await;
    let worker = w.worker(&id);
    w.core.runs.force_stop(&worker, "user").await.unwrap();
    w.until("the worker run ended the task", |w| {
        w.task(&id).state != "running"
    })
    .await;
    // The run's own end cancelled it (a cancelled run is a cancelled task), so a person's cancel is a repeat.
    let done = w.core.crew_cancel_task(&id, None, "user").await.unwrap();
    assert!(done.already);
}

#[tokio::test]
async fn with_the_flag_off_the_cancel_is_not_found() {
    let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
    let error = w
        .core
        .crew_cancel_task("t-1", None, "user")
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::NotFound(_)), "{error}");
}

/// No agent tool cancels a task, and an agent's request to the route is refused by path (N1).
#[test]
fn no_agent_tool_reaches_the_cancel_and_the_path_is_an_always_denied_change() {
    for spec in crate::crew::tool_specs() {
        let name = spec["name"].as_str().unwrap();
        assert!(!name.contains("cancel"), "{name}");
    }
    assert!(!crate::crew::is_tool("crew_cancel_task"));
    assert!(crate::crew::changes::changes_crew_state(
        "/api/crew/tasks/t-1/cancel"
    ));
    assert!(crate::crew::changes::crew_change(
        "curl -X POST http://127.0.0.1:9/api/crew/tasks/t-1/cancel"
    ));
    assert!(
        crate::crew::changes::STATE_CHANGING
            .iter()
            .any(|(method, path)| *method == "POST" && *path == "/api/crew/tasks/{id}/cancel")
    );
}

fn seeded_task(w: &World, label: &str) -> String {
    use orbyn_contracts as wire;
    let stamper = crate::crew::Stamper {
        crew_id: wire::CrewId::new("crew-1").unwrap(),
        engine: wire::CrewToken::new("alpha").unwrap(),
        member: wire::CrewMember::new("lead-01").unwrap(),
        now: wire::CrewTimestamp::new("2026-11-12T10:00:00Z").unwrap(),
    };
    let brief = stamper
        .brief(&brief(json!({ "outcome": format!("Seed {label}.") })))
        .unwrap();
    let key = stamper.answer_key(&key(), &brief.id).unwrap();
    let task = w
        .core
        .store
        .create_crew_task(&brief, &key, &format!("run-{label}"))
        .unwrap();
    w.core
        .store
        .add_crew_run(&task.id, &w.lead, "worker-01")
        .unwrap();
    task.id
}

fn delegate(label: &str) -> orbyn_contracts::DelegateTask {
    orbyn_contracts::DelegateTask {
        prompt: format!("work {label}"),
        title: Some("Crew worker".into()),
        engine: Some("alpha".into()),
        model: None,
        effort: None,
        options: Default::default(),
    }
}

/// The race during a start: the task is cancelled after the worker was routed but before its thread is recorded. No worker is
/// recorded, the thread just made is removed and no engine starts.
#[tokio::test]
async fn a_task_cancelled_during_its_start_gets_no_worker_and_no_run() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    for end in ["cancelled", "stopped"] {
        let id = seeded_task(&w, end);
        if end == "cancelled" {
            w.core.store.move_crew_task(&id, "cancelled", None).unwrap();
        } else {
            w.core
                .store
                .move_crew_task(&id, "needs_person", None)
                .unwrap();
            w.core
                .store
                .move_crew_task(&id, "stopped", Some("budget"))
                .unwrap();
        }
        let threads = w.core.store.child_threads(&w.lead).unwrap().len();
        let sent = w.prompts.lock().unwrap().len();
        let error = w
            .core
            .attach_crew_worker(&w.lead, &delegate(end), &id, None)
            .await
            .unwrap_err();
        assert!(error.message.contains("already ended"), "{error:?}");
        assert!(
            w.core
                .store
                .crew_run(&id)
                .unwrap()
                .unwrap()
                .worker_thread_id
                .is_none()
        );
        assert_eq!(
            w.core.store.child_threads(&w.lead).unwrap().len(),
            threads,
            "the thread was removed"
        );
        assert_eq!(w.prompts.lock().unwrap().len(), sent, "no engine started");
        assert_eq!(w.task(&id).state, end);
    }
}

/// The same race on a retry: the cancel lands after the caller read the task as running, before the message is queued.
#[tokio::test]
async fn a_retry_is_not_queued_for_a_task_cancelled_meanwhile() {
    let (w, id) = running().await;
    let stale = w.task(&id);
    assert_eq!(stale.state, "running");
    w.core.crew_cancel_task(&id, None, "user").await.unwrap();
    let worker = w.worker(&id);
    let sent = w.sent_to(&worker).len();
    let error = w.core.send_to_worker(&stale, "Fix this.").unwrap_err();
    assert_eq!(error.code, orbyn_contracts::ErrorCode::RunEnded);
    assert!(w.core.store.list_queue(&worker).unwrap().is_empty());
    assert!(w.core.store.thread(&worker).unwrap().active_run.is_none());
    assert_eq!(w.sent_to(&worker).len(), sent);
}

#[tokio::test]
async fn two_concurrent_cancels_write_one_outcome_and_one_says_already() {
    let (w, id) = running().await;
    let (a, b) = tokio::join!(
        w.core.crew_cancel_task(&id, None, "user"),
        w.core.crew_cancel_task(&id, None, "user")
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(
        [a.already, b.already].iter().filter(|x| **x).count(),
        1,
        "{a:?} {b:?}"
    );
    assert_eq!(count(&w, "crew.task.cancel"), 1);
    assert_eq!(count(&w, "crew.task.cancelled"), 1);
    assert_eq!(count(&w, "crew.task.cancel_failed"), 0);
    assert_eq!(w.task(&id).state, "cancelled");
    assert_eq!(
        w.core
            .store
            .crew_results(&id)
            .unwrap()
            .iter()
            .filter(|r| r.state == ResultState::Cancelled)
            .count(),
        1
    );
}

/// The worker's run end cancels the task while the person's cancel is between its first row and its last: one final result, no
/// second move, and the row says the run end moved it.
#[tokio::test]
async fn the_run_end_winning_the_race_leaves_one_result_and_says_who_moved_it() {
    let (w, id) = running().await;
    let worker = w.worker(&id);
    assert!(matches!(
        w.core.store.begin_crew_cancel(&id, "user", None).unwrap(),
        crate::store::CancelStart::Begun(_)
    ));
    w.core.runs.force_stop(&worker, "user").await.unwrap();
    w.until("the run end cancelled it", |w| {
        w.task(&id).state == "cancelled"
            && w.core
                .store
                .crew_results(&id)
                .unwrap()
                .iter()
                .any(|r| r.state == ResultState::Cancelled && r.r#final)
    })
    .await;
    let (task, moved) = w.core.store.finish_crew_cancel(&id, "user").unwrap();
    assert_eq!((task.state.as_str(), moved), ("cancelled", false));
    let row = w
        .core
        .store
        .list_audit(None, 100)
        .unwrap()
        .into_iter()
        .find(|r| r.action == "crew.task.cancelled")
        .unwrap();
    assert_eq!(row.detail["moved_by"], "run_end");
    assert_eq!(
        w.core
            .store
            .crew_results(&id)
            .unwrap()
            .iter()
            .filter(|r| r.state == ResultState::Cancelled && r.r#final)
            .count(),
        1
    );
}

/// A stop that does not take (the run is still there after every round): the three rounds run out, the task keeps its state, the
/// failed attempt is recorded and the caller gets the 502.
#[tokio::test]
async fn when_every_stop_round_runs_out_the_state_is_kept_and_the_attempt_audited() {
    let (w, id) = running().await;
    let worker = w.worker(&id);
    let error = w
        .core
        .crew_cancel_with(&id, None, "user", |_: String| async { Ok(()) })
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::EngineUnavailable(_)), "{error}");
    assert_eq!(error.status(), 502);
    assert_eq!(w.task(&id).state, "running");
    assert!(w.core.store.thread(&worker).unwrap().active_run.is_some());
    assert_eq!(count(&w, "crew.task.cancel"), 1);
    assert_eq!(count(&w, "crew.task.cancel_failed"), 1);
    assert_eq!(count(&w, "crew.task.cancelled"), 0);
    let failed = w
        .core
        .store
        .list_audit(None, 50)
        .unwrap()
        .into_iter()
        .find(|r| r.action == "crew.task.cancel_failed")
        .unwrap();
    assert!(
        failed.detail["error"]
            .as_str()
            .unwrap()
            .contains("did not stop")
    );
}
