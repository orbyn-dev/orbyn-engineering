//! Watch on main. After merges, the repository's own checks (`checks` in `.orbyn/crew.json`, read at a commit the crew did not
//! write) run on the branch's tip. If they break, the crew bisects the first-parent commits since the last good one, and
//! reverts the merge it finds only when it is certain: the check fails with the merge (twice), passes without it, and the
//! later merges pass without it too. Otherwise it records the break and asks the person. Local only: the revert moves a local
//! branch that no one has checked out, and nothing is pushed. A merge by someone else is never reverted.

use std::sync::Arc;
use std::time::{Duration, Instant};

use orbyn_contracts::CheckOutcome;
use serde_json::json;

use super::gitops::{self, Refused, Scratch};
use super::queue::MergeTarget;
use crate::app::Core;
use crate::crew::autopilot::Mode;
use crate::crew::checks::run_line;
use crate::error::{CoreError, Result};
use crate::store::MergeRecord;

/// What a watch found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WatchOutcome {
    /// There is nothing the crew merged that the checks have not cleared.
    NothingToWatch,
    /// The repository defines no checks, so there is nothing to watch with. It is never reported as healthy.
    NoChecks,
    /// The checks pass on the tip; the merges up to it are green.
    Healthy { cleared: usize },
    /// The break was a crew merge and the crew was certain; it reverted it.
    Reverted {
        task_id: String,
        revert_commit: String,
    },
    /// The checks fail and the crew is not certain, or the cause is not a crew merge. A person decides.
    NeedsPerson {
        why: String,
        culprit: Option<String>,
        dependents: Vec<String>,
    },
}

const CHECK_TIME: Duration = Duration::from_secs(15 * 60);

/// Whether every command passes at the scratch checkout's current commit.
async fn passes(scratch: &Scratch, commands: &[String]) -> bool {
    let deadline = Instant::now() + CHECK_TIME * 2;
    for command in commands {
        if run_line(command, &scratch.dir, deadline).await.outcome != CheckOutcome::Pass {
            return false;
        }
    }
    true
}

async fn passes_at(scratch: &Scratch, rev: &str, commands: &[String]) -> Result<bool> {
    scratch.checkout(rev).await?;
    Ok(passes(scratch, commands).await)
}

/// Reverts `record` on the branch, checking the result first with the repository's checks. A repository that defines none has nothing
/// to run: the revert is made and recorded with `verified: false`, so it never reads as checked. The inner error is why it was not done.
pub(super) async fn revert_merge(
    core: &Arc<Core>,
    record: &MergeRecord,
    target: &MergeTarget,
    now: i64,
) -> Result<std::result::Result<String, String>> {
    if let Some(folder) = gitops::checked_out(&target.repo, &target.branch).await? {
        return Ok(Err(format!(
            "{} is checked out in {folder}, and the crew never moves a checked-out branch",
            target.branch
        )));
    }
    let tip = gitops::rev(&target.repo, &target.branch).await?;
    // A branch that no longer holds this merge (reset, rewritten, or another history under the same name) is not reverted.
    if !gitops::contains(&target.repo, &target.branch, &record.merge_commit).await? {
        return Ok(Err(format!(
            "{} no longer holds this merge, so there is nothing to revert on it",
            target.branch
        )));
    }
    let commands = gitops::repo_checks(&target.repo, &record.before_commit).await?;
    let scratch = Scratch::at(&target.repo, &core.config.data_dir.join("crew-merge"), &tip).await?;
    let made = async {
        match gitops::revert(&scratch, &record.merge_commit).await {
            Ok(commit) => {
                // Later merges that depended on this one fail without it, or the revert would not apply.
                if !commands.is_empty() && !passes(&scratch, &commands).await {
                    return Ok(Err(
                        "the checks fail without it, so a later merge depends on it".to_owned(),
                    ));
                }
                Ok(Ok(commit))
            }
            Err(Refused::Conflict) => Ok(Err(
                "a later change touches the same lines, so it cannot be reverted cleanly"
                    .to_owned(),
            )),
            Err(Refused::Other(why)) => Err(CoreError::Git(why)),
        }
    }
    .await;
    scratch.finish().await;
    let commit = match made? {
        Ok(commit) => commit,
        Err(why) => return Ok(Err(why)),
    };
    if gitops::move_branch(&target.repo, &target.branch, &commit, &tip)
        .await
        .is_err()
    {
        return Ok(Err(
            "the branch moved while the revert was checked; try again".to_owned(),
        ));
    }
    core.store.set_merge_reverted(&record.task_id, &commit)?;
    // Shadow: the circuit breaker records what this revert means for the areas the merge touched. The merge lock is held by both
    // callers, and a failure here never undoes a revert that already moved the branch.
    if let Err(error) = core.area_breaker_after_revert(record, &commit, now) {
        tracing::warn!(%error, "could not record the revert against its areas");
    }
    core.merge_event(
        &record.task_id,
        "reverted",
        &json!({ "revert": commit, "merge": record.merge_commit, "by": "request", "verified": !commands.is_empty() }),
    );
    Ok(Ok(commit))
}

