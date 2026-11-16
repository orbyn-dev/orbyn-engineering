//! A breaker that failed after a revert is not silent (slice 4j, enforcement). The revert already moved the branch and stays, so the
//! failure is written to the approval ledger as `area_breaker_failed` on the merge's task, where Flow shows it, and the revert is
//! looked at again at the next merge or revert. Every breaker write is idempotent (one row per cause and kind), so the second look
//! records only what the first did not. A retry that works is marked `resolved: true` on the same kind, so it is not tried again.

use std::collections::BTreeSet;

use serde_json::json;

use crate::app::Core;
use crate::error::CoreError;

/// The most unresolved failures one retry looks at.
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
                "retry": if revert.is_some() {
                    "at the next merge or revert"
                } else {
                    "not retried: the next evaluation reads the whole window"
                },
                "resolved": false,
            }),
        );
    }

    /// Looks again at every revert whose breaker failed and has not since worked. Called under the merge lock before a merge or a
    /// revert is evaluated. A failure here is logged only: the first failure is already in the ledger.
    pub(crate) fn area_breaker_retry(&self, now: i64) {
        let Ok(events) = self.store.crew_unresolved_breaker_failures(READ) else {
            return;
        };
        let mut seen = BTreeSet::new();
        for event in &events {
            let Some(revert) = event.body["revert"].as_str().map(str::to_owned) else {
                continue;
            };
            if !seen.insert(revert.clone()) {
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
