//! The two hooks of review capacity (slice 4l): the start check before a worker is granted and the provenance write after a
//! merge row is stored. Real `Core`, fake engines, real git in a temporary repository. Nothing is pushed.

use std::sync::Arc;
use std::time::Duration;

use orbyn_contracts::ErrorCode;
use serde_json::json;

use super::autopilot_merge::{merging, passed};
use super::harness::{Turn, World, brief, key, world};
use crate::crew::capacity::{
    Author, CapacitySettings, Priority, Sampling, record_at_merge, record_provenance,
};
use crate::crew::fixtures::merge_record;
use crate::crew::{MergeOutcome, Mode};

const NOW: i64 = 1_800_000_000;

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
                pause_enabled: true,
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

/// Waits for the background provenance write, at most about five seconds.
async fn hunks_of(w: &World, task: &str) -> Vec<crate::store::StoredHunk> {
    let id = w.core.store.crew_merge(task).unwrap().unwrap().id;
    for _ in 0..100 {
        let hunks = w.core.store.crew_hunks_of_merge(id).unwrap();
        if !hunks.is_empty() {
            return hunks;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Vec::new()
}

/// Waits for a `provenance_failed` row on the task, at most about five seconds.
async fn failures_of(w: &World, task: &str) -> Vec<crate::store::AutopilotEvent> {
    for _ in 0..100 {
        let rows: Vec<_> = w
            .core
            .store
            .crew_autopilot_events(task)
            .unwrap()
            .into_iter()
            .filter(|e| e.kind == "provenance_failed")
            .collect();
        if !rows.is_empty() {
            return rows;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Vec::new()
}

#[tokio::test]
async fn a_merge_through_the_queue_records_its_hunks_and_with_no_person_commit_every_line_is_the_agents()
 {
    let m = merging(Mode::On, |_| vec![("src/feature.txt", "feature\n")]).await;
    let id = passed(&m.w, &["src/**"], "Add a feature.").await;
    m.w.core
        .crew_queue_merge(&id, &m.target, NOW)
        .await
        .unwrap();
    let outcomes = m.process(NOW + 1).await;
    assert!(matches!(&outcomes[..], [MergeOutcome::Merged { .. }]));
    let hunks = hunks_of(&m.w, &id).await;
    assert_eq!(hunks.len(), 1, "{hunks:?}");
    assert_eq!(hunks[0].hunk.hunk.path, "src/feature.txt");
    assert!(matches!(
        &hunks[0].hunk.author,
        Author::Agent { engine, .. } if engine == "alpha"
    ));
    let rows = m.w.core.store.crew_autopilot_events(&id).unwrap();
    assert!(
        rows.iter().all(|e| e.kind != "provenance_failed"),
        "a good write leaves no failure row"
    );
}

#[tokio::test]
async fn a_failing_provenance_write_is_recorded_in_the_ledger_and_does_not_fail_or_undo_the_merge()
{
    let m = merging(Mode::On, |_| vec![("src/feature.txt", "feature\n")]).await;
    // Every hunk insert aborts, so the provenance write fails after the merge row is stored.
    m.w.core
        .store
        .reader()
        .execute_batch(
            "CREATE TRIGGER no_hunks BEFORE INSERT ON crew_hunks BEGIN SELECT RAISE(ABORT, 'refused'); END;",
        )
        .unwrap();
    let id = passed(&m.w, &["src/**"], "Add a feature.").await;
    m.w.core
        .crew_queue_merge(&id, &m.target, NOW)
        .await
        .unwrap();
    let before = m.tip().await;
    let outcomes = m.process(NOW + 1).await;
    assert!(matches!(&outcomes[..], [MergeOutcome::Merged { .. }]));
    assert_ne!(m.tip().await, before, "the branch moved");
    let failed = failures_of(&m.w, &id).await;
    assert_eq!(failed.len(), 1, "the failure path was taken: {failed:?}");
    assert_eq!(failed[0].body["stage"], "hunks");
    assert_eq!(failed[0].body["resolved"], false);
    let merge = m.w.core.store.crew_merge(&id).unwrap().unwrap();
    assert_eq!(merge.review_state, "waiting", "the merge row is intact");
    assert!(
        m.w.core
            .store
            .crew_hunks_of_merge(merge.id)
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn provenance_for_a_range_git_cannot_read_returns_the_error_the_detached_task_discards() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let merge = merge_record("task", &w.repo.to_string_lossy(), "main", "x");
    let agent = Author::Agent {
        engine: "alpha".into(),
        model: None,
    };
    assert!(
        record_at_merge(&w.core.store, &merge, &agent, &[], NOW)
            .await
            .is_err()
    );
    assert!(
        record_provenance(&w.core.store, &merge, agent, NOW)
            .await
            .is_err()
    );
}
