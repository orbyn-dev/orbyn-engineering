//! What keeps a queued merge from going: the risk inputs (revert rates, the rolling size), the holds that wait for a person whatever the
//! mode, the lanes owed, and the mode as it is when the merge happens.

use serde_json::json;

use super::gitops;
use super::guard::{owed_lanes, sensitive};
use super::queue::MergeTarget;
use super::risk::{RECENT_MERGES, Rate, RecentTotals, Risk, WINDOW_DAYS};
use crate::app::Core;
use crate::crew::autopilot::Mode;
use crate::crew::lifecycle::is_final;
use crate::error::{CoreError, Result};
use crate::store::{MergeEntry, MergeRecord};

impl Core {
    /// The revert rate of each area and of an engine over the last 60 days, from the merges the queue made for this project: another
    /// repository's merges are not counted.
    pub(super) fn merge_rates(
        &self,
        project_id: &str,
        areas: &[String],
        engine: &str,
        now: i64,
    ) -> Result<(Vec<(String, Rate)>, Rate)> {
        let since = now - WINDOW_DAYS * 86_400;
        let recent: Vec<MergeRecord> = self
            .store
            .crew_merges_of_project(project_id)?
            .into_iter()
            .filter(|m| m.merged_unix >= since)
            .collect();
        let rate_of = |keep: &dyn Fn(&MergeRecord) -> bool| {
            let mine: Vec<&MergeRecord> = recent.iter().filter(|m| keep(m)).collect();
            Rate {
                merges: u32::try_from(mine.len()).unwrap_or(u32::MAX),
                reverts: u32::try_from(
                    mine.iter().filter(|m| m.review_state == "reverted").count(),
                )
                .unwrap_or(u32::MAX),
            }
        };
        let by_area = areas
            .iter()
            .map(|a| (a.clone(), rate_of(&|m| m.areas.contains(a))))
            .collect();
        Ok((by_area, rate_of(&|m| m.engine == engine)))
    }

    /// What the last merges within a day changed, for the rolling size.
    pub(super) fn merge_recent(&self, target: &MergeTarget, now: i64) -> Result<RecentTotals> {
        let (repo, branch) = (
            target.repo.to_string_lossy().into_owned(),
            target.branch.clone(),
        );
        let merges = self.store.crew_merges(&repo, &branch)?;
        let recent: Vec<&MergeRecord> = merges
            .iter()
            .rev()
            .filter(|m| m.merged_unix >= now - 86_400)
            .take(RECENT_MERGES)
            .collect();
        Ok(RecentTotals {
            merges: u32::try_from(recent.len()).unwrap_or(u32::MAX),
            files: recent.iter().map(|m| m.files).fold(0, u32::saturating_add),
            lines: recent.iter().map(|m| m.lines).fold(0, u32::saturating_add),
        })
    }

    /// Why a diff is held for a person whatever the mode: `.orbyn/`, the answer key's protected files, CI config and the files the
    /// checks run. Reading the repository's checks can fail, and then the diff is held too.
    pub(super) async fn merge_holds(
        &self,
        task_id: &str,
        target: &MergeTarget,
        paths: &[String],
    ) -> Result<Vec<String>> {
        let protected = self
            .store
            .crew_answer_key(task_id)?
            .map(|k| k.protected_paths)
            .unwrap_or_default();
        let tip = gitops::rev(&target.repo, &target.branch).await?;
        // The repository's checks and its files come from the branch's tip, a commit the crew did not write.
        let checks = self.merge_checks(task_id, &target.repo, &tip).await.ok();
        let mut held = sensitive(paths, &protected, checks.as_deref().unwrap_or(&[]));
        if checks.is_none() {
            held.push("the repository's checks could not be read".to_owned());
        }
        Ok(held)
    }

    /// Why a queued task is not ready: its stages must be green. `None` when it is.
    pub(super) fn merge_blocker(
        &self,
        entry: &MergeEntry,
        target: &MergeTarget,
    ) -> Result<Option<String>> {
        let task_id = entry.task_id.as_str();
        let task = self
            .store
            .crew_task(task_id)?
            .ok_or_else(|| CoreError::NotFound(format!("crew task {task_id}")))?;
        if task.state != "passed" {
            return Ok(Some("the task is no longer passed".to_owned()));
        }
        let status = self
            .review_status(task_id)
            .map_err(|e| CoreError::Internal(e.message))?;
        if status["blocked"] == json!(true) {
            return Ok(Some("a confirmed finding blocks it".to_owned()));
        }
        let running = status["lanes"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|l| l["rounds"].as_array().into_iter().flatten())
            .any(|r| {
                r["state"]
                    .as_str()
                    .is_some_and(|s| !is_final(s) && s != "needs_person")
            });
        if running {
            return Ok(Some("a review lane is still running".to_owned()));
        }
        // The lanes owed must have run and passed, unless a person released it.
        if entry.released {
            return Ok(None);
        }
        let owed = target
            .owed
            .clone()
            .unwrap_or_else(|| owed_lanes(Risk::parse(&entry.risk).unwrap_or(Risk::High)));
        let ran = |lane: &str| lane_ran(&status, lane);
        Ok(owed.iter().find(|lane| !ran(lane)).map(|lane| {
            format!("the {lane} review has not run: the merge queue waits for a lane named {lane}")
        }))
    }

    /// The reason a queued entry must wait for a person now: the mode as it is now (the person may have turned Autopilot off since),
    /// and what the diff touches against the branch as it is now. A released entry skips both.
    pub(super) async fn merge_hold_now(
        &self,
        entry: &MergeEntry,
        target: &MergeTarget,
    ) -> Result<Option<String>> {
        if entry.released {
            return Ok(None);
        }
        let risk = Risk::parse(&entry.risk).unwrap_or(Risk::High);
        match (self.merge_mode(&entry.task_id)?, risk) {
            (Mode::Off, _) => {
                return Ok(Some(
                    "Autopilot is off, so a person releases every merge.".to_owned(),
                ));
            }
            (Mode::On, Risk::High) => {
                return Ok(Some(
                    "A high-risk diff waits for a person under On.".to_owned(),
                ));
            }
            _ => {}
        }
        let (paths, _) = gitops::diff_facts(&target.repo, &target.branch, &entry.candidate).await?;
        let held = self.merge_holds(&entry.task_id, target, &paths).await?;
        Ok((!held.is_empty()).then(|| format!("Held for a person: {}.", held.join("; "))))
    }
}

/// Whether the latest round of `lane` in a review status passed. An earlier passed round does not count once a later one has not.
pub fn lane_ran(status: &serde_json::Value, lane: &str) -> bool {
    status["lanes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|l| l["lane"] == lane)
        .filter_map(|l| {
            l["rounds"]
                .as_array()
                .into_iter()
                .flatten()
                .max_by_key(|r| r["round"].as_u64().unwrap_or(0))
        })
        .any(|latest| latest["state"] == "passed")
}
