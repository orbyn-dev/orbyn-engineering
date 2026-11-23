//! What a person reads for a finished Cursor tool call. The SDK's tool union has one result
//! shape per tool; this module extracts the readable text from each and never renders the
//! raw JSON. Every string is bounded by the caller.

use serde_json::Value;

use super::{MAX_RESULT_CHARS, support::capped};

/// A plain result a person can read, never the SDK's JSON. Long output is capped.
pub(super) fn text(call: &Value) -> String {
    let Some(result) = call.get("result") else {
        return String::new();
    };
    let status = result.get("status").and_then(Value::as_str);
    let value = result.get("value").unwrap_or(&Value::Null);
    let shown = match status {
        Some("success") => match call.get("type").and_then(Value::as_str) {
            Some("read") => value
                .get("content")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_default(),
            Some("glob") => glob_text(value),
            Some("grep") => grep_text(value),
            Some("task") => task_text(value),
            Some("shell") => shell_text(value),
            Some("write") => write_text(call, value),
            Some("edit") => edit_text(call, value),
            Some("delete") => delete_text(call),
            Some("ls") => ls_text(value),
            Some("readLints") => lints_text(value),
            Some("mcp") => mcp_text(value),
            Some("semSearch") => value
                .get("results")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            Some("generateImage") => value
                .get("filePath")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            Some("computerUse") => computer_text(value),
            _ => plain_value(value),
        },
        Some("error") => error_text(result.get("error").unwrap_or(&Value::Null)),
        // A completion with no status is still a completion; show whatever value it has.
        _ => plain_value(value),
    };
    capped(&shown, MAX_RESULT_CHARS)
}

fn join_lines(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn glob_text(value: &Value) -> String {
    let files = join_lines(value.get("files"));
    if files.is_empty()
        && value
            .get("totalFiles")
            .and_then(Value::as_u64)
            .is_some_and(|total| total == 0)
    {
        return "No files".into();
    }
    files
}

/// Grep's success value is a union of content, files and count outputs, either under
/// `workspaceResults` or as one `activeEditorResult`. Content matches are flat rows with a
/// `line`, not a nested file tree.
fn grep_text(value: &Value) -> String {
    let mut lines = Vec::new();
    if let Some(workspaces) = value.get("workspaceResults").and_then(Value::as_object) {
        for result in workspaces.values() {
            grep_output(result, &mut lines);
        }
    }
    if let Some(result) = value.get("activeEditorResult") {
        grep_output(result, &mut lines);
    }
    if lines.is_empty() {
        return plain_value(value);
    }
    lines.join("\n")
}

fn grep_output(result: &Value, lines: &mut Vec<String>) {
    let Some(output) = result.get("output") else {
        return;
    };
    match result.get("type").and_then(Value::as_str) {
        Some("content") => {
            let matches = output
                .get("matches")
                .and_then(Value::as_array)
                .map_or(&[][..], Vec::as_slice);
            if matches.is_empty() {
                lines.push("No matches".into());
                return;
            }
            for row in matches {
                let path = row.get("file").and_then(Value::as_str).unwrap_or_default();
                let content = row.get("line").and_then(Value::as_str).unwrap_or_default();
                lines.push(match row.get("lineNumber").and_then(Value::as_u64) {
                    Some(line) => format!("{path}:{line}: {content}"),
                    None => format!("{path}: {content}"),
                });
            }
        }
        Some("files") => {
            let files = join_lines(output.get("files"));
            lines.push(if files.is_empty() {
                "No files".into()
            } else {
                files
            });
        }
        Some("count") => {
            let counts = output
                .get("counts")
                .and_then(Value::as_array)
                .map_or(&[][..], Vec::as_slice);
            for row in counts {
                let path = row.get("file").and_then(Value::as_str).unwrap_or_default();
                let count = row.get("count").and_then(Value::as_u64).unwrap_or(0);
                lines.push(format!("{path}: {count}"));
            }
            if let Some(total) = output.get("total").and_then(Value::as_u64) {
                lines.push(format!("total: {total}"));
            }
        }
        _ => {
            let text = plain_value(output);
            if !text.is_empty() {
                lines.push(text);
            }
        }
    }
}

/// A subagent finishes with its last assistant message, or the SDK's result suffix.
fn task_text(value: &Value) -> String {
    let mut parts = Vec::new();
    if let Some(text) = value
        .get("conversationSteps")
        .and_then(Value::as_array)
        .and_then(|steps| {
            steps.iter().rev().find_map(|step| {
                step.pointer("/assistantMessage/text")
                    .and_then(Value::as_str)
            })
        })
    {
        parts.push(text.to_owned());
    }
    if let Some(suffix) = value
        .get("resultSuffix")
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
    {
        parts.push(suffix.to_owned());
    }
    parts.join("\n")
}

/// Stdout and stderr, both of them: a failed command's reason is usually the stderr a
/// stdout-only rendering would hide.
fn shell_text(value: &Value) -> String {
    let stdout = value
        .get("stdout")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let stderr = value
        .get("stderr")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match (stdout.is_empty(), stderr.is_empty()) {
        (false, false) => format!("{stdout}\n[stderr]\n{stderr}"),
        (false, true) => stdout.to_owned(),
        (true, false) => stderr.to_owned(),
        (true, true) => value
            .get("output")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| plain_value(value)),
    }
}

fn write_text(call: &Value, value: &Value) -> String {
    let path = value
        .get("path")
        .and_then(Value::as_str)
        .or_else(|| call.pointer("/args/path").and_then(Value::as_str));
    match (path, value.get("linesCreated").and_then(Value::as_u64)) {
        (Some(path), Some(lines)) => format!("Wrote {path} ({lines} lines)"),
        (Some(path), None) => format!("Wrote {path}"),
        (None, _) => plain_value(value),
    }
}

