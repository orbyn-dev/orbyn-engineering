//! A task whose worker never ran (the start was refused) is a "could not start" item, not a wait for the person's approval, and a
//! task that waits for a real decision is unchanged. The derivation reads stored data only (`crew_runs.last_report_run`), so the
//! tests build the stored shapes directly. The retry cancels the refused task and asks the lead to start it again, only when another
//! engine could take it.

use std::sync::Arc;

use orbyn_contracts::{
    FlowCellState as State, FlowColumnId as Column, FlowFilter, FlowStage as Stage, FlowTaskList,
    FlowTaskRow,
};
use serde_json::json;

use super::flow::page;
use super::harness::{Turn, World, brief, key, world};
use super::review_support::{arc, crew};
use crate::crew::retry_start::NO_OTHER_ENGINE;
use crate::error::CoreError;

/// The list's row for `task`, found by id: the read owns the row order, so a test does not.
fn task_row<'a>(list: &'a FlowTaskList, task: &str) -> &'a FlowTaskRow {
    list.tasks
        .iter()
        .find(|row| row.id == task)
        .expect("the task's row in its crew's list")
}

/// A task routed to `alpha` whose worker is running, then moved to `needs_person` before any report: the stored shape of a task
/// whose start was refused after it was routed.
async fn refused_after_routing(w: &World, outcome: &str) -> String {
    let (answer, error) = w
        .start(
            brief(
                json!({ "outcome": outcome, "engine": { "id": "alpha", "assigned_by": "person" } }),
            ),
            key(),
        )
        .await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    w.core
        .store
        .move_crew_task(&id, "needs_person", None)
        .unwrap();
    id
}

fn card_of(w: &World, id: &str) -> orbyn_contracts::FlowCard {
    let crew = w.task(id).crew_id;
    let board = w
        .core
        .crew_flow_board(&crew, None, None, None, Some("needs_you"), None, page())
        .unwrap();
    // The board was asked for one column, but its answer is a list: find the column by name, not place.
    board
        .columns
        .iter()
        .find(|column| column.column == Column::NeedsYou)
        .expect("the board's Needs you column")
        .cards
        .iter()
        .find(|c| c.id == id)
        .cloned()
        .expect("the task is a card in Needs you")
}

#[tokio::test]
async fn a_route_refusal_is_could_not_start_and_leaves_the_needs_you_filter() {
    let w = crew(arc(|_, _| Turn::Silent)).await;
    let ghost = brief(json!({ "engine": { "id": "ghost", "assigned_by": "lead" } }));
    let (answer, error) = w.start(ghost, key()).await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    assert_eq!(w.task(&id).state, "needs_person");

    let card = card_of(&w, &id);
    assert!(card.could_not_start);
    assert!(card.reason.as_ref().is_some_and(|r| !r.is_empty()));
    assert!(card.can_cancel, "Dismiss is the person's cancel");

    let crew_id = w.task(&id).crew_id;
    let list = |filter: FlowFilter| {
        w.core
            .crew_flow_tasks(
                &crew_id,
                Some(match filter {
                    FlowFilter::NeedsYou => "needs_you",
                    _ => "failed",
                }),
                None,
                None,
                page(),
            )
            .unwrap()
    };
    let asked = list(FlowFilter::NeedsYou);
    assert_eq!(
        asked.counts.needs_you, 0,
        "it asks no decision of the person"
    );
    assert!(asked.tasks.is_empty());
    let failed = list(FlowFilter::Failed);
    assert_eq!(
        failed.tasks.len(),
        1,
        "the refused task is the only failed row"
    );
    let row = task_row(&failed, &id);
    assert!(row.could_not_start);
    for stage in [Stage::Route, Stage::Build] {
        let cell = row.cells.iter().find(|c| c.stage == stage).unwrap();
        assert_eq!(cell.state, State::Failed, "{stage:?}");
    }
}

#[tokio::test]
async fn a_task_stored_as_routed_then_waiting_with_no_report_derives_as_could_not_start() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let id = refused_after_routing(&w, "Refused after routing.").await;
    let run = w.core.store.crew_run(&id).unwrap().unwrap();
    assert!(
        run.last_report_run.is_none(),
        "the stored field the derivation reads"
    );
    let card = card_of(&w, &id);
    assert!(card.could_not_start);
    assert_eq!(card.engine.as_deref(), Some("alpha"));
}

