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
async fn hooks_finishing_out_of_order_drop_no_merge() {
    let w = quiet().await;
    set_sampling(&w, Sampling::OneInFour);
    let mut merges = Vec::new();
    for i in 0..5 {
        merges.push(landed(&w, i, "low").await);
    }
    // Merge 5's hook finishes before merge 4's (and 1 to 3 never ran theirs).
    let picked = sample_after_merge(&w.core.store, &merges[4], NOW).unwrap();
    assert_eq!(picked.len(), 1);
    assert!(
        sample_after_merge(&w.core.store, &merges[3], NOW)
            .unwrap()
            .is_empty()
    );
    let recorded = |m: &MergeRecord| {
        w.core
            .store
            .reader()
            .query_row(
                "SELECT COUNT(*) FROM crew_review_samples WHERE task_id = ?1",
                [&m.task_id],
                |r| r.get::<_, i64>(0),
            )
            .unwrap()
    };
    // The oldest four are the batch; the fifth is still open. None is dropped.
    assert!(merges[..4].iter().all(|m| recorded(m) == 1));
    assert_eq!(recorded(&merges[4]), 0);
    assert_eq!(
        w.core
            .store
            .crew_open_low_merges(&project(&w), merges[4].id, 0)
            .unwrap()
            .len(),
        1
    );
    // The next three complete the next batch, with the fifth in it.
    for i in 5..8 {
        merges.push(landed(&w, i, "low").await);
    }
    assert_eq!(
        sample_after_merge(&w.core.store, &merges[7], NOW)
            .unwrap()
            .len(),
        1
    );
    assert!(merges.iter().all(|m| recorded(m) == 1));
}

/// Saves sampling and pins the time of that settings audit row, so a test fixes the boundary instead of reading
/// the clock. The audit log refuses edits, so the guard is dropped in this test database only.
fn set_sampling_at(w: &World, sampling: Sampling, limit: u32, at: i64) {
    w.core
        .store
        .set_crew_capacity_settings(
            &project(w),
            CapacitySettings {
                limit,
                sampling,
                pause_enabled: false,
            },
        )
        .unwrap();
    let ts = time::OffsetDateTime::from_unix_timestamp(at)
        .unwrap()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let conn = w.core.store.reader();
    conn.execute_batch("DROP TRIGGER IF EXISTS audit_events_no_update")
        .unwrap();
    conn.execute(
        "UPDATE audit_events SET ts = ?1 WHERE id = (SELECT MAX(id) FROM audit_events WHERE action = 'crew.capacity.settings')",
        [ts],
    )
    .unwrap();
}

/// Like `landed`, with the merge time fixed.
async fn landed_at(w: &World, i: usize, risk: &str, at: i64) -> MergeRecord {
    let merge = landed(w, i, risk).await;
    w.core
        .store
        .reader()
        .execute(
            "UPDATE crew_merges SET merged_unix = ?1 WHERE id = ?2",
            rusqlite::params![at, merge.id],
        )
        .unwrap();
    MergeRecord {
        merged_unix: at,
        ..merge
    }
}

fn recorded(w: &World, m: &MergeRecord) -> i64 {
    w.core
        .store
        .reader()
        .query_row(
            "SELECT COUNT(*) FROM crew_review_samples WHERE task_id = ?1",
            [&m.task_id],
            |r| r.get(0),
        )
        .unwrap()
}

const T1: i64 = 1_800_000_000;

#[tokio::test]
async fn a_backlog_from_before_sampling_was_on_is_never_batched() {
    let w = quiet().await;
    let mut old = Vec::new();
    for i in 0..9 {
        old.push(landed_at(&w, i, "low", T1 - 100 + i as i64).await);
    }
    set_sampling_at(&w, Sampling::OneInFour, 8, T1);
    assert_eq!(
        w.core.store.crew_sampling_since(&project(&w)).unwrap(),
        Some(T1)
    );
    // A new merge lands: the nine old ones are not a batch and nothing is picked or recorded.
    let first = landed_at(&w, 20, "low", T1 + 1).await;
    assert!(
        sample_after_merge(&w.core.store, &first, NOW)
            .unwrap()
            .is_empty()
    );
    assert_eq!(rows(&w), 0);
    // Three more after the boundary close one batch of four, and the old merges are not in it.
    let mut fresh = vec![first];
    let mut picked = Vec::new();
    for i in 21..24 {
        let m = landed_at(&w, i, "low", T1 + 2).await;
        picked = sample_after_merge(&w.core.store, &m, NOW).unwrap();
        fresh.push(m);
    }
    assert_eq!(
        picked.len(),
        1,
        "a batch of four after the boundary picks one"
    );
    assert_eq!(rows(&w), 4);
    assert!(fresh.iter().all(|m| recorded(&w, m) == 1));
    assert!(old.iter().all(|m| recorded(&w, m) == 0));
}

