//! Bounded watch after confirmed forge merges; durable notices reuse the existing digest computation.

use std::sync::Arc;
use std::time::{Duration, Instant};

use orbyn_contracts::CheckOutcome;

use super::{MergeTarget, gitops::Scratch};
use crate::app::Core;
use crate::crew::checks::run_line;
use crate::crew::guard::{redact, refuse_secret};
use crate::error::{CoreError, Result};
use crate::source_control::forge::crew::{CrewRepository, crew_base};
use crate::source_control::git::git;

/// An HTTP future can be dropped independently of service shutdown. Release its read-only work
/// in this epoch; completed receipts and a newer epoch's claim are protected by the Store writer.
struct WatchClaim<'a> {
    store: &'a crate::store::Store,
    identity: &'a str,
    completed: bool,
}

impl Drop for WatchClaim<'_> {
    fn drop(&mut self) {
        if !self.completed
            && let Err(error) = self.store.release_crew_merge_watch(self.identity)
        {
            tracing::warn!(%error, "could not release an abandoned Crew watch claim");
        }
    }
}

impl Core {
    /// Checks the actual remote tip without moving any local branch. Failures ask the person;
    /// the local compatibility watch's automatic update-ref/revert is never used for a forge.
    pub async fn crew_watch_forge_main(
        self: &Arc<Self>,
        target: &MergeTarget,
        now: i64,
    ) -> Result<()> {
        self.crew_preview_only()?;
        let _serial = self.crew_merges.lock().await;
        let records = self
            .store
            .crew_merges(&target.repo.to_string_lossy(), &target.branch)?;
        let mut first = None;
        for record in &records {
            if self.store.crew_forge_merge(&record.task_id)?.is_some() {
                first = Some(record);
                break;
            }
        }
        let Some(first) = first else {
            self.crew_deliver_merge_notices(now)?;
            return Ok(());
        };
        let Some(intent) = self.store.crew_forge_merge(&first.task_id)? else {
            return Ok(());
        };
        let remote = crate::source_control::git::text(
            &git(&target.repo, &["remote", "get-url", "origin"]).await?,
        );
        if intent.remote != remote {
            return Err(CoreError::Invalid(
                "the watched forge remote changed".into(),
            ));
        }
        let selected = CrewRepository::from_remote(&intent.remote)?;
        let tip = crew_base(&selected, &target.repo, &target.branch).await?;
        let identity = format!("watch:{}:{}:{tip}", intent.remote, intent.branch);
        if !self
            .store
            .claim_crew_merge_watch(&identity, &first.task_id)?
        {
            return self.crew_deliver_merge_notices(now);
        }
        let mut claim = WatchClaim {
            store: &self.store,
            identity: &identity,
            completed: false,
        };
        let watched: Result<()> = async {
            git(&target.repo, &["fetch", "--no-tags", &intent.remote, &tip]).await?;
            let commands = super::gitops::repo_checks(&target.repo, &first.before_commit).await?;
            if commands.is_empty() { return Err(CoreError::Invalid("NoChecks: the forge watch has no repository checks".into())); }
            let scratch = Scratch::at(&target.repo, &self.config.data_dir.join("crew-merge"), &tip).await?;
            let checked: Result<bool> = async {
                // A rewritten target must not clear records that no longer belong to its history.
                for record in &records {
                    if self.store.crew_forge_merge(&record.task_id)?.is_some() {
                        git(&scratch.dir, &["merge-base", "--is-ancestor", &record.merge_commit, &tip]).await?;
                    }
                }
                let deadline = Instant::now() + Duration::from_secs(30 * 60);
                for command in commands {
                    if run_line(&command, &scratch.dir, deadline).await.outcome != CheckOutcome::Pass { return Ok(false); }
                }
                Ok(true)
            }.await;
            scratch.finish().await;
            let healthy = checked?;
            for record in &records {
                if self.store.crew_forge_merge(&record.task_id)?.is_none() { continue; }
                self.store.set_merge_watch(&record.task_id, if healthy { "green" } else { "broken" })?;
                if !healthy {
                    self.store.set_merge_needs_person(&record.task_id, Some("repository checks fail on the actual forge tip; a person must review a corrective PR"))?;
                }
            }
            self.store.finish_crew_merge_watch(&identity)?;
            claim.completed = true;
            Ok(())
        }.await;
        watched?;
        self.crew_deliver_merge_notices(now)
    }

