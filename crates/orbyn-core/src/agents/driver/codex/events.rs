use std::collections::HashMap;

use orbyn_contracts::{AgUiEvent, ThreadGoal, ThreadUsage};
use serde_json::{Value, json};

use super::limits::spent_window_reset;
use super::text;
use crate::agents::driver::{
    EngineEvent, EngineOutcome, ItemRecord, diff_activity, proposed_plan_events,
};
use crate::util::new_id;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ItemKind {
    Message,
    Reasoning,
    Tool,
}

#[derive(Default)]
pub(super) struct Step {
    pub(super) events: Vec<EngineEvent>,
    pub(super) outcome: Option<EngineOutcome>,
}

/// Maps turn notifications to events. It tolerates a `completed` without a
/// `started`, and ignores other threads: Codex sub-agents share this stdio.
pub(super) struct TurnMapper {
    session: String,
    pub(super) open: HashMap<String, ItemKind>,
    delivered_text: HashMap<String, bool>,
    pub(super) file_paths: HashMap<String, Vec<String>>,
    last_error: Option<String>,
    /// When the window that has run out resets, from the last limits update.
    limit_reset: Option<i64>,
}

impl TurnMapper {
    pub(super) fn new(session: String) -> Self {
        Self {
            session,
            open: HashMap::new(),
            delivered_text: HashMap::new(),
            file_paths: HashMap::new(),
            last_error: None,
            limit_reset: None,
        }
    }

    pub(super) fn on_notification(&mut self, method: &str, params: &Value) -> Step {
        let mut step = Step::default();
        if params
            .get("threadId")
            .and_then(Value::as_str)
            .is_some_and(|thread| thread != self.session)
        {
            return step;
        }
        let item_id = text(params, "itemId").unwrap_or_default();
        let delta = text(params, "delta").unwrap_or_default();
        match method {
            "item/started" => {
                if let Some(item) = params.get("item") {
                    self.item_started(item, &mut step.events);
                }
            }
            "item/completed" => {
                if let Some(item) = params.get("item") {
                    self.item_completed(item, &mut step.events);
                }
            }
            "item/agentMessage/delta" => {
                self.open_message(&item_id, &mut step.events);
                self.delivered_text.insert(item_id.clone(), true);
                step.events.push(
                    AgUiEvent::TextMessageContent {
                        message_id: item_id,
                        delta,
                    }
                    .into(),
                );
            }
            "item/reasoning/summaryTextDelta" | "item/reasoning/textDelta" => {
                self.open_reasoning(&item_id, &mut step.events);
                step.events.push(
                    AgUiEvent::ReasoningMessageContent {
                        message_id: item_id,
                        delta,
                    }
                    .into(),
                );
            }
            "item/commandExecution/outputDelta" | "item/fileChange/outputDelta" => {
                step.events.push(
                    AgUiEvent::Custom {
                        name: "tool.output".into(),
                        value: json!({ "toolCallId": item_id, "delta": delta }),
                    }
                    .into(),
                );
            }
            "turn/diff/updated" => {
                let diff = text(params, "diff").unwrap_or_default();
                let mut content = diff_activity(&diff);
                content["diff"] = json!(diff);
                step.events.push(
                    AgUiEvent::ActivitySnapshot {
                        message_id: format!("diff-{}", params_turn(params)),
                        activity_type: "diff".into(),
                        content,
                        replace: Some(true),
                    }
                    .into(),
                );
            }
            "turn/plan/updated" => {
                step.events.push(
                    AgUiEvent::ActivitySnapshot {
                        message_id: format!("plan-{}", params_turn(params)),
                        activity_type: "plan".into(),
                        content: plan_content(params),
                        replace: Some(true),
                    }
                    .into(),
                );
            }
            "error" => {
                let message = params
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("Codex reported an error.")
                    .to_owned();
                if params.get("willRetry").and_then(Value::as_bool) == Some(true) {
                    step.events.push(
                        AgUiEvent::Custom {
                            name: "engine.retrying".into(),
                            value: json!({ "message": message }),
                        }
                        .into(),
                    );
                } else {
                    let limited = params
                        .pointer("/error/codexErrorInfo")
                        .is_some_and(|info| info.to_string().contains("usageLimitExceeded"));
                    if limited {
                        let said = params.pointer("/error/resetsAt").and_then(Value::as_i64);
                        step.events.push(EngineEvent::Limited {
                            resets_at: said.or(self.limit_reset),
                        });
                    }
                    self.last_error = Some(message);
                }
            }
            "account/rateLimits/updated" => self.limit_reset = spent_window_reset(params),
            "thread/tokenUsage/updated" => {
                let usage = params.get("tokenUsage").cloned().unwrap_or(Value::Null);
                let report = ThreadUsage {
                    total_tokens: usage.pointer("/total/totalTokens").and_then(Value::as_u64),
                    context_window: usage.get("modelContextWindow").and_then(Value::as_u64),
                    raw: usage,
                };
                step.events.push(
                    AgUiEvent::Custom {
                        name: "usage".into(),
                        value: json!(report),
                    }
                    .into(),
                );
                step.events.push(EngineEvent::Usage(report));
            }
            "thread/goal/updated" => {
                step.events
                    .push(EngineEvent::Goal(params.get("goal").map(parse_goal)));
            }
            "thread/goal/cleared" => step.events.push(EngineEvent::Goal(None)),
            "thread/compacted" => step.events.push(
                AgUiEvent::Custom {
                    name: "context.compacted".into(),
                    value: Value::Null,
                }
                .into(),
            ),
            "warning" => {
                step.events.push(
                    AgUiEvent::Custom {
                        name: "engine.warning".into(),
                        value: json!({ "message": text(params, "message") }),
                    }
                    .into(),
                );
            }
            "turn/completed" => {
                let turn = params.get("turn").cloned().unwrap_or(Value::Null);
                step.outcome = Some(match turn.get("status").and_then(Value::as_str) {
                    Some("completed") => EngineOutcome::Completed,
                    Some("interrupted") => EngineOutcome::Cancelled,
                    _ => EngineOutcome::Failed(
                        turn.pointer("/error/message")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                            .or_else(|| self.last_error.take())
                            .unwrap_or_else(|| "Codex failed the turn.".to_owned()),
                    ),
                });
            }
            _ => tracing::trace!(%method, "ignored Codex notification"),
        }
        step
    }

