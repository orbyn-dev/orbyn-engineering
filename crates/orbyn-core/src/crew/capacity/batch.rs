//! Sampling at merge: which Low merges of a project are picked for a closer look. A batch is the Low merges that
//! landed since the previous batch closed. It closes when it holds four, and one in four (rounded up) is picked at
//! random with a seed that is recorded with the set. Nothing is risk-weighted, High and Medium merges are never in a
//! batch, the choice is kept in `crew_review_samples` and is never given to the worker. With sampling off nothing is
//! read from the batch and nothing is written. There is no digest in the app yet, so a batch that never reaches four
//! Low merges stays open until it does.

use std::sync::Mutex;

use super::{Sampling, Seeded, choose, plan_size};
use crate::error::Result;
use crate::store::{MergeRecord, SampledTask, Store};

/// How many Low merges close a batch.
pub(crate) const BATCH_SIZE: usize = 4;

/// One batch is closed at a time in this process, so two merges that land together cannot draw overlapping batches.
static CLOSING: Mutex<()> = Mutex::new(());

/// Records `batch` (oldest first) and returns the task ids picked: `plan_size` of them, drawn with `seed`. Every
/// task of the batch is stored with whether it was picked. An empty batch or a batch already recorded changes nothing.
pub(crate) fn close_batch(
    store: &Store,
    batch: &[MergeRecord],
    sampling: Sampling,
    seed: u64,
    now: i64,
) -> Result<Vec<String>> {
    let (Some(first), Some(last)) = (batch.first(), batch.last()) else {
        return Ok(Vec::new());
    };
    let plan = plan_size(sampling, u32::try_from(batch.len()).unwrap_or(u32::MAX));
    if plan == 0 {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = batch.iter().map(|m| m.task_id.clone()).collect();
    let picked = choose(&ids, plan, &mut Seeded::new(seed));
    let tasks: Vec<SampledTask> = ids
        .iter()
        .map(|id| SampledTask {
            task_id: id.clone(),
            chosen: picked.contains(id),
        })
        .collect();
    store.record_crew_review_sample(
        &format!("batch-{}-{}", first.id, last.id),
        seed,
        now,
        &tasks,
    )?;
    Ok(picked)
}

/// After a merge is stored: when the person turned sampling on and the merge is Low, adds it to its project's open
/// batch and closes the batch once it holds four. Returns the task ids picked when a batch closed. The caller logs a
/// failure; it never reaches the merge.
pub(crate) fn sample_after_merge(
    store: &Store,
    merge: &MergeRecord,
    now: i64,
) -> Result<Vec<String>> {
    if merge.risk != "low" {
        return Ok(Vec::new());
    }
    let Some(project) = store.crew_project_of_task(&merge.task_id)? else {
        return Ok(Vec::new());
    };
    let sampling = store.crew_capacity_settings(&project)?.sampling;
    if sampling == Sampling::Off {
        return Ok(Vec::new());
    }
    let _one_at_a_time = CLOSING.lock().unwrap_or_else(|p| p.into_inner());
    let open = store.crew_open_low_merges(&project, merge.id)?;
    if open.len() < BATCH_SIZE {
        return Ok(Vec::new());
    }
    // The newest four, ending with this merge. Anything older than them (low merges that landed before sampling
    // was turned on) is left out for good.
    let batch = &open[open.len() - BATCH_SIZE..];
    let seed = u64::from_le_bytes(crate::util::random_bytes::<8>()?);
    close_batch(store, batch, sampling, seed, now)
}
