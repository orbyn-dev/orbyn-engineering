//! Forge final writer. The local queue is an explicitly separate compatibility path.

use std::sync::Arc;
use std::time::{Duration, Instant};

use orbyn_contracts::{CheckOutcome, MergePull};
use serde_json::json;

use super::gitops::{self, Scratch};
use super::queue::{MergeOutcome, MergeTarget};
use crate::app::Core;
use crate::crew::checks::{Ran, run_line};
use crate::error::{CoreError, Result};
use crate::source_control::forge::crew::{
    CrewRepository, crew_base, crew_ci_green, crew_find_pull, crew_pull,
};
use crate::source_control::{
    forge,
    git::{git, text},
};
use crate::store::{ForgeMerge, ForgePhase, MergeEntry, OPEN_MERGE_STATES};

/// Who asked for a forge lane pass, in the ledger's own words.
#[derive(Clone, Copy)]
pub(super) enum ForgeActor {
    /// A lead's merge tool, scoped to its crew.
    Lead,
    /// The local HTTP caller of the service: no proof of who.
    LocalCaller,
}

impl ForgeActor {
    fn as_str(self) -> &'static str {
        match self {
            Self::Lead => "lead",
            Self::LocalCaller => "local_caller",
        }
    }
}

fn blocked(task: &str, why: &str) -> MergeOutcome {
    MergeOutcome::Blocked {
        task_id: task.into(),
        why: why.into(),
    }
}

impl Core {
    /// The target is resolved from the caller's project, never from an agent-supplied path.
    pub async fn crew_queue_forge_merge(
        self: &Arc<Self>,
        task_id: &str,
        target: &MergeTarget,
        now: i64,
    ) -> Result<MergeEntry> {
        self.crew_preview_only()?;
        self.refuse_unrequested_correction(task_id)?;
        self.refuse_unproven_escape_check(task_id, None)?;
        if self.config.crew_merge_required_checks.is_empty() {
            return Err(CoreError::Invalid(
                "the person must configure the forge's required CI names before queueing".into(),
            ));
        }
        let _serial = self.crew_merges.lock().await;
        if let Some(intent) = self.store.crew_forge_merge(task_id)? {
            if intent.repo != target.repo.to_string_lossy().as_ref()
                || intent.branch != target.branch
            {
                return Err(CoreError::Invalid("the queued forge target differs".into()));
            }
            if let Some(entry) = self.store.merge_entry(task_id)? {
                return Ok(entry);
            }
            let correction = self
                .store
                .crew_correction_for_task(task_id)?
                .ok_or_else(|| {
                    CoreError::Invalid(
                        "forge intent has no queue entry; a person must reconcile it".into(),
                    )
                })?;
            let remote = text(&git(&target.repo, &["remote", "get-url", "origin"]).await?);
            let selected = CrewRepository::from_remote(&remote)?;
            let candidate = self
                .store
                .crew_results(task_id)?
                .into_iter()
                .rfind(|result| result.state == orbyn_contracts::ResultState::Passed)
                .and_then(|result| result.commit)
                .map(|commit| commit.as_str().to_owned());
            if intent.phase != ForgePhase::Prepared
                || correction.remote != remote
                || correction.repo != intent.repo
                || correction.branch != intent.branch
                || correction.base != intent.base
                || correction.candidate != intent.candidate
                || candidate.as_deref() != Some(intent.candidate.as_str())
                || crew_base(&selected, &target.repo, &target.branch).await? != intent.base
            {
                return Err(CoreError::Invalid(
                    "the interrupted corrective admission no longer matches its pinned target"
                        .into(),
                ));
            }
            self.correction_review_thread(
                &self.store.thread(
                    &self
                        .store
                        .crew_run(task_id)?
                        .ok_or_else(|| CoreError::NotFound("corrective run".into()))?
                        .lead_thread_id,
                )?,
                task_id,
            )
            .await
            .map_err(|error| CoreError::Invalid(error.message))?;
            return self.crew_queue_merge(task_id, target, now).await;
        }
        let remote = text(&git(&target.repo, &["remote", "get-url", "origin"]).await?);
        let selected = CrewRepository::from_remote(&remote)?;
        let base = crew_base(&selected, &target.repo, &target.branch).await?;
        if let Some(correction) = self.store.crew_correction_for_task(task_id)? {
            if correction.repo != target.repo.to_string_lossy()
                || correction.remote != remote
                || correction.branch != target.branch
                || correction.base != base
            {
                return Err(CoreError::Invalid(
                    "the corrective target moved; a person must reconcile the pinned task".into(),
                ));
            }
            self.correction_review_thread(
                &self.store.thread(
                    &self
                        .store
                        .crew_run(task_id)?
                        .ok_or_else(|| CoreError::NotFound("corrective run".into()))?
                        .lead_thread_id,
                )?,
                task_id,
            )
            .await
            .map_err(|error| CoreError::Invalid(error.message))?;
        }
        let result = self
            .store
            .crew_results(task_id)?
            .into_iter()
            .rfind(|r| r.state == orbyn_contracts::ResultState::Passed)
            .ok_or_else(|| CoreError::Invalid("the task has no passed result".into()))?;
        let candidate = result
            .commit
            .ok_or_else(|| CoreError::Invalid("the result names no commit".into()))?
            .as_str()
            .to_owned();
        // Fetch an object only: never update the person's branch or checkout.
        git(&target.repo, &["fetch", "--no-tags", &remote, &base]).await?;
        let intent = ForgeMerge {
            task_id: task_id.into(),
            repo: target.repo.to_string_lossy().into_owned(),
            remote,
            branch: target.branch.clone(),
            source: format!("crew/{}", task_id.replace(':', "-")),
            candidate,
            base,
            combined: None,
            pull: None,
            phase: ForgePhase::Prepared,
            merge_commit: None,
            evidence: Vec::new(),
            note: None,
            merge_sent: false,
            revision: 0,
        };
        // The intent pins the trusted base used by the normal holds and risk computation.
        self.store.add_crew_forge_merge(&intent)?;
        self.crew_queue_merge(task_id, target, now).await
    }

