//! The merge queue. A passed task asks to merge; its diff is scored; the mode and the tier decide whether it goes on or waits
//! for a person; and merges happen one at a time. Each candidate is merged (`--no-ff`) onto the branch's tip in a scratch
//! checkout and its required checks run again on that combined state before the branch moves. A merge is a merge commit, so
//! a revert is one `git revert -m 1` and a bisect walks first parents.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use orbyn_contracts::CheckOutcome;
use serde_json::{Value, json};

use super::gitops::{self, Refused, Scratch};
use super::review_window;
use super::risk::{DiffFacts, Risk, areas_of, score};
use crate::app::Core;
use crate::crew::autopilot::Mode;
use crate::crew::checks::run_line;
use crate::error::{CoreError, Result};
use crate::store::{MergeEntry, MergeRecord};

/// Where merges go: a repository and the branch in it.
#[derive(Clone, Debug)]
pub struct MergeTarget {
    pub repo: PathBuf,
    pub branch: String,
    /// The review lanes owed before a merge. `None` is the default by risk (`guard::owed_lanes`: code review for medium and high,
    /// security review as well for high); a person's release stands in for them.
    pub owed: Option<Vec<String>>,
}

/// What came of one entry when the queue was processed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MergeOutcome {
    Merged {
        task_id: String,
        merge_commit: String,
    },
    /// Held back; the queue retries it when it is processed again.
    Blocked { task_id: String, why: String },
    /// Needs a person before anything moves.
    NeedsPerson { task_id: String, why: String },
    /// Not merged, and not retried.
    Rejected { task_id: String, why: String },
}

/// The window a check may run in.
const CHECK_TIME: Duration = Duration::from_secs(15 * 60);

impl Core {
    fn merge_key(&self, task_id: &str) -> Vec<String> {
        self.store
            .crew_answer_key(task_id)
            .ok()
            .flatten()
            .map(|key| crate::crew::key_needles(&key))
            .unwrap_or_default()
    }

    /// Appends a merge event to the approval ledger.
    pub(crate) fn merge_event(&self, task_id: &str, kind: &str, body: &Value) {
        let Ok(Some(task)) = self.store.crew_task(task_id) else {
            return;
        };
        let needles = self.merge_key(task_id);
        if let Err(error) = self.autopilot_record(&task, kind, body, &needles) {
            tracing::warn!(%error, "could not write a merge event");
        }
    }

    /// The mode in force for a task's crew and repository.
    pub(crate) fn merge_mode(&self, task_id: &str) -> Result<Mode> {
        let task = self
            .store
            .crew_task(task_id)?
            .ok_or_else(|| CoreError::NotFound(format!("crew task {task_id}")))?;
        let run = self
            .store
            .crew_run(task_id)?
            .ok_or_else(|| CoreError::NotFound(format!("crew run {task_id}")))?;
        let project = self.store.thread(&run.lead_thread_id)?.project_id;
        Ok(self.crew_autopilot_mode(&project, &task.crew_id))
    }

