//! Deterministic corrective tasks. Only the ordinary forge queue may publish or merge them.
use std::path::{Path, PathBuf};
use std::sync::Arc;

use orbyn_contracts::{Brief, BriefEffect, CrewId, CrewMember, CrewObject, ResultState, Thread};
use serde_json::{Value, json};

use super::MergeTarget;
use super::gitops::{self, Scratch};
use crate::app::Core;
use crate::crew::guard::{key_labeled, refuse_leak_in, refuse_secret};
use crate::crew::leases::{Grant, internal, refuse_protected, timestamp};
use crate::crew::tool::invalid;
use crate::error::{CoreError, Result};
use crate::source_control::forge::crew::{CrewRepository, crew_base};
use crate::source_control::git::{git, text};
use crate::store::{ForgeCorrection, MergeRecord};

fn core_error(error: orbyn_contracts::CrewError) -> CoreError {
    CoreError::Invalid(error.message)
}

fn draft(mut object: Value, own: &[&str]) -> Value {
    if let Some(fields) = object.as_object_mut() {
        for name in [
            "type",
            "v",
            "id",
            "crew_id",
            "author",
            "created_at",
            "run_id",
        ]
        .iter()
        .copied()
        .chain(own.iter().copied())
        {
            fields.remove(name);
        }
    }
    object
}

async fn common(repo: &Path) -> Result<PathBuf> {
    let path = text(
        &git(
            repo,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )
        .await?,
    );
    Ok(crate::util::canonicalize(Path::new(&path))?)
}

async fn correction_path(repo: &Path, parent: &str) -> Result<PathBuf> {
    let common = common(repo).await?;
    let primary = common
        .parent()
        .ok_or_else(|| CoreError::Invalid("the repository has no primary checkout".into()))?;
    let name = primary
        .file_name()
        .ok_or_else(|| CoreError::Invalid("the repository has no name".into()))?
        .to_string_lossy();
    Ok(primary.with_file_name(format!("{name}-wt")).join(format!(
        "crew-correction-{}",
        parent
            .get(7..23)
            .ok_or_else(|| CoreError::Invalid("invalid parent task".into()))?
    )))
}

/// The object reference that keeps the candidate reachable while its checkout lives.
fn correction_ref(task: &str) -> Result<String> {
    Ok(format!(
        "refs/orbyn/corrections/{}",
        task.get(7..)
            .ok_or_else(|| CoreError::Invalid("invalid corrective task id".into()))?
    ))
}

struct Leases<'a> {
    core: &'a Core,
    task: &'a str,
}
impl Drop for Leases<'_> {
    fn drop(&mut self) {
        if let Err(error) = self.core.release_crew_leases(self.task) {
            tracing::warn!(%error, "could not release corrective task leases");
        }
    }
}

impl Core {
    /// Local work only: build the exact rollback in a checkout, bind it and verify it. Nothing is queued, pushed or opened on
    /// the forge. `actor` is `person` for a Revert answer and `watch` for the code's own verification of a broken main.
    pub(in crate::crew) async fn prepare_and_verify_forge_correction(
        self: &Arc<Self>,
        parent: &str,
        target: &MergeTarget,
        actor: &str,
        now: i64,
    ) -> Result<ForgeCorrection> {
        self.crew_preview_only()?;
        let _merges = self.crew_merges.lock().await;
        let _starts = self.crew_starts.lock().await;
        let correction = match self.store.crew_forge_correction(parent)? {
            Some(correction) => correction,
            None => {
                self.make_forge_correction(parent, target, actor, now)
                    .await?
            }
        };
        self.verify_forge_correction(&correction, now).await?;
        Ok(correction)
    }

    /// A correction is queued or published only after a person's Revert answer marked it. Agent tools, the person's plain
    /// Queue route and every later step stop here.
    pub(super) fn refuse_unrequested_correction(&self, task: &str) -> Result<()> {
        let refused = match self.store.crew_correction_person_requested(task)? {
            Some(requested) => !requested,
            // Fail closed: a Core correction run without its binding is never queued.
            None => self
                .store
                .crew_run(task)?
                .is_some_and(|run| run.member == "core-correction"),
        };
        if refused {
            return Err(CoreError::Invalid(
                "a corrective task is queued only by a person's Revert answer".into(),
            ));
        }
        Ok(())
    }

