//! The one door out of a script: a tool call goes back through the core, as an engine's own call would.

use super::runs::{Pending, PendingList};
use crate::app::Core;
use crate::error::CoreError;
use crate::tools::mcp::McpBinding;
use async_trait::async_trait;
use orbyn_code_sandbox::{ToolFailure, ToolHost};
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, PoisonError, Weak};

pub(super) struct RunHost {
    pub(super) core: Weak<Core>,
    pub(super) binding: McpBinding,
    pub(super) pending: PendingList,
    next: AtomicUsize,
    /// Calls with an effect run one at a time, in the order the script made them, so a script
    /// cannot ask for many approvals at once.
    effects: tokio::sync::Mutex<()>,
}

impl RunHost {
    pub(super) fn new(core: &Arc<Core>, binding: &McpBinding, pending: PendingList) -> Self {
        Self {
            core: Arc::downgrade(core),
            binding: binding.clone(),
            pending,
            next: AtomicUsize::new(1),
            effects: tokio::sync::Mutex::new(()),
        }
    }
}

/// A tool a script may name: an Orbyn tool, never Code Mode itself.
pub(super) fn is_inner_tool(name: &str) -> bool {
    !name.starts_with("orbyn_code_") && crate::tools::mcp_tools::is_tool(name)
}

fn failure(code: &str, message: impl Into<String>) -> ToolFailure {
    ToolFailure {
        code: code.into(),
        message: message.into(),
    }
}

fn code_of(error: &CoreError) -> &'static str {
    match error {
        CoreError::PolicyDenied(_) => "denied",
        CoreError::NotFound(_) => "not_found",
        CoreError::Invalid(_) => "invalid",
        _ => "failed",
    }
}

struct InFlight<'a>(&'a PendingList, usize);

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|p| p.call != self.1);
    }
}

#[async_trait]
impl ToolHost for RunHost {
    async fn call(&self, name: &str, args: Value) -> Result<Value, ToolFailure> {
        if !is_inner_tool(name) {
            return Err(failure(
                "invalid",
                format!("{name} is not available to scripts."),
            ));
        }
        let Some(core) = self.core.upgrade() else {
            return Err(failure("failed", "Orbyn is shutting down."));
        };
        let call = self.next.fetch_add(1, Ordering::SeqCst);
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Pending {
                tool: name.to_owned(),
                call,
            });
        let _in_flight = InFlight(&self.pending, call);
        let _one_effect = if crate::tools::mcp_tools::has_effect(name) {
            Some(self.effects.lock().await)
        } else {
            None
        };
        if let Err(error) = core.note_tool(&self.binding, name) {
            tracing::warn!(%error, "could not record a tool call");
        }
        let budget = super::super::budget_for(name).unwrap_or(super::super::TOOL_BUDGET);
        match tokio::time::timeout(budget, core.call_orbyn_tool(&self.binding, name, &args)).await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => Err(failure(code_of(&error), error.public_message())),
            Err(_) => Err(failure("failed", "This tool did not finish in time.")),
        }
    }
}
