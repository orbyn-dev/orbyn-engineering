//! A person's way out of a forge merge in `needs_person`. The lane skips such an entry (see `forge.rs`); this module is what
//! moves it: reject it, send it back to be checked again, or record that the forge already has the merge. It also finishes a
//! rejected intent whose merge request may still land, so a late merge is never lost.

use std::path::Path;
use std::sync::Arc;

use orbyn_contracts as wire;

use super::forge::identity_matches;
use super::queue::{MergeOutcome, MergeTarget};
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::source_control::forge::crew::{
    CrewPullReceipt, CrewRepository, crew_base, crew_find_pull, crew_pull,
};
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
    /// the answer, since rejecting a merge the forge already made would leave it unrecorded. No merge, publication or other write
    /// is ever sent to the forge. What a reject guarantees depends on how far the intent got: with a PR it is left as it is on the
    /// forge; if a merge request may have been sent it stays reconcilable (a later merge is still recorded); with no PR it
    /// is looked for first.
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
        let retry = body.action == wire::CrewForgeAction::Retry;
        if retry && intent.merge_sent {
            return Err(CoreError::Invalid(
                "a merge request may already be on the forge, so it cannot be sent again; reject it, or finish or close the PR on the forge".into(),
            ));
        }
        let receipt = self.needs_person_receipt(&mut intent).await?;
        if let Some(receipt) = receipt
            .as_ref()
            .filter(|r| r.state == "merged" && identity_matches(&intent, r))
        {
            let commit = receipt.merge_commit.clone().ok_or_else(|| {
                CoreError::Invalid("the forge merged it but names no merge commit".into())
            })?;
            self.settle_merged(
                &entry,
                &mut intent,
                commit,
                "user",
                "person route: the forge already had the merge",
                unix_now(),
            )
            .await?;
            return Ok(resolved(&intent, "merged"));
        }
        let resolution = if retry {
            // A retry only makes sense for a PR the pipeline can still drive: same identity, still open, and the pinned base
            // still the forge's base (a moved base just sends the intent straight back to `needs_person`).
            if let Some(receipt) = &receipt
                && (receipt.state != "open" || !identity_matches(&intent, receipt))
            {
                return Err(CoreError::Invalid(
                    "the PR is closed or its head changed; reject this merge instead".into(),
                ));
            }
            let context = CrewRepository::from_remote(&intent.remote)?;
            if crew_base(&context, Path::new(&intent.repo), &intent.branch).await? != intent.base {
                return Err(CoreError::Invalid(
                    "the base moved: reject this merge and queue the task again".into(),
                ));
            }
            ForgeResolution::Retry
        } else {
            ForgeResolution::Reject
        };
        let outcome = if retry { "retry" } else { "rejected" };
        let saved = self.store.resolve_crew_forge_merge(
            task_id,
            intent.revision,
            &resolution,
            "user",
            "person route",
        )?;
        self.merge_event(
            task_id,
            if retry { "merge_queued" } else { "merge_rejected" },
            &serde_json::json!({"forge_resolved": outcome, "by": "person", "actor": "local_caller"}),
        );
        Ok(resolved(&saved, outcome))
    }

    /// The receipt of a waiting intent's PR. With no PR number but evidence of a publication attempt, the forge is searched by the
    /// pinned source and target, and a PR found is recorded on the intent (same phase). `None` only when no PR exists.
    async fn needs_person_receipt(
        &self,
        intent: &mut ForgeMerge,
    ) -> Result<Option<CrewPullReceipt>> {
        if intent.pull.is_none() && intent.combined.is_none() {
            // Nothing was ever published: there is no PR to look for, and no forge is read.
            return Ok(None);
        }
        let context = CrewRepository::from_remote(&intent.remote)?;
        let repo = std::path::PathBuf::from(&intent.repo);
        let repo = repo.as_path();
        if intent.pull.is_none()
            && intent.combined.is_some()
            && let Some(number) =
                crew_find_pull(&context, repo, &intent.source, &intent.branch).await?
        {
            intent.pull = Some(number);
            self.store.save_crew_forge_merge(intent)?;
        }
        match intent.pull {
            Some(number) => Ok(Some(crew_pull(&context, repo, number).await?)),
            None => Ok(None),
        }
    }

    /// The forge already merged this intent's PR (a person did it by hand, or a request outlived a reject): record the receipt
    /// and the merge. The intent is saved as `merged` first, in its own audited transaction; a crash before the record is
    /// reconciled from the forge on the next pass.
    pub(super) async fn settle_merged(
        &self,
        entry: &MergeEntry,
        intent: &mut ForgeMerge,
        merge_commit: String,
        actor: &str,
        via: &str,
        now: i64,
    ) -> Result<MergeOutcome> {
        *intent = self.store.resolve_crew_forge_merge(
            &intent.task_id,
            intent.revision,
            &ForgeResolution::Merged { merge_commit },
            actor,
            via,
        )?;
        self.finish_forge_merge(entry, intent, now).await
    }

    /// Read-only look at a waiting intent's PR. `Some` only when the forge says that exact PR is merged.
    pub(super) async fn settle_hand_merged(
        &self,
        entry: &MergeEntry,
        intent: &mut ForgeMerge,
        actor: &str,
        via: &str,
        now: i64,
    ) -> Result<Option<MergeOutcome>> {
        let Some(receipt) = self.needs_person_receipt(intent).await? else {
            return Ok(None);
        };
        if receipt.state != "merged" || !identity_matches(intent, &receipt) {
            return Ok(None);
        }
        let Some(commit) = receipt.merge_commit else {
            return Ok(None);
        };
        self.settle_merged(entry, intent, commit, actor, via, now)
            .await
            .map(Some)
    }

    /// A rejected intent whose merge request may have been sent: its PR may still merge later (a merge queue, auto-merge). A merge
    /// is recorded when it lands; a PR the forge shows closed without a merge settles it; an open one is left to a later pass.
    pub(super) async fn reconcile_rejected_intent(
        &self,
        entry: &MergeEntry,
        intent: &mut ForgeMerge,
        now: i64,
    ) -> Result<Option<MergeOutcome>> {
        let Some(number) = intent.pull else {
            return Ok(None);
        };
        let context = CrewRepository::from_remote(&intent.remote)?;
        let receipt = crew_pull(&context, Path::new(&intent.repo), number).await?;
        if !identity_matches(intent, &receipt) {
            // The request named the pinned head (`--match-head-commit`), so a PR that is no longer that one cannot be merged by it.
            *intent = self.store.resolve_crew_forge_merge(
                &intent.task_id,
                intent.revision,
                &ForgeResolution::Settle,
                "core",
                "lane reconcile: the PR is no longer the one that was requested",
            )?;
            return Ok(None);
        }
        if receipt.state == "merged"
            && let Some(commit) = receipt.merge_commit
        {
            return self
                .settle_merged(
                    entry,
                    intent,
                    commit,
                    "core",
                    "lane reconcile: a merge landed after the reject",
                    now,
                )
                .await
                .map(Some);
        }
        if receipt.state != "open" && receipt.state != "merged" {
            *intent = self.store.resolve_crew_forge_merge(
                &intent.task_id,
                intent.revision,
                &ForgeResolution::Settle,
                "core",
                "lane reconcile: the PR closed without a merge",
            )?;
        }
        Ok(None)
    }

    /// Every rejected intent of this target that may still merge. A failure to read one never fails the pass.
    pub(super) async fn reconcile_rejected_requests(
        &self,
        target: Option<&MergeTarget>,
        crew_id: Option<&str>,
        now: i64,
        outcomes: &mut Vec<MergeOutcome>,
    ) {
        let pending = self
            .store
            .crew_forge_rejected_pending()
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "forge intents awaiting reconciliation could not be listed");
                Vec::new()
            });
        for mut intent in pending {
            if target.is_some_and(|target| {
                intent.repo != target.repo.to_string_lossy() || intent.branch != target.branch
            }) {
                continue;
            }
            let Ok(Some(entry)) = self.store.merge_entry(&intent.task_id) else {
                continue;
            };
            if crew_id.is_some_and(|crew| entry.crew_id != crew) {
                continue;
            }
            match self
                .reconcile_rejected_intent(&entry, &mut intent, now)
                .await
            {
                Ok(Some(outcome)) => outcomes.push(outcome),
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(task = %intent.task_id, %error, "a rejected forge intent could not be reconciled");
                }
            }
        }
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
