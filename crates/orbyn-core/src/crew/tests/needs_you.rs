//! The Crew "needs you" inbox (KC3 PR1) on fake engines: one item for each kind, a task appears once, a dismissal refuses what
//! cannot be hidden, hides only the wait it saw, and the flag off hides the read. The tests seed the stored shapes the rules
//! read, so no test depends on a real engine.

use orbyn_contracts::{NeedsYouItem, NeedsYouKind};
use serde_json::json;

use super::harness::{Turn, World, brief, key, world};
use super::review_support::{arc, built, crew};
use crate::crew::WORKER_STOPPED_TWICE;
use crate::error::CoreError;

fn items(w: &World) -> Vec<NeedsYouItem> {
    w.core.crew_needs_you().unwrap().items
}

fn item_of(w: &World, kind: NeedsYouKind, id: &str) -> NeedsYouItem {
    items(w)
        .into_iter()
        .find(|item| item.kind == kind && item.id == id)
        .unwrap_or_else(|| panic!("no {kind:?} item for {id}"))
}

/// A task whose worker is running and stays running.
async fn started_hanging(w: &World, outcome: &str) -> String {
    let (answer, error) = w.start(brief(json!({ "outcome": outcome })), key()).await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    assert_eq!(w.task(&id).state, "running");
    id
}

/// A task that ran and then waited on a person, so it is not "could not start".
async fn needs_person_task(w: &World, why: &str) -> String {
    let id = started_hanging(w, "Undecided.").await;
    w.core
        .store
        .begin_crew_verification(&id, "engine-run-1", None)
        .unwrap();
    w.core.crew_needs_person(&id, why, false);
    assert_eq!(w.task(&id).state, "needs_person");
    id
}

fn stop_task(w: &World, id: &str, reason: &str) {
    w.core
        .store
        .move_crew_task(id, "stopped", Some(reason))
        .unwrap();
}

fn set_worker_stopped_twice(w: &World, id: &str) {
    let conn = w.core.store.reader();
    conn.execute_batch("PRAGMA ignore_check_constraints = ON")
        .unwrap();
    conn.execute(
        "UPDATE crew_tasks SET stop_reason = ?1 WHERE id = ?2",
        rusqlite::params![WORKER_STOPPED_TWICE, id],
    )
    .unwrap();
    conn.execute_batch("PRAGMA ignore_check_constraints = OFF")
        .unwrap();
}

#[tokio::test]
async fn a_task_whose_worker_never_ran_is_could_not_start_and_dismissible() {
    let w = crew(arc(|_, _| Turn::Silent)).await;
    let (answer, error) = w
        .start(
            brief(json!({ "engine": { "id": "ghost", "assigned_by": "lead" } })),
            key(),
        )
        .await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    assert_eq!(w.task(&id).state, "needs_person");

    let item = item_of(&w, NeedsYouKind::CouldNotStart, &id);
    assert_eq!(item.id, id);
    assert!(item.dismissible);
    assert!(item.project_id.is_some(), "the lead thread names a project");
    assert!(item.crew_id.is_some());
    assert!(!item.since.is_empty());
    let reason = item.reason.expect("a fixed reason");
    assert!(!reason.is_empty() && reason.chars().count() <= 160);
}

#[tokio::test]
async fn a_task_waiting_on_a_person_is_task_needs_person_with_its_scrubbed_reason() {
    let w = crew(arc(|_, _| Turn::Hang)).await;
    let why = format!("A person decides.\n{}", "words ".repeat(80));
    let id = needs_person_task(&w, &why).await;

    let item = item_of(&w, NeedsYouKind::TaskNeedsPerson, &id);
    assert!(!item.dismissible);
    let reason = item.reason.expect("the task's own reason");
    assert!(reason.contains("A person decides"), "{reason}");
    assert!(!reason.contains('\n'), "one line: {reason}");
    assert!(reason.chars().count() <= 160, "{}", reason.chars().count());
}

#[tokio::test]
async fn a_task_waiting_on_a_manual_check_is_manual_check_and_not_dismissible() {
    let w = crew(arc(|_, _| Turn::Hang)).await;
    let id = needs_person_task(&w, "A manual check waits.").await;
    w.core
        .store
        .park_manual_check(&id, "chk-manual", 1_800_000_000)
        .unwrap();

    let item = item_of(&w, NeedsYouKind::ManualCheck, &id);
    assert!(!item.dismissible);
    assert!(item.reason.is_some());
}

