//! Sampling at merge (slice 4l): off by default and then nothing is written; on, a batch of four Low merges closes
//! and one in four is picked at random, recorded once with its seed. High and Medium merges are never sampled.
//! Real `Core`, fake engines. No git is needed: the sampling step reads store rows only.

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;

use super::harness::{Turn, World, brief, key, world};
use crate::crew::capacity::{
    BATCH_SIZE, CapacitySettings, Sampling, close_batch, sample_after_merge,
};
use crate::crew::fixtures::merge_record;
use crate::store::MergeRecord;

const NOW: i64 = 1_800_000_000;

fn project(w: &World) -> String {
    w.core.store.thread(&w.lead).unwrap().project_id
}

fn set_sampling(w: &World, sampling: Sampling) {
    w.core
        .store
        .set_crew_capacity_settings(
            &project(w),
            CapacitySettings {
                limit: 8,
                sampling,
                pause_enabled: false,
            },
        )
        .unwrap();
}

/// Starts a task and stores one merge of `risk` for it, as if it had just landed.
async fn landed(w: &World, i: usize, risk: &str) -> MergeRecord {
    let (answer, error) = w
        .start(
            brief(json!({ "files": [{ "path": format!("src/s{i}/**"), "mode": "write" }], "outcome": format!("Task {i}.") })),
            key(),
        )
        .await;
    assert!(!error, "{answer}");
    let task = answer["task"].as_str().unwrap();
    let mut merge = merge_record(task, "/repo", "main", "x");
    merge.risk = risk.into();
    merge.crew_id = w.task(task).crew_id;
    merge.id = w.core.store.add_crew_merge(&merge).unwrap();
    merge
}

async fn quiet() -> World {
    world(true, Arc::new(|_, _| Turn::Hang)).await
}

fn rows(w: &World) -> i64 {
    w.core
        .store
        .reader()
        .query_row("SELECT COUNT(*) FROM crew_review_samples", [], |r| r.get(0))
        .unwrap()
}

