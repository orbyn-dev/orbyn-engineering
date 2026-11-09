use std::collections::{HashMap, HashSet};

use orbyn_contracts::{AgUiEvent, ThreadUsage};
use serde_json::{Value, json};

use super::plan::{
    TaskStep, created_from_text, task_content, task_result, task_status, todo_content,
};
use super::workflows::Workflows;
use super::{NO_SESSION, init_tools};
use crate::agents::driver::{EngineEvent, EngineOutcome, ItemRecord, proposed_plan_events};
use crate::util::new_id;

#[derive(Clone)]
pub(super) enum Block {
    Text(String),
    Reasoning(String),
    Tool(String),
}

/// Maps Claude stream-json frames to events. Text and tool input come from the
/// `stream_event` partials; `assistant` frames only fill in when partials were absent.
#[derive(Default)]
pub(super) struct StreamMapper {
    pub(super) blocks: Vec<(u64, Block)>,
    pub(super) saw_partials: bool,
    pub(super) outcome: Option<EngineOutcome>,
    /// Plan tools already shown, since Claude can repeat a block across frames.
    pub(super) plans_seen: HashSet<String>,
    /// Files each open write tool named, until its result arrives.
    pub(super) writing: HashMap<String, Vec<String>>,
    /// Claude's modern task list, kept in the order it was reported.
    pub(super) tasks: Vec<TaskStep>,
    /// Task tool calls held until their result reports the new state.
    pub(super) task_calls: HashMap<String, (String, Value)>,
    /// Background workflows this run launched that still run.
    pub(super) workflows: Workflows,
    /// A finished turn's outcome, held while a workflow it launched still runs.
    pub(super) held: Option<EngineOutcome>,
    /// A follow-up turn started after the outcome was held.
    pub(super) resumed: bool,
}

impl StreamMapper {
    /// The turn is done and its workflows have ended, and no follow-up turn has
    /// started yet.
    pub(super) fn waiting_on_hold(&self) -> bool {
        self.held.is_some() && self.workflows.is_empty() && !self.resumed
    }

    pub(super) fn on_frame(&mut self, frame: &Value) -> Vec<EngineEvent> {
        // Subagent traffic carries a parent; the run shows its top-level turn.
        let nested = frame
            .get("parent_tool_use_id")
            .is_some_and(|parent| !parent.is_null());
        let kind = frame.get("type").and_then(Value::as_str);
        if self.held.is_some()
            && !nested
            && (matches!(kind, Some("assistant" | "stream_event"))
                || (kind == Some("system")
                    && frame.get("subtype").and_then(Value::as_str) == Some("init")))
        {
            self.resumed = true;
        }
        match kind {
            Some("control_response") => Workflows::control_response(frame),
            Some("system")
                if matches!(
                    frame.get("subtype").and_then(Value::as_str),
                    Some("task_notification" | "task_updated")
                ) =>
            {
                self.workflows.system(frame)
            }
            Some("system") if frame.get("subtype").and_then(Value::as_str) == Some("init") => {
                let mut events = session(frame);
                events.extend(init_commands(frame));
                events.extend(init_tools(frame));
                events
            }
            Some("stream_event") if !nested => frame
                .get("event")
                .map_or_else(Vec::new, |event| self.on_stream_event(event)),
            Some("assistant") if !nested => self.on_assistant(frame),
            Some("user") if !nested => {
                let mut events = self.writes_finished(frame);
                events.extend(self.tasks_finished(frame));
                events.extend(on_user(frame));
                events.extend(self.workflows.launched(frame));
                events
            }
            Some("assistant") => vec![
                AgUiEvent::Custom {
                    name: "subagent.activity".into(),
                    value: json!({ "parentToolCallId": frame.get("parent_tool_use_id") }),
                }
                .into(),
            ],
            Some("rate_limit_event") => rejected_limit(frame).into_iter().collect(),
            Some("system")
                if frame.get("subtype").and_then(Value::as_str) == Some("compact_boundary") =>
            {
                vec![
                    AgUiEvent::Custom {
                        name: "context.compacted".into(),
                        value: Value::Null,
                    }
                    .into(),
                ]
            }
            Some("result") => {
                let outcome = result_outcome(frame);
                // The CLI keeps a workflow running past the turn that launched it, and
                // closing its input would end the workflow with it.
                if outcome == EngineOutcome::Completed && !self.workflows.is_empty() {
                    self.held = Some(outcome);
                    self.resumed = false;
                } else {
                    self.held = None;
                    self.outcome = Some(outcome);
                }
                let mut events = session(frame);
                events.extend(frame.get("usage").map(usage_event));
                events
            }
            _ => Vec::new(),
        }
    }

