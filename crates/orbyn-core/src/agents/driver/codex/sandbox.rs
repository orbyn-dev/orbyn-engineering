use std::ffi::OsString;

use orbyn_contracts::PermissionMode;
use serde_json::{Value, json};

use crate::agents::console::ConsoleSpec;
use crate::agents::driver::{MCP_SERVER_NAME, RunSpec};
use crate::agents::selection::{SERVICE_TIER, STANDARD_TIER};
use crate::error::{CoreError, Result};

/// What to send for the tier. `None` leaves it out, which keeps what the thread has,
/// so that happens only when no default could be read. Standard is a clear.
pub(super) fn tier_param(spec: &RunSpec) -> Option<Value> {
    let tier = spec.options.get(SERVICE_TIER)?;
    Some(if tier == STANDARD_TIER {
        Value::Null
    } else {
        json!(tier)
    })
}

/// Collaboration mode controls native planning/question behavior independently
/// of the sandbox. State it on ordinary turns too, to leave a resumed plan.
pub(super) fn collaboration_mode(spec: &RunSpec, thread: &Value) -> Result<Value> {
    let model = spec
        .model
        .as_deref()
        .filter(|model| !model.is_empty())
        .or_else(|| {
            thread
                .get("model")
                .and_then(Value::as_str)
                .filter(|model| !model.is_empty())
        })
        .ok_or_else(|| {
            CoreError::EngineUnavailable(
                "Codex returned no effective model for collaboration mode.".into(),
            )
        })?;
    let effort = spec
        .effort
        .as_deref()
        .or_else(|| thread.get("reasoningEffort").and_then(Value::as_str));
    Ok(json!({
        "mode": if spec.plan { "plan" } else { "default" },
        "settings": {
            "model": model,
            "reasoning_effort": effort,
            // Use Codex's built-in mode instructions. The thread's separate
            // developerInstructions still carry Orbyn's run context.
            "developer_instructions": null,
        },
    }))
}

/// The thread and each turn run in the selected checkout, the only writable root,
/// so the library stays read-only. Skills reach Codex through the run's MCP tools.
///
/// Full access stops asking and opens the network, but the writable root stays:
/// `dangerFullAccess` would make the library and other projects writable.
pub(super) fn sandbox_policy(
    sandbox: &str,
    checkout: &std::path::Path,
    mode: PermissionMode,
) -> Value {
    match sandbox {
        "read-only" => json!({ "type": "readOnly" }),
        "danger-full-access" => json!({ "type": "dangerFullAccess" }),
        _ => json!({
            "type": "workspaceWrite",
            "writableRoots": [checkout],
            "networkAccess": mode == PermissionMode::FullAccess,
        }),
    }
}

/// Codex's interactive command line on a run's session, in the thread's checkout.
/// Plan mode stays read-only, and Codex asks before anything else, as it would
/// for a person: Orbyn's approval gate does not reach this console.
pub fn console_args(console: &ConsoleSpec<'_>, sandbox: &str) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec!["resume".into(), console.session.into()];
    if let Some(model) = console.model {
        args.extend(["--model".into(), model.into()]);
    }
    args.extend([
        "--sandbox".into(),
        if console.plan { "read-only" } else { sandbox }.into(),
        "--ask-for-approval".into(),
        "on-request".into(),
        "--cd".into(),
        console.checkout.into(),
    ]);
    args
}

/// Codex has no edits-only policy, so the lesser modes keep asking and the gate
/// answers for them. That also keeps its deny rules in force.
pub(super) fn approval_policy(mode: PermissionMode) -> &'static str {
    if mode == PermissionMode::FullAccess {
        "never"
    } else {
        "on-request"
    }
}

/// How long Codex waits for a user's optional MCP servers to finish starting before
/// the first turn, so a slow connector is ready when the model first looks for tools.
/// It is a ceiling, not a delay: ready servers cost nothing. Each run starts a new
/// app-server, so a server that never comes up costs every first turn the whole wait.
/// Measured with a hung server on 0.166.0 (first token): none 3.6 s, 3 s grace 4.7 s,
/// 10 s grace 11.8 s.
const MCP_STARTUP_GRACE_MS: u64 = 3_000;

/// A line for the model: connectors Codex loads for the user are not in the top-level
/// tool list but inside its code-mode `exec` tool.
pub(super) const CONNECTOR_NOTE: &str = "With an exec tool, the person's connectors and MCP servers (GitHub, Gmail, Calendar) are mcp__* functions inside it.";

/// The per-run MCP server. Orbyn's tools carry their own policy gate, so Codex
/// does not ask a second time before calling them. The dotted key sets only this one
/// entry, so a user server of the same table keeps every field it has; a nested
/// `mcp_servers` object would replace the table.
pub(super) fn thread_config(spec: &RunSpec) -> Value {
    let mut config = json!({ "mcp_optional_startup_grace_ms": MCP_STARTUP_GRACE_MS });
    if let Some(url) = &spec.mcp_url {
        config[format!("mcp_servers.{MCP_SERVER_NAME}")] =
            json!({ "url": url, "default_tools_approval_mode": "approve" });
    }
    config
}