#[tokio::test]
async fn a_merge_waiting_on_a_person_is_merge_waiting() {
    let w = crew(arc(|_, root| built(root))).await;
    let id = w.built_work().await;
    let mut record = crate::crew::fixtures::merge_record(&id, "/repo", "main", "a break");
    record.review_state = "waiting".to_owned();
    record.needs_person = Some("The break needs a person.".to_owned());
    record.merged_unix = 1_800_000_000;
    w.core.store.add_crew_merge(&record).unwrap();

    let item = item_of(&w, NeedsYouKind::MergeWaiting, &id);
    assert_eq!(item.id, id);
    assert!(!item.dismissible);
    assert_eq!(item.since, crate::util::format_unix(1_800_000_000));
    assert!(item.project_id.is_some());
    assert!(item.crew_id.is_some());
}

#[tokio::test]
async fn a_background_run_waiting_for_review_is_background_review() {
    let w = world(true, arc(|_, _| Turn::Silent)).await;
    let project = w.core.store.thread(&w.lead).unwrap().project_id;
    let settings = json!({ "branch": "main" });
    let crew = w
        .core
        .store
        .create_background_crew(
            &crate::store::NewBackground {
                project_id: &project,
                name: "Background review",
                template: "fix_failing_ci",
                trigger_kind: "ci_failure",
                trigger_settings: &settings,
                budget_turns: 10,
                budget_minutes: 30,
                daily_runs: 0,
                daily_turns: 0,
                utc_offset_minutes: 0,
            },
            "user",
        )
        .unwrap();
    w.core
        .store
        .set_background_enabled(&crew.id, true, "user")
        .unwrap();
    let run = w.core.store.start_background_run(&crew.id).unwrap();
    w.core
        .store
        .end_background_run(
            &run.id,
            crate::store::RunEnd::NeedsReview("The run has work to review."),
        )
        .unwrap();

    let item = item_of(&w, NeedsYouKind::BackgroundReview, &run.id);
    assert!(!item.dismissible);
    assert_eq!(item.project_id.as_deref(), Some(project.as_str()));
    assert_eq!(item.crew_id.as_deref(), Some(crew.id.as_str()));
}

#[tokio::test]
async fn a_stopped_task_with_a_tripwire_event_is_tripwire_stop() {
    let w = crew(arc(|_, _| Turn::Hang)).await;
    let id = started_hanging(&w, "Stopped.").await;
    let crew_id = w.task(&id).crew_id;
    stop_task(&w, &id, "too_many_denials");
    w.core
        .store
        .add_crew_autopilot_event(&id, &crew_id, "tripwire", &json!({ "in_a_row": 3 }))
        .unwrap();

    let item = item_of(&w, NeedsYouKind::TripwireStop, &id);
    assert!(item.dismissible);
    assert_eq!(item.since, w.task(&id).updated_at);
    assert!(item.reason.is_some());

    // Another stop with no trip wire is not an item.
    let (answer, error) = w
        .start(
            brief(json!({
                "outcome": "Stopped plainly.",
                "files": [{ "path": "docs/**", "mode": "write" }]
            })),
            key(),
        )
        .await;
    assert!(!error, "{answer}");
    let other = answer["task"].as_str().unwrap().to_owned();
    stop_task(&w, &other, "repeated_failure");
    assert!(items(&w).iter().all(|item| item.id != other));
}

#[tokio::test]
async fn a_stopped_task_whose_worker_stopped_twice_is_worker_stopped_twice() {
    let w = crew(arc(|_, _| Turn::Hang)).await;
    let id = started_hanging(&w, "Stopped twice.").await;
    stop_task(&w, &id, "repeated_failure");
    set_worker_stopped_twice(&w, &id);

    let item = item_of(&w, NeedsYouKind::WorkerStoppedTwice, &id);
    assert!(item.dismissible);
    assert_eq!(item.since, w.task(&id).updated_at);
    assert!(item.reason.is_some());
}