    /// Scores a passed task's diff and queues it. Under `off`, or a high-risk diff under `on`, it waits for a person.
    pub async fn crew_queue_merge(
        self: &Arc<Self>,
        task_id: &str,
        target: &MergeTarget,
        now: i64,
    ) -> Result<MergeEntry> {
        let task = self
            .store
            .crew_task(task_id)?
            .ok_or_else(|| CoreError::NotFound(format!("crew task {task_id}")))?;
        if task.state != "passed" {
            return Err(CoreError::Invalid(
                "only a task that passed can merge".into(),
            ));
        }
        let result = self
            .store
            .crew_results(task_id)?
            .into_iter()
            .rfind(|r| r.state == orbyn_contracts::ResultState::Passed);
        let candidate = result
            .as_ref()
            .and_then(|r| r.commit.as_ref())
            .map(|c| c.as_str().to_owned())
            .ok_or_else(|| CoreError::Invalid("the passed task names no commit to merge".into()))?;
        // A lowered area whose way back is met returns here or when a merge is recorded (at most one merge late). Never waits for the
        // merge lock: a long check or bisect holds it, and the check is skipped then. No GET does this.
        if let Err(error) = self.area_restore_if_free(task_id, now) {
            tracing::warn!(%error, "could not check the areas for a return");
        }
        let repo = target.repo.clone();
        gitops::rev(&repo, &candidate).await.map_err(|_| {
            CoreError::Invalid("the commit to merge is not in the target repository".into())
        })?;
        let (paths, lines) = gitops::diff_facts(&repo, &target.branch, &candidate).await?;
        let engine = self
            .store
            .crew_route_decisions(task_id)?
            .last()
            .map(|d| d.engine.as_str().to_owned())
            .unwrap_or_default();
        let areas = areas_of(&paths);
        let project = self
            .project_of_task(task_id)?
            .ok_or_else(|| CoreError::NotFound(format!("crew run {task_id}")))?;
        let (by_area, by_engine) = self.merge_rates(&project, &areas, &engine, now)?;
        let recent = self.merge_recent(target, now)?;
        let facts = DiffFacts {
            files: u32::try_from(paths.len()).unwrap_or(u32::MAX),
            lines,
            paths,
            areas: by_area,
            engine: Some((engine.clone(), by_engine)),
            recent,
        };
        let scored = score(&facts);
        // The mode the diff runs under: the crew's mode (recorded as the entry's `mode`) lowered by every area it touches.
        let effect = self.merge_effect(task_id, &areas, now)?;
        let mode = effect.ceiling;
        let held = self.merge_holds(task_id, target, &facts.paths).await?;
        let (state, note) = if !held.is_empty() {
            (
                "waiting",
                Some(format!("Held for a person: {}.", held.join("; "))),
            )
        } else if let Some(why) = effect.wait_note(scored.risk) {
            ("waiting", Some(why))
        } else {
            ("queued", None)
        };
        let entry = MergeEntry {
            task_id: task_id.to_owned(),
            crew_id: task.crew_id.clone(),
            repo: repo.to_string_lossy().into_owned(),
            branch: target.branch.clone(),
            candidate,
            state: state.to_owned(),
            risk: scored.risk.name().to_owned(),
            reasons: json!(scored.reasons),
            areas,
            samples: scored.samples.clone(),
            engine,
            files: facts.files,
            lines: facts.lines,
            mode: mode.name().to_owned(),
            note,
            released: false,
            queued_unix: now,
        };
        self.store.add_merge_entry(&entry)?;
        let mut body = json!({ "risk": entry.risk, "reasons": entry.reasons, "mode": entry.mode, "files": facts.files, "lines": facts.lines, "state": state, "not_measured": scored.not_measured, "samples": scored.samples });
        if effect.mode != effect.ceiling {
            body["effective_mode"] = json!(effect.mode.name());
            body["area_modes"] = json!(effect.lines.iter().map(|l| &l.line).collect::<Vec<_>>());
        }
        self.merge_event(task_id, "merge_queued", &body);
        if state == "waiting" {
            self.merge_event(
                task_id,
                "merge_waiting",
                &json!({ "why": entry.note, "risk": entry.risk, "held": held }),
            );
        }
        Ok(entry)
    }

    /// A person lets a waiting merge go. It is reviewed by that act, and its review is closed when it lands.
    pub fn crew_release_merge(&self, task_id: &str) -> Result<()> {
        self.store.release_merge_entry(task_id)
    }

    /// A person turns a waiting merge down.
    pub fn crew_reject_merge(&self, task_id: &str, why: &str) -> Result<()> {
        let entry = self
            .store
            .merge_entry(task_id)?
            .ok_or_else(|| CoreError::NotFound(format!("queued merge {task_id}")))?;
        if !matches!(entry.state.as_str(), "waiting" | "queued" | "blocked") {
            return Err(CoreError::Invalid(
                "this merge is no longer in the queue".into(),
            ));
        }
        self.store
            .set_merge_entry_state(task_id, "rejected", Some(why))?;
        self.merge_event(
            task_id,
            "merge_rejected",
            &json!({ "why": why, "by": "person" }),
        );
        Ok(())
    }

    /// Merges what is ready, one at a time and in the order queued.
    pub async fn crew_process_merges(
        self: &Arc<Self>,
        target: &MergeTarget,
        now: i64,
    ) -> Result<Vec<MergeOutcome>> {
        let _one_at_a_time = self.crew_merges.lock().await;
        let (repo, branch) = (
            target.repo.to_string_lossy().into_owned(),
            target.branch.clone(),
        );
        let mut outcomes = Vec::new();
        for entry in self
            .store
            .merge_entries(&repo, &branch, &["queued", "blocked"])?
        {
            if let Some(why) = self.merge_blocker(&entry, target)? {
                // A change of reason is news; the same reason on every pass is not.
                if entry.note.as_deref() != Some(why.as_str()) {
                    self.merge_event(
                        &entry.task_id,
                        "merge_waiting",
                        &json!({ "why": why, "blocked": true, "risk": entry.risk }),
                    );
                }
                self.store
                    .set_merge_entry_state(&entry.task_id, "blocked", Some(&why))?;
                outcomes.push(MergeOutcome::Blocked {
                    task_id: entry.task_id,
                    why,
                });
                continue;
            }
            self.store
                .set_merge_entry_state(&entry.task_id, "merging", None)?;
            let outcome = self.integrate(&entry, target, now).await;
            let outcome = match outcome {
                Ok(outcome) => outcome,
                Err(error) => {
                    let why = error.public_message();
                    self.store
                        .set_merge_entry_state(&entry.task_id, "queued", Some(&why))?;
                    MergeOutcome::Blocked {
                        task_id: entry.task_id.clone(),
                        why,
                    }
                }
            };
            let stop = matches!(outcome, MergeOutcome::NeedsPerson { .. });
            outcomes.push(outcome);
            if stop {
                break;
            }
        }
        Ok(outcomes)
    }

