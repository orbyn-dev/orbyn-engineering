//! Resume after restart (KC8): one resume for a builder or fix task, a stop for everyone else,
//! and off time out of the budget.

use std::sync::Arc;

use orbyn_contracts::{AuditEvent, CreateThread, NeedsYouKind, StopDetail};
use rusqlite::params;
use serde_json::json;

use super::harness::{Turn, World, brief, key, world};
use crate::crew::{Page, WORKER_STOPPED_TWICE};
use crate::store::ResumeOutcome;

const DAY: i64 = 24 * 60 * 60;

fn page() -> Page {
    Page::new(None, None).unwrap()
}

/// An idle worker thread in the lead's project, as a restart leaves one.
async fn idle_thread(w: &World, title: &str) -> String {
    let project = w.core.store.thread(&w.lead).unwrap().project_id;
    w.core
        .create_thread(CreateThread {
            project_id: Some(project),
            engine: "alpha".into(),
            model: None,
            title: Some(title.to_owned()),
            worktree: None,
            skill_profile: None,
            base_branch: None,
            effort: None,
            options: Default::default(),
            branch_name_seed: None,
        })
        .await
        .unwrap()
        .id
}

/// A `running` task with a worker and nothing active. It is what a restart leaves behind.
async fn dead_task(w: &World, member: &str, minutes: u32) -> (String, String, String) {
    use crate::crew::tests::fake::stamper;
    let s = stamper();
    let draft = brief(json!({
        "outcome": "Resume after a restart.",
        "files": [],
        "budget": { "turns": 20, "minutes": minutes },
    }));
    let brief = s.brief(&draft).unwrap();
    let answer_key = s.answer_key(&key(), &brief.id).unwrap();
    let task = w
        .core
        .store
        .create_crew_task(&brief, &answer_key, &crate::util::new_id())
        .unwrap();
    w.core
        .store
        .add_crew_run(&task.id, &w.lead, member)
        .unwrap();
    let thread = idle_thread(w, "worker").await;
    let cwd = w.core.store.thread(&w.lead).unwrap().cwd;
    w.core
        .store
        .set_crew_worker(&task.id, &thread, &cwd, None)
        .unwrap();
    for step in ["routed", "running"] {
        w.core.store.move_crew_task(&task.id, step, None).unwrap();
    }
    (task.id, thread, answer_key.id.as_str().to_owned())
}

fn audits(w: &World, task: &str, action: &str) -> Vec<AuditEvent> {
    w.core
        .store
        .list_audit(None, 1000)
        .unwrap()
        .into_iter()
        .filter(|row| row.target == task && row.action == action)
        .collect()
}

/// The one audit row `task` recorded for `action`, unpacked from the store's list rather than read by place.
/// A missing row or a second one fails here by the action the test asked for.
fn one_audit(w: &World, task: &str, action: &str) -> AuditEvent {
    let mut rows = audits(w, task, action);
    assert_eq!(rows.len(), 1, "one {action} audit for the task: {rows:?}");
    rows.pop().expect("the task's one audit row")
}

fn set_times(w: &World, task: &str, created: i64, updated: i64) {
    w.core
        .store
        .reader()
        .execute(
            "UPDATE crew_tasks SET created_at = ?2, updated_at = ?3 WHERE id = ?1",
            params![
                task,
                crate::util::format_unix(created),
                crate::util::format_unix(updated)
            ],
        )
        .unwrap();
}

#[tokio::test]
async fn a_running_builder_resumes_once_with_its_key_and_attempt() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (id, worker, key_id) = dead_task(&w, "worker-resume", 30).await;
    w.core.recover_crew_tasks().await;

    let task = w.task(&id);
    assert_eq!(task.state, "running");
    assert_eq!(task.stop_reason, None);
    assert_eq!(task.stop_detail, None);
    let run = w.core.store.crew_run(&id).unwrap().unwrap();
    assert_eq!((run.resume_count, run.attempts), (1, 1));
    assert!(run.resumed_at.is_some(), "the resume time is stored");
    assert!(
        run.downtime_secs <= 2,
        "a live task has no off time beyond a second boundary"
    );

    let resumed = one_audit(&w, &id, "crew.task.resumed");
    assert_eq!(
        (resumed.actor.as_str(), resumed.source.as_str()),
        ("core", "core")
    );
    assert_eq!(
        resumed.detail,
        json!({ "round": 1, "member": "worker-resume" })
    );
    assert!(audits(&w, &id, "crew.task.restart_stopped").is_empty());

    // The same answer key, and one resume brief on the same worker thread.
    assert_eq!(
        w.core
            .store
            .crew_answer_key(&id)
            .unwrap()
            .unwrap()
            .id
            .as_str(),
        key_id
    );
    w.until("the resume prompt starts", |w| {
        !w.sent_to(&worker).is_empty()
    })
    .await;
    let prompts = w.sent_to(&worker);
    assert_eq!(prompts.len(), 1, "one resume brief on the worker thread");
    let prompt = prompts.first().expect("the restart's one resume brief");
    assert!(prompt.contains("lost when Orbyn restarted"), "{prompt}");
    assert!(prompt.contains("call crew_report again"), "{prompt}");
    assert!(prompt.contains("last verified round (round 1)"), "{prompt}");
    assert!(!prompt.contains(&key_id), "the brief held the key id");
}