#[tokio::test]
async fn a_task_is_one_item_and_the_first_rule_wins() {
    let w = crew(arc(|_, _| Turn::Hang)).await;
    let (answer, error) = w
        .start(
            brief(json!({ "engine": { "id": "ghost", "assigned_by": "lead" } })),
            key(),
        )
        .await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    let mut record = crate::crew::fixtures::merge_record(&id, "/repo", "main", "a break");
    record.review_state = "waiting".to_owned();
    record.needs_person = Some("The break needs a person.".to_owned());
    w.core.store.add_crew_merge(&record).unwrap();
    let matching: Vec<_> = items(&w).into_iter().filter(|item| item.id == id).collect();
    assert_eq!(matching.len(), 1, "the task is one item");
    assert_eq!(matching[0].kind, NeedsYouKind::CouldNotStart);

    // A stopped task with both a trip wire and the worker-stopped-twice reason is still one item: the trip wire wins.
    let stopped = started_hanging(&w, "Both reasons.").await;
    let crew_id = w.task(&stopped).crew_id;
    stop_task(&w, &stopped, "repeated_failure");
    set_worker_stopped_twice(&w, &stopped);
    w.core
        .store
        .add_crew_autopilot_event(&stopped, &crew_id, "tripwire", &json!({ "in_a_row": 3 }))
        .unwrap();
    let matching: Vec<_> = items(&w)
        .into_iter()
        .filter(|item| item.id == stopped)
        .collect();
    assert_eq!(matching.len(), 1);
    assert_eq!(matching[0].kind, NeedsYouKind::TripwireStop);
}

#[tokio::test]
async fn dismiss_refuses_a_kind_that_cannot_be_dismissed_and_an_unknown_id() {
    let w = crew(arc(|_, _| Turn::Hang)).await;
    let id = needs_person_task(&w, "A manual check waits.").await;
    w.core
        .store
        .park_manual_check(&id, "chk-manual", 1_800_000_000)
        .unwrap();

    let refused = w
        .core
        .crew_needs_you_dismiss(NeedsYouKind::ManualCheck, &id, "user");
    assert!(matches!(refused, Err(CoreError::Invalid(_))), "{refused:?}");
    let unknown =
        w.core
            .crew_needs_you_dismiss(NeedsYouKind::CouldNotStart, "sha256:unknown", "user");
    assert!(
        matches!(unknown, Err(CoreError::NotFound(_))),
        "{unknown:?}"
    );
}

#[tokio::test]
async fn a_dismissed_item_is_hidden_until_its_since_is_later_and_nothing_else_changes() {
    let w = crew(arc(|_, _| Turn::Silent)).await;
    let (answer, error) = w
        .start(
            brief(json!({ "engine": { "id": "ghost", "assigned_by": "lead" } })),
            key(),
        )
        .await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    let before = w.task(&id);

    assert!(
        w.core
            .crew_needs_you_dismiss(NeedsYouKind::CouldNotStart, &id, "user")
            .unwrap()
            .dismissed
    );
    assert!(items(&w).iter().all(|item| item.id != id));
    // The same dismissal again is idempotent.
    assert!(
        w.core
            .crew_needs_you_dismiss(NeedsYouKind::CouldNotStart, &id, "user")
            .unwrap()
            .dismissed
    );
    assert_eq!(w.task(&id), before, "the task was not touched");

    let dismissals = |w: &World| {
        w.core
            .store
            .list_audit(None, 500)
            .unwrap()
            .into_iter()
            .filter(|row| row.action == "crew.needs_you.dismiss")
            .collect::<Vec<_>>()
    };
    let rows = dismissals(&w);
    assert_eq!(rows.len(), 1, "one audit row for one dismissal");
    assert_eq!(rows[0].actor, "user");
    assert_eq!(rows[0].detail["kind"], "could_not_start");

    // A later wait of the same task shows again.
    let later = crate::util::format_unix(crate::util::parse_time(&before.updated_at).unwrap() + 60);
    {
        let conn = w.core.store.reader();
        conn.execute(
            "UPDATE crew_tasks SET updated_at = ?1 WHERE id = ?2",
            rusqlite::params![later, id],
        )
        .unwrap();
    }
    let shown = item_of(&w, NeedsYouKind::CouldNotStart, &id);
    assert_eq!(shown.since, later);
    assert!(
        w.core
            .crew_needs_you_dismiss(NeedsYouKind::CouldNotStart, &id, "user")
            .unwrap()
            .dismissed
    );
    assert!(items(&w).iter().all(|item| item.id != id));
    assert_eq!(
        dismissals(&w).len(),
        2,
        "the newer wait is its own dismissal"
    );
}

#[tokio::test]
async fn with_the_flag_off_the_inbox_and_its_dismissal_are_not_found() {
    let w = world(false, arc(|_, _| Turn::Silent)).await;
    assert!(matches!(
        w.core.crew_needs_you(),
        Err(CoreError::NotFound(_))
    ));
    assert!(matches!(
        w.core
            .crew_needs_you_dismiss(NeedsYouKind::CouldNotStart, "sha256:abc", "user"),
        Err(CoreError::NotFound(_))
    ));
}
