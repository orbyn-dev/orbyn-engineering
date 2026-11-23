//! Background runs in the Review queue (docs/crew/background-engine-start.md, section 4, PR 4): a run that stops without a
//! merge stays on its own row, and the queue that lists merges now joins those rows too, with the run's state, why it stopped
//! and what it spent. A real `Core`, fake engines and real git. The person's answer is the acknowledgement, which settles the
//! wait and leaves the queue. The wire queue pages the runs with the merges' window and counts them all.

use std::sync::Arc;

use orbyn_contracts as wire;
use serde_json::json;

use super::harness::{Script, Turn, World, report, world};
use crate::crew::Page;
use crate::error::CoreError;

const NOW: i64 = 1_800_000_000;

fn project(w: &World) -> String {
    w.core.store.thread(&w.lead).unwrap().project_id
}

/// A stored crew, on or off.
async fn crew_with(w: &World, name: &str, turns: u32, minutes: u32, on: bool) -> String {
    let body: wire::BackgroundCrewCreate = serde_json::from_value(json!({
        "project_id": project(w), "name": name, "template": "fix_failing_ci",
        "trigger": { "kind": "ci_failure", "branch": "main" },
        "budget": { "turns": turns, "minutes": minutes },
        "daily": { "runs": 100, "turns": 1000 }, "utc_offset_minutes": 600
    }))
    .unwrap();
    let crew = w.core.background_create(&body).await.unwrap();
    if on {
        w.core.background_set_enabled(&crew.id, true).unwrap();
    }
    crew.id
}

/// A worker that changes a file it may change and says so, so its task passes.
fn good() -> Script {
    Arc::new(|n, root| {
        std::fs::write(root.join(format!("src/fix-{n}.txt")), "fixed\n").unwrap();
        Turn::Say(report("done"))
    })
}

/// A worker whose change is outside what it may write, so verification fails each turn.
fn outside() -> Script {
    Arc::new(|_, root| {
        std::fs::write(root.join("outside.txt"), "no\n").unwrap();
        Turn::Say(report("done"))
    })
}

fn run_of(w: &World, crew: &str) -> wire::BackgroundRun {
    let page = Page::new(None, None).unwrap();
    w.core.background_runs(crew, page).unwrap().runs.remove(0)
}

async fn until_run(w: &World, crew: &str, state: wire::BackgroundRunState) -> wire::BackgroundRun {
    w.until("the run reaches its state", |w| {
        run_of(w, crew).state == state
    })
    .await;
    run_of(w, crew)
}

fn queue(w: &World) -> crate::crew::ReviewQueue {
    w.core.crew_review_queue(NOW).unwrap()
}

#[tokio::test]
async fn a_run_that_finished_with_work_waits_in_the_review_queue() {
    let w = world(true, good()).await;
    let id = crew_with(&w, "Fix CI", 10, 30, true).await;
    let run = w.core.crew_background_run_now(&id, "user").await.unwrap();
    let done = until_run(&w, &id, wire::BackgroundRunState::NeedsReview).await;

    let queue = queue(&w);
    assert!(queue.merges.is_empty(), "no merge was made");
    assert_eq!(queue.background.len(), 1);
    let item = &queue.background[0];
    assert_eq!(
        (item.run_id.as_str(), item.crew_id.as_str()),
        (run.id.as_str(), id.as_str())
    );
    assert_eq!(
        (item.crew_name.as_str(), item.template.as_str()),
        ("Fix CI", "fix_failing_ci")
    );
    assert_eq!(item.state, "needs_review");
    assert_eq!(item.stop_reason, None);
    assert_eq!(
        item.review_reason.as_deref(),
        Some("The run finished and has work to review.")
    );
    assert_eq!(
        (item.turns_spent, item.minutes_spent),
        (1, done.minutes_spent)
    );
    assert_eq!((item.budget_turns, item.budget_minutes), (10, 30));
    assert_eq!(item.started_at, run.started_at);
    assert_eq!(item.ended_at.as_deref(), done.ended_at.as_deref());
}

