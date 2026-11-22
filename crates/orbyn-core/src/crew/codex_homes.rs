//! The private Codex home a crew task's worker uses lives only while the task does.
//!
//! The home is made before the worker's first turn and reused by its later turns, so the
//! task's own Codex session continues. When the task ends (passed, stopped, cancelled) the
//! home is removed; at core start every home whose task is no longer live is swept, because a
//! process that died between the two leaves one behind.

use crate::app::Core;

impl Core {
    /// Removes one task's private Codex home when the task ends. A task that never ran on
    /// Codex, or whose home was already swept, has nothing here.
    pub(crate) fn remove_crew_codex_home(&self, task: &str) {
        if let Err(error) = crate::host::confine::remove_codex_home(&self.config.data_dir, task)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(%error, task, "could not remove a Codex worker home");
        }
    }

    /// At core start: keeps the home of every task that can still run, removes the rest.
    /// A store error keeps the home rather than deleting a live worker's session.
    pub(crate) fn sweep_crew_codex_homes(&self) {
        let result = crate::host::confine::sweep_codex_homes(&self.config.data_dir, |task| {
            match self.store.crew_task(task) {
                Ok(Some(task)) => !crate::crew::lifecycle::is_final(&task.state),
                Ok(None) => false,
                Err(_) => true,
            }
        });
        if let Err(error) = result {
            tracing::warn!(%error, "could not sweep stale Codex worker homes");
        }
    }
}
