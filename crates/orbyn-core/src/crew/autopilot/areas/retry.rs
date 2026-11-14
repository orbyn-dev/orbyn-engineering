//! A breaker that failed after a revert is not silent (slice 4j, enforcement). The revert already moved the branch and stays, so the
//! failure is written to the approval ledger as `area_breaker_failed` on the merge's task, where Flow shows it, and the revert is
//! looked at again at the next merge or revert. Every breaker write is idempotent (one row per cause and kind), so the second look
//! records only what the first did not. A retry that works is marked `resolved: true` on the same kind, so it is not tried again.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use crate::app::Core;
use crate::error::CoreError;

/// The newest failures one retry reads.
const READ: u32 = 200;

impl Core {
    /// Writes the failure where the person sees it. `revert` is the revert commit the breaker could not record, for a retry.
    pub(crate) fn area_breaker_failed(
        &self,
        task_id: &str,
        stage: &str,
        revert: Option<&str>,
        error: &CoreError,
    ) {
        tracing::warn!(%error, stage, "the area breaker failed");
        self.merge_event(
            task_id,
            "area_breaker_failed",
            &json!({
                "stage": stage,
                "revert": revert,
                "why": error.public_message(),
                "retry": "at the next merge or revert",
                "resolved": false,
            }),
        );
    }

    /// Looks again at every revert whose breaker failed and has not since worked. Called under the merge lock before a merge or a
    /// revert is evaluated. A failure here is logged only: the first failure is already in the ledger.
    pub(crate) fn area_breaker_retry(&self, now: i64) {
        let Ok((events, _)) =
            self.store
                .crew_autopilot_events_page(None, &["area_breaker_failed"], READ, 0)
        else {
            return;
        };
        let commit = |body: &Value| -> Option<String> {
            (body["stage"] == "revert")
                .then(|| body["revert"].as_str().map(str::to_owned))
                .flatten()
        };
        let resolved: BTreeSet<String> = events
            .iter()
            .filter(|e| e.body["resolved"] == json!(true))
            .filter_map(|e| commit(&e.body))
            .collect();
        let mut seen = BTreeSet::new();
        for event in events.iter().filter(|e| e.body["resolved"] != json!(true)) {
            let Some(revert) = commit(&event.body) else {
                continue;
            };
            if resolved.contains(&revert) || !seen.insert(revert.clone()) {
                continue;
            }
            let Ok(Some(record)) = self.store.crew_merge(&event.task_id) else {
                continue;
            };
            if record.reverted_commit.as_deref() != Some(revert.as_str()) {
                continue;
            }
            match self.area_apply_revert(&record, &revert, now) {
                Ok(()) => self.merge_event(
                    &event.task_id,
                    "area_breaker_failed",
                    &json!({ "stage": "revert", "revert": revert, "resolved": true }),
                ),
                Err(error) => tracing::warn!(%error, "the area breaker still fails for a revert"),
            }
        }
    }
}