#[tokio::test]
async fn a_run_stopped_for_its_budget_waits_with_its_reason_and_spend() {
    let w = world(true, outside()).await;
    let id = crew_with(&w, "One turn", 1, 30, true).await;
    w.core.crew_background_run_now(&id, "user").await.unwrap();
    let stopped = until_run(&w, &id, wire::BackgroundRunState::Stopped).await;
    assert_eq!(
        stopped.stop_reason,
        Some(wire::BackgroundStopReason::Budget)
    );

    let queue = queue(&w);
    assert_eq!(queue.background.len(), 1);
    let item = &queue.background[0];
    assert_eq!(item.state, "stopped");
    assert_eq!(item.stop_reason.as_deref(), Some("budget"));
    assert_eq!(
        item.review_reason.as_deref(),
        Some("The run reached its budget and stopped.")
    );
    assert_eq!(item.turns_spent, 1);
}

#[tokio::test]
async fn a_run_stopped_by_a_failure_still_waits_for_a_person() {
    let w = world(true, Arc::new(|_, _| Turn::Silent)).await;
    let id = crew_with(&w, "Silent", 10, 30, true).await;
    w.core.crew_background_run_now(&id, "user").await.unwrap();
    let stopped = until_run(&w, &id, wire::BackgroundRunState::Stopped).await;
    assert_eq!(
        stopped.stop_reason,
        Some(wire::BackgroundStopReason::EngineError)
    );
    assert!(
        !stopped.review_waiting,
        "an engine error left no work to look at"
    );

    let queue = queue(&w);
    assert_eq!(queue.background.len(), 1);
    assert_eq!(queue.background[0].state, "stopped");
    assert_eq!(
        queue.background[0].stop_reason.as_deref(),
        Some("engine_error")
    );
    assert_eq!(queue.background[0].review_reason, None);
}

#[tokio::test]
async fn a_running_run_is_not_in_the_review_queue() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let id = crew_with(&w, "Running", 10, 30, true).await;
    w.core.crew_background_run_now(&id, "user").await.unwrap();
    assert_eq!(
        run_of(&w, &id).state,
        wire::BackgroundRunState::Running,
        "the run is still going"
    );
    assert!(queue(&w).background.is_empty());
}

#[tokio::test]
async fn a_run_stopped_by_the_person_switch_needs_no_answer() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let id = crew_with(&w, "Switched off", 10, 30, true).await;
    w.core.crew_background_run_now(&id, "user").await.unwrap();
    w.core.background_set_enabled(&id, false).unwrap();
    let stopped = run_of(&w, &id);
    assert_eq!(stopped.state, wire::BackgroundRunState::Stopped);
    assert_eq!(
        stopped.stop_reason,
        Some(wire::BackgroundStopReason::Disabled)
    );
    assert!(
        queue(&w).background.is_empty(),
        "the person's own switch needs nothing from them"
    );
}

#[tokio::test]
async fn background_runs_join_the_queue_oldest_first() {
    let w = world(true, good()).await;
    let first_crew = crew_with(&w, "First", 10, 30, true).await;
    let first = w
        .core
        .crew_background_run_now(&first_crew, "user")
        .await
        .unwrap();
    until_run(&w, &first_crew, wire::BackgroundRunState::NeedsReview).await;
    let second_crew = crew_with(&w, "Second", 10, 30, true).await;
    let second = w
        .core
        .crew_background_run_now(&second_crew, "user")
        .await
        .unwrap();
    until_run(&w, &second_crew, wire::BackgroundRunState::NeedsReview).await;

    let queue = queue(&w);
    let runs: Vec<&str> = queue
        .background
        .iter()
        .map(|item| item.run_id.as_str())
        .collect();
    assert_eq!(runs, [first.id.as_str(), second.id.as_str()]);
}

