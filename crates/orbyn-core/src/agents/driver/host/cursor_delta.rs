//! Cursor's `InteractionUpdate` frames to timeline events. The host passes the SDK's own
//! update object through untouched, so this mapper owns every shape a Cursor run can make:
//! thinking and text streams, tool calls, Cursor's own plan and todo tools, subagent
//! activity, and the token counters that never appear as text.

use std::collections::BTreeMap;

use orbyn_contracts::{AgUiEvent, ThreadUsage};
use serde_json::{Value, json};

use crate::agents::driver::{EngineEvent, proposed_plan_events};
use crate::util::new_id;

/// The longest plain tool result kept for the timeline, in characters.
const MAX_RESULT_CHARS: usize = 16 * 1024;

/// Maps the Cursor SDK's updates. Text and reasoning are one message per streamed segment;
/// Cursor has no "block stopped" event, so a different kind, a tool call, the end of a step
/// or the end of the run closes the open one.
#[derive(Default)]
pub(crate) struct CursorMapper {
    text: Option<String>,
    reasoning: Option<String>,
    /// Open tool calls in start order, so a run that ends mid-tool closes them in order.
    tools: Vec<String>,
    /// The last proposed plan shown for a call, so a streamed plan appears once.
    plans: BTreeMap<String, String>,
    /// The last todo list shown for a call, so a streamed list replaces it in place.
    todos: BTreeMap<String, String>,
    /// Token deltas since the last turn-reported usage.
    tokens: u64,
}

impl CursorMapper {
    pub(crate) fn map(&mut self, update: &Value) -> Vec<EngineEvent> {
        let mut events = Vec::new();
        match field(update, "type").as_deref() {
            Some("text-delta") => {
                let delta = field(update, "text").unwrap_or_default();
                if delta.is_empty() {
                    return events;
                }
                self.close_reasoning(&mut events);
                let message_id = self.open_text(&mut events);
                events.push(AgUiEvent::TextMessageContent { message_id, delta }.into());
            }
            Some("thinking-delta") => {
                let delta = field(update, "text").unwrap_or_default();
                if delta.is_empty() {
                    return events;
                }
                self.close_text(&mut events);
                let message_id = self.open_reasoning(&mut events);
                events.push(AgUiEvent::ReasoningMessageContent { message_id, delta }.into());
            }
            Some("thinking-completed") => self.close_reasoning(&mut events),
            // Cursor reports tokens as a running count. They are never text; a turn-ended
            // report (or the end of an interrupted run) is what the timeline records.
            Some("token-delta") => {
                self.tokens = self
                    .tokens
                    .saturating_add(update.get("tokens").and_then(Value::as_u64).unwrap_or(0));
            }
            Some("partial-tool-call") => {
                self.close_turn(&mut events);
                if let (Some(call_id), Some(call)) =
                    (field(update, "callId"), update.get("toolCall"))
                {
                    self.plan_events(&call_id, call, &mut events);
                }
            }
            Some("tool-call-started") => {
                self.close_turn(&mut events);
                if let (Some(call_id), Some(call)) =
                    (field(update, "callId"), update.get("toolCall"))
                {
                    self.open_tool(&call_id, call, &mut events);
                    self.plan_events(&call_id, call, &mut events);
                }
            }
            Some("tool-call-completed") => {
                self.close_turn(&mut events);
                if let (Some(call_id), Some(call)) =
                    (field(update, "callId"), update.get("toolCall"))
                {
                    // Tolerate a completion without its start: the timeline still gets a
                    // matched start, args, end and result.
                    self.open_tool(&call_id, call, &mut events);
                    self.plan_events(&call_id, call, &mut events);
                    self.finish_tool(&call_id, call, &mut events);
                }
            }
            // A task call carries its subagent's own stream here. Its text stays out of the
            // parent turn; only the nested tool steps are marked as activity.
            Some("tool-call-delta") => self.subagent_activity(update, &mut events),
            Some("step-completed" | "turn-ended") => {
                self.close_turn(&mut events);
                if field(update, "type").as_deref() == Some("turn-ended")
                    && let Some(usage) = update.get("usage")
                {
                    events.push(usage_event(usage));
                    self.tokens = 0;
                }
            }
            Some("summary-completed") => self.close_turn(&mut events),
            // `step-started`, `user-message-appended` and anything this app does not know yet
            // say nothing and show nothing.
            _ => {}
        }
        events
    }

