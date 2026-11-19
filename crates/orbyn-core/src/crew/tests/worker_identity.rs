//! A crew worker's run is handed the task it works, so its engine process can carry the worker git identity, and the
//! worker's brief asks for the `Orbyn-Task` trailer. A run that is not a worker's carries neither. The fake engine
//! starts no process: `agents/driver/worker_identity.rs` tests the variables and the drivers add them.

use std::sync::Arc;

use serde_json::json;

#[cfg(target_os = "macos")]
use super::harness::world_confined;
use super::harness::{Turn, brief, key, world};

async fn check(w: super::harness::World) {
    let (answer, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "{answer}");
    let task = answer["task"].as_str().unwrap().to_owned();
    let worker = w.worker(&task);
    w.until("the worker's run started", |w| {
        w.crew_tasks
            .lock()
            .unwrap()
            .iter()
            .any(|(t, _)| *t == worker)
    })
    .await;
    let seen = w.crew_tasks.lock().unwrap().clone();
    for (thread, crew_task) in &seen {
        if *thread == worker {
            assert_eq!(
                crew_task.as_deref(),
                Some(task.as_str()),
                "the worker's run names its task"
            );
        } else {
            assert_eq!(
                *crew_task, None,
                "a run that is not a worker's has no task: {thread}"
            );
        }
    }
    let prompts = w.sent_to(&worker);
    let trailer = format!("Orbyn-Task: {task}");
    assert!(
        prompts.iter().any(|text| text.contains(&trailer)),
        "the brief asks for the trailer with the task id: {prompts:?}"
    );
}

#[tokio::test]
async fn a_worker_run_names_its_task_and_the_brief_asks_for_the_trailer() {
    check(world(true, Arc::new(|_, _| Turn::Hang)).await).await;
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn a_confined_worker_run_names_its_task_too() {
    check(world_confined(Arc::new(|_, _| Turn::Hang)).await).await;
}
