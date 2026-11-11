//! After a restart. A task's verification and its budget timer live in memory, so a task left
//! `running` or `verifying` is picked up when the core starts: a verifying task is verified
//! now, a running one whose worker has no run and nothing queued stops with `engine_error`,
//! and every budget timer is set again with what is left of its budget.

use std::sync::Arc;
use std::time::Duration;

use super::verify::budget_left;
use crate::app::Core;

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
        // No verification is alive at a start, so a claim found now is from one that died.
        let _ = self.store.clear_crew_claims();
        let Ok(tasks) = self.store.crew_tasks_in_flight() else {
            return;
        };
        for (task, run) in tasks {
            if let Ok(Some(brief)) = self.store.crew_brief(&task.id) {
                let left = budget_left(
                    &task.created_at,
                    u64::from(brief.budget.minutes),
                    crate::util::unix_now(),
                );
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
            } else {
                self.end_for_run(&task.id, orbyn_contracts::RunState::Failed);
            }
        }
    }
}

/// How long the timer for a task waits.
pub(super) fn timer(minutes: u32) -> Duration {
    Duration::from_secs(u64::from(minutes) * 60)
}
