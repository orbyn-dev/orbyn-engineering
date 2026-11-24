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
async fn crew_with(
    w: &World,
    name: &str,
    trigger: Value,
    turns: u32,
    minutes: u32,
    on: bool,
) -> String {
    let body: wire::BackgroundCrewCreate = serde_json::from_value(json!({
        "project_id": project(w), "name": name, "template": "fix_failing_ci",
        "trigger": trigger, "budget": { "turns": turns, "minutes": minutes },
        "daily": { "runs": 100, "turns": 1000 }, "utc_offset_minutes": 600
    }))
    .unwrap();
    let crew = w.core.background_create(&body).await.unwrap();
    if on {
        w.core.background_set_enabled(&crew.id, true).unwrap();
    }
    crew.id
}

async fn crew(w: &World) -> String {
    crew_with(
        w,
        "Fix CI",
        json!({ "kind": "ci_failure", "branch": "main" }),
        10,
        30,
        true,
    )
    .await
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
    let error = w
        .core
        .crew_background_run_now("x", "user")
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::NotFound(_)), "{error}");
}

#[tokio::test]
async fn an_enabled_crew_starts_one_low_priority_run_with_a_lead_thread_of_its_own() {
    let w = world(true, good()).await;
    let id = crew(&w).await;
    let threads_before = threads(&w);
    let run = w.core.crew_background_run_now(&id, "user").await.unwrap();
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

    let run = w
        .core
        .crew_background_run_now(&crew(&w).await, "user")
        .await
        .unwrap();
    assert_eq!(
        w.core.store.crew_task_capacity(&task_of(&w, &run)).unwrap(),
        (Priority::Low, true)
    );
}

#[tokio::test]
async fn a_crew_that_is_off_is_refused_and_nothing_is_made() {
    let w = world(true, good()).await;
    let id = crew_with(&w, "Off", json!({ "kind": "ci_failure" }), 5, 5, false).await;
    let before = (threads(&w), mode_rows(&w));
    let error = w
        .core
        .crew_background_run_now(&id, "user")
        .await
        .unwrap_err();
    assert!(
        matches!(&error, CoreError::PolicyDenied(why) if why.contains("is off")),
        "{error}"
    );
    assert_eq!((threads(&w), mode_rows(&w)), before);
    let page = crate::crew::Page::new(None, None).unwrap();
    assert_eq!(w.core.background_runs(&id, page).unwrap().total, 0);
    assert!(
        w.core
            .crew_background_run_now("missing", "user")
            .await
            .is_err()
    );
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
    )
    .await;
    let bump = crew_with(
        &w,
        "Bump",
        json!({ "kind": "dependency_update" }),
        5,
        5,
        true,
    )
    .await;
    let watch = crew_with(
        &w,
        "Watch",
        json!({ "kind": "watch", "command": "cargo test", "every_hours": 6, "on_failing": "needs_you" }),
        5,
        5,
        true,
    )
    .await;
    let before = threads(&w);
    for id in [label, bump] {
        let error = w
            .core
            .crew_background_run_now(&id, "user")
            .await
            .unwrap_err();
        let CoreError::PolicyDenied(why) = &error else {
            panic!("{error}")
        };
        assert!(why.contains("cannot run yet"), "{why}");
    }
    // A watch runs its command on its own schedule; Run now does not start a run for it.
    let error = w
        .core
        .crew_background_run_now(&watch, "user")
        .await
        .unwrap_err();
    assert!(
        matches!(&error, CoreError::PolicyDenied(why) if why == "A watch crew's command runs on its schedule; Run now does not start a run for it."),
        "{error}"
    );
    assert_eq!(threads(&w), before);
}

