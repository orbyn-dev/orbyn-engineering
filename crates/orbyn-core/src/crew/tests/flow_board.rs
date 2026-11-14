//! The Taskboard read on fake engines: work tasks fall into the seven columns, a reviewer's run is not a card, a card carries no
//! count of checks and no check text, a task that needs the person carries its reason, the reads change nothing, and the flag
//! off hides them. No test depends on the clock: two tasks that start together get briefs of their own.

use orbyn_contracts::{CrewObject, FlowBoard, FlowColumnId as Column, object_id};
use serde_json::{Value, json};

use super::flow::{built_another, crew_rows, nonce, page};
use super::harness::{Turn, brief, key, world};
use super::review_support::{arc, built, crew, reviewed};
use crate::crew::Page;

fn board(w: &super::harness::World, crew_id: &str) -> FlowBoard {
    w.core
        .crew_flow_board(crew_id, None, None, None, None, None, page())
        .unwrap()
}

fn column(board: &FlowBoard, id: Column) -> &orbyn_contracts::FlowColumn {
    board.columns.iter().find(|c| c.column == id).unwrap()
}

#[tokio::test]
async fn a_finished_task_is_one_card_and_its_reviewer_is_not() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        _ => reviewed(vec![]),
    }))
    .await;
    let subject = w.built_work().await;
    let crew_id = w.task(&subject).crew_id;
    let lanes = w.review(&subject, &["correctness"]).await;
    w.until_state(&lanes[0], "passed").await;

    let before = crew_rows(&w);
    let moved = w.task(&subject).updated_at;
    let view = board(&w, &crew_id);
    assert_eq!(view.total, 1, "a review run is a lane, not a card");
    let order: Vec<Column> = view.columns.iter().map(|c| c.column).collect();
    assert_eq!(
        order,
        [
            Column::Queued,
            Column::Routed,
            Column::Running,
            Column::Verifying,
            Column::NeedsYou,
            Column::Passed,
            Column::StoppedCancelled
        ]
    );
    assert_eq!(column(&view, Column::Passed).count, 1);
    assert!(
        view.columns
            .iter()
            .filter(|c| c.column != Column::Passed)
            .all(|c| c.count == 0)
    );
    let card = &column(&view, Column::Passed).cards[0];
    assert_eq!(card.id, subject);
    assert_eq!(card.kind, "implement");
    assert_eq!(card.engine.as_deref(), Some("alpha"));
    assert!(card.model.is_some() && card.effort.is_some());
    assert!(matches!(card.effort.as_deref(), Some("low" | "medium")));
    assert!(!card.can_cancel, "a passed task cannot be cancelled");
    assert!(
        card.can_rerun,
        "a passed task with a checkout may be re-run"
    );
    assert!(card.reason.is_none());
    assert_eq!(view.engines.len(), 1);
    assert_eq!(view.kinds[0].name, "implement");

    // A card has these fields and no cell, count of checks or check text.
    let mut keys: Vec<String> = serde_json::to_value(card)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "can_cancel",
            "can_rerun",
            "effort",
            "engine",
            "id",
            "kind",
            "model",
            "name",
            "owner",
            "reason",
            "state",
            "stop_reason",
            "updated_at"
        ]
    );

    // One column, paged; the others are left out.
    let only = w
        .core
        .crew_flow_board(&crew_id, None, None, None, Some("passed"), None, page())
        .unwrap();
    assert_eq!(only.columns.len(), 1);
    let none = w
        .core
        .crew_flow_board(&crew_id, Some("beta"), None, None, None, None, page())
        .unwrap();
    assert!(none.columns.iter().all(|c| c.count == 0));
    assert_eq!(none.total, 1, "the header total is not narrowed");

    // Reading changed no row of any crew table and no task's time.
    for _ in 0..3 {
        board(&w, &crew_id);
    }
    assert_eq!(before, crew_rows(&w));
    assert_eq!(moved, w.task(&subject).updated_at);
}

