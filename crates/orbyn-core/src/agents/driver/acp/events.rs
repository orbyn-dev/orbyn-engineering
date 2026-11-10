use std::collections::HashSet;

use orbyn_contracts::AgUiEvent;
use serde_json::{Value, json};

use crate::agents::driver::EngineEvent;
use crate::util::new_id;

/// Maps `session/update` payloads. Chunks of one kind form one message, which
/// closes when another kind of update arrives.
#[derive(Default)]
pub(super) struct UpdateMapper {
    text: Option<String>,
    text_role: Option<String>,
    historical_id: Option<u64>,
    reasoning: Option<String>,
    tools: HashSet<String>,
}

impl UpdateMapper {
    pub(super) fn historical() -> Self {
        Self {
            historical_id: Some(0),
            ..Self::default()
        }
    }
    fn next_message_id(&mut self) -> String {
        if let Some(next) = &mut self.historical_id {
            *next += 1;
            format!("acp-history-{next}")
        } else {
            new_id()
        }
    }
    pub(super) fn on_update(&mut self, update: &Value) -> Vec<EngineEvent> {
        let mut events = Vec::new();
        match update.get("sessionUpdate").and_then(Value::as_str) {
            Some("agent_message_chunk" | "user_message_chunk") => {
                let role = if update.get("sessionUpdate").and_then(Value::as_str)
                    == Some("user_message_chunk")
                {
                    "user"
                } else {
                    "assistant"
                };
                if self.text_role.as_deref() != Some(role) {
                    self.close_text(&mut events);
                }
                self.text_role = Some(role.to_owned());
                self.close_reasoning(&mut events);
                let delta = chunk_text(update);
                let id = match &self.text {
                    Some(id) => id.clone(),
                    None => {
                        let id = self.next_message_id();
                        self.text = Some(id.clone());
                        id
                    }
                };
                if !self.tools.contains(&format!("open:{id}")) {
                    self.tools.insert(format!("open:{id}"));
                    events.push(
                        AgUiEvent::TextMessageStart {
                            message_id: id.clone(),
                            role: role.into(),
                        }
                        .into(),
                    );
                }
                events.push(
                    AgUiEvent::TextMessageContent {
                        message_id: id,
                        delta,
                    }
                    .into(),
                );
            }
            Some("agent_thought_chunk") => {
                self.close_text(&mut events);
                let delta = chunk_text(update);
                let id = match &self.reasoning {
                    Some(id) => id.clone(),
                    None => {
                        let id = self.next_message_id();
                        self.reasoning = Some(id.clone());
                        id
                    }
                };
                if !self.tools.contains(&format!("open:{id}")) {
                    self.tools.insert(format!("open:{id}"));
                    events.push(
                        AgUiEvent::ReasoningMessageStart {
                            message_id: id.clone(),
                            role: "reasoning".into(),
                        }
                        .into(),
                    );
                }
                events.push(
                    AgUiEvent::ReasoningMessageContent {
                        message_id: id,
                        delta,
                    }
                    .into(),
                );
            }
            Some("tool_call") => {
                self.close_text(&mut events);
                self.close_reasoning(&mut events);
                let id = field(update, "toolCallId");
                self.tools.insert(id.clone());
                events.push(
                    AgUiEvent::ToolCallStart {
                        tool_call_id: id.clone(),
                        tool_call_name: field(update, "title"),
                    }
                    .into(),
                );
                if let Some(input) = update.get("rawInput") {
                    events.push(
                        AgUiEvent::ToolCallArgs {
                            tool_call_id: id,
                            delta: input.to_string(),
                        }
                        .into(),
                    );
                }
            }
            Some("tool_call_update") => {
                let id = field(update, "toolCallId");
                let status = update.get("status").and_then(Value::as_str);
                if matches!(status, Some("completed" | "failed")) && self.tools.remove(&id) {
                    events.push(
                        AgUiEvent::ToolCallEnd {
                            tool_call_id: id.clone(),
                        }
                        .into(),
                    );
                    events.push(
                        AgUiEvent::Custom {
                            name: "tool.outcome".into(),
                            value: json!({ "toolCallId": id, "outcome": if status == Some("completed") { "success" } else { "failure" } }),
                        }
                        .into(),
                    );
                    events.push(
                        AgUiEvent::ToolCallResult {
                            message_id: self.next_message_id(),
                            tool_call_id: id,
                            content: tool_output(update),
                            role: Some("tool".into()),
                        }
                        .into(),
                    );
                }
            }
            Some("available_commands_update") => {
                let listed = update.get("availableCommands").and_then(Value::as_array);
                events.push(EngineEvent::Commands(
                    listed
                        .into_iter()
                        .flatten()
                        .map(|command| (field(command, "name"), field(command, "description")))
                        .filter(|(name, _)| !name.is_empty())
                        .collect(),
                ));
            }
            Some("plan") => {
                let steps: Vec<Value> = update
                    .get("entries")
                    .and_then(Value::as_array)
                    .map(|entries| {
                        entries
                            .iter()
                            .map(|entry| {
                                json!({
                                    "title": field(entry, "content"),
                                    "status": field(entry, "status"),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
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
        events
    }

    fn close_text(&mut self, events: &mut Vec<EngineEvent>) {
        self.text_role = None;
        if let Some(id) = self.text.take() {
            self.tools.remove(&format!("open:{id}"));
            events.push(AgUiEvent::TextMessageEnd { message_id: id }.into());
        }
    }

    fn close_reasoning(&mut self, events: &mut Vec<EngineEvent>) {
        if let Some(id) = self.reasoning.take() {
            self.tools.remove(&format!("open:{id}"));
            events.push(AgUiEvent::ReasoningMessageEnd { message_id: id }.into());
        }
    }

    pub(super) fn close(&mut self) -> Vec<EngineEvent> {
        let mut events = Vec::new();
        self.close_text(&mut events);
        self.close_reasoning(&mut events);
        for id in self.tools.drain() {
            events.push(AgUiEvent::ToolCallEnd { tool_call_id: id }.into());
        }
        events
    }
}

fn chunk_text(update: &Value) -> String {
    update
        .pointer("/content/text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

pub(super) use crate::util::json::text as field;

fn tool_output(update: &Value) -> String {
    if let Some(output) = update.get("rawOutput") {
        return match output {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        };
    }
    update
        .get("content")
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part.pointer("/content/text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}
