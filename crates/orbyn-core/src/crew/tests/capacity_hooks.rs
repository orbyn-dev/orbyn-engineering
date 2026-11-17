//! The start hook of review capacity (slice 4l): the check before a worker is granted. Real `Core`, fake engines.

use std::sync::Arc;

use orbyn_contracts::ErrorCode;
use serde_json::json;

use super::harness::{Turn, World, brief, key, world};
use crate::crew::capacity::{CapacitySettings, Priority, Sampling};
use crate::crew::fixtures::merge_record;

fn project(w: &World) -> String {
    w.core.store.thread(&w.lead).unwrap().project_id
}

/// Starts `n` tasks and stores one waiting Medium merge for each, so `n` merges wait in the lead's project.
async fn waiting(w: &World, n: usize) {
    for i in 0..n {
        let (answer, error) = w
            .start(
                brief(json!({ "files": [{ "path": format!("src/w{i}/**"), "mode": "write" }], "outcome": format!("Task {i}.") })),
                key(),
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

fn limit(w: &World, n: u32) {
    w.core
        .store
        .set_crew_capacity_settings(
            &project(w),
            CapacitySettings {
                limit: n,
                sampling: Sampling::Off,
            },
        )
        .unwrap();
}

async fn quiet() -> World {
    world(true, Arc::new(|_, _| Turn::Hang)).await
}

#[tokio::test]
async fn a_low_start_that_crew_began_is_refused_over_the_limit_with_the_waiting_count_and_the_limit()
 {
    let w = quiet().await;
    limit(&w, 2);
    waiting(&w, 3).await;
    let error = w
        .core
        .capacity_start_check(&project(&w), Some((Priority::Low, true)))
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::NotAuthorized);
    assert!(error.message.contains("3 merges wait"), "{}", error.message);
    assert!(error.message.contains("limit is 2"), "{}", error.message);
}

#[tokio::test]
async fn at_the_limit_a_low_crew_start_goes() {
    let w = quiet().await;
    limit(&w, 3);
    waiting(&w, 3).await;
    w.core
        .capacity_start_check(&project(&w), Some((Priority::Low, true)))
        .unwrap();
}

#[tokio::test]
async fn a_person_started_task_and_a_medium_or_high_task_go_over_the_limit() {
    let w = quiet().await;
    limit(&w, 1);
    waiting(&w, 3).await;
    let p = project(&w);
    for stored in [
        (Priority::Low, false),
        (Priority::Medium, true),
        (Priority::High, true),
    ] {
        w.core.capacity_start_check(&p, Some(stored)).unwrap();
    }
}

#[tokio::test]
async fn crew_start_is_unchanged_for_now_because_a_new_task_has_no_stored_priority_yet() {
    let w = quiet().await;
    limit(&w, 1);
    waiting(&w, 3).await;
    let (answer, error) = w
        .start(
            brief(json!({ "files": [{ "path": "src/new/**", "mode": "write" }], "outcome": "One more." })),
            key(),
        )
        .await;
    assert!(!error, "{answer}");
}

#[tokio::test]
async fn with_the_preview_flag_off_the_start_check_does_nothing() {
    let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
    limit(&w, 1);
    w.core
        .capacity_start_check(&project(&w), Some((Priority::Low, true)))
        .unwrap();
}