    /// Reconciles intent on every call. A previously requested effect is read, never blindly repeated.
    pub async fn crew_process_forge_merges(
        self: &Arc<Self>,
        target: &MergeTarget,
        now: i64,
    ) -> Result<Vec<MergeOutcome>> {
        self.process_forge_merges_for(target, None, now).await
    }

    pub(in crate::crew) async fn process_forge_merges_for(
        self: &Arc<Self>,
        target: &MergeTarget,
        crew_id: Option<&str>,
        now: i64,
    ) -> Result<Vec<MergeOutcome>> {
        let actor = if crew_id.is_some() {
            ForgeActor::Lead
        } else {
            ForgeActor::LocalCaller
        };
        let outcomes = self.process_forge_lane(target, crew_id, actor, now).await?;
        self.crew_watch_forge_main(target, crew_id, now).await?;
        Ok(outcomes)
    }

    pub(super) async fn process_forge_lane(
        self: &Arc<Self>,
        target: &MergeTarget,
        crew_id: Option<&str>,
        actor: ForgeActor,
        now: i64,
    ) -> Result<Vec<MergeOutcome>> {
        self.crew_preview_only()?;
        let actor = actor.as_str();
        let mut outcomes = Vec::new();
        {
            let _serial = self.crew_merges.lock().await;
            let entries = self.store.merge_entries(
                &target.repo.to_string_lossy(),
                &target.branch,
                &["queued", "blocked", "merging", "waiting"],
            )?;
            for entry in entries {
                let Some(mut intent) = self.store.crew_forge_merge(&entry.task_id)? else {
                    continue;
                };
                // Ordering rule: merges are ordered per repository, and a published or uncertain entry keeps the lane. An intent in
                // `needs_person` is different: it is not progressing, each entry pins its own base and is checked against the
                // forge's base, so no later entry depends on it, and a later merge only moves the base, which that intent would have
                // to rebuild on anyway. It waits for a person and the lane goes on past it, whichever crew owns it. What this does
                // not promise: an intent that reached `needs_person` after a merge request (`merge_sent`) may still have that
                // request on the forge; it is never sent again, and its PR is read below and recorded if it merged. A scoped lead
                // reads only its own crew's entries; another crew's is skipped without a forge read.
                if intent.phase == ForgePhase::NeedsPerson {
                    let own = crew_id.is_none_or(|crew| entry.crew_id == crew);
                    let waiting = MergeOutcome::NeedsPerson {
                        task_id: entry.task_id.clone(),
                        why: intent
                            .note
                            .clone()
                            .unwrap_or_else(|| "reconcile the forge intent".into()),
                    };
                    if !own {
                        outcomes.push(waiting);
                        continue;
                    }
                    match self
                        .settle_hand_merged(&entry, &mut intent, "core", "lane reconcile", now)
                        .await
                    {
                        Ok(Some(outcome)) => outcomes.push(outcome),
                        Ok(None) => outcomes.push(waiting),
                        Err(error) => {
                            tracing::warn!(task = %entry.task_id, %error, "a forge intent waiting on a person could not be settled; the lane goes on");
                            // The forge's merge may already be saved: say what is true, recovery finishes it.
                            let merged = self
                                .store
                                .crew_forge_merge(&entry.task_id)
                                .ok()
                                .flatten()
                                .is_some_and(|saved| saved.phase == ForgePhase::Merged);
                            outcomes.push(if merged {
                                blocked(
                                    &entry.task_id,
                                    &format!("the forge merged it; recording the merge failed and is retried: {}", error.public_message()),
                                )
                            } else {
                                waiting
                            });
                        }
                    }
                    continue;
                }
                // The repository lane outlives one caller/turn. A scoped lead cannot overtake another
                // crew's earlier intent, especially an effect whose remote result is still unknown.
                if crew_id.is_some_and(|crew| entry.crew_id != crew) {
                    outcomes.push(blocked(
                        &entry.task_id,
                        "an earlier intent from another crew owns this repository's merge lane",
                    ));
                    break;
                }
                let outcome = self
                    .process_forge_entry(&entry, target, &mut intent, actor, now)
                    .await?;
                let landed = matches!(outcome, MergeOutcome::Merged { .. });
                outcomes.push(outcome);
                // A published/uncertain first entry owns the lane; later candidates cannot overtake it.
                if !landed {
                    break;
                }
            }
            // A rejected intent whose merge request may still land is read for its PR, once per pass.
            self.reconcile_rejected_requests(Some(target), crew_id, now, &mut outcomes)
                .await;
        }
        Ok(outcomes)
    }