    /// What closes at the end of a run: open messages, then every tool call left open.
    pub(crate) fn finish(&mut self) -> Vec<EngineEvent> {
        let mut events = Vec::new();
        self.close_turn(&mut events);
        for tool_call_id in std::mem::take(&mut self.tools) {
            events.push(AgUiEvent::ToolCallEnd { tool_call_id }.into());
        }
        if self.tokens > 0 {
            let tokens = std::mem::take(&mut self.tokens);
            events.push(EngineEvent::Usage(ThreadUsage {
                total_tokens: Some(tokens),
                context_window: None,
                raw: json!({ "tokens": tokens }),
            }));
        }
        self.plans.clear();
        self.todos.clear();
        events
    }

    fn open_text(&mut self, events: &mut Vec<EngineEvent>) -> String {
        if let Some(message_id) = &self.text {
            return message_id.clone();
        }
        let message_id = new_id();
        self.text = Some(message_id.clone());
        events.push(
            AgUiEvent::TextMessageStart {
                message_id: message_id.clone(),
                role: "assistant".into(),
            }
            .into(),
        );
        message_id
    }

    fn close_text(&mut self, events: &mut Vec<EngineEvent>) {
        if let Some(message_id) = self.text.take() {
            events.push(AgUiEvent::TextMessageEnd { message_id }.into());
        }
    }

    fn open_reasoning(&mut self, events: &mut Vec<EngineEvent>) -> String {
        if let Some(message_id) = &self.reasoning {
            return message_id.clone();
        }
        let message_id = new_id();
        self.reasoning = Some(message_id.clone());
        events.push(
            AgUiEvent::ReasoningMessageStart {
                message_id: message_id.clone(),
                role: "reasoning".into(),
            }
            .into(),
        );
        message_id
    }

    fn close_reasoning(&mut self, events: &mut Vec<EngineEvent>) {
        if let Some(message_id) = self.reasoning.take() {
            events.push(AgUiEvent::ReasoningMessageEnd { message_id }.into());
        }
    }

    fn close_turn(&mut self, events: &mut Vec<EngineEvent>) {
        self.close_text(events);
        self.close_reasoning(events);
    }

    fn open_tool(&mut self, call_id: &str, call: &Value, events: &mut Vec<EngineEvent>) {
        if self.tools.iter().any(|open| open == call_id) {
            return;
        }
        self.tools.push(call_id.to_owned());
        events.push(
            AgUiEvent::ToolCallStart {
                tool_call_id: call_id.to_owned(),
                tool_call_name: field(call, "type").unwrap_or_else(|| "tool".into()),
            }
            .into(),
        );
        events.push(
            AgUiEvent::ToolCallArgs {
                tool_call_id: call_id.to_owned(),
                delta: call.get("args").map(Value::to_string).unwrap_or_default(),
            }
            .into(),
        );
    }

    fn finish_tool(&mut self, call_id: &str, call: &Value, events: &mut Vec<EngineEvent>) {
        self.tools.retain(|open| open != call_id);
        events.push(
            AgUiEvent::ToolCallEnd {
                tool_call_id: call_id.to_owned(),
            }
            .into(),
        );
        if let Some(status) = call.pointer("/result/status").and_then(Value::as_str) {
            events.push(
                AgUiEvent::Custom {
                    name: "tool.outcome".into(),
                    value: json!({
                        "toolCallId": call_id,
                        "outcome": if status == "success" { "success" } else { "failure" },
                    }),
                }
                .into(),
            );
        }
        events.push(
            AgUiEvent::ToolCallResult {
                message_id: format!("cursor-tool-{call_id}"),
                tool_call_id: call_id.to_owned(),
                content: plain_result(call),
                role: Some("tool".into()),
            }
            .into(),
        );
    }

    /// Cursor's `createPlan` and `updateTodos` are the same plan artifacts the Claude and ACP
    /// mappers produce: a proposed-plan message and the plan activity's steps.
    fn plan_events(&mut self, call_id: &str, call: &Value, events: &mut Vec<EngineEvent>) {
        match field(call, "type").as_deref() {
            Some("createPlan") => {
                let plan = call
                    .pointer("/args/plan")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .unwrap_or_default();
                if plan.is_empty()
                    || self
                        .plans
                        .get(call_id)
                        .is_some_and(|seen| seen.as_str() == plan)
                {
                    return;
                }
                self.plans.insert(call_id.to_owned(), plan.to_owned());
                events.extend(proposed_plan_events(call_id, plan));
            }
            Some("updateTodos") => {
                let steps = todo_steps(call);
                if steps.is_empty() {
                    return;
                }
                let key = Value::Array(steps.clone()).to_string();
                if self.todos.get(call_id).is_some_and(|seen| seen == &key) {
                    return;
                }
                self.todos.insert(call_id.to_owned(), key);
                events.push(
                    AgUiEvent::ActivitySnapshot {
                        message_id: "plan".into(),
                        activity_type: "plan".into(),
                        content: json!({ "steps": steps }),
                        replace: Some(true),
                    }
                    .into(),
                );
            }
            _ => {}
        }
    }

