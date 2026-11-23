//! Cursor's `InteractionUpdate` frames to timeline events. The host passes the SDK's own
//! update object through untouched, so this mapper owns every shape a Cursor run can make:
//! thinking and text streams, tool calls, Cursor's own plan and todo tools, subagent
//! activity, and the token counters that never appear as text.
//!
//! Nothing here trusts a size or a sum: every text field is capped before it is copied,
//! usage is added with saturating arithmetic, and an id that repeats is either ignored
//! (a late or duplicate tool event) or made unique with an occurrence suffix.

mod result;
mod support;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use orbyn_contracts::{AgUiEvent, ThreadUsage};
use serde_json::{Value, json};

use crate::agents::driver::{EngineEvent, proposed_plan_events};
use crate::util::new_id;
use support::{capped, field, inside, todo_steps, tool_outcome, tool_update, usage_tokens};

/// The longest plain tool result kept for the timeline, in characters.
pub(crate) const MAX_RESULT_CHARS: usize = 16 * 1024;

/// The longest text or reasoning delta kept for one event, in characters. The transport
/// accepts frames up to 8 MiB, so a single delta could otherwise fill a stored event.
const MAX_TEXT_CHARS: usize = 64 * 1024;

/// The longest nested subagent text kept for one activity event, in characters.
const MAX_SUBAGENT_CHARS: usize = 4 * 1024;

/// What a person reads when a run ends while a tool is still open.
const INTERRUPTED: &str = "Interrupted before a result.";

/// One tool call that started and has not ended, in start order.
struct OpenTool {
    id: String,
    name: String,
}

/// What the parent task call says about its subagent, for the activity rows the web reads.
#[derive(Clone)]
struct Subagent {
    id: String,
    path: Option<String>,
}

/// Maps the Cursor SDK's updates. Text and reasoning are one message per streamed segment;
/// Cursor has no "block stopped" event, so a different kind, a tool call, the end of a step
/// or the end of the run closes the open one.
#[derive(Default)]
pub(crate) struct CursorMapper {
    text: Option<String>,
    reasoning: Option<String>,
    /// Open tool calls in start order, so a run that ends mid-tool closes them in order.
    tools: Vec<OpenTool>,
    /// Tool calls that already completed. A duplicate completion, or a start after a
    /// completion, is a late event and must not reopen or re-emit the call.
    completed: BTreeSet<String>,
    /// Each call id's stable unique form, used in the ids this mapper generates. The suffix
    /// keeps two messages apart even if the SDK reuses a call id.
    occurrences: BTreeMap<String, String>,
    /// The last proposed plan shown for a call, so a streamed plan appears once.
    plans: BTreeMap<String, String>,
    /// The last todo list shown for a call, so a streamed list replaces it in place.
    todos: BTreeMap<String, String>,
    /// The parent task call id to the subagent it named.
    subagents: BTreeMap<String, Subagent>,
    /// The tokens of a `token-delta` run, the fallback when no usage update arrives.
    token_total: u64,
    /// The sum of every `turn-ended` usage in this run (input + output).
    turn_total: u64,
    /// The sum of every `model-call-completed` usage, used only when no turn usage arrives.
    model_total: u64,
    saw_turn_usage: bool,
    saw_model_usage: bool,
    /// The host's `runDone.result.usage`, which is the run total and wins over deltas.
    result_usage: Option<Value>,
    /// The run's working folder: a write outside it is not this run's change.
    cwd: Option<PathBuf>,
}

impl CursorMapper {
    /// Keeps only file changes inside `cwd`; a run without one keeps every reported path.
    #[must_use]
    pub(crate) fn with_cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// The host's result usage, when its `runDone` carried one.
    pub(crate) fn note_result_usage(&mut self, usage: Option<&Value>) {
        if let Some(usage) = usage {
            self.result_usage = Some(usage.clone());
        }
    }