#[tokio::test]
async fn a_second_restart_stops_the_task_without_another_resume() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (id, first, _) = dead_task(&w, "worker-twice", 30).await;
    w.core.recover_crew_tasks().await;
    assert_eq!(w.task(&id).state, "running");
    w.until("the first resume starts", |w| !w.sent_to(&first).is_empty())
        .await;

    // The next restart leaves the same task with a dead worker again.
    let second = idle_thread(&w, "worker-again").await;
    let cwd = w.core.store.thread(&w.lead).unwrap().cwd;
    w.core
        .store
        .set_crew_worker(&id, &second, &cwd, None)
        .unwrap();
    w.core.recover_crew_tasks().await;

    let task = w.task(&id);
    assert_eq!(task.state, "stopped");
    assert_eq!(task.stop_reason.as_deref(), Some("engine_error"));
    let run = w.core.store.crew_run(&id).unwrap().unwrap();
    assert_eq!((run.resume_count, run.attempts), (1, 1));
    assert_eq!(
        one_audit(&w, &id, "crew.task.restart_stopped").detail,
        json!({ "why": "already_resumed" })
    );
    assert_eq!(audits(&w, &id, "crew.task.resumed").len(), 1);
    assert!(w.sent_to(&second).is_empty(), "no second resume was sent");

    // Old readers see the stop they always saw; the typed detail names the second death for the new readers.
    assert_eq!(task.stop_detail.as_deref(), Some(WORKER_STOPPED_TWICE));
    let old: (String, Option<String>) = w
        .core
        .store
        .reader()
        .query_row(
            "SELECT state, stop_reason FROM crew_tasks WHERE id = ?1",
            [&id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        (old.0.as_str(), old.1.as_deref()),
        ("stopped", Some("engine_error"))
    );

    // The Flow row and the board card carry the typed word.
    let crew = task.crew_id.clone();
    let rows = w
        .core
        .crew_flow_tasks(&crew, None, None, None, page())
        .unwrap();
    let row = rows
        .tasks
        .iter()
        .find(|row| row.id == id)
        .expect("the task's Flow row");
    assert_eq!(row.stop_detail, Some(StopDetail::WorkerStoppedTwice));
    let board = w
        .core
        .crew_flow_board(&crew, None, None, None, None, None, page())
        .unwrap();
    let card = board
        .columns
        .iter()
        .flat_map(|column| &column.cards)
        .find(|card| card.id == id)
        .expect("the task's board card");
    assert_eq!(card.stop_reason.as_deref(), Some("engine_error"));
    assert_eq!(card.stop_detail, Some(StopDetail::WorkerStoppedTwice));

    // One needs-you item names it, and only one.
    let ours: Vec<_> = w
        .core
        .crew_needs_you()
        .unwrap()
        .items
        .into_iter()
        .filter(|item| item.id == id)
        .collect();
    assert_eq!(ours.len(), 1, "one item for the task");
    let item = ours.first().expect("the task's one needs-you item");
    assert_eq!(item.kind, NeedsYouKind::WorkerStoppedTwice);
}

#[tokio::test]
async fn a_reviewer_lane_stops_without_a_resume() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (id, worker, _) = dead_task(&w, "reviewer-abc", 30).await;
    w.core.recover_crew_tasks().await;

    let task = w.task(&id);
    assert_eq!(task.state, "stopped");
    assert_eq!(task.stop_reason.as_deref(), Some("engine_error"));
    assert_eq!(task.stop_detail, None);
    let run = w.core.store.crew_run(&id).unwrap().unwrap();
    assert_eq!((run.resume_count, run.attempts), (0, 0));
    assert!(run.resumed_at.is_none());
    assert_eq!(
        one_audit(&w, &id, "crew.task.restart_stopped").detail,
        json!({ "why": "reviewer" })
    );
    assert!(audits(&w, &id, "crew.task.resumed").is_empty());
    assert!(w.sent_to(&worker).is_empty());
    assert!(
        w.core
            .crew_needs_you()
            .unwrap()
            .items
            .iter()
            .all(|item| item.id != id),
        "a reviewer's stop is not the second-death wait"
    );
}

