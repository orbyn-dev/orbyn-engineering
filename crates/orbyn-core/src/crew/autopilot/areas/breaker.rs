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

    /// Lowers `area` from the mode the merge was made under to `On`, as one event and one mode row. Returns whether it was recorded (a
    /// cause already recorded is not recorded twice). The `from` mode is the one the merge actually used (`MergeRecord.mode`), not an
    /// assumed Full. Part 2 (enforcement) must take each merge's recorded mode as its basis, not only the repository's ceiling used here.
    fn demote_area(
        &self,
        project: &str,
        area: &str,
        record: &MergeRecord,
        cause: &str,
        facts: &Facts,
        now: i64,
    ) -> Result<bool> {
        let reason = json!({
            "rule": "reverts_in_window",
            "merges": facts.window.merges,
            "reverts": facts.window.reverts,
            "window_days": WINDOW_DAYS,
            "cause": cause,
            "proposed": true,
        });
        let kept = record_json(facts.window);
        let written = self.store.record_area_change(&AreaWrite {
            project_id: project,
            area,
            kind: "demoted",
            from_mode: Some(&record.mode),
            to_mode: Some(Mode::On.name()),
            decided_by: "breaker",
            cause,
            record: &kept,
            at: now,
            change: AreaChange::Demote {
                trip_reason: reason.clone(),
                recovery_since: now,
            },
        })?;
        if written {
            // On the task whose revert or merge tripped it, so Flow shows it.
            self.merge_event(
                &record.task_id,
                "area_demoted",
                &json!({ "area": area, "from": record.mode, "to": "on", "shadow": true, "reason": reason }),
            );
        }
        Ok(written)
    }

    /// A merge was reverted by `revert_commit` at `now`; the record already says so. For each area the merge touched: a lowered area's
    /// recovery clock restarts, and an area that follows its ceiling is lowered when its window trips. Each write is the event and
    /// the mode row in one transaction, and a revert already recorded is not recorded again. Shadow: nothing reads the result.
    /// A failure is only logged by the caller; part 2 needs a visible ledger event and a rule that fails closed.
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
            match stored {
                Stored::Demoted { .. } => {
                    let kept = record_json(facts.window);
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
                    self.demote_area(&project, area, record, revert_commit, &facts, now)?;
                }
                Stored::Follows => {}
            }
        }
        Ok(())
    }

    /// A merge was just recorded (`record` is the stored row, with its id). For each area it touched that follows its ceiling, the trip
    /// is evaluated again: 3 merges with 2 reverted and then 2 clean ones meet the rule at the fifth merge with no further revert. The
    /// cause is `merge:<id>`, so a retry records once. Then lowered areas whose way back is met return.
    pub(crate) fn area_breaker_after_merge(&self, record: &MergeRecord, now: i64) -> Result<()> {
        let Some(project) = self.project_of_task(&record.task_id)? else {
            return Ok(());
        };
        let merges = self.store.crew_merges_of_project(&project)?;
        let ceiling = self.crew_project_mode(&project);
        for area in &record.areas {
            let row = self.store.area_mode(&project, area)?;
            let stored = stored_of(row.as_ref());
            let facts = facts_of(&merges, area, stored, now);
            if stored == Stored::Follows && decide(&facts, stored, ceiling, now) == Change::Demote {
                let cause = format!("merge:{}", record.id);
                self.demote_area(&project, area, record, &cause, &facts, now)?;
            }
        }
        self.area_restore_check(&record.task_id, now)
    }

    /// The restore check at the start of queuing, skipped when the merge lock is held: queuing must not wait for a long check or a
    /// bisect, and the same check runs when a merge is recorded. Returns whether it ran.
    pub(crate) fn area_restore_if_free(&self, task_id: &str, now: i64) -> Result<bool> {
        let Ok(_free) = self.crew_merges.try_lock() else {
            return Ok(false);
        };
        self.area_restore_check(task_id, now)?;
        Ok(true)
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