    pub(crate) fn map(&mut self, update: &Value) -> Vec<EngineEvent> {
        let mut events = Vec::new();
        match field(update, "type").as_deref() {
            Some("text-delta") => {
                // Borrow the text and cap it before a String is made: an 8 MiB delta
                // must never be cloned whole just to keep its first 64 KiB.
                let Some(raw) = update.get("text").and_then(Value::as_str) else {
                    return events;
                };
                if raw.is_empty() {
                    return events;
                }
                self.close_reasoning(&mut events);
                let message_id = self.open_text(&mut events);
                events.push(
                    AgUiEvent::TextMessageContent {
                        message_id,
                        delta: capped(raw, MAX_TEXT_CHARS),
                    }
                    .into(),
                );
            }
            Some("thinking-delta") => {
                let Some(raw) = update.get("text").and_then(Value::as_str) else {
                    return events;
                };
                if raw.is_empty() {
                    return events;
                }
                self.close_text(&mut events);
                let message_id = self.open_reasoning(&mut events);
                events.push(
                    AgUiEvent::ReasoningMessageContent {
                        message_id,
                        delta: capped(raw, MAX_TEXT_CHARS),
                    }
                    .into(),
                );
            }
            Some("thinking-completed") => self.close_reasoning(&mut events),
            // Cursor reports tokens as a running count. They are never text; they stand in
            // only when the SDK reports neither a turn nor a model call to total.
            Some("token-delta") => {
                self.token_total = self
                    .token_total
                    .saturating_add(update.get("tokens").and_then(Value::as_u64).unwrap_or(0));
            }
            Some("partial-tool-call") => {
                self.close_turn(&mut events);
                if let Some((call_id, call)) = tool_update(update)
                    && !self.completed.contains(&call_id)
                {
                    self.plan_events(&call_id, call, &mut events);
                }
            }
            Some("tool-call-started") => {
                self.close_turn(&mut events);
                if let Some((call_id, call)) = tool_update(update)
                    && !self.completed.contains(&call_id)
                {
                    self.open_tool(&call_id, call, &mut events);
                    self.plan_events(&call_id, call, &mut events);
                }
            }
            Some("tool-call-completed") => {
                self.close_turn(&mut events);
                if let Some((call_id, call)) = tool_update(update)
                    && !self.completed.contains(&call_id)
                {
                    // Tolerate a completion without its start: the timeline still gets a
                    // matched start, args, end and result.
                    self.open_tool(&call_id, call, &mut events);
                    self.plan_events(&call_id, call, &mut events);
                    self.finish_tool(&call_id, call, &mut events);
                    self.completed.insert(call_id);
                }
            }
            // A task call carries its subagent's own stream here. Its text stays out of the
            // parent turn; the activity rows are what the subagent chips read.
            Some("tool-call-delta") => self.subagent_activity(update, &mut events),
            Some("step-completed" | "turn-ended") => {
                self.close_turn(&mut events);
                if field(update, "type").as_deref() == Some("turn-ended")
                    && let Some(usage) = update.get("usage")
                {
                    self.add_turn_usage(usage);
                }
            }
            // A compaction summary is not the assistant's answer. Cursor's UI never showed
            // it as a message and neither does this timeline: only the completed mark
            // closes the open text, as a context change rather than a sentence.
            Some("summary" | "summary-started") => {}
            Some("summary-completed") => self.close_turn(&mut events),
            Some("shell-output-delta") => self.shell_output(update, &mut events),
            // One model call's usage is the fallback total when `turn-ended` is missing;
            // its inputTokens already include cache reads and writes.
            Some("model-call-completed") => {
                if let Some(usage) = update.get("usage") {
                    self.model_total = self.model_total.saturating_add(usage_tokens(usage));
                    self.saw_model_usage = true;
                }
            }
            // `step-started`, `user-message-appended` and `tool-requests-listed` say
            // nothing the timeline shows: the core records steers itself, and a step is
            // not a message. Anything unknown is ignored for forward compatibility.
            _ => {}
        }
        events
    }