    fn on_stream_event(&mut self, event: &Value) -> Vec<EngineEvent> {
        self.saw_partials = true;
        let index = event.get("index").and_then(Value::as_u64).unwrap_or(0);
        match event.get("type").and_then(Value::as_str) {
            Some("content_block_start") => {
                let Some(block) = event.get("content_block") else {
                    return Vec::new();
                };
                self.open_block(index, block)
            }
            Some("content_block_delta") => {
                let Some(open) = self.block(index) else {
                    return Vec::new();
                };
                let delta = event.get("delta").cloned().unwrap_or(Value::Null);
                match (&open, delta.get("type").and_then(Value::as_str)) {
                    (Block::Text(id), Some("text_delta")) => text_of(&delta, "text")
                        .map(|text| AgUiEvent::TextMessageContent {
                            message_id: id.clone(),
                            delta: text,
                        })
                        .into_iter()
                        .map(Into::into)
                        .collect(),
                    (Block::Reasoning(id), Some("thinking_delta")) => text_of(&delta, "thinking")
                        .map(|text| AgUiEvent::ReasoningMessageContent {
                            message_id: id.clone(),
                            delta: text,
                        })
                        .into_iter()
                        .map(Into::into)
                        .collect(),
                    (Block::Tool(id), Some("input_json_delta")) => text_of(&delta, "partial_json")
                        .map(|text| AgUiEvent::ToolCallArgs {
                            tool_call_id: id.clone(),
                            delta: text,
                        })
                        .into_iter()
                        .map(Into::into)
                        .collect(),
                    _ => Vec::new(),
                }
            }
            Some("content_block_stop") => {
                let position = self.blocks.iter().position(|(open, _)| *open == index);
                position
                    .map(|position| vec![end_event(&self.blocks.remove(position).1).into()])
                    .unwrap_or_default()
            }
            _ => Vec::new(),
        }
    }

    fn block(&self, index: u64) -> Option<Block> {
        self.blocks
            .iter()
            .find(|(open, _)| *open == index)
            .map(|(_, block)| block.clone())
    }

