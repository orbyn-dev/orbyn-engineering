//! After a restart. A task's verification and its budget timer live in memory, so a task left
//! `running` or `verifying` is picked up when the core starts: a verifying task is verified now,
//! a running builder or fix task whose worker has no run and nothing queued resumes once with the
//! same thread and key, and every other running task (a reviewer, a second death) stops with
//! `engine_error`. Every budget timer is set again with what is left of its budget, off time free.

use std::sync::Arc;
use std::time::Duration;

use orbyn_contracts::{ResultState, StopReason};

use super::settle::resume_prompt;
use super::verify::budget_left_after_downtime;
use crate::app::Core;
use crate::store::ResumeOutcome;

impl Core {
    /// Starts `recover_crew_tasks` in the background when there is a runtime to run it in.
    pub(crate) fn recover_crew_tasks_soon(self: &Arc<Self>) {
        if !self.crew_enabled() {
            return;
        }
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let core = self.clone();
            runtime.spawn(async move { core.recover_crew_tasks().await });
        }
    }

    pub(crate) async fn recover_crew_tasks(self: &Arc<Self>) {
        let Ok(tasks) = self.store.crew_tasks_in_flight() else {
            return;
        };
        for (task, run) in tasks {
            if self
                .store
                .crew_correction_for_task(&task.id)
                .ok()
                .flatten()
                .is_some()
            {
                // The forge correction owner resumes deterministic work without a worker or provider timer.
                continue;
            }
            if let Ok(Some(brief)) = self.store.crew_brief(&task.id) {
                // The app being off spends no minutes: count only up to the task's last recorded event.
                let last = self
                    .store
                    .crew_task_last_event(&task.id)
                    .ok()
                    .flatten()
                    .or_else(|| crate::util::parse_time(&task.updated_at))
                    .unwrap_or_else(crate::util::unix_now);
                let left = budget_left_after_downtime(
                    &task.created_at,
                    u64::from(brief.budget.minutes),
                    last,
                    run.downtime_secs,
                );
                // Excused only after the allowance above was read from the earlier restarts' downtime.
                let off = crate::util::unix_now().saturating_sub(last);
                if let Err(error) = self.store.excuse_crew_downtime(&task.id, off) {
                    tracing::warn!(%error, task = %task.id, "could not record a crew task's off time");
                }
                if left.is_zero() {
                    self.crew_budget_ended(&task.id);
                    continue;
                }
                self.watch_crew_budget(&task.id, left);
            }
            let Some(worker) = run.worker_thread_id.as_deref() else {
                continue;
            };
            let busy = self
                .store
                .thread(worker)
                .is_ok_and(|thread| thread.active_run.is_some())
                || self
                    .store
                    .list_queue(worker)
                    .is_ok_and(|queued| !queued.is_empty());
            if busy {
                continue;
            }
            if task.state == "verifying" {
                let core = self.clone();
                let id = task.id.clone();
                tokio::spawn(async move { core.verify_crew_task(&id).await });
                continue;
            }
            match self.store.resume_crew_task(&task.id) {
                Ok(ResumeOutcome::Resumed { task, run }) => {
                    if let Err(error) = self.send_to_worker(&task, &resume_prompt(run.attempts)) {
                        tracing::warn!(%error, task = %task.id, "a crew task could not be told to resume");
                        self.stop_after_restart(&task.id, "send_failed");
                    }
                }
                // The stop writes its state change and its audit row in one transaction of its own.
                Ok(ResumeOutcome::Refused(why)) => self.stop_after_restart(&task.id, why),
                Ok(ResumeOutcome::NotInFlight) => {}
                Err(error) => {
                    tracing::warn!(%error, task = %task.id, "a crew task could not be resumed");
                }
            }
        }
        self.excuse_held_downtime();
    }

    /// Credits the off time of the tasks waiting for a person. A held task has no budget timer across a restart and no
    /// recovery picks it up, so its minutes would otherwise count the whole window the app was off: when the lead resumes
    /// it, the remaining minutes are read from the run and that credit is what keeps them right. The credit's own audit row
    /// is the next restart's boundary (`excuse_crew_downtime`), so no off time is credited twice.
    fn excuse_held_downtime(&self) {
        let Ok(tasks) = self.store.crew_tasks_held() else {
            return;
        };
        for (task, _run) in tasks {
            let last = self
                .store
                .crew_task_last_event(&task.id)
                .ok()
                .flatten()
                .or_else(|| crate::util::parse_time(&task.updated_at))
                .unwrap_or_else(crate::util::unix_now);
            let off = crate::util::unix_now().saturating_sub(last);
            if let Err(error) = self.store.excuse_crew_downtime(&task.id, off) {
                tracing::warn!(%error, task = %task.id, "could not record a held crew task's off time");
            }
        }
    }

    /// Stops a task the restart leaves alone: `stopped` with `engine_error`, as before, and the
    /// `crew.task.restart_stopped` row that says why.
    fn stop_after_restart(self: &Arc<Self>, task_id: &str, why: &'static str) {
        match self.store.stop_crew_after_restart(task_id, why) {
            Ok(Some(task)) => self.end_crew_task(
                &task,
                ResultState::Stopped,
                Some(StopReason::EngineError),
                "The worker's engine stopped with an error.",
                None,
            ),
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(%error, task = task_id, "a crew task could not be stopped after a restart");
            }
        }
    }
}

/// How long the timer for a task waits.
pub(super) fn timer(minutes: u32) -> Duration {
    Duration::from_secs(u64::from(minutes) * 60)
}
