//! What a Crew worker may not do with Orbyn's own tools. Depth is 1 (H3): only the lead, the
//! core or a person starts work, so a worker is refused every tool that makes a thread or a
//! schedule, which would otherwise be an unmarked thread that counts as a lead. It is refused
//! the tools that read or write other threads too, since the lead's own messages may quote a
//! check (H1).

use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::tools::mcp::McpBinding;

const REFUSED: [&str; 14] = [
    "orbyn_thread_launch",
    "orbyn_threads_create",
    "orbyn_worktree_handoff",
    "orbyn_delegate_task",
    "orbyn_schedule_task",
    "orbyn_schedule_run",
    "orbyn_schedule_delete",
    "orbyn_project_create",
    "orbyn_project_delete",
    "orbyn_thread_send",
    "orbyn_thread_read",
    "orbyn_thread_search",
    "orbyn_thread_wait",
    "orbyn_thread_list",
];

impl Core {
    /// Refuses `tool` when the caller's thread works a Crew task, whatever the task's state.
    pub(crate) fn refuse_for_crew_worker(&self, binding: &McpBinding, tool: &str) -> Result<()> {
        if REFUSED.contains(&tool)
            && self
                .store
                .crew_task_for_worker(&binding.scope.thread_id)?
                .is_some()
        {
            return Err(CoreError::Invalid(format!(
                "A crew worker cannot use {tool}."
            )));
        }
        Ok(())
    }
}