/// An edit's result is its diff string; without one, say how many lines changed.
fn edit_text(call: &Value, value: &Value) -> String {
    if let Some(diff) = value
        .get("diffString")
        .and_then(Value::as_str)
        .filter(|diff| !diff.is_empty())
    {
        return diff.to_owned();
    }
    let path = call
        .pointer("/args/path")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let added = value.get("linesAdded").and_then(Value::as_u64).unwrap_or(0);
    let removed = value
        .get("linesRemoved")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if path.is_empty() {
        return plain_value(value);
    }
    format!("Edited {path} (+{added}/-{removed})")
}

/// A delete's success value only holds a size; the path is in the args.
fn delete_text(call: &Value) -> String {
    match call
        .pointer("/args/path")
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
    {
        Some(path) => format!("Deleted {path}"),
        None => String::new(),
    }
}

/// A directory tree as indented names, bounded so a huge listing cannot fill the timeline.
fn ls_text(value: &Value) -> String {
    let Some(root) = value.get("directoryTreeRoot") else {
        return plain_value(value);
    };
    let mut lines = Vec::new();
    if let Some(path) = root.get("absPath").and_then(Value::as_str) {
        lines.push(path.to_owned());
    }
    ls_node(root, 1, &mut lines);
    lines.join("\n")
}

fn ls_node(node: &Value, depth: usize, lines: &mut Vec<String>) {
    if lines.len() >= 200 {
        return;
    }
    let indent = "  ".repeat(depth);
    if let Some(dirs) = node.get("childrenDirs").and_then(Value::as_array) {
        for dir in dirs {
            if lines.len() >= 200 {
                return;
            }
            if let Some(name) = dir.get("absPath").and_then(Value::as_str) {
                lines.push(format!("{indent}{name}"));
            }
            ls_node(dir, depth + 1, lines);
        }
    }
    if let Some(files) = node.get("childrenFiles").and_then(Value::as_array) {
        for file in files {
            if lines.len() >= 200 {
                return;
            }
            if let Some(name) = file.get("name").and_then(Value::as_str) {
                lines.push(format!("{indent}{name}"));
            }
        }
    }
}

/// Diagnostics grouped by file, one line each. A clean run says so.
fn lints_text(value: &Value) -> String {
    let Some(files) = value.get("fileDiagnostics").and_then(Value::as_array) else {
        return plain_value(value);
    };
    let mut lines = Vec::new();
    for file in files {
        let path = file.get("path").and_then(Value::as_str).unwrap_or_default();
        let diagnostics = file
            .get("diagnostics")
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice);
        for diagnostic in diagnostics {
            let severity = diagnostic
                .get("severity")
                .and_then(Value::as_str)
                .unwrap_or("diagnostic");
            let message = diagnostic
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let source = diagnostic.get("source").and_then(Value::as_str);
            let code = diagnostic.get("code").and_then(Value::as_str);
            let at = diagnostic
                .pointer("/range/start/line")
                .and_then(Value::as_u64)
                .map(|line| format!(":{line}"))
                .unwrap_or_default();
            let origin = match (source, code) {
                (Some(source), Some(code)) => format!(" [{source} {code}]"),
                (Some(source), None) => format!(" [{source}]"),
                _ => String::new(),
            };
            lines.push(format!("{path}{at}: {severity}: {message}{origin}"));
        }
    }
    if lines.is_empty() {
        return "No diagnostics".into();
    }
    lines.join("\n")
}

/// MCP content is text blocks and images; an image is named, not decoded.
fn mcp_text(value: &Value) -> String {
    let Some(items) = value.get("content").and_then(Value::as_array) else {
        return plain_value(value);
    };
    let mut lines = Vec::new();
    for item in items {
        if let Some(text) = item
            .pointer("/text/text")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
        {
            lines.push(text.to_owned());
        } else if let Some(mime) = item
            .pointer("/image/mimeType")
            .and_then(Value::as_str)
            .filter(|mime| !mime.is_empty())
        {
            lines.push(format!("[image {mime}]"));
        }
    }
    if lines.is_empty() && value.get("isError").and_then(Value::as_bool) == Some(true) {
        return "The MCP server reported an error.".into();
    }
    lines.join("\n")
}

fn computer_text(value: &Value) -> String {
    if let Some(log) = value
        .get("log")
        .and_then(Value::as_str)
        .filter(|log| !log.is_empty())
    {
        return log.to_owned();
    }
    let actions = value
        .get("actionCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let duration = value.get("durationMs").and_then(Value::as_u64).unwrap_or(0);
    let mut text = format!("{actions} actions in {duration} ms");
    if let Some(path) = value.get("screenshotPath").and_then(Value::as_str) {
        text.push_str(&format!("\nScreenshot: {path}"));
    }
    text
}

fn error_text(value: &Value) -> String {
    if let Value::String(text) = value {
        return text.clone();
    }
    for path in ["/message", "/error", "/permissionDenied/error"] {
        if let Some(text) = value.pointer(path).and_then(Value::as_str) {
            return text.to_owned();
        }
    }
    plain_value(value)
}

/// The text inside a structured value, never the structure itself.
fn plain_value(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(plain_value)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(map) => {
            for key in ["content", "output", "text", "message", "error"] {
                if let Some(text) = map
                    .get(key)
                    .map(plain_value)
                    .filter(|text| !text.is_empty())
                {
                    return text;
                }
            }
            String::new()
        }
        _ => String::new(),
    }
}