#[tokio::test]
async fn a_task_waiting_on_a_real_decision_is_unchanged() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (answer, error) = w
        .start(brief(json!({ "outcome": "Undecided." })), key())
        .await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    // A report was taken (the worker ran), then the checks could not decide: the person decides.
    w.core
        .store
        .begin_crew_verification(&id, "engine-run-1", None)
        .unwrap();
    w.core.crew_needs_person(
        &id,
        "No check this version can run decided the task, so a person decides.",
        false,
    );
    assert_eq!(w.task(&id).state, "needs_person");

    let card = card_of(&w, &id);
    assert!(!card.could_not_start);
    assert!(
        card.reason
            .as_ref()
            .is_some_and(|r| r.contains("a person decides"))
    );
    let crew_id = w.task(&id).crew_id;
    let asked = w
        .core
        .crew_flow_tasks(&crew_id, Some("needs_you"), None, None, page())
        .unwrap();
    assert_eq!(asked.counts.needs_you, 1);
    let row = task_row(&asked, &id);
    assert!(!row.could_not_start);
    let build = row.cells.iter().find(|c| c.stage == Stage::Build).unwrap();
    assert_eq!(build.state, State::NeedsYou);
    let board = w
        .core
        .crew_flow_board(&crew_id, None, None, None, None, None, page())
        .unwrap();
    let needs = board
        .columns
        .iter()
        .find(|c| c.column == Column::NeedsYou)
        .unwrap();
    assert_eq!(needs.count, 1);
}

#[tokio::test]
async fn retry_needs_another_engine_and_changes_nothing_without_one() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let id = refused_after_routing(&w, "Only one engine.").await;
    let answer = w.core.crew_retry_elsewhere(&id, "user").await;
    assert!(
        matches!(&answer, Err(CoreError::CrewTaskBusy(why)) if why == NO_OTHER_ENGINE),
        "{answer:?}"
    );
    assert_eq!(w.task(&id).state, "needs_person", "the task is as it was");
    assert_eq!(
        w.core
            .store
            .list_audit(None, 500)
            .unwrap()
            .iter()
            .filter(|row| row.action.starts_with("crew.task.cancel"))
            .count(),
        0,
        "nothing was written"
    );
}

#[tokio::test]
async fn retry_cancels_the_task_and_asks_the_lead_to_start_it_again() {
    let w = crew(arc(|_, _| Turn::Hang)).await;
    let id = refused_after_routing(&w, "Two engines.").await;
    let done = w.core.crew_retry_elsewhere(&id, "user").await.unwrap();
    assert_eq!(done.task, id);
    assert!(done.lead_told);
    assert_eq!(w.task(&id).state, "cancelled");
    w.until("the lead is asked", |w| {
        w.sent_to(&w.lead)
            .iter()
            .any(|t| t.contains("could not start"))
    })
    .await;
    let note = w
        .sent_to(&w.lead)
        .into_iter()
        .find(|t| t.contains("could not start"))
        .unwrap();
    assert!(note.contains(&id) && note.contains("crew_start"));
}

#[tokio::test]
async fn a_task_that_did_start_cannot_be_retried_elsewhere() {
    let w = crew(arc(|_, _| Turn::Hang)).await;
    let (answer, error) = w
        .start(brief(json!({ "outcome": "Running." })), key())
        .await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    let refused = w.core.crew_retry_elsewhere(&id, "user").await;
    assert!(
        matches!(refused, Err(CoreError::CrewTaskBusy(_))),
        "{refused:?}"
    );
    assert_eq!(w.task(&id).state, "running");
    let missing = w.core.crew_retry_elsewhere("no-such-task", "user").await;
    assert!(
        matches!(missing, Err(CoreError::NotFound(_))),
        "{missing:?}"
    );
}

#[tokio::test]
async fn a_dismiss_that_lands_first_means_the_lead_is_not_asked() {
    let w = crew(arc(|_, _| Turn::Hang)).await;
    let id = refused_after_routing(&w, "Dismissed elsewhere.").await;
    let before = w.sent_to(&w.lead).len();
    // Another window dismisses the task after the checks passed and before this call's cancel.
    let other = id.clone();
    let answer = w
        .core
        .crew_retry_with(&id, "user", || {
            w.core
                .store
                .move_crew_task(&other, "cancelled", None)
                .unwrap();
        })
        .await;
    assert!(
        matches!(answer, Err(CoreError::CrewTaskBusy(_))),
        "{answer:?}"
    );
    assert_eq!(w.task(&id).state, "cancelled");
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert_eq!(
        w.sent_to(&w.lead).len(),
        before,
        "the lead was asked to start a dismissed task"
    );
}