    async fn process_forge_entry(
        self: &Arc<Self>,
        entry: &MergeEntry,
        target: &MergeTarget,
        intent: &mut ForgeMerge,
        actor: &str,
        now: i64,
    ) -> Result<MergeOutcome> {
        let task = entry.task_id.as_str();
        let actual_remote = text(&git(&target.repo, &["remote", "get-url", "origin"]).await?);
        if intent.repo != target.repo.to_string_lossy().as_ref()
            || intent.branch != target.branch
            || intent.remote != actual_remote
            || intent.candidate != entry.candidate
        {
            return self.forge_needs_person(intent, "the forge target or candidate changed");
        }
        let selected = CrewRepository::from_remote(&intent.remote)?;
        if let Some(outcome) = self
            .reconcile_forge_intent(entry, target, intent, now)
            .await?
        {
            return Ok(outcome);
        }
        // A `needs_person` intent never reaches here: the lane skips it and a person's answer is the only way out.
        if intent.phase == ForgePhase::NeedsPerson {
            return Ok(MergeOutcome::NeedsPerson {
                task_id: task.into(),
                why: intent
                    .note
                    .clone()
                    .unwrap_or_else(|| "reconcile the forge intent".into()),
            });
        }
        // No person release can stand in for actual current-head CI.
        if self.config.crew_merge_required_checks.is_empty() {
            return Ok(blocked(task, "required forge CI policy is not configured"));
        }
        if let Some(why) = self.forge_review_precondition(entry, target).await? {
            return Ok(blocked(task, &why));
        }
        if let Some(why) = self.merge_blocker(entry, target)? {
            return Ok(blocked(task, &why));
        }
        let pinned = MergeTarget {
            repo: target.repo.clone(),
            branch: intent.base.clone(),
            owed: target.owed.clone(),
        };
        if let Some(why) = self.merge_hold_now(entry, &pinned, now).await? {
            return Ok(blocked(task, &why));
        }
        if crew_base(&selected, &target.repo, &intent.branch).await? != intent.base {
            return self.forge_needs_person(intent, "the target base moved; combined-state evidence must be rebuilt under a new reviewed task");
        }
        if intent.phase == ForgePhase::Prepared {
            // Checked again right before the first forge write: a correction needs a person's Revert answer.
            if let Err(error) = self.refuse_unrequested_correction(task) {
                return Ok(blocked(task, &error.public_message()));
            }
            let scratch = Scratch::at(
                &target.repo,
                &self.config.data_dir.join("crew-merge"),
                &intent.base,
            )
            .await?;
            let checked = self
                .check_forge_combination(&scratch, entry, intent, now)
                .await;
            scratch.finish().await;
            match checked {
                Ok(()) => {}
                // A failed combined-state check is a verdict, not a fault: stop here so the slow checks
                // do not run again on every call. A person or a new reviewed task decides what next.
                Err(CoreError::Invalid(why)) => return self.forge_needs_person(intent, &why),
                Err(error) => return Err(error),
            }
            // Durable before either effect. An interruption here is reconciled, not retried.
            intent.phase = ForgePhase::Publishing;
            self.store.save_crew_forge_merge(intent)?;
            self.merge_event(task, "merge_queued", &json!({"forge_publish_requested": true, "actor": actor, "head": intent.candidate, "base": intent.base}));
            if text(&git(&target.repo, &["remote", "get-url", "origin"]).await?) != intent.remote {
                return self.forge_needs_person(intent, "origin changed before publication");
            }
            git(
                &target.repo,
                &[
                    "push",
                    &intent.remote,
                    &format!("{}:refs/heads/{}", intent.candidate, intent.source),
                ],
            )
            .await?;
            let (title, body) = self.forge_pull_text(task)?;
            let url = forge::create_pull_in(
                selected.forge,
                &target.repo,
                &intent.source,
                &title,
                &body,
                Some(&intent.branch),
                &selected.selector(),
            )
            .await?;
            intent.pull = Some(forge::number_from_url(&url).ok_or_else(|| {
                CoreError::Invalid(
                    "the forge did not return a PR number; reconcile publication".into(),
                )
            })?);
            intent.phase = ForgePhase::AwaitingCi;
            self.store.save_crew_forge_merge(intent)?;
        }
        let number = intent
            .pull
            .ok_or_else(|| CoreError::Invalid("forge intent lacks a PR".into()))?;
        let receipt = crew_pull(&selected, &target.repo, number).await?;
        if !identity_matches(intent, &receipt) {
            return self.forge_needs_person(intent, "the PR identity or head changed");
        }
        if !crew_ci_green(
            &receipt,
            &self.config.crew_merge_required_checks,
            &intent.candidate,
            &intent.base,
        ) {
            return Ok(blocked(
                task,
                "every configured current-head CI check must succeed on the pinned base",
            ));
        }
        // Re-read holds and CI immediately before the final writer, after potentially long scratch checks.
        if let Some(why) = self.merge_blocker(entry, target)? {
            return Ok(blocked(task, &why));
        }
        if let Some(why) = self.merge_hold_now(entry, &pinned, now).await? {
            return Ok(blocked(task, &why));
        }
        if let Some(why) = self.forge_review_precondition(entry, target).await? {
            return Ok(blocked(task, &why));
        }
        let fresh = crew_pull(&selected, &target.repo, number).await?;
        if !identity_matches(intent, &fresh)
            || !crew_ci_green(
                &fresh,
                &self.config.crew_merge_required_checks,
                &intent.candidate,
                &intent.base,
            )
        {
            return Ok(blocked(task, "PR head, base or CI changed before merge"));
        }
        if text(&git(&target.repo, &["remote", "get-url", "origin"]).await?) != intent.remote {
            return self.forge_needs_person(intent, "origin changed before merge");
        }
        let ran = Ran {
            outcome: CheckOutcome::Pass,
            exit_code: None,
            command: "forge CI receipt".into(),
            log: serde_json::to_string(&fresh)?,
            duration_ms: 0,
            reason: None,
            names: Vec::new(),
        };
        let id = self.merge_evidence(task, &intent.candidate, intent.evidence.len(), &ran, now)?;
        intent.evidence.push(id);
        let combined = intent
            .combined
            .as_deref()
            .ok_or_else(|| CoreError::Invalid("combined evidence missing".into()))?;
        self.merge_gate_from_evidence(task, &intent.evidence, combined, &intent.candidate, now)?;
        intent.phase = ForgePhase::MergeRequested;
        self.store.save_crew_forge_merge(intent)?;
        // Re-read the escape check with the request saved: a withdraw either committed before this or sees the saved phase and is
        // refused, so a withdrawn check never lands. `merge_requested` has no way back to `awaiting_ci`, so the intent goes to
        // `needs_person` (nothing was sent to the forge) and the entry is blocked; this answers Blocked, never an error.
        if let Err(error) = self.refuse_unproven_escape_check(task, Some(&intent.candidate)) {
            let why = error.public_message();
            intent.phase = ForgePhase::NeedsPerson;
            intent.note = Some(why.clone());
            self.store.save_crew_forge_merge(intent)?;
            // The entry is only blocked while it is open; a person's reject is left as it is.
            self.merge_state(&intent.task_id, &OPEN_MERGE_STATES, "blocked", Some(&why))?;
            return Ok(blocked(task, &why));
        }
        // Durable before the request: from here on a merge request may be on the forge, so a person can never retry it, and a
        // reject stays reconcilable. A request that was never sent (the check above) leaves this false.
        intent.merge_sent = true;
        self.store.save_crew_forge_merge(intent)?;
        self.merge_event(task, "merge_queued", &json!({"forge_merge_requested": true, "actor": actor, "pull": number, "head": intent.candidate, "base": intent.base}));
        let options = MergePull {
            method: "merge".into(),
            auto: false,
            delete_branch: false,
            subject: None,
            body: None,
            strip_credits: false,
            expected_head: Some(intent.candidate.clone()),
        };
        // Even an error may mean the server merged. Keep MergeRequested until a receipt settles it.
        let requested = forge::merge_in(
            selected.forge,
            &target.repo,
            number,
            &options,
            &selected.selector(),
        )
        .await;
        let receipt = crew_pull(&selected, &target.repo, number).await?;
        if identity_matches(intent, &receipt) && receipt.state == "merged" {
            intent.merge_commit = receipt.merge_commit;
            intent.phase = ForgePhase::Merged;
            self.store.save_crew_forge_merge(intent)?;
            return self.finish_forge_merge(entry, intent, now).await;
        }
        if let Err(error) = requested {
            intent.note = Some(error.public_message());
            self.store.save_crew_forge_merge(intent)?;
        }
        Ok(blocked(
            task,
            "merge requested; awaiting an exact merge receipt",
        ))
    }

