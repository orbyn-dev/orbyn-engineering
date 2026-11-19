//! Starting a run of a background crew by hand (docs/crew/background-engine-start.md, PR 1): the run, its lead thread, the Low-priority
//! start that review capacity can pause, the budget, the run-end contract and the merge that waits for a person. A real `Core`, fake
//! engines and real git; no trigger listener exists, so every run here is started by calling the function a person's button calls.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use orbyn_contracts as wire;
use serde_json::{Value, json};

use super::autopilot_merge::{gitq, main_branch};
use super::harness::{Script, Turn, World, brief, report, world};
use super::review_support::commit_all;
use crate::crew::capacity::{CapacitySettings, PausedKind, Priority, Sampling};
use crate::crew::fixtures::merge_record;
use crate::crew::{Mode, ModeScope};
use crate::error::CoreError;
use crate::testing::run_git;

const NOW: i64 = 1_800_000_000;

fn project(w: &World) -> String {
    w.core.store.thread(&w.lead).unwrap().project_id
}

/// A stored crew, on or off. `trigger` is the wire shape of its trigger.
fn crew_with(w: &World, name: &str, trigger: Value, turns: u32, minutes: u32, on: bool) -> String {
    let body: wire::BackgroundCrewCreate = serde_json::from_value(json!({
        "project_id": project(w), "name": name, "template": "fix_failing_ci",
        "trigger": trigger, "budget": { "turns": turns, "minutes": minutes }
    }))
    .unwrap();
    let crew = w.core.background_create(&body).unwrap();
    if on {
        w.core.background_set_enabled(&crew.id, true).unwrap();
    }
    crew.id
}

fn crew(w: &World) -> String {
    crew_with(
        w,
        "Fix CI",
        json!({ "kind": "ci_failure", "branch": "main" }),
        10,
        30,
        true,
    )
}

/// A worker that changes a file it may change and says so, as a real engine does.
fn good() -> Script {
    Arc::new(|n, root| {
        std::fs::write(root.join(format!("src/fix-{n}.txt")), "fixed\n").unwrap();
        Turn::Say(report("done"))
    })
}

/// The task a run started: the one task of its lead thread.
fn task_of(w: &World, run: &wire::BackgroundRun) -> String {
    let tasks = w.core.store.crew_tasks_in_crew(&run.id).unwrap();
    assert_eq!(tasks.len(), 1, "one task per run");
    tasks[0].0.id.clone()
}

fn run_of(w: &World, crew: &str) -> wire::BackgroundRun {
    let page = crate::crew::Page::new(None, None).unwrap();
    w.core.background_runs(crew, page).unwrap().runs.remove(0)
}

fn threads(w: &World) -> i64 {
    w.core
        .store
        .reader()
        .query_row("SELECT COUNT(*) FROM threads", [], |row| row.get(0))
        .unwrap()
}

fn mode_rows(w: &World) -> usize {
    w.core.store.crew_autopilot_modes_all(100).unwrap().len()
}

async fn until_run(w: &World, crew: &str, state: wire::BackgroundRunState) -> wire::BackgroundRun {
    w.until("the run reaches its state", |w| {
        run_of(w, crew).state == state
    })
    .await;
    run_of(w, crew)
}

#[tokio::test]
async fn with_the_flag_off_nothing_starts() {
    let w = world(false, good()).await;
    let error = w.core.crew_background_run_now("x").await.unwrap_err();
    assert!(matches!(error, CoreError::NotFound(_)), "{error}");
}

