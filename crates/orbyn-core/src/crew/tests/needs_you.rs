//! The Crew "needs you" inbox (KC3 PR1) on fake engines: one item for each kind, a task appears once, a dismissal refuses what
//! cannot be hidden, hides only the wait it saw, and the flag off hides the read. The tests seed the stored shapes the rules
//! read, so no test depends on a real engine.

use orbyn_contracts::{NeedsYouItem, NeedsYouKind, NeedsYouRef};
use serde_json::json;

use super::harness::{Turn, World, brief, give_back_worker_run, key, take_worker_run, world};
use super::review_support::{arc, built, crew};
use crate::crew::WORKER_STOPPED_TWICE;
use crate::crew::needs_you::Scope;
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

fn worker_stopped_twice(w: &World, id: &str) {
    let stopped = w
        .core
        .store
        .stop_crew_after_restart(id, "already_resumed")
        .unwrap()
        .expect("a task still in flight is stopped");
    assert_eq!(stopped.state, "stopped");
    assert_eq!(stopped.stop_reason.as_deref(), Some("engine_error"));
    assert_eq!(stopped.stop_detail.as_deref(), Some(WORKER_STOPPED_TWICE));
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
    // The item carries the task's own scrubbed refusal, not only the kind's fixed words: the person acts on those words.
    let stored = w.core.store.crew_results(&id).unwrap();
    let refusal = stored
        .iter()
        .rev()
        .find(|result| result.state == orbyn_contracts::ResultState::NeedsPerson)
        .expect("the start refusal is a needs_person result")
        .summary
        .clone();
    let reason = item.reason.expect("the refusal");
    assert!(!reason.is_empty() && reason.chars().count() <= 160);
    assert!(
        reason.contains(refusal.split_whitespace().next().unwrap()),
        "{reason} does not hold {refusal}"
    );
    assert_ne!(
        reason, "The worker never ran.",
        "the fixed words replaced it"
    );
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
    // Only a could-not-stop item carries the brief and the route.
    assert!(item.name.is_none(), "{:?}", item.name);
    assert!(item.engine.is_none(), "{:?}", item.engine);
    assert!(item.model.is_none(), "{:?}", item.model);
    assert!(item.task_kind.is_none(), "{:?}", item.task_kind);
}

#[tokio::test]
async fn a_clean_question_is_given_in_full_and_not_hidden() {
    let w = crew(arc(|_, _| Turn::Hang)).await;
    let id = needs_person_task(&w, "Should the export keep the old column names?").await;

    let item = item_of(&w, NeedsYouKind::TaskNeedsPerson, &id);
    assert_eq!(
        item.question.as_deref(),
        Some("Should the export keep the old column names?")
    );
    assert!(!item.question_hidden);
}

#[tokio::test]
async fn a_question_that_holds_a_secret_is_hidden_and_no_part_of_it_is_shown() {
    let w = crew(arc(|_, _| Turn::Hang)).await;
    let secret = format!("ghp_{}", "a1B2c3D4e5".repeat(4));
    let id = needs_person_task(&w, &format!("Is it fine to use {secret} for the deploy?")).await;

    let item = item_of(&w, NeedsYouKind::TaskNeedsPerson, &id);
    assert!(item.question_hidden);
    assert!(item.question.is_none(), "{:?}", item.question);
    // The words around the secret are part of the question too: the reason falls back to the kind's fixed words (none here).
    assert!(item.reason.is_none(), "{:?}", item.reason);
    let everything = serde_json::to_string(&w.core.crew_needs_you().unwrap()).unwrap();
    assert!(!everything.contains("ghp_"), "{everything}");
    assert!(!everything.contains("deploy"), "{everything}");

    // The notify read names ids and kinds only, so the hidden text has no way into it.
    let asked = [NeedsYouRef {
        kind: NeedsYouKind::TaskNeedsPerson,
        id: id.clone(),
    }];
    let claimed = serde_json::to_string(&w.core.crew_needs_you_notify(&asked).unwrap()).unwrap();
    assert!(
        !claimed.contains("ghp_") && !claimed.contains("deploy"),
        "{claimed}"
    );
}

