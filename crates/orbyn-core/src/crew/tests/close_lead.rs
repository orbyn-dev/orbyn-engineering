//! Closing a lead thread's Crew work (KC2) on a real `Core` and a fake worker: every live task the thread leads is cancelled
//! one by one through the audited cancel, a task another thread leads and one already final are left alone, the live read is
//! exactly what the close tries, and a task whose worker cannot be stopped is named without stopping the others or the
//! thread's own close.

use std::sync::Arc;

use orbyn_contracts as wire;
use orbyn_contracts::UpdateThread;
use serde_json::json;

use super::harness::{Turn, World, brief, give_back_worker_run, key, take_worker_run, world};
use crate::error::CoreError;

/// A live task of the world's lead thread, its worker running and hanging. Each task leases its own path, so several can run.
async fn running(w: &World, outcome: &str) -> String {
    let folder = outcome.to_ascii_lowercase().replace('.', "");
    let (answer, error) = w
        .start(
            brief(json!({
                "outcome": outcome,
                "files": [{ "path": format!("src/{folder}/**"), "mode": "write" }]
            })),
            key(),
        )
        .await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    assert_eq!(w.task(&id).state, "running");
    id
}

/// A task with a stored brief and key and no worker: it starts `briefed`.
fn seeded_task(w: &World, label: &str) -> String {
    let stamper = crate::crew::Stamper {
        crew_id: wire::CrewId::new("crew-1").unwrap(),
        engine: wire::CrewToken::new("alpha").unwrap(),
        member: wire::CrewMember::new("lead-01").unwrap(),
        now: wire::CrewTimestamp::new("2026-11-12T10:00:00Z").unwrap(),
    };
    let draft = stamper
        .brief(&brief(json!({ "outcome": format!("Seed {label}.") })))
        .unwrap();
    let answer_key = stamper.answer_key(&key(), &draft.id).unwrap();
    w.core
        .store
        .create_crew_task(&draft, &answer_key, &format!("run-{label}"))
        .unwrap()
        .id
}

/// A second thread of the world's project, with no run of its own, to lead tasks through this close.
fn lead_thread(w: &World) -> String {
    let lead = w.core.store.thread(&w.lead).unwrap();
    w.core
        .store
        .create_thread(crate::store::NewThread {
            project_id: lead.project_id,
            title: "second lead".into(),
            engine: lead.engine,
            model: None,
            cwd: lead.cwd,
            worktree: None,
            skill_profile: None,
            parent_thread_id: None,
            relation: None,
            scratch: false,
        })
        .unwrap()
        .id
}

fn delegate(label: &str) -> orbyn_contracts::DelegateTask {
    orbyn_contracts::DelegateTask {
        prompt: format!("work {label}"),
        title: Some("Crew worker".into()),
        engine: Some("alpha".into()),
        model: None,
        effort: None,
        options: Default::default(),
    }
}

/// A live task of `lead`, its worker running and hanging, made through the same attach a real start uses.
async fn live_task(w: &World, lead: &str, outcome: &str) -> String {
    let folder = outcome.to_ascii_lowercase().replace('.', "");
    let task = seeded_task(w, outcome);
    w.core.store.add_crew_run(&task, lead, "worker-01").unwrap();
    w.core.store.move_crew_task(&task, "routed", None).unwrap();
    w.core
        .attach_crew_worker(lead, &delegate(&folder), &task, None)
        .await
        .unwrap();
    let worker = w.worker(&task);
    w.until("the worker's run", |w| {
        w.core.store.thread(&worker).unwrap().active_run.is_some()
    })
    .await;
    // The stop is only confirmed against a run that has started, as a real one is by the time a person closes the thread.
    let run = w.core.store.thread(&worker).unwrap().active_run.unwrap().id;
    w.until("the worker's run to be running", |w| {
        w.core.store.run(&run).unwrap().state == orbyn_contracts::RunState::Running
    })
    .await;
    task
}

fn intents(w: &World) -> Vec<orbyn_contracts::AuditEvent> {
    w.core
        .store
        .list_audit(None, 200)
        .unwrap()
        .into_iter()
        .filter(|row| row.action == "crew.task.cancel")
        .collect()
}

#[tokio::test]
async fn closing_a_lead_cancels_the_live_tasks_it_leads_and_leaves_the_rest() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let first = running(&w, "First.").await;
    let second = running(&w, "Second.").await;
    // A task another thread leads, and one this thread leads that already ended: neither is live work of this close.
    let elsewhere = seeded_task(&w, "elsewhere");
    w.core
        .store
        .add_crew_run(&elsewhere, "another-lead", "worker-01")
        .unwrap();
    w.core
        .store
        .move_crew_task(&elsewhere, "routed", None)
        .unwrap();
    let ended = seeded_task(&w, "ended");
    w.core
        .store
        .add_crew_run(&ended, &w.lead, "worker-02")
        .unwrap();
    w.core
        .store
        .move_crew_task(&ended, "needs_person", None)
        .unwrap();
    w.core
        .store
        .move_crew_task(&ended, "stopped", Some("budget"))
        .unwrap();

    let answer = w.core.crew_close_lead(&w.lead, "user").await.unwrap();
    assert_eq!(answer.closed, vec![first.clone(), second.clone()]);
    assert!(answer.failed.is_empty(), "{answer:?}");
    for id in [&first, &second] {
        assert_eq!(w.task(id).state, "cancelled");
    }
    assert_eq!(
        w.task(&elsewhere).state,
        "routed",
        "another thread's task is its own"
    );
    assert_eq!(w.task(&ended).state, "stopped", "a final task is skipped");
    // The audited cancel, with the reason the close gives: one intent row per task, and each names the lead's close.
    let rows = intents(&w);
    assert_eq!(rows.len(), 2, "{rows:?}");
    for row in rows {
        assert_eq!(row.actor, "user");
        assert_eq!(row.detail["reason"], "The lead thread was closed.");
    }
}