#[tokio::test]
async fn an_enabled_crew_starts_one_low_priority_run_with_a_lead_thread_of_its_own() {
    let w = world(true, good()).await;
    let id = crew(&w);
    let threads_before = threads(&w);
    let run = w.core.crew_background_run_now(&id).await.unwrap();
    assert_eq!(run.crew_id, id);
    let task = task_of(&w, &run);

    // The run is its lead thread: one thread, named for the crew, on the project's checkout, and nobody ever prompts it.
    assert_eq!(
        run.id,
        w.core.store.crew_task(&task).unwrap().unwrap().crew_id
    );
    let lead = w.core.store.thread(&run.id).unwrap();
    assert_eq!(lead.title, "Background crew: Fix CI");
    assert_eq!(lead.project_id, project(&w));
    assert_ne!(lead.id, w.lead);

    // Crew began it, so it is Low and can be paused; the crew's budget is the brief's.
    assert_eq!(
        w.core.store.crew_task_capacity(&task).unwrap(),
        (Priority::Low, true)
    );
    let brief = w.core.store.crew_brief(&task).unwrap().unwrap();
    assert_eq!((brief.budget.turns, brief.budget.minutes), (10, 30));
    assert_eq!(w.core.store.crew_route_decisions(&task).unwrap().len(), 1);

    // The worker passes; the run waits for a person, with what it spent. The lead thread was never told.
    w.until_state(&task, "passed").await;
    let done = until_run(&w, &id, wire::BackgroundRunState::NeedsReview).await;
    assert_eq!((done.turns_spent, done.review_waiting), (1, true));
    assert!(
        w.sent_to(&run.id).is_empty(),
        "a background lead thread runs no engine"
    );
    assert!(w.core.store.thread(&run.id).unwrap().active_run.is_none());
    // One lead thread and one worker thread were made, nothing else.
    assert_eq!(threads(&w), threads_before + 2);
    let crew = w.core.background_get(&id).unwrap();
    assert_eq!(
        (crew.state, crew.reviews_waiting),
        (wire::BackgroundState::NeedsReview, 1)
    );
}

#[tokio::test]
async fn a_persons_start_is_not_low_and_never_paused() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (answer, error) = w
        .start(
            brief(json!({ "files": [{ "path": "src/a/**", "mode": "write" }] })),
            super::harness::key(),
        )
        .await;
    assert!(!error, "{answer}");
    let person = answer["task"].as_str().unwrap();
    assert_eq!(
        w.core.store.crew_task_capacity(person).unwrap(),
        (Priority::Medium, false)
    );

    let run = w.core.crew_background_run_now(&crew(&w)).await.unwrap();
    assert_eq!(
        w.core.store.crew_task_capacity(&task_of(&w, &run)).unwrap(),
        (Priority::Low, true)
    );
}

#[tokio::test]
async fn a_crew_that_is_off_is_refused_and_nothing_is_made() {
    let w = world(true, good()).await;
    let id = crew_with(&w, "Off", json!({ "kind": "ci_failure" }), 5, 5, false);
    let before = (threads(&w), mode_rows(&w));
    let error = w.core.crew_background_run_now(&id).await.unwrap_err();
    assert!(
        matches!(&error, CoreError::PolicyDenied(why) if why.contains("is off")),
        "{error}"
    );
    assert_eq!((threads(&w), mode_rows(&w)), before);
    let page = crate::crew::Page::new(None, None).unwrap();
    assert_eq!(w.core.background_runs(&id, page).unwrap().total, 0);
    assert!(w.core.crew_background_run_now("missing").await.is_err());
}

#[tokio::test]
async fn a_kind_nothing_can_start_is_refused_with_a_plain_reason() {
    let w = world(true, good()).await;
    let label = crew_with(
        &w,
        "Label",
        json!({ "kind": "issue_label", "label": "bug" }),
        5,
        5,
        true,
    );
    let bump = crew_with(
        &w,
        "Bump",
        json!({ "kind": "dependency_update" }),
        5,
        5,
        true,
    );
    let before = threads(&w);
    for (id, words) in [(label, "issue label"), (bump, "dependency update")] {
        let error = w.core.crew_background_run_now(&id).await.unwrap_err();
        let CoreError::PolicyDenied(why) = &error else {
            panic!("{error}")
        };
        assert!(
            why.contains(words) && why.contains("cannot be started yet"),
            "{why}"
        );
    }
    assert_eq!(threads(&w), before);
}

#[tokio::test]
async fn a_second_run_of_the_same_crew_waits_for_the_first() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let id = crew(&w);
    w.core.crew_background_run_now(&id).await.unwrap();
    let before = threads(&w);
    let error = w.core.crew_background_run_now(&id).await.unwrap_err();
    assert!(matches!(error, CoreError::RunsActive), "{error}");
    assert_eq!(threads(&w), before);
}

async fn waiting_merges(w: &World, n: usize) {
    for i in 0..n {
        let (answer, error) = w
            .start(
                brief(json!({ "files": [{ "path": format!("src/w{i}/**"), "mode": "write" }], "outcome": format!("Task {i}.") })),
                super::harness::key(),
            )
            .await;
        assert!(!error, "{answer}");
        let task = answer["task"].as_str().unwrap();
        let mut merge = merge_record(task, "/repo", "main", "x");
        merge.risk = "medium".into();
        merge.crew_id = w.task(task).crew_id;
        w.core.store.add_crew_merge(&merge).unwrap();
    }
}