    /// A person's Revert answer: mark the request, prepare, then queue the verified task. The only caller that queues a
    /// correction; queueing publishes nothing, the person's merge route does that later.
    pub(super) async fn prepare_forge_correction(
        self: &Arc<Self>,
        parent: &str,
        target: &MergeTarget,
        now: i64,
    ) -> Result<ForgeCorrection> {
        let correction = self
            .prepare_and_verify_forge_correction(parent, target, "person", now)
            .await?;
        // The marker and its audit row commit together; nothing may queue or publish a correction without it.
        self.store
            .request_crew_correction(&correction.task_id, "person")?;
        // Ordinary queue admission owns its locks and revalidates the pinned binding and remote tip.
        self.crew_queue_forge_merge(&correction.task_id, target, now)
            .await?;
        Ok(correction)
    }

    async fn make_forge_correction(
        self: &Arc<Self>,
        parent: &str,
        target: &MergeTarget,
        actor: &str,
        now: i64,
    ) -> Result<ForgeCorrection> {
        let record = self
            .store
            .crew_merge(parent)?
            .ok_or_else(|| CoreError::NotFound("original merge".into()))?;
        if record.repo != target.repo.to_string_lossy()
            || record.branch != target.branch
            || record.review_state == "reverted"
        {
            return Err(CoreError::Invalid(
                "the original merge target changed or was reverted".into(),
            ));
        }
        let intent = self
            .store
            .crew_forge_merge(parent)?
            .ok_or_else(|| CoreError::Invalid("the original merge has no forge receipt".into()))?;
        let remote = text(&git(&target.repo, &["remote", "get-url", "origin"]).await?);
        if remote != intent.remote {
            return Err(CoreError::Invalid("the forge remote changed".into()));
        }
        let selected = CrewRepository::from_remote(&remote)?;
        let base = crew_base(&selected, &target.repo, &target.branch).await?;
        git(&target.repo, &["fetch", "--no-tags", &remote, &base]).await?;
        git(
            &target.repo,
            &["merge-base", "--is-ancestor", &record.merge_commit, &base],
        )
        .await?;
        let commands = gitops::repo_checks(&target.repo, &record.before_commit).await?;
        if commands.is_empty() {
            return Err(CoreError::Invalid(
                "NoChecks: a corrective task requires trusted repository checks".into(),
            ));
        }
        let original = self
            .store
            .crew_brief(parent)?
            .ok_or_else(|| CoreError::NotFound("original brief".into()))?;
        for effect in [BriefEffect::WriteLeased, BriefEffect::RunChecks] {
            if !original.allowed_effects.contains(&effect) {
                return Err(CoreError::Invalid(
                    "the original task grants no corrective write/check effects".into(),
                ));
            }
        }
        self.refuse_lease_conflict(
            &original,
            &CrewMember::new("core-correction").map_err(CoreError::Invalid)?,
            now,
        )
        .map_err(core_error)?;
        let protected = self
            .store
            .crew_answer_key(parent)?
            .ok_or_else(|| CoreError::NotFound("original key".into()))?
            .protected_paths;
        refuse_protected(&original.write_paths().collect::<Vec<_>>(), &protected)
            .map_err(core_error)?;
        let checkout = correction_path(&target.repo, parent).await?;
        let scratch = Scratch::named(&target.repo, &checkout, &base).await?;
        let before = crate::crew::protect::snapshot(&scratch.dir, &protected);
        let candidate = gitops::revert(&scratch, &record.merge_commit)
            .await
            .map_err(|_| {
                CoreError::Invalid(
                    "the original merge cannot be reverted cleanly at the current forge tip".into(),
                )
            })?;
        let tree = text(&git(&scratch.dir, &["rev-parse", "HEAD^{tree}"]).await?);
        let (brief, key) =
            self.correction_drafts(&record, &original, &base, &tree, &commands, &protected)?;
        let run_id = format!(
            "correction-{}",
            brief
                .id
                .as_str()
                .get(7..31)
                .ok_or_else(|| CoreError::Invalid("invalid corrective task id".into()))?
        );
        let correction = ForgeCorrection {
            parent_task: parent.into(),
            parent_commit: record.merge_commit,
            task_id: brief.id.as_str().into(),
            run_id,
            repo: record.repo,
            remote,
            branch: record.branch,
            base,
            candidate,
            tree,
            checkout: checkout.to_string_lossy().into_owned(),
        };
        self.store.add_crew_forge_correction(&correction, actor)?;
        scratch.retain();
        // Keep an object reference even when the registered checkout later needs restoration.
        git(
            &target.repo,
            &[
                "update-ref",
                &correction_ref(&correction.task_id)?,
                &correction.candidate,
            ],
        )
        .await?;
        self.store
            .create_crew_task(&brief, &key, &correction.run_id)?;
        let original_run = self
            .store
            .crew_run(parent)?
            .ok_or_else(|| CoreError::NotFound("original run".into()))?;
        self.store.add_crew_run(
            &correction.task_id,
            &original_run.lead_thread_id,
            "core-correction",
        )?;
        self.store.bind_crew_correction_checkout(&correction)?;
        self.store
            .set_crew_protected(&correction.task_id, &serde_json::to_string(&before)?)?;
        Ok(correction)
    }