    /// What closes at the end of a run: open messages, then every tool call left open,
    /// then the run's usage once.
    pub(crate) fn finish(&mut self) -> Vec<EngineEvent> {
        let mut events = Vec::new();
        self.close_turn(&mut events);
        for tool in std::mem::take(&mut self.tools) {
            self.completed.insert(tool.id.clone());
            self.interrupted(&tool.id, &mut events);
        }
        if let Some(usage) = self.usage_event() {
            events.push(usage);
        }
        self.plans.clear();
        self.todos.clear();
        self.subagents.clear();
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

    /// A stable unique form of a call id. The suffix is the call's occurrence; this mapper
    /// ignores a late reuse, so the same id always names the same occurrence.
    fn unique(&mut self, call_id: &str) -> String {
        if let Some(unique) = self.occurrences.get(call_id) {
            return unique.clone();
        }
        let unique = format!("{call_id}-1");
        self.occurrences.insert(call_id.to_owned(), unique.clone());
        unique
    }

    fn open_tool(&mut self, call_id: &str, call: &Value, events: &mut Vec<EngineEvent>) {
        if self.completed.contains(call_id) || self.tools.iter().any(|open| open.id == call_id) {
            return;
        }
        let name = field(call, "type").unwrap_or_else(|| "tool".into());
        if name == "task" {
            self.remember_subagent(call_id, call);
        }
        self.tools.push(OpenTool {
            id: call_id.to_owned(),
            name,
        });
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
                delta: match call.get("args") {
                    None | Some(Value::Null) => String::new(),
                    Some(args) => capped(&args.to_string(), MAX_TEXT_CHARS),
                },
            }
            .into(),
        );
    }

    fn finish_tool(&mut self, call_id: &str, call: &Value, events: &mut Vec<EngineEvent>) {
        self.tools.retain(|open| open.id != call_id);
        events.push(
            AgUiEvent::ToolCallEnd {
                tool_call_id: call_id.to_owned(),
            }
            .into(),
        );
        if let Some((outcome, exit_code)) = tool_outcome(call) {
            if let Some(exit_code) = exit_code {
                events.push(
                    AgUiEvent::Custom {
                        name: "tool.finished".into(),
                        value: json!({ "toolCallId": call_id, "exitCode": exit_code }),
                    }
                    .into(),
                );
            }
            events.push(
                AgUiEvent::Custom {
                    name: "tool.outcome".into(),
                    value: json!({ "toolCallId": call_id, "outcome": outcome }),
                }
                .into(),
            );
        }
        events.push(
            AgUiEvent::ToolCallResult {
                message_id: format!("cursor-tool-{}", self.unique(call_id)),
                tool_call_id: call_id.to_owned(),
                content: result::text(call),
                role: Some("tool".into()),
            }
            .into(),
        );
        if let Some(paths) = self.written_paths(call) {
            events.push(EngineEvent::Files(paths));
        }
        if field(call, "type").as_deref() == Some("task") {
            self.update_subagent(call_id, call);
        }
    }

    /// A tool still open when the run ends has no result: say so before it ends, so the
    /// timeline shows an interrupted change rather than a tool that silently succeeded.
    fn interrupted(&mut self, call_id: &str, events: &mut Vec<EngineEvent>) {
        events.push(
            AgUiEvent::Custom {
                name: "tool.outcome".into(),
                value: json!({
                    "toolCallId": call_id,
                    "outcome": "failure",
                    "interrupted": true,
                }),
            }
            .into(),
        );
        events.push(
            AgUiEvent::ToolCallResult {
                message_id: format!("cursor-tool-{}", self.unique(call_id)),
                tool_call_id: call_id.to_owned(),
                content: INTERRUPTED.into(),
                role: Some("tool".into()),
            }
            .into(),
        );
        events.push(
            AgUiEvent::ToolCallEnd {
                tool_call_id: call_id.to_owned(),
            }
            .into(),
        );
    }

    /// Cursor's `createPlan` and `updateTodos` are the same plan artifacts the Claude and ACP
    /// mappers produce: a proposed-plan message and the plan activity's steps. An empty
    /// todo list is a real replacement: it clears the steps instead of leaving the old plan.
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
                let unique = self.unique(call_id);
                events.extend(proposed_plan_events(&unique, plan));
            }
            Some("updateTodos") => {
                let Some(todos) = call.pointer("/args/todos").and_then(Value::as_array) else {
                    return;
                };
                let steps = todo_steps(todos);
                let key = Value::Array(steps.clone()).to_string();
                if self.todos.get(call_id).is_some_and(|seen| seen == &key) {
                    return;
                }
                self.todos.insert(call_id.to_owned(), key);
                let unique = self.unique(call_id);
                events.push(
                    AgUiEvent::ActivitySnapshot {
                        message_id: format!("plan-{unique}"),
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

    fn remember_subagent(&mut self, call_id: &str, call: &Value) {
        let id = call
            .pointer("/args/agentId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .unwrap_or(call_id)
            .to_owned();
        let path = call
            .pointer("/args/description")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .or_else(|| {
                call.pointer("/result/value/transcriptPath")
                    .and_then(Value::as_str)
            })
            .map(str::to_owned);
        self.subagents
            .insert(call_id.to_owned(), Subagent { id, path });
    }

    fn update_subagent(&mut self, call_id: &str, call: &Value) {
        let Some(info) = self.subagents.get_mut(call_id) else {
            return;
        };
        if let Some(id) = call
            .pointer("/result/value/agentId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
        {
            info.id = id.to_owned();
        }
        if let Some(path) = call
            .pointer("/result/value/transcriptPath")
            .and_then(Value::as_str)
            .filter(|path| !path.is_empty())
        {
            info.path = Some(path.to_owned());
        }
    }

    fn subagent_activity(&mut self, update: &Value, events: &mut Vec<EngineEvent>) {
        let Some(parent) = field(update, "callId").filter(|id| !id.is_empty()) else {
            return;
        };
        let Some(nested) = update.get("taskUpdate") else {
            return;
        };
        let info = self.subagents.get(&parent).cloned().unwrap_or(Subagent {
            id: parent.clone(),
            path: None,
        });
        let id = info.id;
        let agent_path = info.path;
        let kind = field(nested, "type");
        let nested_call = field(nested, "callId").unwrap_or_else(|| parent.clone());
        match kind.as_deref() {
            Some("text-delta" | "thinking-delta") => {
                let Some(raw) = nested.get("text").and_then(Value::as_str) else {
                    return;
                };
                if raw.is_empty() {
                    return;
                }
                let tool = if kind.as_deref() == Some("thinking-delta") {
                    "thinking"
                } else {
                    "text"
                };
                events.push(
                    AgUiEvent::Custom {
                        name: "subagent.activity".into(),
                        value: json!({
                            "id": id,
                            "agentPath": agent_path,
                            "parentToolCallId": parent,
                            "toolCallId": nested_call,
                            "kind": kind,
                            "tool": tool,
                            "status": "running",
                            "text": capped(raw, MAX_SUBAGENT_CHARS),
                        }),
                    }
                    .into(),
                );
            }
            Some("tool-call-started" | "tool-call-completed" | "partial-tool-call") => {
                let tool = nested
                    .pointer("/toolCall/type")
                    .and_then(Value::as_str)
                    .unwrap_or("tool");
                events.push(
                    AgUiEvent::Custom {
                        name: "subagent.activity".into(),
                        value: json!({
                            "id": id,
                            "agentPath": agent_path,
                            "parentToolCallId": parent,
                            "toolCallId": nested_call,
                            "kind": kind,
                            "tool": tool,
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
            // Nested thinking-completed, step-started and step-completed are bookkeeping;
            // the text and tool rows above are what a person reads.
            _ => {}
        }
    }

    fn shell_output(&mut self, update: &Value, events: &mut Vec<EngineEvent>) {
        let Some(event) = update.get("event") else {
            return;
        };
        let call_id = event
            .get("toolCallId")
            .or_else(|| event.get("callId"))
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .or_else(|| {
                self.tools
                    .iter()
                    .rev()
                    .find(|tool| tool.name == "shell")
                    .map(|tool| tool.id.clone())
            });
        let Some(call_id) = call_id else {
            return;
        };
        if !matches!(
            event.get("case").and_then(Value::as_str),
            Some("stdout" | "stderr")
        ) {
            return;
        }
        let Some(text) = event.pointer("/value/data").and_then(Value::as_str) else {
            return;
        };
        if text.is_empty() {
            return;
        }
        events.push(
            AgUiEvent::Custom {
                name: "tool.output".into(),
                value: json!({ "toolCallId": call_id, "delta": capped(text, MAX_TEXT_CHARS) }),
            }
            .into(),
        );
    }

    fn add_turn_usage(&mut self, usage: &Value) {
        self.turn_total = self.turn_total.saturating_add(usage_tokens(usage));
        self.saw_turn_usage = true;
    }

    /// The run's one usage event: the host's result first, then the turn totals, then the
    /// model-call totals, then the raw token counter. Each cache count is already inside
    /// `inputTokens`, so only input + output is summed.
    fn usage_event(&mut self) -> Option<EngineEvent> {
        let (total, raw) = if let Some(usage) = self.result_usage.take() {
            let total = usage_tokens(&usage);
            (total, usage)
        } else if self.saw_turn_usage {
            (self.turn_total, json!({ "tokens": self.turn_total }))
        } else if self.saw_model_usage {
            (self.model_total, json!({ "tokens": self.model_total }))
        } else if self.token_total > 0 {
            (self.token_total, json!({ "tokens": self.token_total }))
        } else {
            return None;
        };
        self.token_total = 0;
        self.turn_total = 0;
        self.model_total = 0;
        self.saw_turn_usage = false;
        self.saw_model_usage = false;
        Some(EngineEvent::Usage(ThreadUsage {
            total_tokens: Some(total),
            context_window: None,
            raw,
        }))
    }

    /// The paths a completed write, edit or delete touched, when they land inside the run's
    /// working folder. A delete is named by its args because its result carries no path.
    fn written_paths(&self, call: &Value) -> Option<Vec<String>> {
        let kind = field(call, "type")?;
        if !matches!(kind.as_str(), "write" | "edit" | "delete") {
            return None;
        }
        if call.pointer("/result/status").and_then(Value::as_str) != Some("success") {
            return None;
        }
        let path = call
            .pointer("/result/value/path")
            .and_then(Value::as_str)
            .filter(|path| !path.is_empty())
            .or_else(|| {
                call.pointer("/args/path")
                    .and_then(Value::as_str)
                    .filter(|path| !path.is_empty())
            })?;
        if self.cwd.as_deref().is_some_and(|cwd| !inside(cwd, path)) {
            return None;
        }
        Some(vec![path.to_owned()])
    }
}