#[tokio::test]
async fn a_not_resumable_member_stops_without_a_stop_detail() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (id, worker, _) = dead_task(&w, "member-01", 30).await;
    w.core.recover_crew_tasks().await;

    let task = w.task(&id);
    assert_eq!(task.state, "stopped");
    assert_eq!(task.stop_reason.as_deref(), Some("engine_error"));
    assert_eq!(task.stop_detail, None);
    assert_eq!(
        one_audit(&w, &id, "crew.task.restart_stopped").detail,
        json!({ "why": "not_resumable" })
    );
    assert!(audits(&w, &id, "crew.task.resumed").is_empty());
    assert!(w.sent_to(&worker).is_empty());
    assert!(
        w.core
            .crew_needs_you()
            .unwrap()
            .items
            .iter()
            .all(|item| item.id != id),
        "a stop with no typed detail is not the second-death wait"
    );
}

#[tokio::test]
async fn downtime_is_free_for_the_timer() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (id, _, _) = dead_task(&w, "worker-down", 60).await;
    let now = crate::util::unix_now();
    let created = now - 3 * DAY;
    let last = created + 10 * 60;
    set_times(&w, &id, created, last);

    w.core.recover_crew_tasks().await;

    let run = w.core.store.crew_run(&id).unwrap().unwrap();
    assert_eq!(run.resume_count, 1);
    let expected = 3 * DAY - 10 * 60;
    assert!(
        (expected - 5..=expected + 5).contains(&run.downtime_secs),
        "the off time is stored: {} vs {expected}",
        run.downtime_secs
    );
    // The task spent ten minutes before the restart, so fifty of its sixty remain.
    let task = w.task(&id);
    let left = crate::crew::verify::budget_left_after_downtime(
        &task.created_at,
        60,
        crate::util::unix_now(),
        run.downtime_secs,
    );
    assert!(
        (49 * 60..=50 * 60).contains(&left.as_secs()),
        "the timer keeps the fifty minutes: {}s",
        left.as_secs()
    );
}

#[tokio::test]
async fn a_budget_spent_before_the_restart_stops_budget() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (id, worker, _) = dead_task(&w, "worker-spent", 30).await;
    let now = crate::util::unix_now();
    set_times(&w, &id, now - 2 * 60 * 60, now - 31 * 60);

    w.core.recover_crew_tasks().await;

    let task = w.task(&id);
    assert_eq!(task.state, "stopped");
    assert_eq!(task.stop_reason.as_deref(), Some("budget"));
    let run = w.core.store.crew_run(&id).unwrap().unwrap();
    assert_eq!((run.resume_count, run.attempts), (0, 0));
    assert!(audits(&w, &id, "crew.task.resumed").is_empty());
    assert!(audits(&w, &id, "crew.task.restart_stopped").is_empty());
    assert!(w.sent_to(&worker).is_empty());
}

#[tokio::test]
async fn a_failed_resume_recheck_writes_nothing() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (id, _, _) = dead_task(&w, "worker-recheck", 30).await;
    {
        let conn = w.core.store.reader();
        conn.execute(
            "UPDATE crew_runs SET resume_count = 1 WHERE task_id = ?1",
            params![id],
        )
        .unwrap();
    }
    let before = w.core.store.crew_run(&id).unwrap().unwrap();
    match w.core.store.resume_crew_task(&id).unwrap() {
        ResumeOutcome::Refused("already_resumed") => {}
        _ => panic!("the second resume was not refused"),
    }
    let after = w.core.store.crew_run(&id).unwrap().unwrap();
    assert_eq!(after, before, "the refused resume wrote no column");
    assert_eq!(w.task(&id).state, "running");
    assert!(audits(&w, &id, "crew.task.resumed").is_empty());
    assert!(audits(&w, &id, "crew.task.restart_stopped").is_empty());
}