    fn correction_drafts(
        &self,
        record: &MergeRecord,
        original: &Brief,
        base: &str,
        tree: &str,
        commands: &[String],
        protected: &[orbyn_contracts::CrewPathPattern],
    ) -> Result<(Brief, orbyn_contracts::AnswerKey)> {
        let run = self
            .store
            .crew_run(&record.task_id)?
            .ok_or_else(|| CoreError::NotFound("original lead".into()))?;
        let lead = self.store.thread(&run.lead_thread_id)?;
        let mut stamper = self
            .review_stamper(&lead, original.author.engine.as_str())
            .map_err(core_error)?;
        stamper.now = timestamp(record.merged_unix).map_err(core_error)?;
        let mut wanted = draft(CrewObject::Brief(original.clone()).to_value(), &[]);
        wanted["outcome"] = json!(format!(
            "Revert Crew merge {} on pinned target {} through the ordinary reviewed forge queue.",
            record.merge_commit, base
        ));
        wanted["inputs"] = json!([]);
        wanted["refs"] = json!([record.task_id]);
        wanted["stop_when"] = json!(["The exact rollback is verified, or a person must decide."]);
        let brief = stamper.brief(&wanted).map_err(core_error)?;
        let paths: Vec<_> = original.write_paths().map(|p| p.as_str()).collect();
        if paths.is_empty() {
            return Err(CoreError::Invalid(
                "the original task grants no writable files".into(),
            ));
        }
        let mut checks = vec![
            json!({"id":"corrective-scope","kind":"file_scope","params":{"only":paths},"proves":"The rollback stays in the original writable scope."}),
            json!({"id":"corrective-tree","kind":"command","params":{"run":format!("git diff --exit-code {tree} HEAD --")},"proves":"The candidate has the exact deterministic rollback tree."}),
        ];
        checks.extend(commands.iter().enumerate().map(|(n,command)| json!({"id":format!("corrective-repo-{n}"),"kind":"command","params":{"run":command},"proves":"The trusted pre-merge repository check passes on the rollback."})));
        let key = stamper
            .answer_key(
                &json!({"checks":checks,"protected_paths":protected}),
                &brief.id,
            )
            .map_err(core_error)?;
        refuse_secret(&CrewObject::Brief(brief.clone()).to_value()).map_err(core_error)?;
        refuse_secret(&CrewObject::AnswerKey(key.clone()).to_value()).map_err(core_error)?;
        refuse_leak_in(
            &CrewObject::Brief(brief.clone()).to_value(),
            "brief",
            &key_labeled(&key),
        )
        .map_err(core_error)?;
        Ok((brief, key))
    }