    /// The pull request text is written by code alone: fixed words, ids and numbers, never agent text or a mention.
    fn forge_pull_text(&self, task: &str) -> Result<(String, String)> {
        let Some(correction) = self.store.crew_correction_for_task(task)? else {
            return Ok((
                format!(
                    "Merge crew task {}",
                    task.chars().skip(7).take(12).collect::<String>()
                ),
                "Core-checked Crew task. Combined-state checks are recorded in the task proof."
                    .into(),
            ));
        };
        let original = self
            .store
            .crew_forge_merge(&correction.parent_task)?
            .and_then(|intent| intent.pull)
            .map_or_else(
                || "the original Crew PR".to_owned(),
                |number| format!("the original Crew PR #{number}"),
            );
        Ok((
            format!(
                "Revert crew merge {}",
                correction
                    .parent_commit
                    .chars()
                    .take(12)
                    .collect::<String>()
            ),
            format!(
                "Core-prepared rollback of {original}, merge commit {}. The exact rollback tree and the repository checks are recorded in the corrective task proof.",
                correction.parent_commit
            ),
        ))
    }

    fn forge_needs_person(&self, intent: &mut ForgeMerge, why: &str) -> Result<MergeOutcome> {
        intent.phase = ForgePhase::NeedsPerson;
        intent.note = Some(why.into());
        self.store.save_crew_forge_merge(intent)?;
        self.merge_state(&intent.task_id, &OPEN_MERGE_STATES, "blocked", Some(why))?;
        Ok(MergeOutcome::NeedsPerson {
            task_id: intent.task_id.clone(),
            why: why.into(),
        })
    }

