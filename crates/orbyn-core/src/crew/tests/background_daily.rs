//! The daily per-engine budget of a background crew (docs/crew/background-engine-start.md, slice 4h, PR 2): turns counted from
//! engine events, a run cancelled at the cap with what it spent shown, a new event refused with the time the day turns and never
//! queued, and the day itself reset at the person's midnight. A real `Core`, one fake engine and real git; every run is started by
//! calling what a person's Run now calls.

use std::sync::Arc;

use orbyn_contracts as wire;
use serde_json::{Value, json};

use super::harness::{Script, Turn, World, report, world};
use crate::crew::background_daily::local_day_and_reset;
use crate::error::CoreError;

/// The offset the test crews are made with: UTC+10, so the day and its reset are visibly the person's, not UTC's.
const OFFSET: i32 = 600;

fn project(w: &World) -> String {
    w.core.store.thread(&w.lead).unwrap().project_id
}

/// The one row a read returned, unpacked by pattern so a test never reads a place in the list: a read that returns
/// none or several fails here with the rows attached.
fn one<'a, T: std::fmt::Debug>(rows: &'a [T], what: &str) -> &'a T {
    match rows {
        [row] => row,
        [] => panic!("no {what}: {rows:?}"),
        _ => panic!("{} {what}s where one is expected: {rows:?}", rows.len()),
    }
}

/// A stored crew with a daily budget, on. The run budget is wide enough that only the daily one can stop a run here.
async fn crew_daily(w: &World, name: &str, runs: u32, turns: u32) -> String {
    let body: wire::BackgroundCrewCreate = serde_json::from_value(json!({
        "project_id": project(w), "name": name, "template": "fix_failing_ci",
        "trigger": { "kind": "ci_failure", "branch": "main" },
        "budget": { "turns": 20, "minutes": 30 },
        "daily": { "runs": runs, "turns": turns },
        "utc_offset_minutes": OFFSET,
    }))
    .unwrap();
    let crew = w.core.background_create(&body).await.unwrap();
    w.core.background_set_enabled(&crew.id, true).unwrap();
    crew.id
}

fn crew_store_daily(w: &World, name: &str, runs: u32, turns: u32) -> String {
    // A crew made at the store's own level, the way a row from before the daily budget looks: 0 and 0.
    let crew = w
        .core
        .store
        .create_background_crew(
            &crate::store::NewBackground {
                project_id: &project(w),
                name,
                template: "fix_failing_ci",
                trigger_kind: "ci_failure",
                trigger_settings: &json!({}),
                budget_turns: 20,
                budget_minutes: 30,
                daily_runs: runs,
                daily_turns: turns,
                utc_offset_minutes: OFFSET,
            },
            "user",
        )
        .unwrap();
    w.core.background_set_enabled(&crew.id, true).unwrap();
    crew.id
}

/// A worker that never reaches the files it may change, so verification fails each turn.
fn outside() -> Script {
    Arc::new(|_, root| {
        std::fs::write(root.join("outside.txt"), "no\n").unwrap();
        Turn::Say(report("done"))
    })
}

/// A worker that changes a file it may change and says so, as a real engine does.
fn good() -> Script {
    Arc::new(|n, root| {
        std::fs::write(root.join(format!("src/fix-{n}.txt")), "fixed\n").unwrap();
        Turn::Say(report("done"))
    })
}

/// The crew's one run, unpacked after a count of one: the read is newest first, and a test that starts a run waits
/// on that one, so no place in the list is read.
fn run_of(w: &World, crew: &str) -> wire::BackgroundRun {
    let page = crate::crew::Page::new(None, None).unwrap();
    let runs = w.core.background_runs(crew, page).unwrap().runs;
    one(&runs, "run of the crew").clone()
}

async fn until_run(w: &World, crew: &str, state: wire::BackgroundRunState) -> wire::BackgroundRun {
    w.until("the run reaches its state", |w| {
        run_of(w, crew).state == state
    })
    .await;
    run_of(w, crew)
}