#[tokio::test]
async fn a_refusal_that_holds_a_secret_is_not_a_hidden_question() {
    let w = crew(arc(|_, _| Turn::Silent)).await;
    let secret = format!("ghp_{}", "a1B2c3D4e5".repeat(4));
    let (answer, error) = w
        .start(
            brief(json!({ "engine": { "id": "ghost", "assigned_by": "lead" } })),
            key(),
        )
        .await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    w.core
        .record_result(
            &w.task(&id),
            orbyn_contracts::ResultState::NeedsPerson,
            None,
            &format!("It refused with {secret}."),
            None,
        )
        .unwrap();

    let item = item_of(&w, NeedsYouKind::CouldNotStart, &id);
    // The latest refusal holds a secret, so the scan hid it: the reason is the fixed words, and the text is not shown.
    assert_eq!(item.reason.as_deref(), Some("The worker never ran."));
    // A could-not-start item has no question to hide.
    assert!(item.question.is_none());
    assert!(!item.question_hidden);
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
    worker_stopped_twice(&w, &id);

    let item = item_of(&w, NeedsYouKind::WorkerStoppedTwice, &id);
    assert!(item.dismissible);
    assert_eq!(item.since, w.task(&id).updated_at);
    assert!(item.reason.is_some());
}

/// The task KC2 could not stop is a wait of its own, answered and never hidden, and a successful Stop again ends it.
#[tokio::test]
async fn a_task_whose_last_cancel_attempt_failed_is_could_not_stop_and_not_dismissible() {
    let w = crew(arc(|_, _| Turn::Hang)).await;
    let id = started_hanging(&w, "Stuck.").await;
    let epoch = take_worker_run(&w, &id);
    let answer = w.core.crew_close_lead(&w.lead, "user").await.unwrap();
    assert_eq!(answer.failed, vec![id.clone()]);

    let item = item_of(&w, NeedsYouKind::CouldNotStop, &id);
    assert!(!item.dismissible);
    assert!(item.project_id.is_some(), "the lead thread names a project");
    assert_eq!(item.crew_id.as_deref(), Some(w.task(&id).crew_id.as_str()));
    assert!(!item.since.is_empty());
    let reason = item.reason.expect("fixed words");
    assert!(!reason.is_empty() && reason.chars().count() <= 160);
    // The row carries the brief and the route, so it draws without a board read.
    let name = item.name.expect("the task's name");
    assert!(!name.is_empty() && !name.contains('\n'), "{name}");
    assert!(name.contains("Stuck"), "{name} does not hold the outcome");
    assert!(item.task_kind.is_some(), "{:?}", item.task_kind);
    match w.core.store.crew_route_decisions(&id).unwrap().pop() {
        Some(route) => {
            assert_eq!(item.engine.as_deref(), Some(route.engine.as_str()));
            assert_eq!(item.model.as_deref(), Some(route.model.as_str()));
        }
        None => {
            assert!(item.engine.is_none(), "{:?}", item.engine);
            assert!(item.model.is_none(), "{:?}", item.model);
        }
    }

    // It is answered, never hidden: the person stops it again.
    let refused = w
        .core
        .crew_needs_you_dismiss(NeedsYouKind::CouldNotStop, &id, "user");
    assert!(matches!(refused, Err(CoreError::Invalid(_))), "{refused:?}");

    // The stop now takes: the task is cancelled, and the item goes with it.
    give_back_worker_run(&w, &id, &epoch);
    w.core.crew_cancel_task(&id, None, "user").await.unwrap();
    assert!(
        !w.core
            .store
            .crew_could_not_stop(None, None)
            .unwrap()
            .iter()
            .any(|one| one.task.id == id)
    );
    assert!(items(&w).iter().all(|item| item.id != id));
}