#[tokio::test]
async fn the_wire_queue_carries_the_background_run_without_a_merge_or_a_risk() {
    let w = world(true, good()).await;
    let id = crew_with(&w, "Fix CI", 10, 30, true).await;
    let run = w.core.crew_background_run_now(&id, "user").await.unwrap();
    until_run(&w, &id, wire::BackgroundRunState::NeedsReview).await;

    let page = Page::new(None, None).unwrap();
    let queue = w.core.autopilot_review_queue(NOW, page).unwrap();
    assert!(queue.items.is_empty(), "a background run has no merge");
    assert_eq!(queue.total, 0);
    assert_eq!(queue.background_runs.len(), 1);
    let item = &queue.background_runs[0];
    assert_eq!(
        (
            item.run_id.as_str(),
            item.crew_id.as_str(),
            item.crew_name.as_str()
        ),
        (run.id.as_str(), id.as_str(), "Fix CI")
    );
    assert_eq!(item.template, wire::BackgroundTemplate::FixFailingCi);
    assert_eq!(item.state, wire::BackgroundRunState::NeedsReview);
    assert_eq!(item.stop_reason, None);
    assert_eq!(
        item.review_reason.as_deref(),
        Some("The run finished and has work to review.")
    );
    assert_eq!(item.turns_spent, 1);
    assert_eq!(
        item.budget,
        wire::BackgroundBudget {
            turns: 10,
            minutes: 30
        }
    );
    assert_eq!(item.project_id, project(&w));
}

#[tokio::test]
async fn the_wire_queue_is_not_found_with_the_preview_off() {
    let w = world(false, good()).await;
    let page = Page::new(None, None).unwrap();
    let error = w.core.autopilot_review_queue(NOW, page).unwrap_err();
    assert!(matches!(error, CoreError::NotFound(_)), "{error}");
}

/// The person's acknowledgement is the answer to a waiting review: the wait clears under one write, the crew reads idle and the
/// same crew starts again, while the run keeps its state, its spend and why it waited.
#[tokio::test]
async fn acknowledging_a_waiting_review_settles_it_and_the_crew_may_run_again() {
    let w = world(true, good()).await;
    let id = crew_with(&w, "Fix CI", 10, 30, true).await;
    let run = w.core.crew_background_run_now(&id, "user").await.unwrap();
    let waiting = until_run(&w, &id, wire::BackgroundRunState::NeedsReview).await;
    assert!(waiting.review_waiting);
    let before = w.core.background_get(&id).unwrap();
    assert_eq!(
        (before.state, before.reviews_waiting),
        (wire::BackgroundState::NeedsReview, 1)
    );
    assert_eq!(
        queue(&w).background.len(),
        1,
        "an unacknowledged review still waits"
    );

    let settled = w.core.background_acknowledge_review(&run.id).unwrap();
    assert!(!settled.review_waiting);
    assert_eq!(
        settled.review_reason.as_deref(),
        waiting.review_reason.as_deref(),
        "the run keeps why it waited"
    );
    assert_eq!(
        (
            settled.state,
            settled.stop_reason,
            settled.turns_spent,
            settled.minutes_spent,
            settled.started_at.clone(),
            settled.ended_at.clone(),
        ),
        (
            waiting.state,
            waiting.stop_reason,
            waiting.turns_spent,
            waiting.minutes_spent,
            waiting.started_at.clone(),
            waiting.ended_at.clone(),
        ),
        "nothing else about the run changes"
    );
    let after = w.core.background_get(&id).unwrap();
    assert_eq!(
        (after.state, after.reviews_waiting),
        (wire::BackgroundState::Idle, 0)
    );
    assert!(
        queue(&w).background.is_empty(),
        "a settled review leaves the queue"
    );
    let paged = w
        .core
        .autopilot_review_queue(NOW, Page::new(None, None).unwrap())
        .unwrap();
    assert!(paged.background_runs.is_empty());
    assert_eq!(
        paged.background_total, 0,
        "the paged queue counts a settled review as settled"
    );
    // The crew may run again, at once.
    let next = w.core.crew_background_run_now(&id, "user").await.unwrap();
    assert_eq!(next.state, wire::BackgroundRunState::Running);

    // One audit row, the person's, records what was settled.
    let audits = w.core.store.list_audit(None, 100).unwrap();
    let rows: Vec<_> = audits
        .iter()
        .filter(|row| row.action == "crew.background.run.acknowledge")
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].actor.as_str(), rows[0].source.as_str()),
        ("user", "user")
    );
    assert_eq!(rows[0].target, id);
    assert_eq!(rows[0].detail["run"], run.id);
    assert_eq!(rows[0].detail["state"], "needs_review");
    assert_eq!(
        rows[0].detail["review_reason"],
        waiting.review_reason.clone().unwrap()
    );
}