#[tokio::test]
async fn sampling_off_then_on_again_resets_the_boundary() {
    let w = quiet().await;
    set_sampling_at(&w, Sampling::OneInFour, 8, T1);
    // Landed while on, but the person then turned sampling off and on again.
    let kept_open: Vec<MergeRecord> = {
        let mut v = Vec::new();
        for i in 0..3 {
            v.push(landed_at(&w, i, "low", T1 + 10).await);
        }
        v
    };
    set_sampling_at(&w, Sampling::Off, 8, T1 + 1000);
    assert_eq!(
        w.core.store.crew_sampling_since(&project(&w)).unwrap(),
        None
    );
    let while_off = landed_at(&w, 10, "low", T1 + 1500).await;
    assert!(
        sample_after_merge(&w.core.store, &while_off, NOW)
            .unwrap()
            .is_empty()
    );
    set_sampling_at(&w, Sampling::OneInFour, 8, T1 + 2000);
    assert_eq!(
        w.core.store.crew_sampling_since(&project(&w)).unwrap(),
        Some(T1 + 2000)
    );
    let mut last = None;
    let mut picked = Vec::new();
    let mut fresh = Vec::new();
    for i in 20..24 {
        let m = landed_at(&w, i, "low", T1 + 2010).await;
        picked = sample_after_merge(&w.core.store, &m, NOW).unwrap();
        fresh.push(m.clone());
        last = Some(m);
    }
    assert!(last.is_some());
    assert_eq!(picked.len(), 1);
    assert_eq!(rows(&w), 4);
    assert!(fresh.iter().all(|m| recorded(&w, m) == 1));
    assert!(kept_open.iter().all(|m| recorded(&w, m) == 0));
    assert_eq!(recorded(&w, &while_off), 0);
    // Off and on a third time moves the boundary again.
    set_sampling_at(&w, Sampling::Off, 8, T1 + 3000);
    set_sampling_at(&w, Sampling::OneInFour, 8, T1 + 4000);
    assert_eq!(
        w.core.store.crew_sampling_since(&project(&w)).unwrap(),
        Some(T1 + 4000)
    );
}

#[tokio::test]
async fn saving_other_settings_with_sampling_still_on_keeps_the_boundary() {
    let w = quiet().await;
    set_sampling_at(&w, Sampling::OneInFour, 8, T1);
    set_sampling_at(&w, Sampling::OneInFour, 5, T1 + 500);
    assert_eq!(
        w.core.store.crew_sampling_since(&project(&w)).unwrap(),
        Some(T1)
    );
    let mut picked = Vec::new();
    for i in 0..4 {
        let m = landed_at(&w, i, "low", T1 + 10).await;
        picked = sample_after_merge(&w.core.store, &m, NOW).unwrap();
    }
    assert_eq!(
        picked.len(),
        1,
        "merges after the first turn-on are still sampled"
    );
}

#[tokio::test]
async fn a_merge_landing_in_the_boundary_second_counts_as_after_and_one_second_earlier_does_not() {
    let w = quiet().await;
    set_sampling_at(&w, Sampling::OneInFour, 8, T1);
    let early = landed_at(&w, 0, "low", T1 - 1).await;
    let mut all = Vec::new();
    for i in 1..4 {
        all.push(landed_at(&w, i, "low", T1).await);
    }
    assert!(
        sample_after_merge(&w.core.store, &early, NOW)
            .unwrap()
            .is_empty()
    );
    assert!(
        sample_after_merge(&w.core.store, &all[2], NOW)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        rows(&w),
        0,
        "three merges from the boundary on are not a batch of four"
    );
    assert_eq!(recorded(&w, &early), 0);
}

#[tokio::test]
async fn with_the_preview_flag_off_the_hook_samples_nothing_though_the_same_merges_would_be() {
    let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
    set_sampling(&w, Sampling::OneInFour);
    // The flag is off, so no task can be started: store four tasks of the lead's project and four Low merges.
    let mut merges = Vec::new();
    for n in 0..4u32 {
        let task = format!("sha256:{n:064x}");
        w.core
            .store
            .reader()
            .execute(
                "INSERT INTO crew_tasks (id, crew_id, run_id, created_at, updated_at) VALUES (?1, 'crew-1', ?2, 't', 't')",
                rusqlite::params![task, format!("run-{n}")],
            )
            .unwrap();
        w.core
            .store
            .add_crew_run(&task, &w.lead, "member-01")
            .unwrap();
        let mut merge = merge_record(&task, "/repo", "main", "x");
        merge.risk = "low".into();
        merge.id = w.core.store.add_crew_merge(&merge).unwrap();
        merges.push(merge);
    }
    for merge in &merges {
        w.core.capacity_after_merge(merge, NOW);
    }
    // With the flag off the hook returns before it spawns anything, so there is nothing to wait for.
    assert_eq!(rows(&w), 0);
    // Not vacuous: the same four merges are sampled when the step runs.
    assert_eq!(
        sample_after_merge(&w.core.store, &merges[3], NOW)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(rows(&w), 4);
}