    /// One owned future, with no spawned children or OS schedule. The service must opt in and await it on shutdown.
    pub async fn run_crew_merge_watch(
        self: &Arc<Self>,
        stop: tokio_util::sync::CancellationToken,
    ) -> Result<()> {
        if !self.crew_enabled() {
            return Ok(());
        }
        if self.config.crew_merge_watch_interval.is_some_and(|every| {
            !(Duration::from_secs(60)..=Duration::from_secs(86400)).contains(&every)
        }) {
            return Err(CoreError::Invalid(
                "Crew watch interval must be 60 seconds to 24 hours".into(),
            ));
        }
        // Startup reconciles already-requested effects even with the periodic schedule off.
        tokio::select! { biased; _ = stop.cancelled() => return Ok(()), result = self.crew_merge_watch_tick() => result? }
        let Some(every) = self.config.crew_merge_watch_interval else {
            return Ok(());
        };
        let mut timer = tokio::time::interval_at(tokio::time::Instant::now() + every, every);
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                _ = stop.cancelled() => return Ok(()),
                _ = timer.tick() => {}
            }
            tokio::select! {
                biased;
                _ = stop.cancelled() => return Ok(()),
                result = self.crew_merge_watch_tick() => result?,
            }
        }
    }

    async fn crew_merge_watch_tick(self: &Arc<Self>) -> Result<()> {
        if let Err(error) = self
            .recover_crew_forge_merges(crate::util::unix_now())
            .await
        {
            tracing::warn!(%error, "Crew forge intent reconciliation needs another observation");
        }
        let mut targets = std::collections::BTreeSet::new();
        for record in self.store.crew_all_merges()? {
            if self.store.crew_forge_merge(&record.task_id)?.is_some() {
                targets.insert((record.repo, record.branch));
            }
        }
        if targets.len() > 32 {
            return Err(CoreError::Invalid(
                "Crew watch is limited to 32 forge targets per service".into(),
            ));
        }
        for (repo, branch) in targets {
            let target = MergeTarget {
                repo: repo.into(),
                branch,
                owed: None,
            };
            if let Err(error) = self
                .crew_watch_forge_main(&target, crate::util::unix_now())
                .await
            {
                tracing::warn!(%error, "Crew forge watch needs another observation");
            }
        }
        self.crew_deliver_merge_notices(crate::util::unix_now())
    }

    /// Daily digest, one overdue notice per due window and one break notice per merge.
    /// Notice text is code-authored and contains no check commands or answer keys.
    pub fn crew_deliver_merge_notices(&self, now: i64) -> Result<()> {
        self.crew_preview_only()?;
        let digest = self.autopilot_digest(now, Some(24))?;
        for record in self.store.crew_all_merges()? {
            let run = self
                .store
                .crew_run(&record.task_id)?
                .ok_or_else(|| CoreError::NotFound("merge lead".into()))?;
            let deliver = |identity: String, text: String| -> Result<()> {
                let text = redact(&text);
                refuse_secret(&serde_json::json!({"notice": text}))
                    .map_err(|e| CoreError::Invalid(e.message))?;
                self.store.crew_merge_notice(
                    &identity,
                    &record.task_id,
                    &run.lead_thread_id,
                    &text,
                )?;
                Ok(())
            };
            if !digest.quiet {
                deliver(
                    format!("digest:{}:{}", run.lead_thread_id, now.div_euclid(86400)),
                    format!(
                        "Crew daily digest: {} merged in the last day; {} waiting for review; {} overdue; {} need a person.",
                        digest.merged_count,
                        digest.waiting_review,
                        digest.overdue,
                        digest.needs_person
                    ),
                )?;
            }
            if record.review_state == "waiting" && record.due_unix.is_some_and(|due| now > due) {
                deliver(
                    format!(
                        "overdue:{}:{}",
                        record.task_id,
                        record.due_unix.unwrap_or_default()
                    ),
                    format!(
                        "Crew review overdue: task {} ({}) is waiting for your review.",
                        record.task_id, record.risk
                    ),
                )?;
            }
            if record.needs_person.is_some() {
                deliver(
                    format!("break:{}:{}", record.task_id, record.merge_commit),
                    format!(
                        "Crew needs your review of task {}: the merged result needs a person. Open the Review queue for its proof.",
                        record.task_id
                    ),
                )?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::WatchClaim;
    use orbyn_contracts::CrewObject;

    #[tokio::test]
    async fn dropped_watch_future_releases_its_claim_for_the_same_epoch() {
        let dir = crate::testing::TempDir::new();
        let store =
            crate::store::Store::open(&dir.path().join("watch.sqlite3"), "same-epoch").unwrap();
        let CrewObject::Brief(brief) = CrewObject::parse(
            &serde_json::from_str(include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../docs/crew/protocol/schemas/examples/brief.valid.basic.json"
            )))
            .unwrap(),
        )
        .unwrap() else {
            panic!("brief fixture")
        };
        let CrewObject::AnswerKey(key) = CrewObject::parse(
            &serde_json::from_str(include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../docs/crew/protocol/schemas/examples/answer-key.valid.basic.json"
            )))
            .unwrap(),
        )
        .unwrap() else {
            panic!("key fixture")
        };
        let task = store.create_crew_task(&brief, &key, "run-01").unwrap();
        let identity = "watch:owned-fixture:main:commit";
        let (signal, reached) = tokio::sync::oneshot::channel();
        let mut abandoned = Box::pin(async {
            assert!(store.claim_crew_merge_watch(identity, &task.id).unwrap());
            let _claim = WatchClaim {
                store: &store,
                identity,
                completed: false,
            };
            signal.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        tokio::select! { _ = &mut abandoned => panic!("the fixture should be waiting"), _ = reached => {} }
        assert!(!store.claim_crew_merge_watch(identity, &task.id).unwrap());
        drop(abandoned);
        assert!(store.claim_crew_merge_watch(identity, &task.id).unwrap());
        let mut complete = WatchClaim {
            store: &store,
            identity,
            completed: false,
        };
        store.finish_crew_merge_watch(identity).unwrap();
        complete.completed = true;
        drop(complete);
        assert!(!store.claim_crew_merge_watch(identity, &task.id).unwrap());
    }
}