    fn open_block(&mut self, index: u64, block: &Value) -> Vec<EngineEvent> {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                let id = new_id();
                self.blocks.push((index, Block::Text(id.clone())));
                vec![
                    AgUiEvent::TextMessageStart {
                        message_id: id,
                        role: "assistant".into(),
                    }
                    .into(),
                ]
            }
            Some("thinking") => {
                let id = new_id();
                self.blocks.push((index, Block::Reasoning(id.clone())));
                vec![
                    AgUiEvent::ReasoningMessageStart {
                        message_id: id,
                        role: "reasoning".into(),
                    }
                    .into(),
                ]
            }
            Some("tool_use") => {
                let id = text_of(block, "id").unwrap_or_else(new_id);
                self.blocks.push((index, Block::Tool(id.clone())));
                vec![
                    AgUiEvent::ToolCallStart {
                        tool_call_id: id,
                        tool_call_name: text_of(block, "name").unwrap_or_else(|| "tool".into()),
                    }
                    .into(),
                ]
            }
            _ => Vec::new(),
        }
    }

    fn on_assistant(&mut self, frame: &Value) -> Vec<EngineEvent> {
        let content = frame
            .pointer("/message/content")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut events: Vec<EngineEvent> = content
            .iter()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_use"))
            .filter_map(|block| {
                Some(EngineEvent::Item(ItemRecord {
                    id: text_of(block, "id")?,
                    kind: "tool_use".to_owned(),
                    raw: block.clone(),
                    exit_code: None,
                }))
            })
            .collect();
        for block in &content {
            events.extend(self.plan_events(block));
            self.write_started(block);
            self.task_started(block);
        }
        if self.saw_partials {
            return events;
        }
        for block in content {
            match block.get("type").and_then(Value::as_str) {
                Some("text") => {
                    let id = new_id();
                    events.push(
                        AgUiEvent::TextMessageStart {
                            message_id: id.clone(),
                            role: "assistant".into(),
                        }
                        .into(),
                    );
                    events.push(
                        AgUiEvent::TextMessageContent {
                            message_id: id.clone(),
                            delta: text_of(&block, "text").unwrap_or_default(),
                        }
                        .into(),
                    );
                    events.push(AgUiEvent::TextMessageEnd { message_id: id }.into());
                }
                Some("tool_use") => {
                    let id = text_of(&block, "id").unwrap_or_else(new_id);
                    events.push(
                        AgUiEvent::ToolCallStart {
                            tool_call_id: id.clone(),
                            tool_call_name: text_of(&block, "name")
                                .unwrap_or_else(|| "tool".into()),
                        }
                        .into(),
                    );
                    events.push(
                        AgUiEvent::ToolCallArgs {
                            tool_call_id: id.clone(),
                            delta: block.get("input").map(Value::to_string).unwrap_or_default(),
                        }
                        .into(),
                    );
                    events.push(AgUiEvent::ToolCallEnd { tool_call_id: id }.into());
                }
                _ => {}
            }
        }
        events
    }

    /// Remembers what a write tool names until its result says it ran.
    fn write_started(&mut self, block: &Value) {
        if block.get("type").and_then(Value::as_str) != Some("tool_use") {
            return;
        }
        let paths = crate::conversation::checkpoints::claude_write_paths(block);
        if let (false, Some(id)) = (paths.is_empty(), text_of(block, "id")) {
            self.writing.insert(id, paths);
        }
    }

    fn writes_finished(&mut self, frame: &Value) -> Vec<EngineEvent> {
        let empty = Vec::new();
        frame
            .pointer("/message/content")
            .and_then(Value::as_array)
            .unwrap_or(&empty)
            .iter()
            .filter_map(|block| self.writing.remove(block.get("tool_use_id")?.as_str()?))
            .map(EngineEvent::Files)
            .collect()
    }

    /// `TodoWrite` is the plan as a checklist and `ExitPlanMode` carries a proposed
    /// plan. Their input is only whole in the `assistant` frame, which is why this reads
    /// them there and not from the partials.
    fn plan_events(&mut self, block: &Value) -> Vec<EngineEvent> {
        if block.get("type").and_then(Value::as_str) != Some("tool_use") {
            return Vec::new();
        }
        let Some(id) = text_of(block, "id") else {
            return Vec::new();
        };
        let name = block.get("name").and_then(Value::as_str);
        let input = block.get("input").cloned().unwrap_or(Value::Null);
        let events = match name {
            Some("TodoWrite") => vec![
                AgUiEvent::ActivitySnapshot {
                    message_id: "plan".into(),
                    activity_type: "plan".into(),
                    content: todo_content(&input),
                    replace: Some(true),
                }
                .into(),
            ],
            Some("ExitPlanMode") => text_of(&input, "plan")
                .map(|plan| proposed_plan_events(&id, &plan))
                .unwrap_or_default(),
            _ => return Vec::new(),
        };
        if self.plans_seen.insert(id) {
            events
        } else {
            Vec::new()
        }
    }

    /// Claude's modern task tools only report their state in the result, so the
    /// call is held until it arrives.
    fn task_started(&mut self, block: &Value) {
        if block.get("type").and_then(Value::as_str) != Some("tool_use") {
            return;
        }
        let Some(name) = block.get("name").and_then(Value::as_str) else {
            return;
        };
        if !matches!(name, "TaskCreate" | "TaskUpdate" | "TaskList") {
            return;
        }
        if let Some(id) = text_of(block, "id") {
            self.task_calls.insert(
                id,
                (
                    name.to_owned(),
                    block.get("input").cloned().unwrap_or(Value::Null),
                ),
            );
        }
    }

    fn tasks_finished(&mut self, frame: &Value) -> Vec<EngineEvent> {
        let Some(content) = frame.pointer("/message/content").and_then(Value::as_array) else {
            return Vec::new();
        };
        let mut changed = false;
        for block in content {
            if block.get("type").and_then(Value::as_str) != Some("tool_result") {
                continue;
            }
            let Some(id) = text_of(block, "tool_use_id") else {
                continue;
            };
            let Some((name, input)) = self.task_calls.remove(&id) else {
                continue;
            };
            if block.get("is_error").and_then(Value::as_bool) == Some(true) {
                continue;
            }
            let result = task_result(frame, block);
            changed |= self.apply_task(&name, &input, &result);
        }
        if !changed {
            return Vec::new();
        }
        vec![
            AgUiEvent::ActivitySnapshot {
                message_id: "plan".into(),
                activity_type: "plan".into(),
                content: task_content(&self.tasks),
                replace: Some(true),
            }
            .into(),
        ]
    }

    fn apply_task(&mut self, name: &str, input: &Value, result: &Value) -> bool {
        match name {
            "TaskList" => {
                let Some(listed) = result.get("tasks").and_then(Value::as_array) else {
                    return false;
                };
                let mut next = Vec::new();
                for task in listed {
                    let Some(id) = text_of(task, "id") else {
                        continue;
                    };
                    next.push(TaskStep {
                        id,
                        title: text_of(task, "subject").unwrap_or_default(),
                        status: task_status(text_of(task, "status").as_deref())
                            .unwrap_or("pending")
                            .to_owned(),
                    });
                }
                self.tasks = next;
                true
            }
            "TaskCreate" => {
                let created = result.get("task");
                let from_text = created_from_text(result);
                let id = created
                    .and_then(|task| text_of(task, "id"))
                    .or_else(|| from_text.as_ref().map(|(id, _)| id.clone()));
                let title = created
                    .and_then(|task| text_of(task, "subject"))
                    .or_else(|| from_text.map(|(_, subject)| subject))
                    .or_else(|| text_of(input, "subject"));
                let (Some(id), Some(title)) = (id, title) else {
                    return false;
                };
                match self.tasks.iter_mut().find(|task| task.id == id) {
                    Some(task) => task.title = title,
                    None => self.tasks.push(TaskStep {
                        id,
                        title,
                        status: "pending".into(),
                    }),
                }
                true
            }
            "TaskUpdate" => {
                if result.get("success").and_then(Value::as_bool) == Some(false) {
                    return false;
                }
                if let Some(text) = result.as_str()
                    && !text.starts_with("Updated task #")
                {
                    return false;
                }
                let id = text_of(input, "taskId").or_else(|| text_of(result, "taskId"));
                let Some(index) =
                    id.and_then(|id| self.tasks.iter().position(|task| task.id == id))
                else {
                    return false;
                };
                let native = text_of(input, "status");
                if native.as_deref() == Some("deleted") {
                    self.tasks.remove(index);
                    return true;
                }
                let title =
                    text_of(input, "subject").unwrap_or_else(|| self.tasks[index].title.clone());
                let status = native
                    .as_deref()
                    .and_then(|value| task_status(Some(value)))
                    .map(str::to_owned)
                    .unwrap_or_else(|| self.tasks[index].status.clone());
                let changed =
                    self.tasks[index].title != title || self.tasks[index].status != status;
                self.tasks[index].title = title;
                self.tasks[index].status = status;
                changed
            }
            _ => false,
        }
    }

    pub(super) fn close_open_blocks(&mut self) -> Vec<EngineEvent> {
        self.blocks
            .drain(..)
            .map(|(_, block)| end_event(&block).into())
            .collect()
    }
}