fn capacity(w: &World, limit: u32, pause: bool) {
    w.core
        .store
        .set_crew_capacity_settings(
            &project(w),
            CapacitySettings {
                limit,
                sampling: Sampling::Off,
                pause_enabled: pause,
            },
        )
        .unwrap();
}

#[tokio::test]
async fn review_capacity_pauses_a_background_start_and_leaves_nothing_behind() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let id = crew(&w);
    waiting_merges(&w, 3).await;
    capacity(&w, 2, true);
    let before = (
        threads(&w),
        mode_rows(&w),
        w.core.store.crew_tasks_in_crew(&w.lead).unwrap().len(),
    );

    let error = w.core.crew_background_run_now(&id).await.unwrap_err();
    let CoreError::PolicyDenied(why) = &error else {
        panic!("{error}")
    };
    assert!(
        why.starts_with("review capacity:") && why.contains("3 merges wait"),
        "{why}"
    );
    // No run, no thread, no pinned mode: the refusal stored nothing but the one held row.
    assert_eq!(
        (
            threads(&w),
            mode_rows(&w),
            w.core.store.crew_tasks_in_crew(&w.lead).unwrap().len()
        ),
        before
    );
    let page = crate::crew::Page::new(None, None).unwrap();
    assert_eq!(w.core.background_runs(&id, page).unwrap().total, 0);
    let rows = w.core.store.crew_paused_starts(&project(&w)).unwrap();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(
        (
            rows[0].kind,
            rows[0].waiting,
            rows[0].limit,
            rows[0].started
        ),
        (PausedKind::Background, 3, 2, false)
    );
    // A second press is the same held row, not another.
    w.core.crew_background_run_now(&id).await.unwrap_err();
    assert_eq!(
        w.core.store.crew_paused_starts(&project(&w)).unwrap().len(),
        1
    );

    // Once the waiting merges are reviewed, the same press starts the run and the held row reads as started.
    w.core
        .store
        .reader()
        .execute_batch(
            "UPDATE crew_merges SET review_state = 'looks_good' WHERE review_state = 'waiting'",
        )
        .unwrap();
    let run = w.core.crew_background_run_now(&id).await.unwrap();
    assert_eq!(run.state, wire::BackgroundRunState::Running);
    let rows = w.core.store.crew_paused_starts(&project(&w)).unwrap();
    assert!(rows.len() == 1 && rows[0].started, "{rows:?}");
}

#[tokio::test]
async fn pausing_is_off_until_a_person_turns_it_on() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    waiting_merges(&w, 3).await;
    capacity(&w, 1, false);
    let run = w.core.crew_background_run_now(&crew(&w)).await.unwrap();
    assert_eq!(run.state, wire::BackgroundRunState::Running);
}

/// A worker that never reaches the files it may change, so verification fails each turn.
fn outside() -> Script {
    // The same file every turn, so the failure repeats exactly and the loop guard sees it.
    Arc::new(|_, root| {
        std::fs::write(root.join("outside.txt"), "no\n").unwrap();
        Turn::Say(report("done"))
    })
}

#[tokio::test]
async fn a_run_stops_for_its_budget_when_its_last_turn_did_not_pass() {
    let w = world(true, outside()).await;
    let id = crew_with(&w, "One turn", json!({ "kind": "ci_failure" }), 1, 30, true);
    let run = w.core.crew_background_run_now(&id).await.unwrap();
    let stopped = until_run(&w, &id, wire::BackgroundRunState::Stopped).await;
    assert_eq!(
        stopped.stop_reason,
        Some(wire::BackgroundStopReason::Budget)
    );
    assert_eq!(
        (stopped.turns_spent, stopped.review_waiting),
        (1, true),
        "a budget stop waits for review"
    );
    let task = task_of(&w, &run);
    assert_eq!(w.task(&task).state, "stopped");
    assert_eq!(
        w.core.store.crew_worker_turns(&task).unwrap(),
        1,
        "no second turn was started"
    );
}

#[tokio::test]
async fn the_time_budget_cancels_a_running_worker_and_the_run_records_it() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let id = crew(&w);
    let run = w.core.crew_background_run_now(&id).await.unwrap();
    let task = task_of(&w, &run);
    // What the minute timer calls when the budget runs out.
    w.core.crew_budget_ended(&task);
    let stopped = until_run(&w, &id, wire::BackgroundRunState::Stopped).await;
    assert_eq!(
        stopped.stop_reason,
        Some(wire::BackgroundStopReason::Budget)
    );
    assert!(stopped.review_waiting);
    assert_eq!(w.task(&task).state, "stopped");
}