    fn scratch_root(&self) -> PathBuf {
        self.config.data_dir.join("crew-merge")
    }

    /// The commands the merge runs again on the combined state: the task's own command checks, and the repository's.
    pub(super) async fn merge_checks(
        &self,
        task_id: &str,
        repo: &std::path::Path,
        tip: &str,
    ) -> Result<Vec<String>> {
        let mut commands: Vec<String> = self
            .store
            .crew_answer_key(task_id)?
            .map(|key| {
                key.checks
                    .iter()
                    .filter_map(orbyn_contracts::CrewCheck::command)
                    .collect()
            })
            .unwrap_or_default();
        commands.extend(gitops::repo_checks(repo, tip).await?);
        Ok(commands)
    }

    async fn integrate(
        self: &Arc<Self>,
        entry: &MergeEntry,
        target: &MergeTarget,
        now: i64,
    ) -> Result<MergeOutcome> {
        let task_id = entry.task_id.clone();
        if let Some(why) = self.merge_hold_now(entry, target, now).await? {
            self.store
                .set_merge_entry_state(&task_id, "waiting", Some(&why))?;
            self.merge_event(
                &task_id,
                "merge_waiting",
                &json!({ "why": why, "risk": entry.risk }),
            );
            return Ok(MergeOutcome::NeedsPerson { task_id, why });
        }
        let reject = |why: &str, retry: bool| -> Result<MergeOutcome> {
            self.store.set_merge_entry_state(
                &task_id,
                if retry { "queued" } else { "rejected" },
                Some(why),
            )?;
            if retry {
                return Ok(MergeOutcome::Blocked {
                    task_id: task_id.clone(),
                    why: why.to_owned(),
                });
            }
            self.merge_event(
                &task_id,
                "merge_rejected",
                &json!({ "why": why, "by": "queue" }),
            );
            Ok(MergeOutcome::Rejected {
                task_id: task_id.clone(),
                why: why.to_owned(),
            })
        };
        if let Some(folder) = gitops::checked_out(&target.repo, &target.branch).await? {
            let why = format!(
                "{} is checked out in {folder}, and the crew never moves a checked-out branch",
                target.branch
            );
            self.store
                .set_merge_entry_state(&task_id, "waiting", Some(&why))?;
            self.merge_event(
                &task_id,
                "merge_waiting",
                &json!({ "why": "the branch is checked out" }),
            );
            return Ok(MergeOutcome::NeedsPerson { task_id, why });
        }
        let before = gitops::rev(&target.repo, &target.branch).await?;
        let scratch = Scratch::at(&target.repo, &self.scratch_root(), &before).await?;
        let merged = self
            .merge_in_scratch(&scratch, entry, &before, &target.repo)
            .await;
        scratch.finish().await;
        let (merge_commit, ran) = match merged? {
            Ok(done) => done,
            Err(why) => return reject(&why, false),
        };
        if gitops::move_branch(&target.repo, &target.branch, &merge_commit, &before)
            .await
            .is_err()
        {
            return reject(
                "the branch moved while this merged; it will be tried again",
                true,
            );
        }
        self.record_merge(entry, &before, &merge_commit, ran, now)?;
        Ok(MergeOutcome::Merged {
            task_id,
            merge_commit,
        })
    }