fn end_event(block: &Block) -> AgUiEvent {
    match block {
        Block::Text(id) => AgUiEvent::TextMessageEnd {
            message_id: id.clone(),
        },
        Block::Reasoning(id) => AgUiEvent::ReasoningMessageEnd {
            message_id: id.clone(),
        },
        Block::Tool(id) => AgUiEvent::ToolCallEnd {
            tool_call_id: id.clone(),
        },
    }
}

fn usage_event(usage: &Value) -> EngineEvent {
    let count = |key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0);
    let total = count("input_tokens")
        + count("output_tokens")
        + count("cache_read_input_tokens")
        + count("cache_creation_input_tokens");
    EngineEvent::Usage(ThreadUsage {
        total_tokens: Some(total),
        context_window: None,
        raw: usage.clone(),
    })
}

pub(super) fn session(frame: &Value) -> Vec<EngineEvent> {
    text_of(frame, "session_id")
        .map(EngineEvent::Session)
        .into_iter()
        .collect()
}

/// The init frame lists names alone.
fn init_commands(frame: &Value) -> Option<EngineEvent> {
    let names = frame.get("slash_commands")?.as_array()?;
    Some(EngineEvent::Commands(
        names
            .iter()
            .filter_map(Value::as_str)
            .map(|name| (name.to_owned(), String::new()))
            .collect(),
    ))
}

