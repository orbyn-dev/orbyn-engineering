//! The Taskboard read on fake engines: work tasks fall into the seven columns, a reviewer's run is not a card, a card carries no
//! count of checks and no check text, a task that needs the person carries its reason, the reads change nothing, and the flag
//! off hides them. No test depends on the clock: two tasks that start together get briefs of their own.

use orbyn_contracts::{
    CrewObject, FlowBoard, FlowColumnId as Column, FlowWorking, NeedsYouKind, ResultState,
    object_id,
};
use serde_json::{Value, json};

use super::flow::{built_another, crew_rows, nonce, page};
use super::harness::{Turn, brief, give_back_worker_run, key, take_worker_run, world};
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
    let stored = w.core.store.crew_evidence(&subject).unwrap();
    assert!(!stored.is_empty(), "the task ran its checks");
    let passed = stored
        .iter()
        .filter(|e| e.outcome == orbyn_contracts::CheckOutcome::Pass)
        .count();
    let counts = card
        .checks
        .as_ref()
        .expect("a passed task shows its counts");
    assert_eq!(
        (counts.passed as usize, counts.of as usize),
        (passed, stored.len()),
        "the card counts the latest verification"
    );
    assert_eq!(view.engines.len(), 1);
    assert_eq!(view.kinds[0].name, "implement");

    // A card has these fields and no cell or check text; its checks are two counts.
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
            "checks",
            "could_not_start",
            "effort",
            "engine",
            "id",
            "kind",
            "model",
            "name",
            "needs_you",
            "owner",
            "reason",
            "state",
            "stop_reason",
            "updated_at",
            "working"
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
    assert!(
        card.checks.is_none(),
        "a running task shows no count, though it holds evidence"
    );
    assert_eq!(
        card.working,
        Some(FlowWorking::Running),
        "KC3: a live turn is Working"
    );
    assert!(card.needs_you.is_none(), "nothing waits for the person");
    assert!(
        column(&view, Column::Passed).cards[0].checks.is_some(),
        "the passed one still does"
    );
}

#[tokio::test]
async fn hostile_evidence_fields_reach_a_card_only_as_two_counts() {
    let w = crew(arc(|_, root| built(root))).await;
    let done = w.built_work().await;
    let crew_id = w.task(&done).crew_id;
    let base = w.core.store.crew_evidence(&done).unwrap().remove(0);
    let ran = w.core.store.crew_evidence(&done).unwrap().len();
    let before = board(&w, &crew_id);
    let was = column(&before, Column::Passed).cards[0]
        .checks
        .clone()
        .unwrap();
    assert_eq!(was.of as usize, ran);

    // A failing row, with hostile text in every field a check has, built at run time.
    let marker = nonce("count");
    let mut value: Value = CrewObject::Evidence(base).to_value();
    value["outcome"] = json!("fail");
    value["check"] = json!(format!("chk-{marker}"));
    for field in ["command", "excerpt", "proves", "does_not_prove"] {
        value[field] = json!(format!("{field} {marker} <script>alert(1)</script>"));
    }
    value["id"] = json!(object_id(&value).unwrap());
    w.core
        .store
        .add_crew_object(&done, &CrewObject::parse(&value).unwrap())
        .unwrap();

    let view = board(&w, &crew_id);
    let card = &column(&view, Column::Passed).cards[0];
    let counts = card.checks.clone().expect("counts");
    // No finished verification named the grafted row, so it is not part of the one the task passed on.
    assert_eq!(
        (counts.passed, counts.of),
        (was.passed, was.of),
        "a row no verification named changed the count"
    );

    // A retry: a later verification names the hostile row alone. The count is that verification's, not both attempts' together.
    let gate = w
        .core
        .store
        .crew_gates(&done)
        .unwrap()
        .into_iter()
        .find(|g| g.stage == orbyn_contracts::GateStage::Checks)
        .expect("the task's own checks gate");
    let mut later: Value = CrewObject::Gate(gate).to_value();
    later["evidence"] = json!([value["id"]]);
    later["decision"] = json!("fail");
    later["id"] = json!(object_id(&later).unwrap());
    w.core
        .store
        .add_crew_object(&done, &CrewObject::parse(&later).unwrap())
        .unwrap();
    let view = board(&w, &crew_id);
    let counts = column(&view, Column::Passed).cards[0]
        .checks
        .clone()
        .expect("counts");
    assert_eq!(
        (counts.passed, counts.of),
        (0, 1),
        "the newest verification is the one counted"
    );
    let text = serde_json::to_string(&view).unwrap();
    assert!(
        !text.contains(&marker) && !text.contains("<script>"),
        "evidence text reached the board"
    );
    let checks = serde_json::to_value(counts).unwrap();
    let mut keys: Vec<&str> = checks
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["of", "passed"], "a count holds two numbers only");
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

