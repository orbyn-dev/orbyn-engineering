//! What a Crew worker may do with Orbyn's own tools: an allow list, so a tool added later is
//! refused by default. Depth is 1 (H3): a worker makes no thread and no schedule. It reads no
//! other thread, queue or pending request, since the lead's messages and a pending `crew_start`
//! may quote the answer key (H1). It changes no setting, permission mode or project (N6).

use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::tools::mcp::McpBinding;

/// The only `orbyn_*` tools a worker may call: its preview tabs, the worktree and environment
/// it works in, and its own attachments.
pub(crate) const ALLOWED: [&str; 7] = [
    "orbyn_worktree_status",
    "orbyn_worktree_list",
    "orbyn_environment_read",
    "orbyn_attachment_add",
    "orbyn_attachment_discard",
    // Every `orbyn_preview_*` tool is allowed too; see `allowed`.
    "orbyn_preview_list",
    "orbyn_preview_status",
];

pub(crate) fn allowed(tool: &str) -> bool {
    ALLOWED.contains(&tool) || tool.starts_with("orbyn_preview_")
}

impl Core {
    /// Refuses `tool` when the caller's thread works a Crew task, whatever the task's state.
    pub(crate) fn refuse_for_crew_worker(&self, binding: &McpBinding, tool: &str) -> Result<()> {
        if !allowed(tool)
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
