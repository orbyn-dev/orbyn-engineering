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
use crate::store::{ForgeMerge, ForgePhase, MergeEntry};

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
            return self.store.merge_entry(task_id)?.ok_or_else(|| {
                CoreError::Invalid(
                    "forge intent has no queue entry; a person must reconcile it".into(),
                )
            });
        }
        let remote = text(&git(&target.repo, &["remote", "get-url", "origin"]).await?);
        let selected = CrewRepository::from_remote(&remote)?;
        let base = crew_base(&selected, &target.repo, &target.branch).await?;
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
        self.crew_preview_only()?;
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
                    .process_forge_entry(&entry, target, &mut intent, now)
                    .await?;
                let landed = matches!(outcome, MergeOutcome::Merged { .. });
                outcomes.push(outcome);
                // A published/uncertain first entry owns the lane; later candidates cannot overtake it.
                if !landed {
                    break;
                }
            }
        }
        // The watch owns its own lock. It runs only after confirmed merges and retries read-only work on restart.
        self.crew_watch_forge_main(target, now).await?;
        Ok(outcomes)
    }

    async fn process_forge_entry(
        self: &Arc<Self>,
        entry: &MergeEntry,
        target: &MergeTarget,
        intent: &mut ForgeMerge,
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
            checked?;
            // Durable before either effect. An interruption here is reconciled, not retried.
            intent.phase = ForgePhase::Publishing;
            self.store.save_crew_forge_merge(intent)?;
            self.merge_event(task, "merge_queued", &json!({"forge_publish_requested": true, "head": intent.candidate, "base": intent.base}));
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
            let url = forge::create_pull_in(
                selected.forge,
                &target.repo,
                &intent.source,
                &format!(
                    "Merge crew task {}",
                    task.chars().skip(7).take(12).collect::<String>()
                ),
                "Core-checked Crew task. Combined-state checks are recorded in the task proof.",
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
        self.merge_event(task, "merge_queued", &json!({"forge_merge_requested": true, "pull": number, "head": intent.candidate, "base": intent.base}));
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

    fn forge_needs_person(&self, intent: &mut ForgeMerge, why: &str) -> Result<MergeOutcome> {
        intent.phase = ForgePhase::NeedsPerson;
        intent.note = Some(why.into());
        self.store.save_crew_forge_merge(intent)?;
        self.store
            .set_merge_entry_state(&intent.task_id, "blocked", Some(why))?;
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

    async fn finish_forge_merge(
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
        Ok(MergeOutcome::Merged {
            task_id: entry.task_id.clone(),
            merge_commit: commit,
        })
    }
}

fn identity_matches(intent: &ForgeMerge, receipt: &forge::crew::CrewPullReceipt) -> bool {
    CrewRepository::from_remote(&intent.remote).is_ok_and(|context| {
        context.host == receipt.host && context.repository == receipt.repository
    }) && intent.pull == Some(receipt.number)
        && intent.source == receipt.source
        && intent.branch == receipt.target
        && intent.candidate == receipt.head
}