    fn open_message(&mut self, id: &str, events: &mut Vec<EngineEvent>) {
        if self.open.insert(id.to_owned(), ItemKind::Message).is_none() {
            events.push(
                AgUiEvent::TextMessageStart {
                    message_id: id.to_owned(),
                    role: "assistant".into(),
                }
                .into(),
            );
        }
    }

    fn open_reasoning(&mut self, id: &str, events: &mut Vec<EngineEvent>) {
        if self
            .open
            .insert(id.to_owned(), ItemKind::Reasoning)
            .is_none()
        {
            events.push(
                AgUiEvent::ReasoningMessageStart {
                    message_id: id.to_owned(),
                    role: "reasoning".into(),
                }
                .into(),
            );
        }
    }

    fn open_tool(&mut self, id: &str, name: String, args: Value, events: &mut Vec<EngineEvent>) {
        if self.open.insert(id.to_owned(), ItemKind::Tool).is_none() {
            events.push(
                AgUiEvent::ToolCallStart {
                    tool_call_id: id.to_owned(),
                    tool_call_name: name,
                }
                .into(),
            );
            events.push(
                AgUiEvent::ToolCallArgs {
                    tool_call_id: id.to_owned(),
                    delta: args.to_string(),
                }
                .into(),
            );
        }
    }

    fn item_started(&mut self, item: &Value, events: &mut Vec<EngineEvent>) {
        let Some(id) = item.get("id").and_then(Value::as_str) else {
            return;
        };
        match item.get("type").and_then(Value::as_str) {
            Some("agentMessage") => self.open_message(id, events),
            Some("reasoning") => self.open_reasoning(id, events),
            Some("collabAgentToolCall" | "subAgentActivity") => {
                events.push(
                    AgUiEvent::Custom {
                        name: "subagent.activity".into(),
                        value: json!({
                            "id": id,
                            "kind": item.get("type"),
                            "tool": item.get("tool"),
                            "status": item.get("status"),
                            "agentPath": item.get("agentPath"),
                        }),
                    }
                    .into(),
                );
            }
            Some(kind) => {
                if let Some((name, args)) = tool_shape(kind, item) {
                    if kind == "fileChange" {
                        self.file_paths.insert(id.to_owned(), change_paths(item));
                    }
                    self.open_tool(id, name, args, events);
                }
            }
            None => {}
        }
    }