    async fn verify_forge_correction(
        self: &Arc<Self>,
        correction: &ForgeCorrection,
        now: i64,
    ) -> Result<()> {
        if self.store.crew_task(&correction.task_id)?.is_none() {
            let record = self
                .store
                .crew_merge(&correction.parent_task)?
                .ok_or_else(|| CoreError::NotFound("original merge".into()))?;
            let original = self
                .store
                .crew_brief(&correction.parent_task)?
                .ok_or_else(|| CoreError::NotFound("original brief".into()))?;
            let protected = self
                .store
                .crew_answer_key(&correction.parent_task)?
                .ok_or_else(|| CoreError::NotFound("original key".into()))?
                .protected_paths;
            let commands =
                gitops::repo_checks(Path::new(&correction.repo), &record.before_commit).await?;
            let (brief, key) = self.correction_drafts(
                &record,
                &original,
                &correction.base,
                &correction.tree,
                &commands,
                &protected,
            )?;
            if brief.id.as_str() != correction.task_id {
                return Err(CoreError::Invalid(
                    "the corrective task draft differs from its binding".into(),
                ));
            }
            self.store
                .create_crew_task(&brief, &key, &correction.run_id)?;
        }
        let parent_run = self
            .store
            .crew_run(&correction.parent_task)?
            .ok_or_else(|| CoreError::NotFound("original run".into()))?;
        self.store.add_crew_run(
            &correction.task_id,
            &parent_run.lead_thread_id,
            "core-correction",
        )?;
        self.store.bind_crew_correction_checkout(correction)?;
        let existing = self
            .store
            .crew_run(&correction.task_id)?
            .ok_or_else(|| CoreError::NotFound("corrective run".into()))?;
        if existing.protected.is_none() {
            let protected = self
                .store
                .crew_answer_key(&correction.task_id)?
                .ok_or_else(|| CoreError::NotFound("corrective key".into()))?
                .protected_paths;
            let before = Scratch::at(
                Path::new(&correction.repo),
                &self.config.data_dir.join("crew-merge"),
                &correction.base,
            )
            .await?;
            let snapshot = crate::crew::protect::snapshot(&before.dir, &protected);
            before.finish().await;
            self.store
                .set_crew_protected(&correction.task_id, &serde_json::to_string(&snapshot)?)?;
        }
        let mut task = self
            .store
            .crew_task(&correction.task_id)?
            .ok_or_else(|| CoreError::NotFound("corrective task".into()))?;
        if task.state == "passed" && !self.store.crew_results(&task.id)?.is_empty() {
            return Ok(());
        }
        if !["briefed", "routed", "running", "verifying", "passed"].contains(&task.state.as_str()) {
            return Err(CoreError::Invalid(
                "the corrective task waits for a person or has ended".into(),
            ));
        }
        self.checked_correction_checkout(correction).await?;
        let brief = self
            .store
            .crew_brief(&task.id)?
            .ok_or_else(|| CoreError::NotFound("corrective brief".into()))?;
        let member = CrewMember::new("core-correction").map_err(CoreError::Invalid)?;
        // H10: the live-lease check below and the grants it allows are one step with every other grant.
        let _granting = self.crew_lease_grants.lock().await;
        self.refuse_lease_conflict(&brief, &member, now)
            .map_err(core_error)?;
        let _leases = Leases {
            core: self,
            task: &correction.task_id,
        };
        for lease in (Grant {
            brief: &brief,
            run_id: &CrewId::new(correction.run_id.clone()).map_err(CoreError::Invalid)?,
            holder: &member,
            now,
        })
        .leases()
        .map_err(core_error)?
        {
            self.store.add_crew_lease_grant(&task.id, &lease)?;
        }
        drop(_granting);
        let flags = crate::crew::index_flags::flagged(Path::new(&correction.checkout))
            .await
            .ok_or_else(|| CoreError::Invalid("the corrective index cannot be read".into()))?;
        if !flags.is_empty() {
            return Err(CoreError::Invalid(
                "the corrective checkout has hidden index entries".into(),
            ));
        }
        self.store.set_crew_index_flags(
            &task.id,
            &json!({"dir":correction.checkout,"flags":flags}).to_string(),
        )?;
        self.store.bind_crew_correction_checkout(correction)?;
        for state in ["routed", "running"] {
            if (task.state == "briefed" && state == "routed")
                || (task.state == "routed" && state == "running")
            {
                task = self.store.move_crew_task(&task.id, state, None)?;
            }
        }
        if task.state == "running" {
            task = self
                .store
                .begin_crew_verification(&task.id, "core-correction", None)?;
        }
        let run = self
            .store
            .crew_run(&task.id)?
            .ok_or_else(|| CoreError::NotFound("corrective run".into()))?;
        let verdict = self.crew_verify(&task, &run).await.map_err(core_error)?;
        if !verdict.passed() || verdict.commit.as_str() != correction.candidate {
            if task.state != "passed" {
                self.store.move_crew_task(&task.id, "needs_person", None)?;
            }
            return Err(CoreError::Invalid(
                "the corrective checks or exact candidate did not pass".into(),
            ));
        }
        if task.state != "passed" {
            task = self.store.pass_crew_task(
                &task.id,
                &verdict
                    .evidence
                    .iter()
                    .map(|id| id.as_str().to_owned())
                    .collect::<Vec<_>>(),
            )?;
        }
        self.record_result(
            &task,
            ResultState::Passed,
            None,
            "Core prepared and verified a deterministic rollback. No model ran.",
            Some(&verdict),
        )
        .map_err(core_error)?;
        Ok(())
    }

