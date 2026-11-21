//! The one door out of a script: a tool call goes back through the core, as an engine's own call would.

use super::runs::{Pending, PendingList};
use crate::app::Core;
use crate::error::CoreError;
use crate::store::code_runs::StoredCodeCall;
use crate::tools::mcp::McpBinding;
use async_trait::async_trait;
use orbyn_code_sandbox::{ToolFailure, ToolHost};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::Instant;

/// Set once a replay cannot go on; the run then ends as `lost` with this text.
pub(super) type ReplayFailure = Arc<Mutex<Option<String>>>;

pub(super) const NOT_REPEATABLE: &str =
    "The script changed or is not repeatable, so the recorded calls can't be replayed.";
pub(super) const IN_FLIGHT: &str = "A call that may have changed something was running when Orbyn restarted, so the run can't continue.";

pub(super) struct RunHost {
    pub(super) core: Weak<Core>,
    pub(super) binding: McpBinding,
    pub(super) pending: PendingList,
    next: AtomicUsize,
    run_id: String,
    script_sha256: String,
    /// The calls a lost run recorded, by index, when this run is its replay.
    replay: HashMap<u32, StoredCodeCall>,
    failure: ReplayFailure,
    /// Calls with an effect run one at a time, in the order the script made them, so a script
    /// cannot ask for many approvals at once.
    effects: tokio::sync::Mutex<()>,
}

impl RunHost {
    pub(super) fn new(
        core: &Arc<Core>,
        binding: &McpBinding,
        pending: PendingList,
        run_id: &str,
        script_sha256: &str,
        replay: HashMap<u32, StoredCodeCall>,
        failure: ReplayFailure,
    ) -> Self {
        Self {
            core: Arc::downgrade(core),
            binding: binding.clone(),
            pending,
            next: AtomicUsize::new(1),
            run_id: run_id.to_owned(),
            script_sha256: script_sha256.to_owned(),
            replay,
            failure,
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

/// A call's answer as the record keeps it: a value, or `{code, message}` for a failure.
fn failure_text(failure: &ToolFailure) -> String {
    json!({ "code": failure.code, "message": failure.message }).to_string()
}

enum Replayed {
    Answer(Result<Value, ToolFailure>),
    RunAgain,
}

impl RunHost {
    fn give_up(&self, message: &str) -> Replayed {
        *self.failure.lock().unwrap_or_else(PoisonError::into_inner) = Some(message.to_owned());
        Replayed::Answer(Err(failure("lost", message)))
    }

    /// What a recorded call answers again, without running anything. A call that does not match, or has
    /// no stored answer, ends the replay.
    fn replayed(&self, recorded: &StoredCodeCall, name: &str, digest: &str) -> Replayed {
        if recorded.tool != name || recorded.args_digest != digest {
            return self.give_up(NOT_REPEATABLE);
        }
        match (recorded.status.as_str(), recorded.result.as_deref()) {
            ("ok", Some(text)) => match serde_json::from_str(text) {
                Ok(value) => Replayed::Answer(Ok(value)),
                Err(_) => self.give_up(NOT_REPEATABLE),
            },
            ("failed" | "denied", Some(text)) => match serde_json::from_str::<Value>(text) {
                Ok(value) => Replayed::Answer(Err(failure(
                    value["code"].as_str().unwrap_or("failed"),
                    value["message"].as_str().unwrap_or("This tool failed."),
                ))),
                Err(_) => self.give_up(NOT_REPEATABLE),
            },
            ("running", _) if recorded.effect => self.give_up(IN_FLIGHT),
            ("running", _) => Replayed::RunAgain,
            _ => self.give_up(NOT_REPEATABLE),
        }
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
        let index = u32::try_from(call).unwrap_or(u32::MAX);
        if let Some(message) = self
            .failure
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
        {
            return Err(failure("lost", message));
        }
        let digest = crate::util::sha256_hex(args.to_string().as_bytes());
        if let Some(recorded) = self.replay.get(&index)
            && let Replayed::Answer(answer) = self.replayed(recorded, name, &digest)
        {
            return answer;
        }
        let effect = crate::tools::mcp_tools::has_effect(name);
        if let Err(error) = core
            .store
            .code_call_begin(&self.run_id, index, name, &digest, effect)
        {
            tracing::warn!(%error, "could not record a script call");
        }
        let began = Instant::now();
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Pending {
                tool: name.to_owned(),
                call,
            });
        let _in_flight = InFlight(&self.pending, call);
        let _one_effect = if effect {
            Some(self.effects.lock().await)
        } else {
            None
        };
        if let Err(error) = core.note_tool(&self.binding, name) {
            tracing::warn!(%error, "could not record a tool call");
        }
        let budget = super::super::budget_for(name).unwrap_or(super::super::TOOL_BUDGET);
        let via = orbyn_contracts::ApprovalVia {
            code_run: self.run_id.clone(),
            call: index,
            script_sha256: self.script_sha256.clone(),
        };
        let outcome = match tokio::time::timeout(
            budget,
            crate::governance::approvals::with_via(
                via,
                core.call_orbyn_tool(&self.binding, name, &args),
            ),
        )
        .await
        {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => Err(failure(code_of(&error), error.public_message())),
            Err(_) => Err(failure("failed", "This tool did not finish in time.")),
        };
        let (status, text) = match &outcome {
            Ok(value) => ("ok", value.to_string()),
            Err(error) if error.code == "denied" => ("denied", failure_text(error)),
            Err(error) => ("failed", failure_text(error)),
        };
        let took = u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX);
        if let Err(error) = core
            .store
            .code_call_end(&self.run_id, index, status, &text, took)
        {
            tracing::warn!(%error, "could not record a script call");
        }
        outcome
    }
}