/// The one needs-you read narrowed to one crew leaves another crew's item out, and the device read keeps it: the
/// Taskboard's 30-second read is scoped the same way, so it never loads every crew's records.
#[tokio::test]
async fn a_needs_you_read_for_one_crew_leaves_out_another_crews_item() {
    let w = crew(arc(|_, _| Turn::Hang)).await;
    let id = started_hanging(&w, "Stuck elsewhere.").await;
    let _epoch = take_worker_run(&w, &id);
    let answer = w.core.crew_close_lead(&w.lead, "user").await.unwrap();
    assert_eq!(answer.failed, vec![id.clone()]);
    let crew_id = w.task(&id).crew_id;

    let other = w.core.needs_you_all(Scope::Crew("another-crew")).unwrap();
    assert!(other.iter().all(|item| item.id != id));

    let own = w.core.needs_you_all(Scope::Crew(&crew_id)).unwrap();
    assert!(
        own.iter()
            .any(|item| item.id == id && item.kind == NeedsYouKind::CouldNotStop)
    );
    // The device read (the Inbox) still holds it: scoping narrows a reader, never the field.
    let device = w.core.needs_you_all(Scope::Device).unwrap();
    assert!(device.iter().any(|item| item.id == id));
}

/// A task that waits on a person and whose worker could not be stopped is the stop, not the wait: its worker may still run.
#[tokio::test]
async fn a_failed_stop_outranks_the_same_task_waiting_on_a_person() {
    let w = crew(arc(|_, _| Turn::Hang)).await;
    let id = started_hanging(&w, "Stuck and waiting.").await;
    let _epoch = take_worker_run(&w, &id);
    let answer = w.core.crew_close_lead(&w.lead, "user").await.unwrap();
    assert_eq!(answer.failed, vec![id.clone()]);
    w.core
        .store
        .move_crew_task(&id, "needs_person", None)
        .unwrap();

    let ours: Vec<_> = items(&w).into_iter().filter(|item| item.id == id).collect();
    assert_eq!(ours.len(), 1, "one item for the task");
    assert_eq!(ours[0].kind, NeedsYouKind::CouldNotStop);
    assert!(!ours[0].dismissible);
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

    // A stopped task with both a trip wire and the worker-stopped-twice detail is still one item: the trip wire wins.
    let stopped = started_hanging(&w, "Both reasons.").await;
    let crew_id = w.task(&stopped).crew_id;
    worker_stopped_twice(&w, &stopped);
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
    assert!(matches!(
        w.core.crew_needs_you_notify(&[]),
        Err(CoreError::NotFound(_))
    ));
}

fn refer(kind: NeedsYouKind, id: &str) -> NeedsYouRef {
    NeedsYouRef {
        kind,
        id: id.to_owned(),
    }
}

/// The first window to ask for an item is given it and every later one is not, even after a restart; an id that is not a current
/// item and a dismissed item are never given.
#[tokio::test]
async fn an_item_is_announced_once_to_the_first_window_that_asks() {
    let w = crew(arc(|_, _| Turn::Hang)).await;
    let waiting = needs_person_task(&w, "Which database?").await;
    let asked = [
        refer(NeedsYouKind::TaskNeedsPerson, &waiting),
        refer(NeedsYouKind::TaskNeedsPerson, "sha256:unknown"),
        // The right id with the wrong kind is not an item either.
        refer(NeedsYouKind::MergeWaiting, &waiting),
    ];

    let first = w.core.crew_needs_you_notify(&asked).unwrap();
    assert_eq!(
        first.claimed,
        [refer(NeedsYouKind::TaskNeedsPerson, &waiting)]
    );
    let second = w.core.crew_needs_you_notify(&asked).unwrap();
    assert!(second.claimed.is_empty(), "{:?}", second.claimed);
    // Asking does not touch the items.
    assert_eq!(items(&w).len(), 1);
}

#[tokio::test]
async fn a_dismissed_item_is_not_given_and_a_later_wait_is_announced_again() {
    let w = crew(arc(|_, _| Turn::Silent)).await;
    let (answer, error) = w
        .start(
            brief(json!({ "engine": { "id": "ghost", "assigned_by": "lead" } })),
            key(),
        )
        .await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    w.core
        .crew_needs_you_dismiss(NeedsYouKind::CouldNotStart, &id, "user")
        .unwrap();
    // A dismissed item is not in the list, so nothing is given for it, and it is not marked either.
    let asked = [refer(NeedsYouKind::CouldNotStart, &id)];
    assert!(
        w.core
            .crew_needs_you_notify(&asked)
            .unwrap()
            .claimed
            .is_empty()
    );
    // A wait with a later `since` is a new wait: it is announced once it is in the list again.
    assert!(
        w.core
            .store
            .claim_crew_needs_you_notice("task_needs_person", "t-1", "2026-11-01T00:00:00Z")
            .unwrap()
    );
    assert!(
        !w.core
            .store
            .claim_crew_needs_you_notice("task_needs_person", "t-1", "2026-11-01T00:00:00Z")
            .unwrap()
    );
    assert!(
        !w.core
            .store
            .claim_crew_needs_you_notice("task_needs_person", "t-1", "2026-10-01T00:00:00Z")
            .unwrap(),
        "an older wait is not announced again"
    );
    assert!(
        w.core
            .store
            .claim_crew_needs_you_notice("task_needs_person", "t-1", "2026-11-02T00:00:00Z")
            .unwrap(),
        "a later wait of the same item is announced again"
    );
}