#[tokio::test]
async fn a_second_run_of_the_same_crew_waits_for_the_first() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let id = crew(&w).await;
    w.core.crew_background_run_now(&id, "user").await.unwrap();
    let before = threads(&w);
    let error = w
        .core
        .crew_background_run_now(&id, "user")
        .await
        .unwrap_err();
    assert!(
        matches!(&error, CoreError::PolicyDenied(why) if why == "This crew already has a run in progress."),
        "{error}"
    );
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
    let id = crew(&w).await;
    waiting_merges(&w, 3).await;
    capacity(&w, 2, true);
    let before = (
        threads(&w),
        mode_rows(&w),
        w.core.store.crew_tasks_in_crew(&w.lead).unwrap().len(),
    );

    let error = w
        .core
        .crew_background_run_now(&id, "user")
        .await
        .unwrap_err();
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
    w.core
        .crew_background_run_now(&id, "user")
        .await
        .unwrap_err();
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
    let run = w.core.crew_background_run_now(&id, "user").await.unwrap();
    assert_eq!(run.state, wire::BackgroundRunState::Running);
    let rows = w.core.store.crew_paused_starts(&project(&w)).unwrap();
    assert!(rows.len() == 1 && rows[0].started, "{rows:?}");
}

#[tokio::test]
async fn pausing_is_off_until_a_person_turns_it_on() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    waiting_merges(&w, 3).await;
    capacity(&w, 1, false);
    let run = w
        .core
        .crew_background_run_now(&crew(&w).await, "user")
        .await
        .unwrap();
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
    let id = crew_with(&w, "One turn", json!({ "kind": "ci_failure" }), 1, 30, true).await;
    let run = w.core.crew_background_run_now(&id, "user").await.unwrap();
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
    let id = crew(&w).await;
    let run = w.core.crew_background_run_now(&id, "user").await.unwrap();
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
    let id = crew(&w).await;
    let run = w.core.crew_background_run_now(&id, "user").await.unwrap();
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
    )
    .await;
    w.core.crew_background_run_now(&id, "user").await.unwrap();
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
    let id = crew(&w).await;
    let run = w.core.crew_background_run_now(&id, "user").await.unwrap();
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
    let id = crew(&w).await;
    w.core
        .store
        .reader()
        .execute_batch("UPDATE projects SET trusted = 0")
        .unwrap();
    let before = threads(&w);
    let error = w
        .core
        .crew_background_run_now(&id, "user")
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::FolderUntrusted), "{error}");
    assert_eq!(threads(&w), before);
    let page = crate::crew::Page::new(None, None).unwrap();
    assert_eq!(w.core.background_runs(&id, page).unwrap().total, 0);
}

/// The disk guard (KC4 PR B): a background start it refuses keeps one stopped history row and no thread, the person still
/// gets the sentence, and the crew reads idle because it never ran. Faked free space lets this run on any machine.
#[tokio::test]
async fn a_background_start_refused_for_low_disk_keeps_one_stopped_history_row() {
    const GIB: u64 = 1024 * 1024 * 1024;
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let id = crew(&w).await;
    let path = w.repo.clone();
    let threads_before = threads(&w);
    crate::host::disk::set_free_for_test(&path, Some(7 * GIB / 2));

    let error = w
        .core
        .crew_background_run_now(&id, "user")
        .await
        .unwrap_err();
    let CoreError::PolicyDenied(why) = &error else {
        panic!("{error}");
    };
    assert_eq!(
        why,
        "Not started: 3.5 GB free, Crew needs 5 GB. Free space and try again."
    );
    // The run is the crew's history: stopped, the code and the sentence, nothing spent, nobody waiting, and no thread or
    // task left behind (the disk gate sits before either is stored).
    assert_eq!(threads(&w), threads_before);
    let page = crate::crew::Page::new(None, None).unwrap();
    let listed = w.core.background_runs(&id, page).unwrap();
    assert_eq!(listed.total, 1);
    let run = &listed.runs[0];
    assert!(w.core.store.crew_tasks_in_crew(&run.id).unwrap().is_empty());
    assert_eq!(run.state, wire::BackgroundRunState::Stopped);
    assert_eq!(run.stop_reason, None);
    assert_eq!(run.reason_code.as_deref(), Some("disk_low"));
    assert!(!run.review_waiting);
    assert_eq!(run.review_reason.as_deref(), Some(why.as_str()));
    assert_eq!((run.turns_spent, run.minutes_spent), (0, 0));
    assert_eq!((run.budget.turns, run.budget.minutes), (10, 30));
    assert!(run.ended_at.is_some());
    // The crew never ran, so it reads idle: the stopped row is only the history of the refused start.
    assert_eq!(
        w.core.background_get(&id).unwrap().state,
        wire::BackgroundState::Idle
    );
    // The refusal is audited with its code in the same transaction.
    let audits = w.core.store.list_audit(None, 500).unwrap();
    let refusal = audits
        .iter()
        .find(|row| row.action == "crew.background.run.refused")
        .expect("the refusal audit row");
    assert_eq!(refusal.detail["run"].as_str(), Some(run.id.as_str()));
    assert_eq!(refusal.detail["reason_code"], "disk_low");

    // A later start on a volume with room runs, and the stopped row stays in the history.
    crate::host::disk::set_free_for_test(&path, Some(20 * GIB));
    let started = w.core.crew_background_run_now(&id, "user").await.unwrap();
    assert_eq!(started.state, wire::BackgroundRunState::Running);
    assert_eq!(started.reason_code, None);
    let listed = w.core.background_runs(&id, page).unwrap();
    assert_eq!(listed.total, 2);
    assert_eq!(listed.runs[0].id, started.id);
    assert_eq!(listed.runs[1].id, run.id);
    crate::host::disk::set_free_for_test(&path, None);
}