impl Core {
    /// Runs the repository's checks on the branch's tip and, if they fail, finds and reverts the merge that broke it.
    pub async fn crew_watch_main(
        self: &Arc<Self>,
        target: &MergeTarget,
        now: i64,
    ) -> Result<WatchOutcome> {
        let _one_at_a_time = self.crew_merges.lock().await;
        let (repo, branch) = (
            target.repo.to_string_lossy().into_owned(),
            target.branch.clone(),
        );
        let merges: Vec<MergeRecord> = self.store.crew_merges(&repo, &branch)?;
        let unwatched: Vec<&MergeRecord> = merges
            .iter()
            .filter(|m| m.watch != "green" && m.review_state != "reverted")
            .collect();
        let Some(first) = unwatched.first() else {
            return Ok(WatchOutcome::NothingToWatch);
        };
        // The checks come from the last commit known good, or from where the first unwatched merge began.
        let boundary = merges
            .iter()
            .rfind(|m| m.watch == "green" && m.review_state != "reverted")
            .map_or_else(|| first.before_commit.clone(), |m| m.merge_commit.clone());
        let commands = gitops::repo_checks(&target.repo, &boundary).await?;
        if commands.is_empty() {
            return Ok(WatchOutcome::NoChecks);
        }
        let tip = gitops::rev(&target.repo, &branch).await?;
        let scratch =
            Scratch::at(&target.repo, &self.config.data_dir.join("crew-merge"), &tip).await?;
        let found = self
            .find_break(&scratch, &target.repo, &boundary, &tip, &commands, &merges)
            .await;
        scratch.finish().await;
        let found = found?;
        match found {
            Found::Healthy => {
                for m in &unwatched {
                    self.store.set_merge_watch(&m.task_id, "green")?;
                }
                Ok(WatchOutcome::Healthy {
                    cleared: unwatched.len(),
                })
            }
            Found::Unsure {
                why,
                culprit,
                dependents,
            } => {
                if let Some(task) = &culprit {
                    self.store.set_merge_watch(task, "broken")?;
                    self.store.set_merge_needs_person(task, Some(&why))?;
                    self.merge_event(
                        task,
                        "main_broken",
                        &json!({ "why": why, "dependents": dependents, "asks": true }),
                    );
                    self.merge_event(
                        task,
                        "person_told",
                        &json!({ "about": "main_broken", "why": why }),
                    );
                }
                Ok(WatchOutcome::NeedsPerson {
                    why,
                    culprit,
                    dependents,
                })
            }
            Found::Certain(record) => {
                let mode = self.merge_mode(&record.task_id)?;
                if mode == Mode::Off {
                    let why = "Autopilot is off, so a person decides the revert.".to_owned();
                    self.store.set_merge_watch(&record.task_id, "broken")?;
                    self.store
                        .set_merge_needs_person(&record.task_id, Some(&why))?;
                    self.merge_event(
                        &record.task_id,
                        "main_broken",
                        &json!({ "why": why, "asks": true, "certain": true }),
                    );
                    self.merge_event(
                        &record.task_id,
                        "person_told",
                        &json!({ "about": "main_broken", "why": why }),
                    );
                    return Ok(WatchOutcome::NeedsPerson {
                        why,
                        culprit: Some(record.task_id.clone()),
                        dependents: Vec::new(),
                    });
                }
                self.revert_found(&record, target, &merges, now).await
            }
        }
    }

