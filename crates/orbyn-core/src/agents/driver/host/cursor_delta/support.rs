//! Small pure helpers the Cursor mapper shares: bounds, path containment, tool identity and
//! outcome, usage totals, and Cursor's todo steps. None of them holds mapper state.

use std::path::{Component, Path, PathBuf};

use serde_json::{Value, json};

/// The marker a capped string ends with, so a truncated result never looks complete.
pub(super) fn capped(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push_str(&format!("\n\n[truncated: {count} characters]"));
    out
}

/// True when `path` resolves lexically to a place inside `cwd`. A relative path is taken
/// from `cwd`, as the engine's tool means it; `..` cannot climb out.
pub(super) fn inside(cwd: &Path, path: &str) -> bool {
    let path = Path::new(path);
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    let mut normalized = PathBuf::new();
    for part in joined.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized.starts_with(cwd)
}

pub(super) fn field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// A tool update with a usable call id and a tool object. A missing or empty call id, or a
/// missing `toolCall`, is not guessed at: the event is ignored deterministically because
/// there is no id to match a start to an end or a result.
pub(super) fn tool_update(update: &Value) -> Option<(String, &Value)> {
    let call_id = field(update, "callId").filter(|id| !id.is_empty())?;
    let call = update.get("toolCall").filter(|call| call.is_object())?;
    Some((call_id, call))
}

/// The outcome a tool call reports, and its shell exit code when it has one. A non-zero
/// exit code or an MCP `isError` is a failure even when the SDK status says success.
pub(super) fn tool_outcome(call: &Value) -> Option<(&'static str, Option<i64>)> {
    let result = call.get("result")?;
    let status = result.get("status").and_then(Value::as_str);
    let exit_code = result.pointer("/value/exitCode").and_then(Value::as_i64);
    let mcp_error = result
        .pointer("/value/isError")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let success = status == Some("success") && exit_code.unwrap_or(0) == 0 && !mcp_error;
    Some((if success { "success" } else { "failure" }, exit_code))
}

/// Input + output only. The SDK counts cache reads and writes inside `inputTokens`, so
/// adding those fields again would double the prompt.
pub(super) fn usage_tokens(usage: &Value) -> u64 {
    let count = |key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0);
    count("inputTokens").saturating_add(count("outputTokens"))
}

/// The plan activity's steps from Cursor's `updateTodos` input. Cancelled items are not
/// steps; an empty list is a real empty plan.
pub(super) fn todo_steps(todos: &[Value]) -> Vec<Value> {
    todos
        .iter()
        .filter_map(|todo| {
            let title = todo.get("content").and_then(Value::as_str).map(str::trim)?;
            if title.is_empty() {
                return None;
            }
            let status = match todo.get("status").and_then(Value::as_str) {
                Some("completed") => "completed",
                Some("in_progress" | "inProgress") => "running",
                Some("cancelled") => return None,
                _ => "pending",
            };
            Some(json!({ "title": title, "status": status }))
        })
        .collect()
}