    /// Merges and re-checks in the scratch checkout. The inner error is a reason to reject; the outer is a failure to try.
    async fn merge_in_scratch(
        &self,
        scratch: &Scratch,
        entry: &MergeEntry,
        before: &str,
        repo: &std::path::Path,
    ) -> Result<std::result::Result<(String, u32), String>> {
        let message = format!(
            "Merge crew task {}",
            entry.task_id.chars().skip(7).take(8).collect::<String>()
        );
        let merge_commit = match gitops::merge(scratch, &entry.candidate, &message).await {
            Ok(commit) => commit,
            Err(Refused::Conflict) => return Ok(Err("it conflicts with the branch".to_owned())),
            Err(Refused::Other(why)) => return Err(CoreError::Git(why)),
        };
        let commands = self.merge_checks(&entry.task_id, repo, before).await?;
        let deadline = Instant::now() + CHECK_TIME * 2;
        let mut ran = 0u32;
        for command in &commands {
            let result = run_line(command, &scratch.dir, deadline).await;
            ran += 1;
            if result.outcome != CheckOutcome::Pass {
                return Ok(Err(
                    "a required check failed on the combined state".to_owned()
                ));
            }
        }
        Ok(Ok((merge_commit, ran)))
    }

    fn record_merge(
        &self,
        entry: &MergeEntry,
        before: &str,
        merge_commit: &str,
        merge_checks: u32,
        now: i64,
    ) -> Result<()> {
        let task_id = &entry.task_id;
        let results = self.store.crew_results(task_id)?;
        let passed = results
            .iter()
            .rfind(|r| r.state == orbyn_contracts::ResultState::Passed);
        let reviews = self.store.crew_reviews_of(task_id)?;
        let reviewers: Vec<Value> = reviews
            .iter()
            .map(|r| {
                let engine = self.store.crew_route_decisions(&r.task_id).ok().and_then(|d| d.last().map(|d| d.engine.as_str().to_owned()));
                let state = self.store.crew_task(&r.task_id).ok().flatten().map_or_else(String::new, |t| t.state);
                json!({ "lane": r.lane, "round": r.round, "engine": engine, "state": state, "role": r.role })
            })
            .collect();
        let status = self.review_status(task_id).ok();
        let findings = |check: &str| {
            status.as_ref().map_or(0, |s| {
                s["findings"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|f| f["check"] == check)
                    .count()
            })
        };
        let fixed = status.as_ref().map_or(0, |s| {
            s["findings"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|f| f["status"] == "fixed")
                .count()
        });
        let proof = json!({
            "task_checks": passed.map(|r| r.checks.iter().map(|c| json!({ "check": c.check, "outcome": c.outcome })).collect::<Vec<_>>()),
            "merge_checks_run": merge_checks,
            "reviewers": reviewers,
            "findings": { "confirmed": findings("confirmed"), "unconfirmed": findings("unconfirmed"), "fixed": fixed },
            "cost": passed.map(|r| &r.usage),
            "risk_samples": entry.samples,
        });
        let mut not_verified: Vec<String> = passed
            .map(|r| {
                r.not_verified
                    .iter()
                    .map(|n| n.as_str().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        not_verified.push("test coverage of the touched code".to_owned());
        not_verified
            .push("a forge or CI run: the checks ran locally on the merged state".to_owned());
        let risk = Risk::parse(&entry.risk).unwrap_or(Risk::High);
        let record = MergeRecord {
            id: 0,
            task_id: task_id.clone(),
            crew_id: entry.crew_id.clone(),
            repo: entry.repo.clone(),
            branch: entry.branch.clone(),
            before_commit: before.to_owned(),
            merge_commit: merge_commit.to_owned(),
            candidate: entry.candidate.clone(),
            risk: entry.risk.clone(),
            reasons: entry.reasons.clone(),
            areas: entry.areas.clone(),
            engine: entry.engine.clone(),
            mode: entry.mode.clone(),
            files: entry.files,
            lines: entry.lines,
            proof,
            not_verified: json!(not_verified),
            merged_unix: now,
            due_unix: review_window(risk).map(|w| now + w),
            review_state: "waiting".to_owned(),
            comment: None,
            watch: "unknown".to_owned(),
            reverted_commit: None,
            needs_person: None,
        };
        let id = self.store.add_crew_merge(&record)?;
        // Under the merge lock `crew_process_merges` holds. The merge just recorded counts toward a trip and toward the way back.
        let stored = MergeRecord {
            id,
            ..record.clone()
        };
        if let Err(error) = self.area_breaker_after_merge(&stored, now) {
            self.area_breaker_failed(task_id, "merge", None, &error);
        }
        // A merge a person released was reviewed by that act.
        if entry.released {
            self.store.set_merge_review(task_id, "looks_good", None)?;
        }
        self.merge_event(task_id, "merged", &json!({ "merge": merge_commit, "before": before, "risk": entry.risk, "mode": entry.mode, "due": record.due_unix }));
        Ok(())
    }
}
