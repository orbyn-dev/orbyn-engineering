//! Background runs in the Review queue (docs/crew/background-engine-start.md, section 4, PR 4): a run that stops without a
//! merge stays on its own row, and the queue that lists merges now joins those rows too, with the run's state, why it stopped
//! and what it spent. A real `Core`, fake engines and real git. The person's answer to a background run has no route yet, so
//! these tests read the join only.

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
fn crew_with(w: &World, name: &str, turns: u32, minutes: u32, on: bool) -> String {
    let body: wire::BackgroundCrewCreate = serde_json::from_value(json!({
        "project_id": project(w), "name": name, "template": "fix_failing_ci",
        "trigger": { "kind": "ci_failure", "branch": "main" },
        "budget": { "turns": turns, "minutes": minutes }
    }))
    .unwrap();
    let crew = w.core.background_create(&body).unwrap();
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
    let id = crew_with(&w, "Fix CI", 10, 30, true);
    let run = w.core.crew_background_run_now(&id).await.unwrap();
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
    let id = crew_with(&w, "One turn", 1, 30, true);
    w.core.crew_background_run_now(&id).await.unwrap();
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
    let id = crew_with(&w, "Silent", 10, 30, true);
    w.core.crew_background_run_now(&id).await.unwrap();
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
    let id = crew_with(&w, "Running", 10, 30, true);
    w.core.crew_background_run_now(&id).await.unwrap();
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
    let id = crew_with(&w, "Switched off", 10, 30, true);
    w.core.crew_background_run_now(&id).await.unwrap();
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
    let first_crew = crew_with(&w, "First", 10, 30, true);
    let first = w.core.crew_background_run_now(&first_crew).await.unwrap();
    until_run(&w, &first_crew, wire::BackgroundRunState::NeedsReview).await;
    let second_crew = crew_with(&w, "Second", 10, 30, true);
    let second = w.core.crew_background_run_now(&second_crew).await.unwrap();
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
    let id = crew_with(&w, "Fix CI", 10, 30, true);
    let run = w.core.crew_background_run_now(&id).await.unwrap();
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