#[tokio::test]
async fn too_many_items_in_one_request_are_refused() {
    let w = crew(arc(|_, _| Turn::Silent)).await;
    let many: Vec<NeedsYouRef> = (0..51)
        .map(|n| refer(NeedsYouKind::TaskNeedsPerson, &format!("sha256:{n}")))
        .collect();
    assert!(matches!(
        w.core.crew_needs_you_notify(&many),
        Err(CoreError::Invalid(_))
    ));
    assert!(w.core.crew_needs_you_notify(&many[..50]).is_ok());
}

/// The Autopilot digest's `needs_person` count is the one needs-you field, not a recount of its own merge records: a task
/// waiting on a person (no merge at all) and a merge whose break waits are both counted, a dismissal drops an item from
/// both, and the digest's quiet day follows the field.
#[tokio::test]
async fn the_autopilot_digest_needs_you_count_is_the_one_field() {
    let w = crew(arc(|n, root| match n {
        0 => built(root),
        _ => Turn::Silent,
    }))
    .await;
    let now = crate::util::unix_now();
    // Nothing waits yet: the digest is quiet and says nobody needs a person.
    let empty = w.core.autopilot_digest(now, Some(24)).unwrap();
    assert_eq!(empty.needs_person, 0);
    assert_eq!(w.core.crew_needs_you().unwrap().count, 0);
    assert!(empty.quiet, "nothing happened");

    // A passed task whose merge waits on a person, and a task whose worker never ran: two field items, one merge and one
    // task, so a count of merge records alone would undercount.
    let merged = w.built_work().await;
    let crew_id = w.task(&merged).crew_id;
    let mut record = crate::crew::fixtures::merge_record(&merged, "/repo", "main", "a break");
    record.crew_id = crew_id;
    record.review_state = "waiting".to_owned();
    record.needs_person = Some("The break needs a person.".to_owned());
    w.core.store.add_crew_merge(&record).unwrap();
    let (answer, error) = w
        .start(
            brief(json!({ "engine": { "id": "ghost", "assigned_by": "lead" } })),
            key(),
        )
        .await;
    assert!(!error, "{answer}");
    let could_not_start = answer["task"].as_str().unwrap().to_owned();

    let digest = w.core.autopilot_digest(now, Some(24)).unwrap();
    let field = w.core.crew_needs_you().unwrap();
    assert_eq!(digest.needs_person, 2, "a merge wait and a task wait");
    assert_eq!(
        digest.needs_person as usize, field.count as usize,
        "the digest counts the one field the Inbox answers from"
    );
    assert!(!digest.quiet, "a wait is something to say");

    // A dismissal hides the item in the field, so the digest's count follows it down.
    w.core
        .crew_needs_you_dismiss(NeedsYouKind::CouldNotStart, &could_not_start, "user")
        .unwrap();
    let digest = w.core.autopilot_digest(now, Some(24)).unwrap();
    assert_eq!(digest.needs_person, 1, "the dismissed task is not counted");
    assert_eq!(
        digest.needs_person as usize,
        w.core.crew_needs_you().unwrap().count as usize,
        "both read the same field after a dismissal"
    );
    assert!(
        !digest.quiet,
        "the merge wait is still a person's to answer"
    );
}

