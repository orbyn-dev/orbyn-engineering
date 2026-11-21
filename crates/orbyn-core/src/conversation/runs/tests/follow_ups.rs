//! Follow-up tasks: one fires once for each run that ends after the task was saved,
//! can ask for success only, and cannot follow itself.

use super::*;

fn follow_up(project: &str, target: &str, on_success: bool) -> orbyn_contracts::UpsertTask {
    orbyn_contracts::UpsertTask {
        id: None,
        project_id: project.to_owned(),
        thread_id: None,
        name: "After".into(),
        prompt: "next step".into(),
        engine: "script".into(),
        schedule: orbyn_contracts::TaskSchedule {
            kind: "after_thread".into(),
            every_minutes: None,
            time: None,
            weekdays: Vec::new(),
            utc_offset_minutes: 0,
            after_thread: Some(target.to_owned()),
            only_on_success: on_success,
        },
        signature: None,
        secret: None,
        filters: Vec::new(),
        dedupe: None,
        max_age_seconds: None,
        timestamp_path: None,
    }
}

fn finishing() -> Script {
    Arc::new(|_, events, _| {
        tokio::spawn(async move {
            let _ = events.send(text("done")).await;
            let _ = events
                .send(EngineEvent::Finished(EngineOutcome::Completed))
                .await;
        })
    })
}

#[tokio::test]
async fn a_follow_up_fires_once_for_each_run_that_ends_and_not_for_old_ones() {
    let f = fixture(finishing()).await;
    let project = f.core.store.thread(&f.thread).unwrap().project_id;
    let first = f.core.runs.admit(&f.thread, "go", "t").unwrap();
    until_terminal(&f.core, &first.id).await;

    let task = f
        .core
        .save_task(follow_up(&project, &f.thread, true), "t")
        .unwrap();
    assert!(task.next_run_at.is_none());
    f.core.run_due_tasks().await;
    let threads = |core: &Core| {
        core.store
            .list_threads(Some(&project), false)
            .unwrap()
            .len()
    };
    assert_eq!(
        threads(&f.core),
        1,
        "a run from before the task does not count"
    );

    let second = f.core.runs.admit(&f.thread, "go", "t").unwrap();
    until_terminal(&f.core, &second.id).await;
    f.core.run_due_tasks().await;
    f.core.run_due_tasks().await;
    assert_eq!(threads(&f.core), 2, "once, however often the clock ticks");
}

#[tokio::test]
async fn a_follow_up_can_ask_for_success_and_cannot_follow_itself() {
    let script: Script = Arc::new(|_, events, _| {
        tokio::spawn(async move {
            let outcome = EngineOutcome::Failed("no".into());
            let _ = events.send(EngineEvent::Finished(outcome)).await;
        })
    });
    let f = fixture(script).await;
    let project = f.core.store.thread(&f.thread).unwrap().project_id;
    let task = f
        .core
        .save_task(follow_up(&project, &f.thread, true), "t")
        .unwrap();
    let run = f.core.runs.admit(&f.thread, "go", "t").unwrap();
    until_terminal(&f.core, &run.id).await;
    f.core.run_due_tasks().await;
    let threads = f.core.store.list_threads(Some(&project), false).unwrap();
    assert_eq!(threads.len(), 1, "a failed run sets off nothing here");
    let kept = f.core.store.task(&task.id).unwrap();
    assert_eq!(kept.after_run.as_deref(), Some(run.id.as_str()));

    let mut itself = follow_up(&project, &f.thread, false);
    itself.thread_id = Some(f.thread.clone());
    assert!(f.core.save_task(itself, "t").is_err());
    let mut unnamed = follow_up(&project, &f.thread, false);
    unnamed.schedule.after_thread = None;
    assert!(f.core.save_task(unnamed, "t").is_err());
    let mut elsewhere = follow_up(&project, "nothing", false);
    elsewhere.name = "x".into();
    assert!(f.core.save_task(elsewhere, "t").is_err());
}