/// A task whose last cancel attempt could not stop its worker is a Needs-you card from the one field, with its own kind and
/// line; once a stop holds, the field no longer names it and the card leaves the column.
#[tokio::test]
async fn a_cancelled_task_whose_worker_could_not_be_stopped_is_a_needs_you_card_until_a_stop_holds()
{
    let w = crew(arc(|_, _| Turn::Hang)).await;
    let (answer, error) = w
        .start(
            brief(json!({
                "outcome": "Stuck in a loop.",
                "files": [{ "path": "src/stuck/**", "mode": "write" }]
            })),
            key(),
        )
        .await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    let crew_id = w.task(&id).crew_id;
    let epoch = take_worker_run(&w, &id);
    let closed = w.core.crew_close_lead(&w.lead, "user").await.unwrap();
    assert_eq!(closed.failed, vec![id.clone()]);

    let view = board(&w, &crew_id);
    let card = column(&view, Column::NeedsYou)
        .cards
        .iter()
        .find(|card| card.id == id)
        .expect("the could-not-stop card");
    assert_eq!(card.needs_you, Some(NeedsYouKind::CouldNotStop));
    assert!(
        card.reason
            .as_ref()
            .is_some_and(|reason| !reason.is_empty())
    );

    give_back_worker_run(&w, &id, &epoch);
    w.core.crew_cancel_task(&id, None, "user").await.unwrap();
    let view = board(&w, &crew_id);
    assert!(
        column(&view, Column::NeedsYou)
            .cards
            .iter()
            .all(|card| card.id != id),
        "the card stays after the stop held"
    );
}

#[tokio::test]
async fn a_reason_that_quotes_another_tasks_check_is_withheld() {
    // A real TaskNeedsPerson task, not a could-not-start one: the reason under test is the task's own scrubbed
    // `needs_person` result, which is the path a held question takes.
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        _ => Turn::Hang,
    }))
    .await;
    // The first task's key has a check of its own; a second task, held for a person, will quote it.
    let other_check = format!("echo {}", nonce("board-check"));
    let other_key = json!({
        "checks": [{ "id": "chk-other", "kind": "command", "params": { "run": other_check },
            "proves": "It runs." }],
        "protected_paths": ["tests/**"]
    });
    let done = built_another(&w, "First task.", other_key).await;
    let crew_id = w.task(&done).crew_id;
    let second = brief(json!({
        "outcome": "Second task.",
        "files": [{ "path": "src/held/**", "mode": "write" }]
    }));
    let (answer, error) = w.start(second, key()).await;
    assert!(!error, "the second task did not start");
    let held = answer["task"].as_str().unwrap().to_owned();
    w.core
        .store
        .begin_crew_verification(&held, "engine-run-1", None)
        .unwrap();
    w.core.crew_needs_person(&held, "A person decides.", false);
    assert_eq!(w.task(&held).state, "needs_person");

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
    assert_eq!(card.needs_you, Some(NeedsYouKind::TaskNeedsPerson));
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