/// A command Autopilot held for the person is a wait of its own in the one field: it names the hold, not the task, and
/// deciding the hold drops it. The Inbox cannot dismiss it — only the Approvals tab decides a held command.
#[tokio::test]
async fn a_held_command_is_its_own_wait_and_a_decision_drops_it() {
    let w = crew(arc(|_, _| Turn::Silent)).await;
    let task = started_hanging(&w, "Hold a command.").await;
    let hold = crate::crew::fixtures::hold_command(
        &w.core,
        &task,
        std::path::Path::new("checkout"),
        "make deploy",
    );
    let item = item_of(&w, NeedsYouKind::CommandApproval, &hold);
    assert_eq!(
        item.reason.as_deref(),
        Some("A command waits for your approval.")
    );
    assert!(!item.dismissible, "only Approvals decides a held command");
    let crew_id = w.task(&task).crew_id;
    assert_eq!(item.crew_id.as_deref(), Some(crew_id.as_str()));
    assert!(matches!(
        w.core
            .crew_needs_you_dismiss(NeedsYouKind::CommandApproval, &hold, "user"),
        Err(CoreError::Invalid(_))
    ));
    // The task keeps working beside the hold, so the hold is not a task wait and does not take the task's place.
    assert!(
        w.core
            .crew_needs_you()
            .unwrap()
            .items
            .iter()
            .filter(|item| item.kind == NeedsYouKind::CommandApproval)
            .count()
            == 1
    );
    // The person dismisses the hold itself; the wait leaves the field.
    w.core.autopilot_dismiss_held(&hold).unwrap();
    assert!(
        items(&w)
            .iter()
            .all(|item| item.kind != NeedsYouKind::CommandApproval)
    );
}

/// A hold whose worker's task ended is not a wait: approving it would fail (the approve path finds no live worker and
/// closes the hold as over), so it must not be counted or announced. The field drops it on its own read, which sweeps
/// nothing and leaves the hold's book as it found it; a sweep closes the hold later.
#[tokio::test]
async fn a_held_command_for_an_ended_task_is_not_a_wait() {
    let w = crew(arc(|_, _| Turn::Silent)).await;
    let task = started_hanging(&w, "Hold a command.").await;
    let hold = crate::crew::fixtures::hold_command(
        &w.core,
        &task,
        std::path::Path::new("checkout"),
        "make deploy",
    );
    let found = || {
        items(&w)
            .into_iter()
            .find(|item| item.kind == NeedsYouKind::CommandApproval && item.id == hold)
    };
    // Its worker still runs the task, so the hold waits for the person.
    assert!(found().is_some(), "a live worker's hold is a wait");

    stop_task(&w, &task, "repeated_failure");
    assert!(found().is_none(), "a hold whose task ended is not a wait");
    // The read wrote nothing behind the item: no hold ended, and no ledger row says one did.
    assert!(
        w.core
            .store
            .crew_autopilot_events(&task)
            .unwrap()
            .iter()
            .all(|event| event.kind != "hold_ended"),
        "the needs-you read must not sweep the held book"
    );
}

/// A merge the queue holds for the person at the gate is its own kind, not the review wait of a merge that already
/// landed; the person's release moves the entry out of the gate and drops the wait.
#[tokio::test]
async fn a_merge_held_at_the_gate_is_its_own_wait_and_a_release_drops_it() {
    let w = crew(arc(|_, root| built(root))).await;
    let task = w.built_work().await;
    let crew_id = w.task(&task).crew_id;
    let mut entry =
        crate::crew::fixtures::merge_entry(&task, "/repo", "The merge is held at the gate.");
    entry.crew_id = crew_id.clone();
    w.core.store.add_merge_entry(&entry).unwrap();

    let item = item_of(&w, NeedsYouKind::MergeGateWaiting, &task);
    assert_eq!(
        item.reason.as_deref(),
        Some("A merge waits for your release.")
    );
    assert!(
        !item.dismissible,
        "the merge queue answers it, not the Inbox"
    );
    assert_eq!(item.crew_id.as_deref(), Some(crew_id.as_str()));
    assert!(
        items(&w)
            .iter()
            .all(|item| item.kind != NeedsYouKind::MergeWaiting),
        "a merge still at the gate has not landed and is not a review wait"
    );

    w.core.crew_release_merge(&task).unwrap();
    assert!(
        items(&w)
            .iter()
            .all(|item| item.kind != NeedsYouKind::MergeGateWaiting),
        "the release moves it out of the gate"
    );
}