#[tokio::test]
async fn with_sampling_off_nothing_is_written_however_many_low_merges_land() {
    let w = quiet().await;
    assert_eq!(BATCH_SIZE, 4);
    for i in 0..8 {
        let merge = landed(&w, i, "low").await;
        assert!(
            sample_after_merge(&w.core.store, &merge, NOW)
                .unwrap()
                .is_empty()
        );
    }
    assert_eq!(rows(&w), 0);
    assert!(
        w.core
            .store
            .crew_picked_for_review(&project(&w))
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn the_fourth_low_merge_closes_a_batch_and_one_is_picked_and_recorded_once() {
    let w = quiet().await;
    set_sampling(&w, Sampling::OneInFour);
    let mut all = Vec::new();
    let mut picked = Vec::new();
    for i in 0..4 {
        let merge = landed(&w, i, "low").await;
        picked = sample_after_merge(&w.core.store, &merge, NOW).unwrap();
        if i < 3 {
            assert!(picked.is_empty(), "the batch is still open");
            assert_eq!(rows(&w), 0);
        }
        all.push(merge);
    }
    assert_eq!(picked.len(), 1);
    assert_eq!(rows(&w), 4, "every task of the batch is recorded");
    let batch = w
        .core
        .store
        .crew_review_sample(&format!("batch-{}-{}", all[0].id, all[3].id))
        .unwrap()
        .unwrap();
    let chosen: Vec<_> = batch.tasks.iter().filter(|t| t.chosen).collect();
    assert_eq!(chosen.len(), 1);
    assert_eq!(chosen[0].task_id, picked[0]);
    // The picked merge waits for a person, so the capacity state lists it; nothing else changed on the merge.
    assert_eq!(
        w.core.crew_capacity_state(&project(&w)).unwrap().picked,
        picked
    );
    // A later merge starts the next batch and does not touch the recorded one.
    let next = landed(&w, 4, "low").await;
    assert!(
        sample_after_merge(&w.core.store, &next, NOW)
            .unwrap()
            .is_empty()
    );
    assert_eq!(rows(&w), 4);
}

#[tokio::test]
async fn eight_low_merges_make_two_batches_and_two_picks_and_a_replay_changes_nothing() {
    let w = quiet().await;
    set_sampling(&w, Sampling::OneInFour);
    let mut picks = 0;
    let mut last = None;
    for i in 0..8 {
        let merge = landed(&w, i, "low").await;
        picks += sample_after_merge(&w.core.store, &merge, NOW)
            .unwrap()
            .len();
        last = Some(merge);
    }
    assert_eq!(picks, 2);
    assert_eq!(rows(&w), 8);
    // The same merge again finds no open batch: a recorded batch is never drawn twice.
    assert!(
        sample_after_merge(&w.core.store, &last.unwrap(), NOW)
            .unwrap()
            .is_empty()
    );
    assert_eq!(rows(&w), 8);
}

#[tokio::test]
async fn high_and_medium_merges_are_never_sampled_or_counted_toward_a_batch() {
    let w = quiet().await;
    set_sampling(&w, Sampling::OneInFour);
    let mut low = Vec::new();
    for i in 0..3 {
        low.push(landed(&w, i, "low").await);
    }
    for (i, risk) in ["high", "medium", "high", "medium"].iter().enumerate() {
        let merge = landed(&w, 10 + i, risk).await;
        assert!(
            sample_after_merge(&w.core.store, &merge, NOW)
                .unwrap()
                .is_empty()
        );
    }
    assert_eq!(rows(&w), 0, "three Low merges do not make a batch");
    let fourth = landed(&w, 20, "low").await;
    let picked = sample_after_merge(&w.core.store, &fourth, NOW).unwrap();
    assert_eq!(picked.len(), 1);
    let in_batch: Vec<String> = low
        .iter()
        .chain([&fourth])
        .map(|m| m.task_id.clone())
        .collect();
    assert!(in_batch.contains(&picked[0]));
    assert_eq!(rows(&w), 4, "only the four Low merges are recorded");
}

#[tokio::test]
async fn the_same_seed_picks_the_same_merges_and_the_choice_is_close_batch_not_the_workers() {
    let w = quiet().await;
    let mut batch = Vec::new();
    for i in 0..8 {
        batch.push(landed(&w, i, "low").await);
    }
    // A batch of eight picks ceil(8 / 4) = 2, recorded once with its seed.
    let first = close_batch(&w.core.store, &batch, Sampling::OneInFour, 42, NOW).unwrap();
    assert_eq!(first.len(), 2);
    assert_eq!(rows(&w), 8);
    let stored = w
        .core
        .store
        .crew_review_sample(&format!("batch-{}-{}", batch[0].id, batch[7].id))
        .unwrap()
        .unwrap();
    assert_eq!(stored.seed, 42);
    assert!(close_batch(&w.core.store, &batch, Sampling::OneInFour, 42, NOW).is_err());
    assert_eq!(rows(&w), 8, "a second record is refused");
    // Off plans nothing and writes nothing.
    assert!(
        close_batch(&w.core.store, &batch[..4], Sampling::Off, 1, NOW)
            .unwrap()
            .is_empty()
    );
    assert_eq!(rows(&w), 8);
}

#[tokio::test]
async fn the_choice_depends_on_the_seed_alone_never_on_the_merges_risk_reasons_or_size() {
    // Two worlds with the same seed pick the same positions in the batch, whatever the merges look like.
    let (a, b) = (quiet().await, quiet().await);
    let (mut batch_a, mut batch_b) = (Vec::new(), Vec::new());
    for i in 0..8 {
        let mut x = landed(&a, i, "low").await;
        x.lines = 1;
        batch_a.push(x);
        let mut y = landed(&b, i, "low").await;
        y.lines = 10_000;
        y.files = 99;
        batch_b.push(y);
    }
    let position = |batch: &[MergeRecord], picked: &[String]| -> Vec<usize> {
        picked
            .iter()
            .map(|t| batch.iter().position(|m| &m.task_id == t).unwrap())
            .collect()
    };
    let pa = close_batch(&a.core.store, &batch_a, Sampling::OneInFour, 9, NOW).unwrap();
    let pb = close_batch(&b.core.store, &batch_b, Sampling::OneInFour, 9, NOW).unwrap();
    assert_eq!(position(&batch_a, &pa), position(&batch_b, &pb));
}

#[tokio::test]
async fn the_merge_hook_samples_in_its_detached_task_and_logs_instead_of_failing() {
    let w = quiet().await;
    set_sampling(&w, Sampling::OneInFour);
    for i in 0..4 {
        let merge = landed(&w, i, "low").await;
        // `/repo` is not a repository: provenance fails and is written to the ledger, and sampling still runs.
        w.core.capacity_after_merge(&merge, NOW);
    }
    for _ in 0..100 {
        if rows(&w) == 4 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(rows(&w), 4);
    assert_eq!(
        w.core
            .crew_capacity_state(&project(&w))
            .unwrap()
            .picked
            .len(),
        1
    );
}

#[tokio::test]
async fn with_the_preview_flag_off_the_hook_samples_nothing() {
    let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
    let merge = merge_record("t", "/repo", "main", "x");
    w.core.capacity_after_merge(&merge, NOW);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(rows(&w), 0);
}