#[tokio::test]
async fn a_worker_turn_with_no_reply_is_an_engine_error_not_a_finished_run() {
    let w = world(true, Arc::new(|_, _| Turn::Silent)).await;
    let id = crew(&w);
    let run = w.core.crew_background_run_now(&id).await.unwrap();
    let stopped = until_run(&w, &id, wire::BackgroundRunState::Stopped).await;
    assert_eq!(
        stopped.stop_reason,
        Some(wire::BackgroundStopReason::EngineError)
    );
    assert!(
        !stopped.review_waiting,
        "an engine error left nothing to review"
    );
    assert_eq!(w.task(&task_of(&w, &run)).state, "stopped");
    assert_eq!(
        w.core.background_get(&id).unwrap().state,
        wire::BackgroundState::Stopped
    );
}

#[tokio::test]
async fn the_same_failure_again_is_a_repeated_failure_stop() {
    let w = world(true, outside()).await;
    let id = crew_with(
        &w,
        "Stubborn",
        json!({ "kind": "ci_failure" }),
        20,
        30,
        true,
    );
    w.core.crew_background_run_now(&id).await.unwrap();
    let stopped = until_run(&w, &id, wire::BackgroundRunState::Stopped).await;
    assert_eq!(
        stopped.stop_reason,
        Some(wire::BackgroundStopReason::RepeatedFailure)
    );
    assert!(stopped.turns_spent >= 2, "{stopped:?}");
}

#[tokio::test]
async fn a_task_that_waits_for_a_person_ends_the_run_as_needs_review() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let id = crew(&w);
    let run = w.core.crew_background_run_now(&id).await.unwrap();
    let task = task_of(&w, &run);
    w.core
        .crew_needs_person(&task, "No check could decide the task.", false);
    let waiting = until_run(&w, &id, wire::BackgroundRunState::NeedsReview).await;
    assert_eq!(
        waiting.review_reason.as_deref(),
        Some("No check could decide the task.")
    );
    assert!(waiting.review_waiting);
}

/// A worker that commits its change on a side branch from `main`, as a real one does.
fn committing(main: Arc<OnceLock<String>>) -> Script {
    Arc::new(move |n, root: &Path| {
        run_git(root, &["checkout", "-q", main.get().unwrap()]);
        run_git(root, &["checkout", "-q", "-b", &format!("work-{n}")]);
        std::fs::write(root.join(format!("src/feat-{n}.txt")), "feature\n").unwrap();
        commit_all(root, &format!("work {n}"));
        Turn::Say(report("done"))
    })
}

#[tokio::test]
async fn a_project_that_is_not_trusted_is_refused_and_nothing_is_made() {
    let w = world(true, good()).await;
    let id = crew(&w);
    w.core
        .store
        .reader()
        .execute_batch("UPDATE projects SET trusted = 0")
        .unwrap();
    let before = threads(&w);
    let error = w.core.crew_background_run_now(&id).await.unwrap_err();
    assert!(matches!(error, CoreError::FolderUntrusted), "{error}");
    assert_eq!(threads(&w), before);
    let page = crate::crew::Page::new(None, None).unwrap();
    assert_eq!(w.core.background_runs(&id, page).unwrap().total, 0);
}

#[tokio::test]
async fn a_crew_with_no_engine_ready_is_refused_and_nothing_is_made() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let id = crew(&w);
    // The one engine is turned off for the project, the way a person does in settings; no usable one is left.
    w.core
        .set_preferences(
            Some(&project(&w)),
            &BTreeMap::from([("disabled_engines".to_owned(), json!(["alpha"]))]),
        )
        .unwrap();
    let before = threads(&w);
    let error = w.core.crew_background_run_now(&id).await.unwrap_err();
    let CoreError::EngineUnavailable(why) = &error else {
        panic!("{error}")
    };
    assert!(why.starts_with("No engine is ready"), "{why}");
    assert_eq!(threads(&w), before);
    let page = crate::crew::Page::new(None, None).unwrap();
    assert_eq!(w.core.background_runs(&id, page).unwrap().total, 0);
}