/// The refusals that are not the disk guard still take the run and its lead thread back: a start that never happened
/// leaves no history row, and nothing changes about what the person could not start.
#[tokio::test]
async fn the_refusals_that_are_not_low_disk_still_leave_no_run_row() {
    let page = || crate::crew::Page::new(None, None).unwrap();

    // An untrusted folder is refused before a lead thread or a run row exists.
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let id = crew(&w).await;
    w.core
        .store
        .reader()
        .execute_batch("UPDATE projects SET trusted = 0")
        .unwrap();
    let threads_before = threads(&w);
    let error = w
        .core
        .crew_background_run_now(&id, "user")
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::FolderUntrusted), "{error}");
    assert_eq!(w.core.background_runs(&id, page()).unwrap().total, 0);
    assert_eq!(threads(&w), threads_before);

    // No usable engine is refused before a lead thread or a run row exists.
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let id = crew(&w).await;
    w.core
        .set_preferences(
            Some(&project(&w)),
            &BTreeMap::from([("disabled_engines".to_owned(), json!(["alpha"]))]),
        )
        .unwrap();
    let threads_before = threads(&w);
    let error = w
        .core
        .crew_background_run_now(&id, "user")
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::EngineUnavailable(_)), "{error}");
    assert_eq!(w.core.background_runs(&id, page()).unwrap().total, 0);
    assert_eq!(threads(&w), threads_before);

    // A start held by review capacity (a not-authorized refusal) reaches the run row, and takes it and the thread back.
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let id = crew(&w).await;
    waiting_merges(&w, 3).await;
    capacity(&w, 2, true);
    let threads_before = threads(&w);
    let error = w
        .core
        .crew_background_run_now(&id, "user")
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::PolicyDenied(_)), "{error}");
    assert_eq!(w.core.background_runs(&id, page()).unwrap().total, 0);
    assert_eq!(threads(&w), threads_before);
}

#[tokio::test]
async fn a_crew_with_no_engine_ready_is_refused_and_nothing_is_made() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let id = crew(&w).await;
    // The one engine is turned off for the project, the way a person does in settings; no usable one is left.
    w.core
        .set_preferences(
            Some(&project(&w)),
            &BTreeMap::from([("disabled_engines".to_owned(), json!(["alpha"]))]),
        )
        .unwrap();
    let before = threads(&w);
    let error = w
        .core
        .crew_background_run_now(&id, "user")
        .await
        .unwrap_err();
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
    let id = crew(&w).await;
    // An unborn HEAD: the checkout keeps its files but has nothing committed to list.
    run_git(&w.repo, &["checkout", "-q", "--orphan", "nothing"]);
    let before = threads(&w);
    let error = w
        .core
        .crew_background_run_now(&id, "user")
        .await
        .unwrap_err();
    let CoreError::Invalid(why) = &error else {
        panic!("{error}")
    };
    assert!(why.contains("no files committed yet"), "{why}");
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
    ];
    for (name, state, stop, words) in cases {
        let id = crew_with(&w, name, json!({ "kind": "ci_failure" }), 5, 5, true).await;
        let run = w.core.crew_background_run_now(&id, "user").await.unwrap();
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

/// A person's cancel of the task, through the real cancel, ends the run `needs_review` with the plain sentence.
#[tokio::test]
async fn a_cancelled_task_ends_its_run_through_the_real_cancel() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let id = crew(&w).await;
    let run = w.core.crew_background_run_now(&id, "user").await.unwrap();
    let task = task_of(&w, &run);
    w.core
        .crew_cancel_task(&task, Some("Cancelled by a test."), "user")
        .await
        .unwrap();
    let waiting = until_run(&w, &id, wire::BackgroundRunState::NeedsReview).await;
    assert!(
        waiting
            .review_reason
            .as_deref()
            .is_some_and(|why| why.contains("cancelled")),
        "{waiting:?}"
    );
}

