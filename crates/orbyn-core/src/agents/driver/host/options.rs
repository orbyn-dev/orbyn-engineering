//! What a run asks the host for, in each family's own shape (the protocol keeps `options`
//! family-shaped on purpose, so one SDK's vocabulary never leaks into another's).

use std::path::Path;

use serde_json::{Map, Value, json};

use super::acp::HostAcpAgent;
use crate::agents::driver::{MCP_SERVER_NAME, RunSpec};
use crate::error::Result;

/// Claude: the Agent SDK's `query()` options under `sdk`, and the person's own `claude` path.
/// The permission mode is never one that skips asking: every tool the SDK is unsure about
/// reaches the core as a `permission` frame.
pub fn claude(spec: &RunSpec, claude_path: Option<&Path>) -> Value {
    let mut sdk = Map::new();
    sdk.insert(
        "permissionMode".into(),
        json!(if spec.plan { "plan" } else { "default" }),
    );
    // The CLI driver loads every settings source, so the checkout's CLAUDE.md and skills work.
    sdk.insert("settingSources".into(), json!(["user", "project", "local"]));
    if let Some(model) = &spec.model {
        sdk.insert("model".into(), json!(model));
    }
    if let Some(effort) = &spec.effort {
        sdk.insert("effort".into(), json!(effort));
    }
    if let Some(url) = &spec.mcp_url {
        sdk.insert(
            "mcpServers".into(),
            json!({ (MCP_SERVER_NAME): { "type": "http", "url": url } }),
        );
    }
    if !spec.context.trim().is_empty() {
        sdk.insert(
            "systemPrompt".into(),
            json!({ "type": "preset", "preset": "claude_code", "append": spec.context }),
        );
    }
    // The checkout is the start folder; the library comes in for its skills, as `--add-dir` does.
    let extra: Vec<&Path> = [&spec.checkout, &spec.library]
        .into_iter()
        .map(std::path::PathBuf::as_path)
        .filter(|dir| *dir != spec.cwd)
        .collect();
    let mut extra_dirs: Vec<&Path> = Vec::new();
    for dir in extra {
        if !extra_dirs.contains(&dir) {
            extra_dirs.push(dir);
        }
    }
    if !extra_dirs.is_empty() {
        sdk.insert("additionalDirectories".into(), json!(extra_dirs));
    }
    let mut options = Map::new();
    options.insert("sdk".into(), Value::Object(sdk));
    if let Some(path) = claude_path {
        options.insert("claudePath".into(), json!(path));
    }
    Value::Object(options)
}

/// ACP: the agent the core resolved, the turn's model, effort and mode, and the run's MCP
/// servers. Policy stays in Rust: no autopilot value is ever sent, and the host's own
/// session rules decide the rest.
pub fn acp(spec: &RunSpec, agent: &HostAcpAgent) -> Result<Value> {
    let mut options = Map::new();
    options.insert("agent".into(), agent.json()?);
    if let Some(model) = &spec.model {
        options.insert("model".into(), json!(model));
    }
    if let Some(effort) = &spec.effort {
        options.insert("effort".into(), json!(effort));
    }
    if let Some(mode) = spec.options.get(crate::agents::selection::MODE) {
        options.insert("mode".into(), json!(mode));
    }
    options.insert(
        "mcpServers".into(),
        super::acp::host_mcp_servers(spec.mcp_url.as_deref(), &spec.run_dir),
    );
    Ok(Value::Object(options))
}

#[cfg(test)]
#[path = "options_tests.rs"]
mod tests;