#[tokio::test]
async fn a_project_with_nothing_committed_is_refused_with_a_plain_reason() {
    let w = world(true, good()).await;
    let id = crew(&w);
    // An unborn HEAD: the checkout keeps its files but has nothing committed to list.
    run_git(&w.repo, &["checkout", "-q", "--orphan", "nothing"]);
    let before = threads(&w);
    let error = w.core.crew_background_run_now(&id).await.unwrap_err();
    let CoreError::Invalid(why) = &error else {
        panic!("{error}")
    };
    assert!(why.contains("nothing committed yet"), "{why}");
    assert_eq!(threads(&w), before);
    let page = crate::crew::Page::new(None, None).unwrap();
    assert_eq!(w.core.background_runs(&id, page).unwrap().total, 0);
}

/// The task ends that carry no stored stop reason end the run `needs_review` with a plain sentence a person can read.
#[tokio::test]
async fn task_ends_without_a_stored_reason_wait_with_their_plain_sentence() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let cases = [
        (
            "Denials",
            wire::ResultState::Stopped,
            Some(wire::StopReason::TooManyDenials),
            "too many refused actions",
        ),
        (
            "Injection",
            wire::ResultState::Stopped,
            Some(wire::StopReason::InjectionSuspected),
            "tried to give it orders",
        ),
        ("Cancelled", wire::ResultState::Cancelled, None, "cancelled"),
    ];
    for (name, state, stop, words) in cases {
        let id = crew_with(&w, name, json!({ "kind": "ci_failure" }), 5, 5, true);
        let run = w.core.crew_background_run_now(&id).await.unwrap();
        let task = w.core.store.crew_task(&task_of(&w, &run)).unwrap().unwrap();
        w.core.crew_background_task_ended(&task, state, stop);
        let waiting = until_run(&w, &id, wire::BackgroundRunState::NeedsReview).await;
        assert!(
            waiting
                .review_reason
                .as_deref()
                .is_some_and(|why| why.contains(words)),
            "{name}: {waiting:?}"
        );
        assert!(waiting.review_waiting, "{name}");
    }
}

#[tokio::test]
async fn a_background_merge_waits_for_a_person_even_when_autopilot_is_full() {
    let cell = Arc::new(OnceLock::new());
    let w = world(true, committing(cell.clone())).await;
    let main = main_branch(&w.repo).await;
    cell.set(main.clone()).unwrap();
    // The repository's choice is the widest there is.
    w.core
        .set_crew_autopilot_mode(&ModeScope::Default, Mode::Full)
        .unwrap();
    let target = crate::crew::MergeTarget {
        repo: w.repo.clone(),
        branch: main.clone(),
        owed: Some(Vec::new()),
    };

    // Control: a person's task under the same mode is queued to land by itself.
    let (answer, error) = w
        .start(
            brief(json!({ "files": [{ "path": "src/**", "mode": "write" }] })),
            json!({ "checks": [{ "id": "chk-ok", "kind": "command", "params": { "run": "echo background-control-1" }, "proves": "It runs." }],
                    "protected_paths": ["tests/**"] }),
        )
        .await;
    assert!(!error, "{answer}");
    let person = answer["task"].as_str().unwrap().to_owned();
    w.until_state(&person, "passed").await;
    let queued = w
        .core
        .crew_queue_merge(&person, &target, NOW)
        .await
        .unwrap();
    assert_eq!(queued.state, "queued", "{queued:?}");

    // The background run's merge is held as waiting, under the mode `off` pinned to its own crew.
    let id = crew(&w);
    let run = w.core.crew_background_run_now(&id).await.unwrap();
    assert_eq!(w.core.crew_autopilot_mode(&project(&w), &run.id), Mode::Off);
    let task = task_of(&w, &run);
    w.until_state(&task, "passed").await;
    let tip = gitq(&w.repo, &["rev-parse", &main]).await;
    let entry = w
        .core
        .crew_queue_merge(&task, &target, NOW + 1)
        .await
        .unwrap();
    assert_eq!(
        (entry.state.as_str(), entry.mode.as_str()),
        ("waiting", "off"),
        "{entry:?}"
    );
    assert!(entry.note.is_some());
    // Only the person's merge is processed; the background one stays where it is until a person lets it go.
    let outcomes = w.core.crew_process_merges(&target, NOW + 2).await.unwrap();
    assert_eq!(outcomes.len(), 1, "{outcomes:?}");
    assert_eq!(
        w.core.store.merge_entry(&task).unwrap().unwrap().state,
        "waiting"
    );
    assert_ne!(
        gitq(&w.repo, &["rev-parse", &main]).await,
        tip,
        "the person's merge landed"
    );
}