/// The run's actor is who pressed the button.
#[tokio::test]
async fn the_audit_names_who_pressed_run_now() {
    let w = world(true, good()).await;
    let id = crew(&w).await;
    w.core.crew_background_run_now(&id, "user").await.unwrap();
    let rows = w.core.store.list_audit(None, 500).unwrap();
    let start = rows
        .iter()
        .find(|row| row.action == "crew.background.run.start")
        .expect("the start is audited");
    assert_eq!(start.actor, "user");
}

/// A removed project cannot be started in: the refusal is plain and nothing is made.
#[tokio::test]
async fn a_removed_project_is_refused_and_nothing_is_made() {
    let w = world(true, good()).await;
    let id = crew(&w).await;
    w.core
        .store
        .reader()
        .execute_batch("UPDATE projects SET removed_at = '2026-01-01T00:00:00Z'")
        .unwrap();
    let before = threads(&w);
    let error = w
        .core
        .crew_background_run_now(&id, "user")
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::ProjectRemoved(_)), "{error}");
    assert_eq!(threads(&w), before);
    let page = crate::crew::Page::new(None, None).unwrap();
    assert_eq!(w.core.background_runs(&id, page).unwrap().total, 0);
}

/// Turning the crew off stops the worker, not only the record.
#[tokio::test]
async fn turning_a_crew_off_cancels_the_worker_with_the_record() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let id = crew(&w).await;
    let run = w.core.crew_background_run_now(&id, "user").await.unwrap();
    let task = task_of(&w, &run);
    w.until("the worker is running", |w| {
        w.core
            .store
            .crew_run(&task)
            .ok()
            .flatten()
            .and_then(|r| r.worker_thread_id)
            .and_then(|t| w.core.store.thread(&t).ok())
            .is_some_and(|t| t.active_run.is_some())
    })
    .await;
    // A crew with a live worker cannot be deleted.
    assert!(w.core.background_delete(&id).is_err());

    let off = w.core.background_turn_off(&id).await.unwrap();
    assert!(!off.enabled);
    assert_eq!(w.task(&task).state, "cancelled");
    let worker = w
        .core
        .store
        .crew_run(&task)
        .unwrap()
        .unwrap()
        .worker_thread_id
        .unwrap();
    assert!(w.core.store.thread(&worker).unwrap().active_run.is_none());
    let stopped = run_of(&w, &id);
    assert_eq!(stopped.state, wire::BackgroundRunState::Stopped);
    assert_eq!(
        stopped.stop_reason,
        Some(wire::BackgroundStopReason::Disabled)
    );
    w.core.background_delete(&id).unwrap();
}

/// The lead thread of a run takes no message and is never retried into.
#[tokio::test]
async fn a_background_lead_takes_no_message_and_a_retry_never_prompts_it() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let id = crew(&w).await;
    let run = w.core.crew_background_run_now(&id, "user").await.unwrap();
    let error = w.core.runs.enqueue(&run.id, "hello", &[]).unwrap_err();
    assert!(
        matches!(&error, CoreError::Invalid(why) if why.contains("takes no messages")),
        "{error}"
    );
    let error = w.core.runs.admit(&run.id, "hello", "user").unwrap_err();
    assert!(
        matches!(&error, CoreError::Invalid(why) if why.contains("takes no messages")),
        "{error}"
    );
    let task = task_of(&w, &run);
    w.core
        .store
        .move_crew_task(&task, "needs_person", None)
        .unwrap();
    // The task derives as could-not-start, so the refusal is the background-lead guard and not the "did start" check.
    let facts = w
        .core
        .flow_facts(w.core.store.crew_task(&task).unwrap().unwrap())
        .unwrap();
    assert!(
        facts.could_not_start(),
        "the stored shape of a refused start"
    );
    let answer = w.core.crew_retry_elsewhere(&task, "user").await;
    assert!(
        matches!(&answer, Err(CoreError::CrewTaskBusy(why)) if why.contains("background run is not retried")),
        "{answer:?}"
    );
    assert!(w.sent_to(&run.id).is_empty());
    assert_ne!(w.task(&task).state, "cancelled");
}