#[tokio::test]
async fn a_running_task_can_be_cancelled_and_shows_no_check_text() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        _ => Turn::Hang,
    }))
    .await;
    let done = w.built_work().await;
    let (answer, error) = w
        .start(
            brief(json!({ "outcome": "Keeps running.", "engine": { "id": "alpha", "assigned_by": "person" } })),
            key(),
        )
        .await;
    assert!(!error, "the second task did not start");
    let running = answer["task"].as_str().unwrap().to_owned();
    let task = w.task(&running);
    assert_eq!(task.state, "running");

    // Give the running task Evidence that carries text a reader must not see, built at run time.
    let marker = nonce("board");
    let evidence = w.core.store.crew_evidence(&done).unwrap().remove(0);
    let mut value: Value = CrewObject::Evidence(evidence).to_value();
    value["run_id"] = json!(task.run_id);
    value["crew_id"] = json!(task.crew_id);
    value["check"] = json!("chk-graft");
    for field in ["command", "excerpt", "proves", "does_not_prove"] {
        value[field] = json!(format!("{field} {marker}"));
    }
    value["id"] = json!(object_id(&value).unwrap());
    w.core
        .store
        .add_crew_object(&running, &CrewObject::parse(&value).unwrap())
        .unwrap();

    let view = board(&w, &task.crew_id);
    assert_eq!(column(&view, Column::Running).count, 1);
    assert_eq!(column(&view, Column::Passed).count, 1);
    let card = &column(&view, Column::Running).cards[0];
    assert_eq!(card.id, running);
    assert!(card.can_cancel);
    assert!(
        !card.can_rerun,
        "a task still being worked on is not re-run"
    );
    let text = serde_json::to_string(&view).unwrap();
    assert!(
        !text.contains(&marker),
        "check text reached the board of a running task"
    );
}

#[tokio::test]
async fn a_task_that_needs_the_person_carries_its_reason() {
    let w = crew(arc(|_, root| built(root))).await;
    let ghost = brief(json!({ "engine": { "id": "ghost", "assigned_by": "lead" } }));
    let (answer, error) = w.start(ghost, key()).await;
    assert!(!error, "the task did not start");
    let id = answer["task"].as_str().unwrap().to_owned();
    let crew_id = w.task(&id).crew_id;

    let view = board(&w, &crew_id);
    let needs = column(&view, Column::NeedsYou);
    assert_eq!(needs.count, 1);
    let card = &needs.cards[0];
    assert_eq!(card.id, id);
    assert!(card.engine.is_none(), "it was never routed");
    assert!(card.reason.as_ref().is_some_and(|r| !r.is_empty()));
    assert!(card.can_cancel);
    assert!(!card.can_rerun);
}

#[tokio::test]
async fn a_reason_that_quotes_another_tasks_check_is_withheld() {
    let w = crew(arc(|_, root| built(root))).await;
    // The first task's key has a check of its own; a second task, held for a person, will quote it.
    let other_check = format!("echo {}", nonce("board-check"));
    let other_key = json!({
        "checks": [{ "id": "chk-other", "kind": "command", "params": { "run": other_check },
            "proves": "It runs." }],
        "protected_paths": ["tests/**"]
    });
    let done = built_another(&w, "First task.", other_key).await;
    let crew_id = w.task(&done).crew_id;
    let ghost = brief(
        json!({ "outcome": "Second task.", "engine": { "id": "ghost", "assigned_by": "lead" } }),
    );
    let (answer, error) = w.start(ghost, key()).await;
    assert!(!error, "the second task did not start");
    let held = answer["task"].as_str().unwrap().to_owned();

    // The stored reason of the held task is rewritten to quote the first task's check.
    let task = w.task(&held);
    let mut result =
        CrewObject::Result(w.core.store.crew_results(&held).unwrap().remove(0)).to_value();
    result["summary"] = json!(format!("ran {other_check} and it failed"));
    result["id"] = json!(object_id(&result).unwrap());
    w.core
        .store
        .add_crew_object(&held, &CrewObject::parse(&result).unwrap())
        .unwrap();
    assert_eq!(task.state, "needs_person");

    let view = board(&w, &crew_id);
    let card = &column(&view, Column::NeedsYou).cards[0];
    let reason = card.reason.clone().unwrap_or_default();
    assert!(reason.contains("ran"), "the reason is read");
    assert!(
        !reason.contains(&other_check),
        "the reason holds another task's check"
    );
    assert!(!serde_json::to_string(&view).unwrap().contains(&other_check));
}

#[tokio::test]
async fn bad_input_is_refused_and_the_flag_off_hides_the_board() {
    let w = crew(arc(|_, root| built(root))).await;
    let ask = |crew: &str, column: Option<&str>, per: Option<u32>, q: Option<&str>| {
        w.core.crew_flow_board(
            crew,
            None,
            None,
            q,
            column,
            per,
            Page::new(None, None).unwrap(),
        )
    };
    assert!(ask("../x", None, None, None).is_err());
    assert!(ask("crew-1", Some("nowhere"), None, None).is_err());
    assert!(ask("crew-1", None, Some(0), None).is_err());
    assert!(ask("crew-1", None, Some(51), None).is_err());
    assert!(ask("crew-1", None, None, Some(&"q".repeat(81))).is_err());
    let empty = ask("nobody", None, None, None).unwrap();
    assert_eq!(empty.columns.len(), 7);
    assert!(empty.columns.iter().all(|c| c.count == 0) && empty.total == 0);

    let off = world(false, arc(|_, _| Turn::Silent)).await;
    assert!(matches!(
        off.core
            .crew_flow_board("crew-1", None, None, None, None, None, page()),
        Err(crate::error::CoreError::NotFound(_))
    ));
}