    /// Reads existing effects only. Safe for startup even when scheduled work and CI policy are disabled.
    pub(super) async fn recover_crew_forge_merges(self: &Arc<Self>, now: i64) -> Result<()> {
        let _serial = self.crew_merges.lock().await;
        let mut first_error = None;
        for mut intent in self.store.crew_forge_unfinished()? {
            let recovered: Result<()> = async {
                let entry = self.store.merge_entry(&intent.task_id)?.ok_or_else(|| {
                    CoreError::Invalid("an unfinished forge intent has no queue record".into())
                })?;
                let target = MergeTarget {
                    repo: intent.repo.clone().into(),
                    branch: intent.branch.clone(),
                    owed: None,
                };
                if text(&git(&target.repo, &["remote", "get-url", "origin"]).await?)
                    != intent.remote
                {
                    self.forge_needs_person(&mut intent, "origin changed before recovery")?;
                    return Ok(());
                }
                self.reconcile_forge_intent(&entry, &target, &mut intent, now)
                    .await?;
                Ok(())
            }
            .await;
            if let Err(error) = recovered {
                tracing::warn!(task = %intent.task_id, %error, "Crew forge intent remains unresolved; later intents are still observed");
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        // Rejected intents whose merge request may still land: a failure here never blocks the work above.
        self.reconcile_rejected_requests(None, None, now, &mut Vec::new())
            .await;
        first_error.map_or(Ok(()), Err)
    }

    async fn reconcile_forge_intent(
        &self,
        entry: &MergeEntry,
        target: &MergeTarget,
        intent: &mut ForgeMerge,
        now: i64,
    ) -> Result<Option<MergeOutcome>> {
        if !matches!(
            intent.phase,
            ForgePhase::Publishing | ForgePhase::MergeRequested | ForgePhase::Merged
        ) {
            return Ok(None);
        }
        let context = CrewRepository::from_remote(&intent.remote)?;
        let number = match intent.pull {
            Some(number) => Some(number),
            None => crew_find_pull(&context, &target.repo, &intent.source, &intent.branch).await?,
        };
        let Some(number) = number else {
            return self.forge_needs_person(intent, "publication result is unknown; no matching PR was found, so publication is not repeated").map(Some);
        };
        intent.pull = Some(number);
        let receipt = crew_pull(&context, &target.repo, number).await?;
        if !identity_matches(intent, &receipt) {
            return self
                .forge_needs_person(intent, "the PR identity or head changed")
                .map(Some);
        }
        if receipt.state == "merged" {
            intent.merge_commit = receipt.merge_commit;
            intent.phase = ForgePhase::Merged;
            self.store.save_crew_forge_merge(intent)?;
            return self.finish_forge_merge(entry, intent, now).await.map(Some);
        }
        if intent.phase == ForgePhase::MergeRequested || intent.phase == ForgePhase::Merged {
            return Ok(Some(blocked(
                &entry.task_id,
                "the requested merge is not confirmed; reconcile this PR before any retry",
            )));
        }
        if receipt.state != "open" {
            return self
                .forge_needs_person(intent, "the published PR is closed without a merge")
                .map(Some);
        }
        intent.phase = ForgePhase::AwaitingCi;
        self.store.save_crew_forge_merge(intent)?;
        Ok(None)
    }

    /// Fresh lane Gates and persisted same-engine authorization. Locked lanes
    /// remain owed; a historical pass or a model verdict is never admission.
    pub(in crate::crew) async fn forge_review_precondition(
        &self,
        entry: &MergeEntry,
        target: &MergeTarget,
    ) -> Result<Option<String>> {
        let intent = self
            .store
            .crew_forge_merge(&entry.task_id)?
            .ok_or_else(|| CoreError::Invalid("forge intent missing".into()))?;
        let run = self
            .store
            .crew_run(&entry.task_id)?
            .ok_or_else(|| CoreError::NotFound("merge run".into()))?;
        let base = run
            .base_commit
            .as_deref()
            .ok_or_else(|| CoreError::Invalid("review rule base missing".into()))?;
        let rules = crate::crew::review::read_rules_at(std::path::Path::new(&entry.repo), base)
            .await
            .map_err(CoreError::Invalid)?;
        let (paths, _) = gitops::diff_facts(
            std::path::Path::new(&entry.repo),
            &intent.base,
            &entry.candidate,
        )
        .await?;
        let status = self
            .review_status(&entry.task_id)
            .map_err(|e| CoreError::Invalid(e.message))?;
        if status["blocked"] == json!(true) {
            return Ok(Some("a confirmed finding blocks it".into()));
        }
        let risk = self.refresh_merge_review_risk(entry)?;
        let mut required: std::collections::BTreeSet<String> = if entry.released {
            Default::default()
        } else {
            target
                .owed
                .clone()
                .unwrap_or_else(|| super::owed_lanes(risk))
                .into_iter()
                .collect()
        };
        // A person may stand in for an ordinary required lane. Locked lanes remain owed in every mode.
        for rule in rules
            .iter()
            .filter(|rule| rule.applies(Some(&paths)) && (rule.locked || !entry.released))
        {
            required.insert(rule.lane.as_str().to_owned());
        }
        for lane in &required {
            let locked = rules.iter().any(|r| r.lane.as_str() == lane && r.locked);
            if !locked && self.review_lane_removed(&entry.task_id, lane)? {
                continue;
            }
            let gate = self
                .review_lane_gate(&entry.task_id, lane, &entry.candidate)
                .map_err(|e| CoreError::Invalid(e.message))?;
            if gate.is_none_or(|g| g.decision != orbyn_contracts::GateDecision::Pass) {
                return Ok(Some(format!(
                    "the required {lane} lane has no passing Gate for the current candidate; locked lanes survive a person's release"
                )));
            }
        }
        if entry.released {
            return Ok(None);
        }
        if let Some(review) = self
            .store
            .crew_reviews_of(&entry.task_id)?
            .iter()
            .filter(|r| r.role == "review" && r.lane == crate::crew::SECURITY_LANE)
            .max_by_key(|r| r.round)
        {
            let routes = self.store.crew_route_decisions(&review.task_id)?;
            if routes.last().is_none_or(|route| {
                let same = route
                    .reasons
                    .iter()
                    .any(|r| r.code.as_str() == "same_engine_as_author");
                let authorized = route.reasons.iter().any(|r| {
                    matches!(
                        r.code.as_str(),
                        "same_engine_person_once" | "same_engine_full_security"
                    )
                });
                same && (!authorized
                    || route.tier != orbyn_contracts::RouteTier::Lead
                    || risk != super::Risk::High)
            }) {
                return Ok(Some("Security review has no distinct-vendor consent/risk receipt; a person must release this entry".into()));
            }
        }
        Ok(None)
    }

    async fn check_forge_combination(
        &self,
        scratch: &Scratch,
        entry: &MergeEntry,
        intent: &mut ForgeMerge,
        now: i64,
    ) -> Result<()> {
        let combined = gitops::merge(
            scratch,
            &entry.candidate,
            "Merge crew candidate for verification",
        )
        .await
        .map_err(|_| {
            CoreError::Invalid("the candidate cannot merge cleanly onto the pinned base".into())
        })?;
        let commands = self
            .merge_checks(&entry.task_id, &scratch.dir, &intent.base)
            .await?;
        if commands.is_empty() {
            return Err(CoreError::Invalid(
                "forge merging needs runnable combined-state checks".into(),
            ));
        }
        let deadline = Instant::now() + Duration::from_secs(30 * 60);
        let mut ids = Vec::new();
        for (number, command) in commands.iter().enumerate() {
            let ran = run_line(command, &scratch.dir, deadline).await;
            ids.push(self.merge_evidence(&entry.task_id, &combined, number, &ran, now)?);
            if ran.outcome != CheckOutcome::Pass {
                return Err(CoreError::Invalid(
                    "a combined-state check did not pass".into(),
                ));
            }
        }
        intent.combined = Some(combined);
        intent.evidence = ids;
        self.store.save_crew_forge_merge(intent)
    }

    pub(super) async fn finish_forge_merge(
        &self,
        entry: &MergeEntry,
        intent: &mut ForgeMerge,
        now: i64,
    ) -> Result<MergeOutcome> {
        let commit = intent
            .merge_commit
            .clone()
            .ok_or_else(|| CoreError::Invalid("merge receipt lacks a commit".into()))?;
        if self.store.crew_merge(&entry.task_id)?.is_none() {
            git(
                std::path::Path::new(&intent.repo),
                &["fetch", "--no-tags", &intent.remote, &commit],
            )
            .await?;
            // A forge that merged a different base/tree is an observed merge needing a person, never a retry.
            let tree = text(
                &git(
                    std::path::Path::new(&intent.repo),
                    &["rev-parse", &format!("{commit}^{{tree}}")],
                )
                .await?,
            );
            let combined = intent
                .combined
                .as_deref()
                .ok_or_else(|| CoreError::Invalid("combined commit missing".into()))?;
            let checked_tree = text(
                &git(
                    std::path::Path::new(&intent.repo),
                    &["rev-parse", &format!("{combined}^{{tree}}")],
                )
                .await?,
            );
            let parents = text(
                &git(
                    std::path::Path::new(&intent.repo),
                    &["show", "-s", "--format=%P", &commit],
                )
                .await?,
            );
            let mismatch = (tree != checked_tree
                || parents.split_whitespace().collect::<Vec<_>>()
                    != [intent.base.as_str(), intent.candidate.as_str()])
            .then_some("the actual forge merge differs from the checked combined state");
            self.record_merge(
                entry,
                &intent.base,
                &commit,
                u32::try_from(intent.evidence.len()).unwrap_or(u32::MAX),
                now,
                mismatch,
            )?;
        }
        self.project_forge_correction(&entry.task_id, now)?;
        Ok(MergeOutcome::Merged {
            task_id: entry.task_id.clone(),
            merge_commit: commit,
        })
    }
}

pub(super) fn identity_matches(
    intent: &ForgeMerge,
    receipt: &forge::crew::CrewPullReceipt,
) -> bool {
    CrewRepository::from_remote(&intent.remote).is_ok_and(|context| {
        context.host == receipt.host && context.repository == receipt.repository
    }) && intent.pull == Some(receipt.number)
        && intent.source == receipt.source
        && intent.branch == receipt.target
        && intent.candidate == receipt.head
}