    async fn checked_correction_checkout(&self, correction: &ForgeCorrection) -> Result<PathBuf> {
        let path = PathBuf::from(&correction.checkout);
        if path != correction_path(Path::new(&correction.repo), &correction.parent_task).await?
            || std::fs::symlink_metadata(&path).is_err()
            || std::fs::symlink_metadata(&path)?.file_type().is_symlink()
            || common(&path).await? != common(Path::new(&correction.repo)).await?
        {
            return Err(CoreError::Invalid(
                "the corrective checkout identity changed".into(),
            ));
        }
        let registered = text(
            &git(
                Path::new(&correction.repo),
                &["worktree", "list", "--porcelain"],
            )
            .await?,
        );
        let registered = registered
            .lines()
            .filter_map(|line| line.strip_prefix("worktree "))
            .any(|entry| std::fs::canonicalize(entry).ok() == std::fs::canonicalize(&path).ok());
        if !registered
            || text(&git(&path, &["rev-parse", "HEAD"]).await?) != correction.candidate
            || !git(&path, &["status", "--porcelain"]).await?.is_empty()
        {
            return Err(CoreError::Invalid(
                "the corrective checkout is missing, dirty or at another candidate".into(),
            ));
        }
        Ok(path)
    }

    /// All correction review callers resolve the same durable candidate, without moving the lead's folder.
    pub(in crate::crew) async fn correction_review_thread(
        &self,
        thread: &Thread,
        subject: &str,
    ) -> std::result::Result<Thread, orbyn_contracts::CrewError> {
        let Some(correction) = self
            .store
            .crew_correction_for_task(subject)
            .map_err(internal)?
        else {
            return Ok(thread.clone());
        };
        let run = self
            .store
            .crew_run(subject)
            .map_err(internal)?
            .ok_or_else(|| invalid("the corrective task has no run"))?;
        if run.lead_thread_id != thread.id {
            return Err(invalid("the corrective task belongs to another crew"));
        }
        let path = self
            .checked_correction_checkout(&correction)
            .await
            .map_err(internal)?;
        let mut selected = thread.clone();
        selected.cwd = path.to_string_lossy().into_owned();
        selected.worktree = None;
        Ok(selected)
    }