    fn item_completed(&mut self, item: &Value, events: &mut Vec<EngineEvent>) {
        let Some(id) = item.get("id").and_then(Value::as_str) else {
            return;
        };
        let kind = item.get("type").and_then(Value::as_str).unwrap_or("item");
        let exit_code = item.get("exitCode").and_then(Value::as_i64);
        events.push(EngineEvent::Item(ItemRecord {
            id: id.to_owned(),
            kind: kind.to_owned(),
            raw: item.clone(),
            exit_code,
        }));
        if kind == "fileChange" {
            events.push(EngineEvent::Files(change_paths(item)));
        }
        if let Some(code) = exit_code {
            events.push(
                AgUiEvent::Custom {
                    name: "tool.finished".into(),
                    value: json!({ "toolCallId": id, "exitCode": code }),
                }
                .into(),
            );
        }
        match item.get("type").and_then(Value::as_str) {
            Some("agentMessage") => {
                self.open_message(id, events);
                if !self.delivered_text.contains_key(id) {
                    events.push(
                        AgUiEvent::TextMessageContent {
                            message_id: id.to_owned(),
                            delta: text(item, "text").unwrap_or_default(),
                        }
                        .into(),
                    );
                }
                self.open.remove(id);
                events.push(
                    AgUiEvent::TextMessageEnd {
                        message_id: id.to_owned(),
                    }
                    .into(),
                );
            }
            // Deltas may not add up to this, so the finished item is the plan.
            Some("plan") => {
                events.extend(proposed_plan_events(
                    id,
                    &text(item, "text").unwrap_or_default(),
                ));
            }
            Some("reasoning") => {
                if self.open.remove(id).is_some() {
                    events.push(
                        AgUiEvent::ReasoningMessageEnd {
                            message_id: id.to_owned(),
                        }
                        .into(),
                    );
                }
            }
            Some(kind) => {
                let Some((name, args)) = tool_shape(kind, item) else {
                    return;
                };
                self.open_tool(id, name, args, events);
                let outcome = match (exit_code, item.get("status").and_then(Value::as_str)) {
                    (_, Some("failed")) => Some("failure"),
                    (Some(code), _) if code != 0 => Some("failure"),
                    (Some(0), _) | (None, Some("completed")) => Some("success"),
                    _ => None,
                };
                if let Some(outcome) = outcome {
                    events.push(
                        AgUiEvent::Custom {
                            name: "tool.outcome".into(),
                            value: json!({ "toolCallId": id, "outcome": outcome }),
                        }
                        .into(),
                    );
                }
                self.open.remove(id);
                self.file_paths.remove(id);
                events.push(
                    AgUiEvent::ToolCallEnd {
                        tool_call_id: id.to_owned(),
                    }
                    .into(),
                );
                events.push(
                    AgUiEvent::ToolCallResult {
                        message_id: new_id(),
                        tool_call_id: id.to_owned(),
                        content: tool_output(item),
                        role: Some("tool".into()),
                    }
                    .into(),
                );
            }
            None => {}
        }
    }

    pub(super) fn close_open(&mut self) -> Vec<EngineEvent> {
        self.open
            .drain()
            .map(|(id, kind)| {
                match kind {
                    ItemKind::Message => AgUiEvent::TextMessageEnd { message_id: id },
                    ItemKind::Reasoning => AgUiEvent::ReasoningMessageEnd { message_id: id },
                    ItemKind::Tool => AgUiEvent::ToolCallEnd { tool_call_id: id },
                }
                .into()
            })
            .collect()
    }
}

pub(super) fn parse_goal(goal: &Value) -> ThreadGoal {
    let number = |key: &str| goal.get(key).and_then(Value::as_u64);
    let status = text(goal, "status").unwrap_or_default();
    ThreadGoal {
        objective: text(goal, "objective").unwrap_or_default(),
        status: match status.as_str() {
            "usageLimited" => "usage_limited".to_owned(),
            "budgetLimited" => "budget_limited".to_owned(),
            _ => status,
        },
        tokens_used: number("tokensUsed"),
        token_budget: number("tokenBudget"),
        seconds_used: number("timeUsedSeconds"),
    }
}

fn params_turn(params: &Value) -> String {
    text(params, "turnId").unwrap_or_default()
}

pub(super) fn plan_content(params: &Value) -> Value {
    let steps: Vec<Value> = params
        .get("plan")
        .and_then(Value::as_array)
        .map(|plan| {
            plan.iter()
                .map(|step| {
                    let status = match step.get("status").and_then(Value::as_str) {
                        Some("inProgress") => "in_progress",
                        Some(other) => other,
                        None => "pending",
                    };
                    json!({ "title": text(step, "step").unwrap_or_default(), "status": status })
                })
                .collect()
        })
        .unwrap_or_default();
    json!({ "steps": steps, "explanation": params.get("explanation") })
}

fn tool_shape(kind: &str, item: &Value) -> Option<(String, Value)> {
    match kind {
        "commandExecution" => Some((
            "command".to_owned(),
            json!({ "command": item.get("command"), "cwd": item.get("cwd") }),
        )),
        "fileChange" => Some((
            "file_change".to_owned(),
            json!({ "paths": change_paths(item) }),
        )),
        "mcpToolCall" => Some((
            format!(
                "{}.{}",
                text(item, "server").unwrap_or_default(),
                text(item, "tool").unwrap_or_default()
            ),
            item.get("arguments").cloned().unwrap_or(Value::Null),
        )),
        "dynamicToolCall" => Some((
            text(item, "tool").unwrap_or_else(|| "tool".to_owned()),
            item.get("arguments").cloned().unwrap_or(Value::Null),
        )),
        "webSearch" => Some((
            "web_search".to_owned(),
            json!({ "query": item.get("query") }),
        )),
        "imageView" => Some(("image_view".to_owned(), json!({ "path": item.get("path") }))),
        _ => None,
    }
}

fn change_paths(item: &Value) -> Vec<String> {
    item.get("changes")
        .and_then(Value::as_array)
        .map(|changes| {
            changes
                .iter()
                .filter_map(|change| text(change, "path"))
                .collect()
        })
        .unwrap_or_default()
}

fn tool_output(item: &Value) -> String {
    for key in ["aggregatedOutput", "error"] {
        if let Some(value) = item.get(key).filter(|value| !value.is_null()) {
            return match value {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            };
        }
    }
    item.get("result")
        .filter(|value| !value.is_null())
        .map_or_else(
            || text(item, "status").unwrap_or_default(),
            Value::to_string,
        )
}