    async fn revert_found(
        self: &Arc<Self>,
        record: &MergeRecord,
        target: &MergeTarget,
        merges: &[MergeRecord],
        now: i64,
    ) -> Result<WatchOutcome> {
        let dependents: Vec<String> = merges
            .iter()
            .filter(|m| m.id > record.id && m.review_state != "reverted")
            .map(|m| m.task_id.clone())
            .collect();
        // `revert_merge` re-checks the tip after the revert, so a later merge that needs this one stops it.
        match revert_merge(self, record, target, now).await? {
            Ok(revert_commit) => {
                for m in merges.iter().filter(|m| {
                    m.id < record.id && m.watch != "green" && m.review_state != "reverted"
                }) {
                    self.store.set_merge_watch(&m.task_id, "green")?;
                }
                self.merge_event(&record.task_id, "person_told", &json!({ "about": "reverted", "revert": revert_commit, "merge": record.merge_commit }));
                Ok(WatchOutcome::Reverted {
                    task_id: record.task_id.clone(),
                    revert_commit,
                })
            }
            Err(why) => {
                self.store.set_merge_watch(&record.task_id, "broken")?;
                self.store
                    .set_merge_needs_person(&record.task_id, Some(&why))?;
                self.merge_event(
                    &record.task_id,
                    "main_broken",
                    &json!({ "why": why, "dependents": dependents, "asks": true }),
                );
                self.merge_event(
                    &record.task_id,
                    "person_told",
                    &json!({ "about": "main_broken", "why": why }),
                );
                Ok(WatchOutcome::NeedsPerson {
                    why,
                    culprit: Some(record.task_id.clone()),
                    dependents,
                })
            }
        }
    }

    /// Runs the checks at the tip and, if they fail, bisects to the first failing first-parent commit.
    async fn find_break(
        &self,
        scratch: &Scratch,
        repo: &std::path::Path,
        boundary: &str,
        tip: &str,
        commands: &[String],
        merges: &[MergeRecord],
    ) -> Result<Found> {
        if passes_at(scratch, tip, commands).await? {
            return Ok(Found::Healthy);
        }
        let unsure = |why: &str, culprit: Option<String>, dependents: Vec<String>| Found::Unsure {
            why: why.to_owned(),
            culprit,
            dependents,
        };
        if !passes_at(scratch, boundary, commands).await? {
            return Ok(unsure(
                "The checks already failed before the first merge the crew watches.",
                None,
                Vec::new(),
            ));
        }
        let commits = gitops::first_parents(repo, boundary, tip).await?;
        if commits.is_empty() {
            return Ok(unsure(
                "The checks fail on a commit already known to be good.",
                None,
                Vec::new(),
            ));
        }
        // The first commit that fails, taking the checks to stay broken once broken.
        let (mut low, mut high) = (0usize, commits.len() - 1);
        while low < high {
            let mid = (low + high) / 2;
            if passes_at(scratch, &commits[mid], commands).await? {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        let culprit = &commits[low];
        let Some(record) = merges.iter().find(|m| &m.merge_commit == culprit) else {
            return Ok(unsure(
                "The commit that broke the checks is not a merge the crew made.",
                None,
                Vec::new(),
            ));
        };
        let later: Vec<String> = merges
            .iter()
            .filter(|m| m.id > record.id && m.review_state != "reverted")
            .map(|m| m.task_id.clone())
            .collect();
        // Fails with the merge, twice; passes without it.
        let again = !passes_at(scratch, culprit, commands).await?
            && !passes_at(scratch, culprit, commands).await?;
        let parent = gitops::parent(repo, culprit).await?;
        let clean = passes_at(scratch, &parent, commands).await?;
        if !again || !clean {
            return Ok(unsure(
                "The failure does not reproduce the same way with and without that merge, so it may be flaky.",
                Some(record.task_id.clone()),
                later,
            ));
        }
        Ok(Found::Certain(Box::new(record.clone())))
    }
}

enum Found {
    Healthy,
    Unsure {
        why: String,
        culprit: Option<String>,
        dependents: Vec<String>,
    },
    Certain(Box<MergeRecord>),
}
