//! The breaker's writes. A revert that passes through `revert_merge` calls [`Core::area_breaker_after_revert`] under the merge lock;
//! a merge being recorded or queued calls [`Core::area_restore_check`]. Both are idempotent: the same cause is recorded once.

use serde_json::{Value, json};

use super::decide::{Change, Counts, Facts, Stored, count, decide, window_start};
use super::{RECOVERY_DAYS, WINDOW_DAYS};
use crate::app::Core;
use crate::crew::autopilot::Mode;
use crate::error::{CoreError, Result};
use crate::store::{AreaChange, AreaMode, AreaWrite, MergeRecord};

/// The record an event keeps: the window's counts at that moment.
pub(super) fn record_json(window: Counts) -> Value {
    json!({ "merges": window.merges, "reverts": window.reverts, "window_days": WINDOW_DAYS, "escapes": null })
}

/// What the breaker knows of a stored row.
pub(super) fn stored_of(row: Option<&AreaMode>) -> Stored {
    match row {
        Some(m) if m.state == "demoted" => Stored::Demoted {
            recovery_since: m.recovery_since.or(m.tripped_at).unwrap_or(m.updated_at),
        },
        _ => Stored::Follows,
    }
}

/// The counts the rule reads for one area at `now`.
pub(super) fn facts_of(merges: &[MergeRecord], area: &str, stored: Stored, now: i64) -> Facts {
    Facts {
        window: count(merges, area, window_start(now)),
        since_recovery: match stored {
            Stored::Demoted { recovery_since } => count(merges, area, recovery_since),
            Stored::Follows => Counts::default(),
        },
    }
}

impl Core {
    /// The project whose lead started a task, or `None` when the task has no run or its thread is gone.
    pub(crate) fn project_of_task(&self, task_id: &str) -> Result<Option<String>> {
        let Some(run) = self.store.crew_run(task_id)? else {
            return Ok(None);
        };
        match self.store.thread(&run.lead_thread_id) {
            Ok(thread) => Ok(Some(thread.project_id)),
            Err(CoreError::NotFound(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// A merge was reverted by `revert_commit` at `now`; the record already says so. For each area the merge touched: a lowered area's
    /// recovery clock restarts, and an area that follows its ceiling is lowered when its window trips. Each write is the event and
    /// the mode row in one transaction, and a revert already recorded is not recorded again. Shadow: nothing reads the result.
    pub(crate) fn area_breaker_after_revert(
        &self,
        record: &MergeRecord,
        revert_commit: &str,
        now: i64,
    ) -> Result<()> {
        let Some(project) = self.project_of_task(&record.task_id)? else {
            return Ok(());
        };
        let merges = self.store.crew_merges_of_project(&project)?;
        let ceiling = self.crew_project_mode(&project);
        for area in &record.areas {
            let row = self.store.area_mode(&project, area)?;
            let stored = stored_of(row.as_ref());
            let facts = facts_of(&merges, area, stored, now);
            let kept = record_json(facts.window);
            match stored {
                Stored::Demoted { .. } => {
                    self.store.record_area_change(&AreaWrite {
                        project_id: &project,
                        area,
                        kind: "recovery_reset",
                        from_mode: Some(Mode::On.name()),
                        to_mode: Some(Mode::On.name()),
                        decided_by: "breaker",
                        cause: revert_commit,
                        record: &kept,
                        at: now,
                        change: AreaChange::ResetRecovery { since: now },
                    })?;
                }
                Stored::Follows if decide(&facts, stored, ceiling, now) == Change::Demote => {
                    let reason = json!({
                        "rule": "reverts_in_window",
                        "merges": facts.window.merges,
                        "reverts": facts.window.reverts,
                        "window_days": WINDOW_DAYS,
                        "revert": revert_commit,
                        "proposed": true,
                    });
                    let written = self.store.record_area_change(&AreaWrite {
                        project_id: &project,
                        area,
                        kind: "demoted",
                        from_mode: Some(Mode::Full.name()),
                        to_mode: Some(Mode::On.name()),
                        decided_by: "breaker",
                        cause: revert_commit,
                        record: &kept,
                        at: now,
                        change: AreaChange::Demote {
                            trip_reason: reason.clone(),
                            recovery_since: now,
                        },
                    })?;
                    if written {
                        // On the task whose revert tripped it, so Flow shows it.
                        self.merge_event(
                            &record.task_id,
                            "area_demoted",
                            &json!({ "area": area, "from": "full", "to": "on", "shadow": true, "reason": reason }),
                        );
                    }
                }
                Stored::Follows => {}
            }
        }
        Ok(())
    }

    /// Returns the lowered areas of the task's project that have met the way back: `RECOVERY_DAYS` without a revert and the merges
    /// since, none reverted. Called when a merge is recorded and when one is queued, so a return waits for at most one merge.
    pub(crate) fn area_restore_check(&self, task_id: &str, now: i64) -> Result<()> {
        let Some(project) = self.project_of_task(task_id)? else {
            return Ok(());
        };
        let lowered: Vec<AreaMode> = self
            .store
            .area_modes(&project)?
            .into_iter()
            .filter(|m| m.state == "demoted")
            .collect();
        if lowered.is_empty() {
            return Ok(());
        }
        let merges = self.store.crew_merges_of_project(&project)?;
        let ceiling = self.crew_project_mode(&project);
        for row in &lowered {
            let stored = stored_of(Some(row));
            let facts = facts_of(&merges, &row.area, stored, now);
            if decide(&facts, stored, ceiling, now) != Change::Restore {
                continue;
            }
            let Stored::Demoted { recovery_since } = stored else {
                continue;
            };
            let kept = record_json(facts.window);
            let cause = format!("restore:{recovery_since}");
            self.store.record_area_change(&AreaWrite {
                project_id: &project,
                area: &row.area,
                kind: "restored",
                from_mode: Some(Mode::On.name()),
                to_mode: Some(ceiling.name()),
                decided_by: "breaker",
                cause: &cause,
                record: &kept,
                at: now,
                change: AreaChange::Restore,
            })?;
        }
        Ok(())
    }
}

/// Days of the recovery clock a lowered area has run, whole days.
pub(super) fn clean_days(recovery_since: i64, now: i64) -> u32 {
    u32::try_from(((now - recovery_since).max(0) / 86_400).min(i64::from(u32::MAX))).unwrap_or(0)
}

/// The days the way back needs, for the views.
pub(super) fn days_needed() -> u32 {
    u32::try_from(RECOVERY_DAYS).unwrap_or(14)
}