    pub(super) fn project_forge_correction(&self, task: &str, now: i64) -> Result<()> {
        if !self.store.finish_crew_correction(task)? {
            return Ok(());
        }
        let correction = self
            .store
            .crew_correction_for_task(task)?
            .ok_or_else(|| CoreError::NotFound("corrective binding".into()))?;
        let parent = self
            .store
            .crew_merge(&correction.parent_task)?
            .ok_or_else(|| CoreError::NotFound("corrected merge".into()))?;
        let receipt = self
            .store
            .crew_merge(task)?
            .ok_or_else(|| CoreError::NotFound("corrective merge".into()))?;
        match self.area_breaker_after_revert(&parent, &receipt.merge_commit, now) {
            Ok(()) => self
                .store
                .complete_crew_correction(task, &receipt.merge_commit),
            Err(error) => {
                self.area_breaker_failed(
                    &parent.task_id,
                    "revert",
                    Some(&receipt.merge_commit),
                    &error,
                );
                Err(error)
            }
        }
    }

    /// Startup and ticks repair local state only: an interrupted corrective task is verified again, a finished one is
    /// projected, a completed one is cleaned up. Nothing here queues, pushes, opens or merges anything.
    pub(in crate::crew) async fn resume_forge_corrections(
        self: &Arc<Self>,
        now: i64,
    ) -> Result<()> {
        for correction in self.store.crew_pending_corrections()? {
            if self.store.crew_merge(&correction.task_id)?.is_some() {
                // The breaker and a person's area override share this owner. Live merge completion already holds it.
                let _merges = self.crew_merges.lock().await;
                if let Err(error) = self.project_forge_correction(&correction.task_id, now) {
                    tracing::warn!(%error,"a corrective receipt or area projection needs another observation");
                }
                continue;
            }
            if self.store.merge_entry(&correction.task_id)?.is_some() {
                // Queued by a person's Revert: the queue and the forge intent own it from here.
                continue;
            }
            let target = MergeTarget {
                repo: correction.repo.clone().into(),
                branch: correction.branch.clone(),
                owed: None,
            };
            if let Err(error) = self
                .prepare_and_verify_forge_correction(&correction.parent_task, &target, "watch", now)
                .await
            {
                self.store.set_merge_needs_person(&correction.parent_task,Some("The corrective task needs reconciliation. Open its task proof before retrying."))?;
                tracing::warn!(%error,"a corrective task could not resume");
            }
        }
        for correction in self.store.crew_completed_corrections()? {
            if let Err(error) = self.retire_forge_correction(&correction).await {
                tracing::warn!(%error, task = %correction.task_id, "a finished correction's checkout could not be cleaned up yet");
            }
        }
        Ok(())
    }

    /// A completed correction no longer needs its checkout or object reference, unless a review or fix still reads that
    /// checkout. Idempotent, so a busy checkout is simply tried again on a later tick. The pushed forge branch stays: Core
    /// never deletes a branch on the forge.
    async fn retire_forge_correction(&self, correction: &ForgeCorrection) -> Result<()> {
        let path = PathBuf::from(&correction.checkout);
        let repo = Path::new(&correction.repo);
        let present = std::fs::symlink_metadata(&path).is_ok();
        let reference = correction_ref(&correction.task_id)?;
        let kept = git(repo, &["rev-parse", "--verify", "--quiet", &reference])
            .await
            .is_ok();
        if !present && !kept {
            return self.store.mark_crew_correction_retired(&correction.task_id);
        }
        // A review start takes this lock before it reads the checkout, so none is admitted between the check and the removal.
        let _starts = self.crew_starts.lock().await;
        if self.store.crew_correction_checkout_in_use(correction)? {
            return Ok(());
        }
        if present {
            // Same identity checks as a review: remove only the registered checkout this binding created.
            self.checked_correction_checkout(correction).await?;
            git(
                repo,
                &["worktree", "remove", "--force", &path.to_string_lossy()],
            )
            .await?;
        }
        if kept {
            git(repo, &["update-ref", "-d", &reference]).await?;
        }
        self.store.mark_crew_correction_retired(&correction.task_id)
    }
}