/// A cancel that failed leaves a `disabled` run with a live task: turning the crew off again cancels it, and then the crew can go.
#[tokio::test]
async fn turning_off_again_cancels_a_task_left_after_a_failed_cancel() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let id = crew(&w).await;
    let run = w.core.crew_background_run_now(&id, "user").await.unwrap();
    let task = task_of(&w, &run);
    // The first press only ended the record, as when its cancel failed.
    w.core.background_set_enabled(&id, false).unwrap();
    assert_ne!(w.task(&task).state, "cancelled");
    assert!(w.core.background_delete(&id).is_err());
    w.core.background_turn_off(&id).await.unwrap();
    assert_eq!(w.task(&task).state, "cancelled");
    w.core.background_delete(&id).unwrap();
}

/// A stored row for the run's crew scope cannot widen what a background merge may do.
#[tokio::test]
async fn a_stored_full_mode_for_a_background_crew_is_ignored() {
    let w = world(true, good()).await;
    let id = crew(&w).await;
    let rows = mode_rows(&w);
    let run = w.core.crew_background_run_now(&id, "user").await.unwrap();
    assert_eq!(mode_rows(&w), rows, "a run stores no mode row");
    w.core
        .set_crew_autopilot_mode(&ModeScope::Crew(run.id.clone()), Mode::Full)
        .unwrap();
    assert_eq!(w.core.crew_autopilot_mode(&project(&w), &run.id), Mode::Off);
    // The settings neither list the stale row nor accept a new one.
    let listed = w.core.autopilot_modes().unwrap();
    assert!(
        listed
            .entries
            .iter()
            .all(|entry| entry.id.as_deref() != Some(run.id.as_str())),
        "{listed:?}"
    );
    let change: wire::AutopilotModeChange =
        serde_json::from_value(json!({ "scope": "crew", "id": run.id, "mode": "full" })).unwrap();
    let error = w.core.autopilot_set_mode(&change).unwrap_err();
    assert!(
        matches!(&error, CoreError::PolicyDenied(why) if why.contains("always wait for a person")),
        "{error}"
    );
}

#[tokio::test]
async fn a_bad_branch_name_is_refused_and_nothing_is_made() {
    let w = world(true, good()).await;
    let bad_branch = crew_with(
        &w,
        "Branchy",
        json!({ "kind": "ci_failure", "branch": "bad branch;rm" }),
        5,
        5,
        true,
    )
    .await;
    let before = threads(&w);
    let error = w
        .core
        .crew_background_run_now(&bad_branch, "user")
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::Invalid(_)), "{error}");
    assert_eq!(threads(&w), before);
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

    // The background run's merge is held as waiting, under the mode `off` the code gives its crew.
    let id = crew(&w).await;
    let run = w.core.crew_background_run_now(&id, "user").await.unwrap();
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