/// KC3: the Needs you column is the one needs-you field (a passed task whose merge waits and a stopped task whose trip
/// wire stopped it are cards there; a dismissed stop is not, and no second rule puts it back), and a live turn's card is
/// Working from the one working function while its column still says its run state.
#[tokio::test]
async fn the_needs_you_column_is_the_one_field_and_a_live_card_is_working() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        _ => Turn::Hang,
    }))
    .await;
    // A passed task whose merge waits: the field says MergeWaiting, so the card is in Needs you, not in Passed.
    let merged = w.built_work().await;
    let crew_id = w.task(&merged).crew_id;
    let mut record = crate::crew::fixtures::merge_record(&merged, "/repo", "main", "a break");
    record.crew_id = crew_id.clone();
    record.review_state = "waiting".to_owned();
    record.needs_person = Some("The break needs a person.".to_owned());
    w.core.store.add_crew_merge(&record).unwrap();

    // A task still running: Working, in Running.
    let (answer, error) = w
        .start(
            brief(json!({ "outcome": "Keeps working.", "files": [{ "path": "src/live/**", "mode": "write" }] })),
            key(),
        )
        .await;
    assert!(!error, "{answer}");
    let running = answer["task"].as_str().unwrap().to_owned();
    assert_eq!(w.task(&running).state, "running");

    // A task stopped by a trip wire: it waits for the person even though its run state is stopped.
    let (answer, error) = w
        .start(
            brief(json!({ "outcome": "Stopped by a trip wire.", "files": [{ "path": "src/stop/**", "mode": "write" }] })),
            key(),
        )
        .await;
    assert!(!error, "{answer}");
    let stopped = answer["task"].as_str().unwrap().to_owned();
    w.core
        .store
        .move_crew_task(&stopped, "stopped", Some("too_many_denials"))
        .unwrap();
    w.core
        .store
        .add_crew_autopilot_event(&stopped, &crew_id, "tripwire", &json!({ "in_a_row": 3 }))
        .unwrap();

    let view = board(&w, &crew_id);
    let needs = column(&view, Column::NeedsYou);
    assert_eq!(needs.count, 2, "the merge wait and the trip-wire stop");
    assert_eq!(
        column(&view, Column::Passed).count,
        0,
        "a waiting merge is not Passed"
    );
    assert_eq!(column(&view, Column::StoppedCancelled).count, 0);
    let waiting_merge = needs.cards.iter().find(|c| c.id == merged).unwrap();
    assert_eq!(waiting_merge.state, "passed");
    assert_eq!(waiting_merge.needs_you, Some(NeedsYouKind::MergeWaiting));
    assert_eq!(
        waiting_merge.reason.as_deref(),
        Some("A merge needs your decision."),
        "the reason is the field's own line"
    );
    assert!(
        waiting_merge.working.is_none(),
        "passed work is not Working"
    );
    let tripwire = needs.cards.iter().find(|c| c.id == stopped).unwrap();
    assert_eq!(tripwire.state, "stopped");
    assert_eq!(tripwire.needs_you, Some(NeedsYouKind::TripwireStop));

    let live = &column(&view, Column::Running).cards[0];
    assert_eq!(live.id, running);
    assert_eq!(live.working, Some(FlowWorking::Running));
    assert!(live.needs_you.is_none());

    // The column is the field's own count for this crew: no second rule adds or removes a card.
    let field: Vec<_> = w
        .core
        .crew_needs_you()
        .unwrap()
        .items
        .into_iter()
        .filter(|i| i.crew_id.as_deref() == Some(crew_id.as_str()))
        .collect();
    assert_eq!(field.len(), needs.count as usize);

    // A dismissed stop leaves the column on the next read, and the field is the reason.
    w.core
        .crew_needs_you_dismiss(NeedsYouKind::TripwireStop, &stopped, "user")
        .unwrap();
    let view = board(&w, &crew_id);
    let needs = column(&view, Column::NeedsYou);
    assert_eq!(needs.count, 1, "the dismissed stop is gone");
    assert!(needs.cards.iter().all(|c| c.id != stopped));
    assert_eq!(column(&view, Column::StoppedCancelled).count, 1);
    assert_eq!(column(&view, Column::StoppedCancelled).cards[0].id, stopped);
}
/// KC3 F1, the product decision: a dismissed could-not-start wait leaves the Inbox and the dashboard (they read the one field), but
/// not the Taskboard. The task still cannot run, so its card stays in Needs you with no field kind and no sentence: the board never
/// hides a task that still needs the person, and it does not invent a wait the field no longer names.
#[tokio::test]
async fn a_dismissed_could_not_start_card_keeps_its_place_without_a_sentence() {
    let w = crew(arc(|_, _| Turn::Silent)).await;
    let (answer, error) = w
        .start(
            brief(json!({ "engine": { "id": "ghost", "assigned_by": "lead" } })),
            key(),
        )
        .await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    let crew_id = w.task(&id).crew_id;
    let view = board(&w, &crew_id);
    let card = &column(&view, Column::NeedsYou).cards[0];
    assert_eq!(card.needs_you, Some(NeedsYouKind::CouldNotStart));
    assert!(card.reason.as_ref().is_some_and(|r| !r.is_empty()));

    w.core
        .crew_needs_you_dismiss(NeedsYouKind::CouldNotStart, &id, "user")
        .unwrap();
    // The field no longer names it, so the Inbox and the dashboard drop it.
    assert!(
        w.core
            .crew_needs_you()
            .unwrap()
            .items
            .iter()
            .all(|item| item.id != id)
    );

    // The card keeps its place, with no field kind and no sentence.
    let view = board(&w, &crew_id);
    let needs = column(&view, Column::NeedsYou);
    assert_eq!(needs.count, 1, "the task still cannot run");
    let card = needs.cards.iter().find(|card| card.id == id).unwrap();
    assert_eq!(card.state, "needs_person");
    assert!(card.could_not_start);
    assert_eq!(card.needs_you, None);
    assert_eq!(card.reason, None);
}