    fn subagent_activity(&mut self, update: &Value, events: &mut Vec<EngineEvent>) {
        let Some(nested) = update.get("taskUpdate") else {
            return;
        };
        let kind = field(nested, "type");
        if !matches!(
            kind.as_deref(),
            Some("tool-call-started" | "tool-call-completed")
        ) {
            return;
        }
        let call = nested.get("toolCall");
        events.push(
            AgUiEvent::Custom {
                name: "subagent.activity".into(),
                value: json!({
                    "parentToolCallId": field(update, "callId").unwrap_or_default(),
                    "toolCallId": field(nested, "callId").unwrap_or_default(),
                    "kind": kind,
                    "tool": call
                        .and_then(|call| call.get("type"))
                        .and_then(Value::as_str)
                        .unwrap_or("tool"),
                    "status": if kind.as_deref() == Some("tool-call-completed") {
                        "completed"
                    } else {
                        "running"
                    },
                }),
            }
            .into(),
        );
    }
}

fn field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn usage_event(usage: &Value) -> EngineEvent {
    let count = |key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0);
    let total = count("inputTokens")
        + count("outputTokens")
        + count("cacheReadTokens")
        + count("cacheWriteTokens");
    EngineEvent::Usage(ThreadUsage {
        total_tokens: Some(total),
        context_window: None,
        raw: usage.clone(),
    })
}

/// The plan activity's steps from Cursor's `updateTodos` input.
fn todo_steps(call: &Value) -> Vec<Value> {
    call.pointer("/args/todos")
        .and_then(Value::as_array)
        .map(|todos| {
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
        })
        .unwrap_or_default()
}

/// A plain result a person can read, never the SDK's JSON. Long output is capped.
fn plain_result(call: &Value) -> String {
    let Some(result) = call.get("result") else {
        return String::new();
    };
    let value = result.get("value").unwrap_or(&Value::Null);
    let text = if result.get("status").and_then(Value::as_str) == Some("success") {
        match field(call, "type").as_deref() {
            Some("read") => value
                .get("content")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_default(),
            Some("glob") => join_lines(value.get("files")),
            Some("grep") => grep_text(value),
            Some("task") => task_text(value),
            Some("shell") => shell_text(value),
            _ => plain_value(value),
        }
    } else {
        error_text(value)
    };
    text.chars().take(MAX_RESULT_CHARS).collect()
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

fn grep_text(value: &Value) -> String {
    let Some(workspaces) = value.get("workspaceResults").and_then(Value::as_object) else {
        return plain_value(value);
    };
    let mut lines = Vec::new();
    for result in workspaces.values() {
        let output = result.get("output").unwrap_or(&Value::Null);
        if let Some(matches) = output.get("matches").and_then(Value::as_array) {
            for file in matches {
                let path = file.get("file").and_then(Value::as_str).unwrap_or_default();
                let rows = file
                    .get("matches")
                    .and_then(Value::as_array)
                    .map_or(&[][..], Vec::as_slice);
                if rows.is_empty() && !path.is_empty() {
                    lines.push(path.to_owned());
                    continue;
                }
                for row in rows {
                    let content = row
                        .get("content")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    lines.push(match row.get("lineNumber").and_then(Value::as_u64) {
                        Some(line) => format!("{path}:{line}: {content}"),
                        None => format!("{path}: {content}"),
                    });
                }
            }
        } else if let Some(total) = output.get("totalMatches").and_then(Value::as_u64) {
            lines.push(format!("{total} matches"));
        }
    }
    lines.join("\n")
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

fn shell_text(value: &Value) -> String {
    for key in ["output", "stdout", "stderr"] {
        if let Some(text) = value.get(key).and_then(Value::as_str)
            && !text.is_empty()
        {
            return text.to_owned();
        }
    }
    plain_value(value)
}

fn error_text(value: &Value) -> String {
    if let Value::String(text) = value {
        return text.clone();
    }
    for path in ["/error", "/permissionDenied/error", "/message"] {
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

#[cfg(test)]
#[path = "cursor_delta_tests.rs"]
mod tests;