#[tokio::test]
async fn a_post_row_launch_refusal_keeps_its_task_and_lead_reviewable() {
    let w = world(true, good()).await;
    let id = crew(&w).await;
    // Use an existing tracked file regardless of the harness fixture's names.
    let listed = gitq(&w.repo, &["ls-files"]).await;
    let path = listed.lines().next().unwrap();
    run_git(&w.repo, &["update-index", "--assume-unchanged", path]);
    let before = (threads(&w), mode_rows(&w));
    // A refusal after the task exists keeps the run for review and returns it, like a route refusal does.
    let returned = w.core.crew_background_run_now(&id, "user").await.unwrap();
    assert!(
        returned
            .review_reason
            .as_deref()
            .is_some_and(|why| why.contains("git index")),
        "{returned:?}"
    );
    let run = run_of(&w, &id);
    assert_eq!(returned.id, run.id);
    assert_eq!(run.state, wire::BackgroundRunState::NeedsReview);
    assert!(run.review_waiting);
    let task = task_of(&w, &run);
    assert_eq!(
        w.core.store.crew_task(&task).unwrap().unwrap().state,
        "needs_person"
    );
    // The task was moved to waiting by the start itself, which wrote no result: the unwind writes exactly one.
    let results = w.core.store.crew_results(&task).unwrap();
    assert_eq!(results.len(), 1, "{results:?}");
    assert_eq!(results[0].state, wire::ResultState::NeedsPerson);
    assert!(w.core.store.thread(&run.id).is_ok());
    assert_eq!((threads(&w), mode_rows(&w)), (before.0 + 1, before.1));
    let saved = w.core.store.background_run(&run.id).unwrap();
    w.core
        .store
        .unwind_background_run(&run.id, "retry")
        .unwrap();
    assert_eq!(w.core.store.background_run(&run.id).unwrap(), saved);
    let refusals: i64 = w.core.store.reader().query_row(
        "SELECT COUNT(*) FROM audit_events WHERE action = 'crew.background.run.refused' AND json_extract(detail, '$.run') = ?1",
        [&run.id], |row| row.get(0),
    ).unwrap();
    assert_eq!(refusals, 1);
    assert!(w.sent_to(&run.id).is_empty());
}

#[tokio::test]
async fn a_failed_refusal_transaction_preserves_the_lead_and_reports_the_store_error() {
    let w = world(true, good()).await;
    let id = crew(&w).await;
    let listed = gitq(&w.repo, &["ls-files"]).await;
    run_git(
        &w.repo,
        &[
            "update-index",
            "--assume-unchanged",
            listed.lines().next().unwrap(),
        ],
    );
    rusqlite::Connection::open(w._dir.path().join("data/orbyn.sqlite3")).unwrap().execute_batch(
        "CREATE TRIGGER refuse_unwind BEFORE INSERT ON audit_events WHEN NEW.action = 'crew.background.run.refused' BEGIN SELECT RAISE(ABORT, 'refusal audit unavailable'); END;",
    ).unwrap();
    let error = w
        .core
        .crew_background_run_now(&id, "user")
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("refusal audit unavailable"),
        "{error}"
    );
    let run = run_of(&w, &id);
    assert_eq!(run.state, wire::BackgroundRunState::Running);
    assert!(w.core.store.thread(&run.id).is_ok());
    assert_eq!(w.core.crew_autopilot_mode(&project(&w), &run.id), Mode::Off);
    assert_eq!(w.core.store.crew_tasks_in_crew(&run.id).unwrap().len(), 1);
}

fn refusals(w: &World, run: &str) -> i64 {
    w.core.store.reader().query_row(
        "SELECT COUNT(*) FROM audit_events WHERE action = 'crew.background.run.refused' AND json_extract(detail, '$.run') = ?1",
        [run], |row| row.get(0),
    ).unwrap()
}

