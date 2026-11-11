//! What a Crew worker may do with Orbyn's own tools: an allow list, so a tool added later is
//! refused by default. Depth is 1 (H3): a worker makes no thread and no schedule. It reads no
//! other thread, queue or pending request, since the lead's messages and a pending `crew_start`
//! may quote the answer key (H1). It changes no setting, permission mode or project (N6).

use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::tools::mcp::McpBinding;
use serde_json::Value;

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

/// A worker also talks to its crew through the bus tools (`crew_post`, `crew_ask`,
/// `crew_board_read`, `crew_board_write`) and reports through `crew_report`. It never calls
/// `crew_start` (H3) or `crew_route`, so those are not here.
pub(crate) fn allowed(tool: &str) -> bool {
    ALLOWED.contains(&tool)
        || tool.starts_with("orbyn_preview_")
        || super::BUS_TOOLS.contains(&tool)
}

impl Core {
    /// Why `tool` is refused to the caller, if it is: for the tools outside `orbyn_*`, such as
    /// `computer_*`, which could drive the app window that shows the lead's thread.
    pub(crate) fn crew_worker_refusal(
        &self,
        binding: &McpBinding,
        tool: &str,
        args: &Value,
    ) -> Option<String> {
        self.refuse_for_crew_worker(binding, tool, args)
            .err()
            .map(|e| e.public_message())
    }

    /// Refuses `tool` when the caller's thread works a Crew task, whatever the task's state.
    pub(crate) fn refuse_for_crew_worker(
        &self,
        binding: &McpBinding,
        tool: &str,
        args: &Value,
    ) -> Result<()> {
        let own = &binding.scope.thread_id;
        if self.store.crew_task_for_worker(own)?.is_none() {
            return Ok(());
        }
        if !allowed(tool) {
            return Err(CoreError::Invalid(format!(
                "A crew worker cannot use {tool}."
            )));
        }
        // An attachment tool takes any thread of the project; a worker's is its own.
        if tool.starts_with("orbyn_attachment_")
            && args.get("thread_id").and_then(Value::as_str) != Some(own.as_str())
        {
            return Err(CoreError::Invalid(format!(
                "A crew worker can use {tool} only on its own thread."
            )));
        }
        Ok(())
    }
}