pub(super) fn text_of(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

pub(super) fn on_user(frame: &Value) -> Vec<EngineEvent> {
    let Some(content) = frame.pointer("/message/content").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut events = Vec::new();
    for block in content
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_result"))
    {
        let Some(id) = text_of(block, "tool_use_id") else {
            continue;
        };
        events.push(
            AgUiEvent::ToolCallResult {
                message_id: new_id(),
                tool_call_id: id.clone(),
                content: tool_text(block.get("content")),
                role: Some("tool".into()),
            }
            .into(),
        );
        let outcome = match block.get("is_error") {
            None | Some(Value::Bool(false)) => Some("success"),
            Some(Value::Bool(true)) => Some("failure"),
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
    }
    events
}

fn tool_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

/// Claude reports limits as it goes; only a rejected one stops the turn.
pub(super) fn rejected_limit(frame: &Value) -> Option<EngineEvent> {
    let info = frame.get("rate_limit_info")?;
    (info.get("status").and_then(Value::as_str) == Some("rejected")).then(|| EngineEvent::Limited {
        resets_at: info.get("resetsAt").and_then(Value::as_i64),
    })
}

fn result_outcome(frame: &Value) -> EngineOutcome {
    let failed = frame.get("is_error").and_then(Value::as_bool) == Some(true)
        || frame
            .get("subtype")
            .and_then(Value::as_str)
            .is_some_and(|subtype| subtype != "success");
    if !failed {
        return EngineOutcome::Completed;
    }
    let lost = |text: &str| text.contains(NO_SESSION);
    let errors = frame.get("errors").and_then(Value::as_array).map(|errors| {
        errors
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join("; ")
    });
    let message = errors
        .filter(|text| !text.is_empty())
        .or_else(|| text_of(frame, "result").filter(|text| !text.is_empty()))
        .or_else(|| text_of(frame, "subtype"))
        .unwrap_or_else(|| "Claude ended the turn with an error.".to_owned());
    if lost(&message) {
        return EngineOutcome::SessionLost;
    }
    EngineOutcome::Failed(message)
}