#[tokio::test]
async fn a_start_refused_while_the_task_is_briefed_holds_the_task_through_the_real_start() {
    let w = world(true, good()).await;
    let id = crew(&w).await;
    // The run row cannot be stored, so the start fails after the task exists and while it is still `briefed`.
    rusqlite::Connection::open(w._dir.path().join("data/orbyn.sqlite3")).unwrap().execute_batch(
        "CREATE TRIGGER no_crew_run BEFORE INSERT ON crew_runs BEGIN SELECT RAISE(ABORT, 'no crew run'); END;",
    ).unwrap();
    let before = (threads(&w), mode_rows(&w));
    let returned = w.core.crew_background_run_now(&id, "user").await.unwrap();
    let run = run_of(&w, &id);
    assert_eq!(returned.id, run.id);
    assert_eq!(run.state, wire::BackgroundRunState::NeedsReview);
    assert!(run.review_waiting);
    // The task has no run row (that insert is what failed), so read it by its crew, not through the run join.
    let states: Vec<String> = {
        let conn = w.core.store.reader();
        let mut statement = conn
            .prepare("SELECT state FROM crew_tasks WHERE crew_id = ?1")
            .unwrap();
        statement
            .query_map([&run.id], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert_eq!(states, ["needs_person"]);
    assert!(w.core.store.thread(&run.id).is_ok());
    assert_eq!((threads(&w), mode_rows(&w)), (before.0 + 1, before.1));
    assert_eq!(refusals(&w, &run.id), 1);
    assert!(w.sent_to(&run.id).is_empty());
}

#[tokio::test]
async fn a_crash_between_the_refusal_and_the_cleanup_leaves_nothing_partial() {
    let w = world(true, good()).await;
    let id = crew(&w).await;
    let crew_row = w.core.background_row(&id).unwrap();
    let thread = w
        .core
        .store
        .create_thread(crate::store::NewThread {
            project_id: project(&w),
            title: "Background crew: crash".into(),
            engine: "claude".into(),
            model: None,
            cwd: w.repo.to_string_lossy().into_owned(),
            worktree: None,
            skill_profile: None,
            parent_thread_id: None,
            relation: None,
            scratch: false,
        })
        .unwrap();
    w.core
        .store
        .start_background_run_as(&crew_row.id, &thread.id, "user")
        .unwrap();
    let before = (threads(&w), mode_rows(&w));
    // The last step of the cleanup fails: the refusal audit and every earlier delete must go with it.
    let connection = rusqlite::Connection::open(w._dir.path().join("data/orbyn.sqlite3")).unwrap();
    connection.execute_batch(
        "CREATE TRIGGER no_thread_delete BEFORE DELETE ON threads BEGIN SELECT RAISE(ABORT, 'thread delete unavailable'); END;",
    ).unwrap();
    assert!(
        w.core
            .store
            .unwind_background_run(&thread.id, "refused")
            .is_err()
    );
    assert_eq!(
        w.core
            .store
            .background_run(&thread.id)
            .unwrap()
            .unwrap()
            .state,
        "running"
    );
    assert_eq!((threads(&w), mode_rows(&w)), before);
    assert_eq!(refusals(&w, &thread.id), 0);
    // With the fault gone the same call finishes everything in one commit.
    connection
        .execute_batch("DROP TRIGGER no_thread_delete")
        .unwrap();
    let unwound = w
        .core
        .store
        .unwind_background_run(&thread.id, "refused")
        .unwrap();
    assert!(unwound.discarded);
    assert!(w.core.store.background_run(&thread.id).unwrap().is_none());
    assert_eq!((threads(&w), mode_rows(&w)), (before.0 - 1, before.1));
    assert_eq!(refusals(&w, &thread.id), 1);
    assert!(
        !w.core
            .store
            .unwind_background_run(&thread.id, "again")
            .unwrap()
            .discarded
    );
}

#[tokio::test]
async fn a_held_task_gives_up_its_leases() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let id = crew(&w).await;
    // The lead thread of this harness stands for a run's lead: the task started under it has a lease and a worker.
    w.core
        .store
        .start_background_run_as(&id, &w.lead, "user")
        .unwrap();
    let (answer, error) = w
        .start(
            brief(json!({ "files": [{ "path": "src/**", "mode": "write" }] })),
            json!({ "checks": [{ "id": "chk-ok", "kind": "command", "params": { "run": "echo background-lease-1" }, "proves": "It runs." }],
                    "protected_paths": ["tests/**"] }),
        )
        .await;
    assert!(!error, "{answer}");
    let task = answer["task"].as_str().unwrap().to_owned();
    let live = |w: &World| {
        w.core
            .live_crew_leases(&w.lead, crate::util::unix_now())
            .unwrap()
            .len()
    };
    assert_eq!(live(&w), 1, "the running task holds its lease");

    let held = w
        .core
        .unwind_background_start(&w.lead, CoreError::Invalid("start refused".into()))
        .unwrap();
    assert_eq!(held.state, wire::BackgroundRunState::NeedsReview);
    assert_eq!(w.task(&task).state, "needs_person");
    assert_eq!(live(&w), 0, "the held task released its lease");
    let results = w.core.store.crew_results(&task).unwrap();
    assert_eq!(results.len(), 1, "{results:?}");
}