#[tokio::test]
async fn acknowledging_a_review_that_is_not_waiting_is_refused_plainly_and_changes_nothing() {
    let w = world(true, Arc::new(|_, _| Turn::Silent)).await;
    let id = crew_with(&w, "Silent", 10, 30, true).await;
    let run = w.core.crew_background_run_now(&id, "user").await.unwrap();
    // A running run is not waiting.
    let refused = |error: CoreError| {
        assert!(
            matches!(&error, CoreError::Invalid(why) if why == "this run is not waiting for the person's review"),
            "{error}"
        );
    };
    refused(w.core.background_acknowledge_review(&run.id).unwrap_err());
    // A run that stopped for an engine error left nothing to review.
    let stopped = until_run(&w, &id, wire::BackgroundRunState::Stopped).await;
    assert!(!stopped.review_waiting);
    refused(w.core.background_acknowledge_review(&run.id).unwrap_err());
    // A run that is not there is not found.
    assert!(matches!(
        w.core.background_acknowledge_review("nothing").unwrap_err(),
        CoreError::NotFound(_)
    ));
    // Nothing was written.
    assert!(
        !w.core
            .store
            .list_audit(None, 100)
            .unwrap()
            .iter()
            .any(|row| row.action == "crew.background.run.acknowledge")
    );
    assert_eq!(w.core.background_get(&id).unwrap().reviews_waiting, 0);
    // A failed stop still shows in Review: it is not a waiting review, so it has no acknowledgement (PR 4's open question).
    let listed = queue(&w);
    assert_eq!(listed.background.len(), 1);
    assert_eq!(listed.background[0].state, "stopped");
}

#[tokio::test]
async fn acknowledging_is_not_found_with_the_preview_off() {
    let w = world(false, good()).await;
    let error = w.core.background_acknowledge_review("nothing").unwrap_err();
    assert!(matches!(error, CoreError::NotFound(_)), "{error}");
}

#[tokio::test]
async fn the_wire_queue_pages_the_background_runs_and_counts_them_all() {
    let w = world(true, good()).await;
    let mut runs = Vec::new();
    for name in ["First", "Second", "Third"] {
        let crew = crew_with(&w, name, 10, 30, true).await;
        let run = w.core.crew_background_run_now(&crew, "user").await.unwrap();
        until_run(&w, &crew, wire::BackgroundRunState::NeedsReview).await;
        runs.push(run.id);
    }

    let first = w
        .core
        .autopilot_review_queue(NOW, Page::new(Some(2), None).unwrap())
        .unwrap();
    assert_eq!(first.background_total, 3);
    let ids: Vec<&str> = first
        .background_runs
        .iter()
        .map(|item| item.run_id.as_str())
        .collect();
    assert_eq!(ids, [runs[0].as_str(), runs[1].as_str()]);
    assert_eq!(first.next_offset, Some(2), "another page of runs remains");

    let last = w
        .core
        .autopilot_review_queue(NOW, Page::new(Some(2), Some(2)).unwrap())
        .unwrap();
    assert_eq!(last.background_runs.len(), 1);
    assert_eq!(last.background_runs[0].run_id, runs[2]);
    assert_eq!(last.next_offset, None);
    assert_eq!(last.background_total, 3);
}