#[tokio::test]
async fn a_task_that_cannot_be_stopped_is_named_and_the_others_and_the_thread_still_close() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    // A thread of its own leads these tasks, so settling it is not a settle of the world's lead.
    let lead = lead_thread(&w);
    let first = live_task(&w, &lead, "First.").await;
    let stuck = live_task(&w, &lead, "Stuck.").await;
    let last = live_task(&w, &lead, "Last.").await;
    let epoch = take_worker_run(&w, &stuck);

    let answer = w.core.crew_close_lead(&lead, "user").await.unwrap();
    assert_eq!(answer.closed, vec![first.clone(), last.clone()]);
    assert_eq!(answer.failed, vec![stuck.clone()]);
    assert_eq!(w.task(&first).state, "cancelled");
    assert_eq!(
        w.task(&last).state,
        "cancelled",
        "the loop goes on after a failure"
    );
    assert_eq!(
        w.task(&stuck).state,
        "running",
        "a failed stop keeps the state"
    );
    // The lead is not told about the tasks this close cancelled: a note would start the thread again.
    assert!(
        w.core.store.list_queue(&lead).unwrap().is_empty(),
        "no note waits for the closing lead"
    );
    assert!(
        w.core.store.crew_cancel_reason(&first).unwrap().as_deref()
            == Some("The lead thread was closed."),
        "the cancel says which close it is"
    );

    // The thread's own close is the caller's and does not wait on the task that failed.
    let settled = w
        .core
        .update_thread(
            &lead,
            &UpdateThread {
                settled: Some(true),
                ..Default::default()
            },
            "user",
        )
        .unwrap();
    assert!(
        settled
            .lifecycle
            .as_ref()
            .is_some_and(|lifecycle| lifecycle.settled_at.is_some()),
        "the thread closed whatever the close said"
    );

    // Stop it again once the stop can be confirmed: the task ends then.
    give_back_worker_run(&w, &stuck, &epoch);
    w.core.crew_cancel_task(&stuck, None, "user").await.unwrap();
    assert_eq!(w.task(&stuck).state, "cancelled");
}
/// The close chooses its tasks again when it runs, so one that started after the confirm was read is cancelled too: the
/// confirm's list is the person's view, not a fixed set.
#[tokio::test]
async fn a_task_started_after_the_confirm_is_cancelled_by_the_close() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let first = running(&w, "Seen in the confirm.").await;
    let shown: Vec<String> = w
        .core
        .crew_lead_live(&w.lead)
        .unwrap()
        .tasks
        .into_iter()
        .map(|task| task.id)
        .collect();
    assert_eq!(shown, vec![first.clone()]);

    let late = running(&w, "Started after the confirm.").await;
    let answer = w.core.crew_close_lead(&w.lead, "user").await.unwrap();
    assert_eq!(answer.closed, vec![first, late.clone()]);
    assert_eq!(w.task(&late).state, "cancelled");
}

#[tokio::test]
async fn the_live_read_is_exactly_what_the_close_tries() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let first = running(&w, "First task.").await;
    let second = running(&w, "Second task.").await;
    let ended = seeded_task(&w, "ended");
    w.core
        .store
        .add_crew_run(&ended, &w.lead, "worker-01")
        .unwrap();
    w.core
        .store
        .move_crew_task(&ended, "needs_person", None)
        .unwrap();
    w.core
        .store
        .move_crew_task(&ended, "stopped", Some("budget"))
        .unwrap();

    let live = w.core.crew_lead_live(&w.lead).unwrap();
    let shown: Vec<String> = live.tasks.iter().map(|task| task.id.clone()).collect();
    assert_eq!(shown, vec![first.clone(), second.clone()]);
    assert_eq!(live.tasks[0].state, "running");
    assert_eq!(
        live.tasks[0].name, "First task.",
        "the brief's outcome, as the card names it"
    );

    let answer = w.core.crew_close_lead(&w.lead, "user").await.unwrap();
    let tried: Vec<String> = answer.closed.into_iter().chain(answer.failed).collect();
    assert_eq!(
        tried, shown,
        "the confirm's list is exactly what the close tries"
    );
    assert!(w.core.crew_lead_live(&w.lead).unwrap().tasks.is_empty());
}

#[tokio::test]
async fn with_the_flag_off_the_close_and_the_live_read_are_not_found() {
    let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
    assert!(matches!(
        w.core.crew_close_lead(&w.lead, "user").await,
        Err(CoreError::NotFound(_))
    ));
    assert!(matches!(
        w.core.crew_lead_live(&w.lead),
        Err(CoreError::NotFound(_))
    ));
}

#[tokio::test]
async fn a_bad_thread_id_is_refused() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    for id in ["", "a b"] {
        assert!(matches!(
            w.core.crew_close_lead(id, "user").await,
            Err(CoreError::Invalid(_))
        ));
        assert!(matches!(
            w.core.crew_lead_live(id),
            Err(CoreError::Invalid(_))
        ));
    }
}