/// KC3 F2: a could-not-start card carries the task's own scrubbed refusal, not only the kind's fixed words, because the one
/// needs-you list now reads the task's latest `needs_person` result for every task kind.
#[tokio::test]
async fn a_could_not_start_card_carries_its_own_refusal() {
    let w = crew(arc(|_, _| Turn::Silent)).await;
    let (answer, error) = w
        .start(
            brief(json!({ "engine": { "id": "ghost", "assigned_by": "lead" } })),
            key(),
        )
        .await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    let crew_id = w.task(&id).crew_id;
    let stored = w.core.store.crew_results(&id).unwrap();
    let refusal = stored
        .iter()
        .rev()
        .find(|result| result.state == ResultState::NeedsPerson)
        .expect("the start refusal is a needs_person result")
        .summary
        .clone();
    let first_word = refusal.split_whitespace().next().expect("a refusal");

    let view = board(&w, &crew_id);
    let card = &column(&view, Column::NeedsYou).cards[0];
    let reason = card.reason.clone().expect("the refusal is carried");
    assert!(
        reason.contains(first_word),
        "{reason} does not hold {refusal}"
    );
    assert_ne!(
        reason, "The worker never ran.",
        "the fixed words replaced the task's refusal"
    );
    // The Inbox row carries the same scrubbed sentence: it is one field.
    let item = w
        .core
        .crew_needs_you()
        .unwrap()
        .items
        .into_iter()
        .find(|item| item.id == id)
        .unwrap();
    assert_eq!(item.reason.as_deref(), Some(reason.as_str()));
}

#[tokio::test]
async fn the_board_names_every_wait_of_its_crew_past_the_inboxs_two_hundred() {
    let w = crew(arc(|_, _| Turn::Silent)).await;
    let mut ids = Vec::new();
    // Two hundred and one tasks that could not start: the Inbox shows the newest 200, the board of their crew all of them.
    for n in 0..201 {
        let (answer, error) = w
            .start(
                brief(json!({
                    "outcome": format!("Wait number {n}."),
                    "engine": { "id": "ghost", "assigned_by": "lead" }
                })),
                key(),
            )
            .await;
        assert!(!error, "{answer}");
        ids.push(answer["task"].as_str().unwrap().to_owned());
    }
    let crew_id = w.task(&ids[0]).crew_id;
    assert_eq!(w.core.crew_needs_you().unwrap().items.len(), 200);
    // The Needs you column, past the Inbox's 200: its last page holds the oldest waits, which the Inbox no longer lists.
    let view = w
        .core
        .crew_flow_board(
            &crew_id,
            None,
            None,
            None,
            Some("needs_you"),
            None,
            Page::new(Some(50), Some(190)).unwrap(),
        )
        .unwrap();
    let needs = column(&view, Column::NeedsYou);
    assert_eq!(needs.count, 201);
    assert_eq!(needs.cards.len(), 11);
    assert!(
        needs
            .cards
            .iter()
            .all(|card| card.needs_you == Some(NeedsYouKind::CouldNotStart)),
        "a wait past the Inbox's cap lost its field kind on the board"
    );
}
