//! A person's way out of a forge merge in `needs_person`. The lane skips such an entry (see `forge.rs`); this module is the only
//! thing that moves it: reject it, send it back to be checked again, or record that the forge already has the merge.

use std::path::Path;
use std::sync::Arc;

use orbyn_contracts as wire;

use super::forge::identity_matches;
use super::queue::MergeOutcome;
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::source_control::forge::crew::{CrewRepository, crew_pull};
use crate::store::{ForgeMerge, ForgePhase, ForgeResolution, MergeEntry};

fn phase_word(phase: ForgePhase) -> String {
    serde_json::to_value(phase)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

impl Core {
    /// `POST /api/crew/autopilot/merge-queue/{id}/resolve`. Reads the forge first, because a person may have merged the PR by
    /// hand: then the intent is recorded as merged and the asked-for answer is not applied. A PR that cannot be read refuses
    /// the answer, since rejecting a merge the forge already made would leave it unrecorded. Nothing is ever sent to the forge.
    pub async fn crew_forge_resolve(
        self: &Arc<Self>,
        task_id: &str,
        body: &wire::CrewForgeResolve,
    ) -> Result<wire::CrewForgeResolved> {
        self.crew_preview_only()?;
        super::valid_id(task_id)?;
        let _serial = self.crew_merges.lock().await;
        let entry = self
            .store
            .merge_entry(task_id)?
            .ok_or_else(|| CoreError::NotFound(format!("queued merge {task_id}")))?;
        let mut intent = self
            .store
            .crew_forge_merge(task_id)?
            .ok_or_else(|| CoreError::Invalid("this merge has no forge intent".into()))?;
        if intent.revision != body.seen_revision || intent.phase != ForgePhase::NeedsPerson {
            return Err(CoreError::Stale(
                "this merge changed since you saw it; look again".into(),
            ));
        }
        let receipt = match intent.pull {
            Some(number) => {
                let context = CrewRepository::from_remote(&intent.remote)?;
                Some(crew_pull(&context, Path::new(&intent.repo), number).await?)
            }
            None => None,
        };
        if let Some(receipt) = receipt
            .as_ref()
            .filter(|r| r.state == "merged" && identity_matches(&intent, r))
        {
            let commit = receipt.merge_commit.clone().ok_or_else(|| {
                CoreError::Invalid("the forge merged it but names no merge commit".into())
            })?;
            self.settle_merged(&entry, &mut intent, commit, "user", unix_now())
                .await?;
            return Ok(resolved(&intent, "merged"));
        }
        let resolution = match body.action {
            wire::CrewForgeAction::Reject => ForgeResolution::Reject,
            wire::CrewForgeAction::Retry => {
                // A retry only makes sense for a PR the pipeline can still drive: same identity, still open.
                if let Some(receipt) = &receipt
                    && (receipt.state != "open" || !identity_matches(&intent, receipt))
                {
                    return Err(CoreError::Invalid(
                        "the PR is closed or its head changed; reject this merge instead".into(),
                    ));
                }
                ForgeResolution::Retry
            }
        };
        let outcome = match resolution {
            ForgeResolution::Reject => "rejected",
            _ => "retry",
        };
        let saved = self.store.resolve_crew_forge_merge(
            task_id,
            body.seen_revision,
            &resolution,
            "user",
        )?;
        self.merge_event(
            task_id,
            if outcome == "rejected" {
                "merge_rejected"
            } else {
                "merge_queued"
            },
            &serde_json::json!({"forge_resolved": outcome, "by": "person", "actor": "local_caller"}),
        );
        Ok(resolved(&saved, outcome))
    }

    /// The forge already merged this intent's PR (a person did it by hand): record the receipt and the merge. The intent is saved
    /// as `merged` first, in its own audited transaction; a crash before the record is reconciled from the forge on the next pass.
    pub(super) async fn settle_merged(
        &self,
        entry: &MergeEntry,
        intent: &mut ForgeMerge,
        merge_commit: String,
        actor: &str,
        now: i64,
    ) -> Result<MergeOutcome> {
        *intent = self.store.resolve_crew_forge_merge(
            &intent.task_id,
            intent.revision,
            &ForgeResolution::Merged { merge_commit },
            actor,
        )?;
        self.finish_forge_merge(entry, intent, now).await
    }

    /// Read-only look at a `needs_person` intent's PR. `Some` only when the forge says that exact PR is merged.
    pub(super) async fn settle_hand_merged(
        &self,
        entry: &MergeEntry,
        intent: &mut ForgeMerge,
        actor: &str,
        now: i64,
    ) -> Result<Option<MergeOutcome>> {
        let Some(number) = intent.pull else {
            return Ok(None);
        };
        let context = CrewRepository::from_remote(&intent.remote)?;
        let receipt = crew_pull(&context, Path::new(&intent.repo), number).await?;
        if receipt.state != "merged" || !identity_matches(intent, &receipt) {
            return Ok(None);
        }
        let Some(commit) = receipt.merge_commit else {
            return Ok(None);
        };
        self.settle_merged(entry, intent, commit, actor, now)
            .await
            .map(Some)
    }
}

fn unix_now() -> i64 {
    crate::util::unix_now()
}

fn resolved(intent: &ForgeMerge, outcome: &str) -> wire::CrewForgeResolved {
    wire::CrewForgeResolved {
        task_id: intent.task_id.clone(),
        outcome: outcome.into(),
        phase: phase_word(intent.phase),
        revision: intent.revision,
    }
}
