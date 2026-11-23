//! What a Crew worker may do with Orbyn's own tools: an allow list, so a tool added later is
//! refused by default. Depth is 1 (H3): a worker makes no thread and no schedule. It reads no
//! other thread, queue or pending request, since the lead's messages and a pending `crew_start`
//! may quote the answer key (H1). It changes no setting, permission mode or project (N6).

use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::tools::mcp::McpBinding;
use serde_json::Value;

/// The only `orbyn_*` tools a worker may call: its preview tabs, the worktree and environment
/// it works in, its own attachments, and the three Code Mode tools.
pub(crate) const ALLOWED: [&str; 10] = [
    "orbyn_worktree_status",
    "orbyn_worktree_list",
    "orbyn_environment_read",
    "orbyn_attachment_add",
    "orbyn_attachment_discard",
    // Every `orbyn_preview_*` tool is allowed too; see `allowed`.
    "orbyn_preview_list",
    "orbyn_preview_status",
    // One entry each, never a prefix rule: a worker may search, describe and run a Code Mode
    // script. Every inner call a script makes is re-checked per call by `refuse_for_crew_worker`
    // inside `call_orbyn_tool`, so adding these three names widens nothing a script can reach.
    crate::tools::mcp::code::SEARCH_TOOL,
    crate::tools::mcp::code::DESCRIBE_TOOL,
    crate::tools::mcp::code::RUN_TOOL,
];

/// A worker also talks to its crew through the bus tools (`crew_post`, `crew_ask`,
/// `crew_board_read`, `crew_board_write`), reports through `crew_report` and, for a reviewer, files findings
/// through `crew_finding` (the tool itself refuses anyone who is not a reviewer). It never calls
/// `crew_start` (H3) or `crew_route`, so those are not here.
pub(crate) fn allowed(tool: &str) -> bool {
    ALLOWED.contains(&tool)
        || tool.starts_with("orbyn_preview_")
        || super::BUS_TOOLS.contains(&tool)
        || tool == super::FINDING_TOOL
}

/// The allow-list rule itself, without a store lookup: `None` when a Crew worker may call `tool`.
/// `refuse_for_crew_worker` is the store-aware wrapper around this one rule.
pub(crate) fn worker_refusal(tool: &str, args: &Value, own: &str) -> Option<String> {
    if !allowed(tool) {
        return Some(format!("A crew worker cannot use {tool}."));
    }
    // An attachment tool takes any thread of the project; a worker's is its own.
    if tool.starts_with("orbyn_attachment_")
        && args.get("thread_id").and_then(Value::as_str) != Some(own)
    {
        return Some(format!(
            "A crew worker can use {tool} only on its own thread."
        ));
    }
    None
}

impl Core {
    /// Whether `binding`'s thread works a Crew task, looked up once.
    pub(crate) fn is_crew_worker(&self, binding: &McpBinding) -> Result<bool> {
        Ok(self
            .store
            .crew_task_for_worker(&binding.scope.thread_id)?
            .is_some())
    }

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
        if !self.is_crew_worker(binding)? {
            return Ok(());
        }
        match worker_refusal(tool, args, &binding.scope.thread_id) {
            Some(message) => Err(CoreError::Invalid(message)),
            None => Ok(()),
        }
    }
}