#[tokio::test]
async fn a_resume_brief_that_cannot_be_sent_stops_send_failed() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (id, _, _) = dead_task(&w, "worker-gone", 30).await;
    {
        let conn = w.core.store.reader();
        conn.execute(
            "UPDATE crew_runs SET worker_thread_id = 'thread-that-does-not-exist' WHERE task_id = ?1",
            params![id],
        )
        .unwrap();
    }

    w.core.recover_crew_tasks().await;

    let task = w.task(&id);
    assert_eq!(task.state, "stopped");
    assert_eq!(task.stop_reason.as_deref(), Some("engine_error"));
    assert_eq!(task.stop_detail, None);
    let run = w.core.store.crew_run(&id).unwrap().unwrap();
    assert_eq!((run.resume_count, run.attempts), (1, 1));
    assert_eq!(
        one_audit(&w, &id, "crew.task.restart_stopped").detail,
        json!({ "why": "send_failed" })
    );
    assert!(
        w.core
            .crew_needs_you()
            .unwrap()
            .items
            .iter()
            .all(|item| item.id != id),
        "a resume that could not be sent is not the second-death wait"
    );
}

#[tokio::test]
async fn a_restart_on_the_exact_budget_boundary_stops_without_a_resume() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (id, worker, _) = dead_task(&w, "worker-edge", 30).await;
    // The newest recorded event is the second the thirty minutes ran out: exactly nothing is left. The
    // boundary is the case, not a budget blown long ago (that is a_budget_spent_before_the_restart...):
    // one second earlier would be a resume and one second of clock drift would hide an off-by-one.
    let created = crate::util::unix_now() - 2 * DAY;
    let last = created + 30 * 60;
    set_times(&w, &id, created, last);

    w.core.recover_crew_tasks().await;

    let task = w.task(&id);
    assert_eq!(task.state, "stopped");
    assert_eq!(task.stop_reason.as_deref(), Some("budget"));
    assert_eq!(task.stop_detail, None);
    let run = w.core.store.crew_run(&id).unwrap().unwrap();
    assert_eq!(
        (run.resume_count, run.attempts),
        (0, 0),
        "one resume then a turn were counted"
    );
    assert!(run.resumed_at.is_none());
    assert!(audits(&w, &id, "crew.task.resumed").is_empty());
    assert!(audits(&w, &id, "crew.task.restart_stopped").is_empty());
    assert!(w.sent_to(&worker).is_empty(), "no turn was sent");
}

#[tokio::test]
async fn a_crash_between_the_resume_commit_and_the_prompt_stops_once() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (id, worker, _) = dead_task(&w, "worker-crash", 30).await;
    // A restart commits the one resume (resume_crew_task) and then dies before the worker's prompt is
    // queued: the task is left running with nothing on the worker and the one resume already spent. A crash
    // at that instruction boundary cannot be injected into the process, so the test takes the commit itself,
    // which is exactly what recover calls, and skips only the send that would follow it.
    let run = w.core.store.crew_run(&id).unwrap().unwrap();
    assert_eq!((run.resume_count, run.attempts), (0, 0));
    match w.core.store.resume_crew_task(&id).unwrap() {
        ResumeOutcome::Resumed { task, .. } => assert_eq!(task.state, "running"),
        ResumeOutcome::Refused(why) => panic!("the resume was refused: {why}"),
        ResumeOutcome::NotInFlight => panic!("the task was not in flight"),
    }
    assert_eq!(w.task(&id).state, "running");
    assert!(
        w.core.store.list_queue(&worker).unwrap().is_empty(),
        "the prompt never reached the worker"
    );

    // The next start must not resume it a second time (a second turn for one resume): the one resume is
    // spent, so it stops the task as the second death and tells the person, with no prompt sent.
    w.core.recover_crew_tasks().await;

    let run = w.core.store.crew_run(&id).unwrap().unwrap();
    assert_eq!(
        (run.resume_count, run.attempts),
        (1, 1),
        "the resume was taken again"
    );
    assert_eq!(audits(&w, &id, "crew.task.resumed").len(), 1);
    let task = w.task(&id);
    assert_eq!(task.state, "stopped");
    assert_eq!(task.stop_reason.as_deref(), Some("engine_error"));
    assert_eq!(task.stop_detail.as_deref(), Some(WORKER_STOPPED_TWICE));
    let stopped = audits(&w, &id, "crew.task.restart_stopped");
    assert_eq!(stopped.len(), 1);
    assert_eq!(stopped[0].detail, json!({ "why": "already_resumed" }));
    assert!(w.sent_to(&worker).is_empty(), "no second turn was sent");
    assert!(
        w.core
            .crew_needs_you()
            .unwrap()
            .items
            .iter()
            .any(|item| item.id == id),
        "the person is asked about the second death"
    );
}