fn threads(w: &World) -> i64 {
    w.core
        .store
        .reader()
        .query_row("SELECT COUNT(*) FROM threads", [], |row| row.get(0))
        .unwrap()
}

/// The audit rows of one action, oldest first.
fn audits(w: &World, action: &str) -> Vec<Value> {
    w.core
        .store
        .list_audit(None, 500)
        .unwrap()
        .into_iter()
        .filter(|row| row.action == action)
        .map(|row| row.detail)
        .collect()
}

#[tokio::test]
async fn a_run_is_cancelled_at_the_daily_cap_and_shows_what_it_spent() {
    let w = world(true, outside()).await;
    let id = crew_daily(&w, "One turn a day", 5, 1).await;
    let run = w.core.crew_background_run_now(&id, "user").await.unwrap();

    // The engine's one turn is counted as it happens, and the run is cancelled at the cap: stopped
    // `budget`, waiting for the person, with the engine, its turns against the cap and the reset time.
    let stopped = until_run(&w, &id, wire::BackgroundRunState::Stopped).await;
    assert_eq!(
        stopped.stop_reason,
        Some(wire::BackgroundStopReason::Budget)
    );
    assert!(stopped.review_waiting, "a daily stop waits for review");
    let why = stopped.review_reason.unwrap_or_default();
    assert!(why.contains("alpha"), "{why}");
    assert!(why.contains("1 of its 1 turns today"), "{why}");
    let (_, reset) = local_day_and_reset(OFFSET, crate::util::unix_now()).unwrap();
    assert!(why.contains(&reset), "{why}");
    assert_eq!(
        (stopped.turns_spent, stopped.state),
        (1, wire::BackgroundRunState::Stopped)
    );

    // The task stopped with it, and the ledger holds one row: this run, today, one turn of alpha.
    let tasks = w.core.store.crew_tasks_in_crew(&run.id).unwrap();
    let (task, _) = one(&tasks, "task of the stopped run");
    assert_eq!(w.task(&task.id).state, "stopped");
    let rows: i64 = w
        .core
        .store
        .reader()
        .query_row(
            "SELECT COUNT(*) FROM crew_background_daily WHERE background_id = ?1 AND engine = 'alpha'",
            [&id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(rows, 1, "the turn is on the engine's day");
    // The audits read is oldest first, so the last spend row is the newest: the turn just counted.
    assert!(
        audits(&w, "crew.background.daily.spend")
            .last()
            .and_then(|detail| detail.get("turns"))
            .is_some_and(|turns| turns == &json!(1))
    );
}

#[tokio::test]
async fn a_new_event_is_refused_with_the_reset_time_and_never_queued() {
    let w = world(true, outside()).await;
    let id = crew_daily(&w, "Spent", 5, 1).await;
    w.core.crew_background_run_now(&id, "user").await.unwrap();
    until_run(&w, &id, wire::BackgroundRunState::Stopped).await;

    let before = (
        threads(&w),
        w.core.store.crew_paused_starts(&project(&w)).unwrap().len(),
    );
    let error = w
        .core
        .crew_background_run_now(&id, "user")
        .await
        .unwrap_err();
    let CoreError::PolicyDenied(why) = &error else {
        panic!("{error}")
    };
    let (_, reset) = local_day_and_reset(OFFSET, crate::util::unix_now()).unwrap();
    assert!(
        why.starts_with("daily budget spent: alpha") && why.contains(&reset),
        "{why}"
    );

    // Nothing was made and nothing was held: the same threads, the one run, no held start.
    let page = crate::crew::Page::new(None, None).unwrap();
    assert_eq!(
        (
            threads(&w),
            w.core.store.crew_paused_starts(&project(&w)).unwrap().len()
        ),
        before
    );
    assert_eq!(w.core.background_runs(&id, page).unwrap().total, 1);

    // The audit row reads the same and names the engine.
    let refused = audits(&w, "crew.background.daily.refused");
    let refused = one(&refused, "refused event's audit row");
    assert!(
        refused["reason"]
            .as_str()
            .is_some_and(|reason| reason == "daily budget spent: alpha")
            && refused["resets_at"].as_str().is_some_and(|at| at == reset),
        "{refused:?}"
    );

    // The next day the engine may spend again: yesterday's row never counts as today's.
    let (yesterday, _) = local_day_and_reset(OFFSET, crate::util::unix_now() - 86_400).unwrap();
    w.core
        .store
        .reader()
        .execute(
            "UPDATE crew_background_daily SET day = ?2 WHERE background_id = ?1",
            [&id, &yesterday],
        )
        .unwrap();
    let run = w.core.crew_background_run_now(&id, "user").await.unwrap();
    assert_eq!(run.state, wire::BackgroundRunState::Running);
}

#[tokio::test]
async fn an_engine_out_of_daily_runs_is_refused_even_with_turns_left() {
    let w = world(true, good()).await;
    let id = crew_daily(&w, "One run a day", 1, 100).await;
    w.core.crew_background_run_now(&id, "user").await.unwrap();
    until_run(&w, &id, wire::BackgroundRunState::NeedsReview).await;

    let error = w
        .core
        .crew_background_run_now(&id, "user")
        .await
        .unwrap_err();
    let CoreError::PolicyDenied(why) = &error else {
        panic!("{error}")
    };
    assert!(why.starts_with("daily budget spent: alpha"), "{why}");
    let refused = audits(&w, "crew.background.daily.refused");
    assert_eq!(refused.len(), 1, "the runs cap refused the event");
}

#[tokio::test]
async fn a_crew_from_before_the_daily_budget_has_none() {
    let w = world(true, outside()).await;
    let id = crew_store_daily(&w, "Legacy", 0, 0);
    w.core.crew_background_run_now(&id, "user").await.unwrap();

    // No daily cap, so the run is not cancelled at a turn: the loop guard ends it as usual.
    let stopped = until_run(&w, &id, wire::BackgroundRunState::Stopped).await;
    assert_eq!(
        stopped.stop_reason,
        Some(wire::BackgroundStopReason::RepeatedFailure)
    );
    assert!(stopped.turns_spent >= 2, "{stopped:?}");
    assert!(audits(&w, "crew.background.daily.spend").is_empty());
    // And a new event is not refused for the day.
    w.core.crew_background_run_now(&id, "user").await.unwrap();
}

#[tokio::test]
async fn a_turn_that_reaches_the_cap_but_passes_ends_the_run_needs_review() {
    let w = world(true, good()).await;
    let id = crew_daily(&w, "Pass at the cap", 5, 1).await;
    let run = w.core.crew_background_run_now(&id, "user").await.unwrap();

    // The engine's only turn is its last allowed one for the day, and it passes: the run ends
    // finished and waiting for the person, the way a run that passes on its budget's last turn
    // is not over budget. Nothing is cancelled.
    let done = until_run(&w, &id, wire::BackgroundRunState::NeedsReview).await;
    assert_eq!(done.stop_reason, None, "{done:?}");
    assert!(done.review_waiting);
    let tasks = w.core.store.crew_tasks_in_crew(&run.id).unwrap();
    let (task, _) = one(&tasks, "task of the passed run");
    assert_eq!(w.task(&task.id).state, "passed");

    // The next event is the one the daily budget refuses.
    let error = w
        .core
        .crew_background_run_now(&id, "user")
        .await
        .unwrap_err();
    let CoreError::PolicyDenied(why) = &error else {
        panic!("{error}")
    };
    assert!(why.starts_with("daily budget spent: alpha"), "{why}");
}

#[tokio::test]
async fn the_daily_budget_is_set_at_creation_only() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let refused = async |daily: Value, offset: Value| {
        let body: wire::BackgroundCrewCreate = serde_json::from_value(json!({
            "project_id": project(&w), "name": "Crew", "template": "fix_failing_ci",
            "trigger": { "kind": "ci_failure" },
            "budget": { "turns": 5, "minutes": 5 },
            "daily": daily, "utc_offset_minutes": offset,
        }))
        .unwrap();
        matches!(
            w.core.background_create(&body).await.unwrap_err(),
            CoreError::Invalid(_)
        )
    };
    assert!(refused(json!({ "runs": 0, "turns": 1 }), json!(0)).await);
    assert!(refused(json!({ "runs": 1001, "turns": 1 }), json!(0)).await);
    assert!(refused(json!({ "runs": 1, "turns": 0 }), json!(0)).await);
    assert!(refused(json!({ "runs": 1, "turns": 10_001 }), json!(0)).await);
    assert!(refused(json!({ "runs": 1, "turns": 1 }), json!(841)).await);
    assert!(refused(json!({ "runs": 1, "turns": 1 }), json!(-841)).await);
    // The bounds themselves are accepted, and the crew answers with its daily budget.
    let edge: wire::BackgroundCrewCreate = serde_json::from_value(json!({
        "project_id": project(&w), "name": "Edge", "template": "fix_failing_ci",
        "trigger": { "kind": "ci_failure" },
        "budget": { "turns": 1, "minutes": 1 },
        "daily": { "runs": 1000, "turns": 10_000 }, "utc_offset_minutes": -840,
    }))
    .unwrap();
    let crew = w.core.background_create(&edge).await.unwrap();
    assert_eq!(
        crew.daily,
        wire::BackgroundDailyBudget {
            runs: 1000,
            turns: 10_000
        }
    );
    // The wire refuses what the core would have to: a missing or foreign daily field.
    for (bad, why) in [
        (
            json!({ "project_id": project(&w), "name": "No daily", "template": "fix_failing_ci",
                    "trigger": { "kind": "ci_failure" }, "budget": { "turns": 5, "minutes": 5 } }),
            "missing field `daily`",
        ),
        (
            json!({ "project_id": project(&w), "name": "Tokens", "template": "fix_failing_ci",
                    "trigger": { "kind": "ci_failure" }, "budget": { "turns": 5, "minutes": 5 },
                    "daily": { "runs": 1, "turns": 1, "tokens": 9 }, "utc_offset_minutes": 0 }),
            "unknown field `tokens`",
        ),
    ] {
        let error = serde_json::from_value::<wire::BackgroundCrewCreate>(bad).unwrap_err();
        assert!(error.to_string().contains(why), "{error}");
    }
}

#[tokio::test]
async fn a_daily_budget_that_cannot_be_read_refuses_the_start_instead_of_allowing_it() {
    let w = world(true, good()).await;
    let id = crew_daily(&w, "Unreadable", 5, 100).await;
    w.core
        .store
        .reader()
        .execute_batch("DROP TABLE crew_background_daily")
        .unwrap();
    let before = threads(&w);
    let error = w
        .core
        .crew_background_run_now(&id, "user")
        .await
        .unwrap_err();
    let CoreError::PolicyDenied(why) = &error else {
        panic!("{error}")
    };
    assert!(why.contains("could not be read"), "{why}");
    // Nothing was made: no run, no thread.
    let page = crate::crew::Page::new(None, None).unwrap();
    assert_eq!(w.core.background_runs(&id, page).unwrap().total, 0);
    assert_eq!(threads(&w), before);
}

#[tokio::test]
async fn a_turn_that_cannot_be_counted_stops_the_run_instead_of_letting_it_go_on() {
    let w = world(true, outside()).await;
    let id = crew_daily(&w, "Uncountable", 5, 100).await;
    // The write that counts a daily turn fails (it is audited in the same transaction), as under a brief database lock.
    w.core
        .store
        .reader()
        .execute_batch(
            "CREATE TRIGGER refuse_daily_count BEFORE INSERT ON audit_events
             WHEN NEW.action = 'crew.background.daily.spend' BEGIN SELECT RAISE(ABORT, 'locked'); END;",
        )
        .unwrap();
    w.core.crew_background_run_now(&id, "user").await.unwrap();
    let stopped = until_run(&w, &id, wire::BackgroundRunState::Stopped).await;
    assert_eq!(
        stopped.stop_reason,
        Some(wire::BackgroundStopReason::Budget)
    );
    assert!(stopped.review_waiting);
    let why = stopped.review_reason.clone().unwrap_or_default();
    assert!(why.contains("could not be updated"), "{why}");
    // One turn only: the run was not retried over a ledger that was missing it.
    assert_eq!(stopped.turns_spent, 1, "{stopped:?}");
}

#[tokio::test]
async fn a_late_turn_that_cannot_be_counted_leaves_the_ended_run_and_its_task_alone() {
    let w = world(true, good()).await;
    let id = crew_daily(&w, "Late and uncountable", 5, 100).await;
    let run = w.core.crew_background_run_now(&id, "user").await.unwrap();
    until_run(&w, &id, wire::BackgroundRunState::NeedsReview).await;
    let tasks = w.core.store.crew_tasks_in_crew(&run.id).unwrap();
    let (task, _) = one(&tasks, "task of the ended run");
    let worker = w
        .core
        .store
        .crew_run(&task.id)
        .unwrap()
        .unwrap()
        .worker_thread_id
        .unwrap();
    let results = |w: &World| w.core.store.crew_results(&task.id).unwrap().len();
    let before = (w.task(&task.id).state, results(&w), run_of(&w, &id));
    w.core
        .store
        .reader()
        .execute_batch(
            "CREATE TRIGGER refuse_daily_count BEFORE INSERT ON audit_events
             WHEN NEW.action = 'crew.background.daily.spend' BEGIN SELECT RAISE(ABORT, 'locked'); END;",
        )
        .unwrap();
    // The count fails, but the run is over: best effort, nothing stops and nothing changes.
    assert!(w.core.crew_background_turn_ended(task, &worker).is_ok());
    assert_eq!(
        (w.task(&task.id).state, results(&w), run_of(&w, &id)),
        before
    );
}

#[tokio::test]
async fn a_turn_that_ends_after_its_run_still_counts_for_the_engines_day() {
    let w = world(true, good()).await;
    let id = crew_daily(&w, "Late turn", 5, 100).await;
    let run = w.core.crew_background_run_now(&id, "user").await.unwrap();
    let done = until_run(&w, &id, wire::BackgroundRunState::NeedsReview).await;
    let tasks = w.core.store.crew_tasks_in_crew(&run.id).unwrap();
    let (task, _) = one(&tasks, "task of the run");
    let worker = w
        .core
        .store
        .crew_run(&task.id)
        .unwrap()
        .unwrap()
        .worker_thread_id
        .unwrap();
    let day_turns = |w: &World| -> i64 {
        w.core
            .store
            .reader()
            .query_row(
                "SELECT COALESCE(SUM(turns_spent), 0) FROM crew_background_daily WHERE background_id = ?1",
                [&id],
                |row| row.get(0),
            )
            .unwrap()
    };
    let counted = day_turns(&w);
    w.core.crew_background_turn_ended(task, &worker).unwrap();
    // The run is over, so its own total stays; the engine's day still grows by the turn.
    assert_eq!(day_turns(&w), counted + 1);
    assert_eq!(run_of(&w, &id).turns_spent, done.turns_spent);
}

#[test]
fn the_day_is_the_persons_not_utc() {
    // The epoch, for a person at UTC+10: their day is the same date, and their midnight is +10:00,
    // never 00:00Z.
    let (day, reset) = local_day_and_reset(600, 0).unwrap();
    assert_eq!(day, "1970-01-01");
    assert_eq!(reset, "1970-01-02T00:00:00+10:00");
    // One second before their midnight (13:59:59Z): still their first day.
    let (day, reset) = local_day_and_reset(600, 50_399).unwrap();
    assert_eq!(day, "1970-01-01");
    assert_eq!(reset, "1970-01-02T00:00:00+10:00");
    // Their midnight exactly: the day turned for them and only them.
    let (day, reset) = local_day_and_reset(600, 50_400).unwrap();
    assert_eq!(day, "1970-01-02");
    assert_eq!(reset, "1970-01-03T00:00:00+10:00");
    // A day that turns in the other direction: at the epoch, UTC-11 is still their New Year's Eve.
    let (day, reset) = local_day_and_reset(-660, 0).unwrap();
    assert_eq!(day, "1969-12-31");
    assert_eq!(reset, "1970-01-01T00:00:00-11:00");
}
