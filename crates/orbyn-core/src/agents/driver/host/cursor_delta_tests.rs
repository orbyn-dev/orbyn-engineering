//! Cursor's recorded SDK transcripts, replayed through the Cursor mapper: the exact
//! timeline each fixture makes, the text a person sees, and what stays open at the end.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use orbyn_contracts::AgUiEvent;
use serde_json::{Value, json};

use super::CursorMapper;
use crate::agents::driver::EngineEvent;

/// Every `interaction.update` the fixture recorded, in order.
fn fixture(name: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../engine-host/test/fixtures/cursor")
        .join(format!("{name}.ndjson"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|frame| frame.get("type").and_then(Value::as_str) == Some("emit_inbound"))
        .filter_map(|frame| frame.get("frame").cloned())
        .filter(|frame| frame.get("type").and_then(Value::as_str) == Some("interaction.update"))
        .filter_map(|frame| frame.get("update").cloned())
        .collect()
}

fn replay(name: &str) -> Vec<EngineEvent> {
    let mut mapper = CursorMapper::default();
    let mut events = Vec::new();
    for update in fixture(name) {
        events.extend(mapper.map(&update));
    }
    events.extend(mapper.finish());
    assert!(!events.is_empty(), "{name} produced no events at all");
    assert!(
        mapper.finish().is_empty(),
        "a second finish closes nothing new"
    );
    events
}

fn agui(events: &[EngineEvent]) -> Vec<&AgUiEvent> {
    events
        .iter()
        .filter_map(|event| match event {
            EngineEvent::Agui(event) => Some(event),
            _ => None,
        })
        .collect()
}

fn text(events: &[EngineEvent]) -> String {
    agui(events)
        .into_iter()
        .filter_map(|event| match event {
            AgUiEvent::TextMessageContent { delta, .. } => Some(delta.as_str()),
            _ => None,
        })
        .collect()
}

fn reasoning(events: &[EngineEvent]) -> String {
    agui(events)
        .into_iter()
        .filter_map(|event| match event {
            AgUiEvent::ReasoningMessageContent { delta, .. } => Some(delta.as_str()),
            _ => None,
        })
        .collect()
}

/// The text the fixture's own `text-delta` updates say, without the proposed plan.
fn fixture_text(name: &str) -> String {
    fixture(name)
        .iter()
        .filter(|update| update.get("type").and_then(Value::as_str) == Some("text-delta"))
        .filter_map(|update| update.get("text").and_then(Value::as_str))
        .collect()
}

fn id_of(event: &EngineEvent, want: fn(&AgUiEvent) -> Option<&str>) -> Option<String> {
    match event {
        EngineEvent::Agui(event) => want(event).map(str::to_owned),
        _ => None,
    }
}

fn text_start(event: &AgUiEvent) -> Option<&str> {
    match event {
        AgUiEvent::TextMessageStart { message_id, .. } => Some(message_id),
        _ => None,
    }
}

/// A compact, id-normalized view of the event stream: messages collapse to their full text,
/// tool calls to their start/args/end/result sequence, and plans to their steps.
fn trace(events: &[EngineEvent]) -> Vec<String> {
    let mut out = Vec::new();
    let mut text_labels = BTreeMap::new();
    let mut reasoning_labels = BTreeMap::new();
    let mut tool_labels = BTreeMap::new();
    let mut next = (0usize, 0usize, 0usize);
    let mut text_body = BTreeMap::new();
    let mut reasoning_body = BTreeMap::new();
    let mut open_tools = BTreeSet::new();
    for event in events {
        match event {
            EngineEvent::Agui(AgUiEvent::TextMessageStart { message_id, .. }) => {
                let label = label(&mut text_labels, message_id, "T", &mut next.0);
                out.push(format!("text.start:{label}"));
            }
            EngineEvent::Agui(AgUiEvent::TextMessageContent { message_id, delta }) => {
                let label = label(&mut text_labels, message_id, "T", &mut next.0);
                text_body
                    .entry(label)
                    .or_insert_with(String::new)
                    .push_str(delta);
            }
            EngineEvent::Agui(AgUiEvent::TextMessageEnd { message_id }) => {
                let label = label(&mut text_labels, message_id, "T", &mut next.0);
                let body = text_body.remove(&label).unwrap_or_default();
                out.push(format!("text.content:{label}={}", show(&body)));
                out.push(format!("text.end:{label}"));
            }
            EngineEvent::Agui(AgUiEvent::ReasoningMessageStart { message_id, .. }) => {
                let label = label(&mut reasoning_labels, message_id, "R", &mut next.1);
                out.push(format!("reasoning.start:{label}"));
            }
            EngineEvent::Agui(AgUiEvent::ReasoningMessageContent { message_id, delta }) => {
                let label = label(&mut reasoning_labels, message_id, "R", &mut next.1);
                reasoning_body
                    .entry(label)
                    .or_insert_with(String::new)
                    .push_str(delta);
            }
            EngineEvent::Agui(AgUiEvent::ReasoningMessageEnd { message_id }) => {
                let label = label(&mut reasoning_labels, message_id, "R", &mut next.1);
                let body = reasoning_body.remove(&label).unwrap_or_default();
                out.push(format!("reasoning.content:{label}={}", show(&body)));
                out.push(format!("reasoning.end:{label}"));
            }
            EngineEvent::Agui(AgUiEvent::ToolCallStart {
                tool_call_id,
                tool_call_name,
            }) => {
                let known = tool_labels.contains_key(tool_call_id);
                let label = label(&mut tool_labels, tool_call_id, "K", &mut next.2);
                if !known {
                    out.push(format!("tool.start:{label}:{tool_call_name}"));
                }
                open_tools.insert(label);
            }
            EngineEvent::Agui(AgUiEvent::ToolCallArgs { tool_call_id, .. }) => {
                let label = label(&mut tool_labels, tool_call_id, "K", &mut next.2);
                out.push(format!("tool.args:{label}"));
            }
            EngineEvent::Agui(AgUiEvent::ToolCallEnd { tool_call_id }) => {
                let label = label(&mut tool_labels, tool_call_id, "K", &mut next.2);
                out.push(format!("tool.end:{label}"));
                open_tools.remove(&label);
            }
            EngineEvent::Agui(AgUiEvent::ToolCallResult { tool_call_id, .. }) => {
                let label = label(&mut tool_labels, tool_call_id, "K", &mut next.2);
                out.push(format!("tool.result:{label}"));
            }
            EngineEvent::Agui(AgUiEvent::ActivitySnapshot { content, .. }) => {
                let steps = content
                    .get("steps")
                    .and_then(Value::as_array)
                    .map(|steps| {
                        steps
                            .iter()
                            .map(|step| {
                                format!(
                                    "{}={}",
                                    step.get("title").and_then(Value::as_str).unwrap_or(""),
                                    step.get("status").and_then(Value::as_str).unwrap_or("")
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(",")
                    })
                    .unwrap_or_default();
                out.push(format!("plan:{steps}"));
            }
            EngineEvent::Agui(AgUiEvent::Custom { name, value }) => match name.as_str() {
                "tool.outcome" => {
                    let id = value
                        .get("toolCallId")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let label = label(&mut tool_labels, id, "K", &mut next.2);
                    out.push(format!(
                        "tool.outcome:{label}:{}",
                        value
                            .get("outcome")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                    ));
                }
                "subagent.activity" => {
                    let parent = value
                        .get("parentToolCallId")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let label = label(&mut tool_labels, parent, "K", &mut next.2);
                    out.push(format!(
                        "subagent.activity:{label}:{}:{}",
                        value
                            .get("kind")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                        value
                            .get("tool")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                    ));
                }
                other => out.push(format!("custom:{other}")),
            },
            EngineEvent::Usage(usage) => {
                out.push(format!("usage:{}", usage.total_tokens.unwrap_or(0)));
            }
            _ => out.push("engine".into()),
        }
    }
    out
}

fn label(
    labels: &mut BTreeMap<String, String>,
    id: &str,
    prefix: &str,
    next: &mut usize,
) -> String {
    if let Some(label) = labels.get(id) {
        return label.clone();
    }
    *next += 1;
    let label = format!("{prefix}{next}");
    labels.insert(id.to_owned(), label.clone());
    label
}

fn show(text: &str) -> String {
    if text.chars().count() <= 80 {
        serde_json::to_string(text).unwrap_or_default()
    } else {
        format!("<{} chars>", text.chars().count())
    }
}

/// One text/reasoning message is one start, any content, one end, and one id throughout.
/// Every tool call that started ended, and nothing is left open after `finish()`.
fn assert_balanced(events: &[EngineEvent]) {
    let mut text = BTreeSet::new();
    let mut reasoning = BTreeSet::new();
    for event in events {
        match event {
            EngineEvent::Agui(AgUiEvent::TextMessageStart { message_id, .. }) => {
                assert!(
                    text.insert(message_id.clone()),
                    "text started twice: {message_id}"
                );
            }
            EngineEvent::Agui(AgUiEvent::TextMessageContent { message_id, .. }) => {
                assert!(
                    text.contains(message_id),
                    "content without start: {message_id}"
                );
            }
            EngineEvent::Agui(AgUiEvent::TextMessageEnd { message_id }) => {
                assert!(text.remove(message_id), "end without start: {message_id}");
            }
            EngineEvent::Agui(AgUiEvent::ReasoningMessageStart { message_id, .. }) => {
                assert!(
                    reasoning.insert(message_id.clone()),
                    "reasoning started twice: {message_id}"
                );
            }
            EngineEvent::Agui(AgUiEvent::ReasoningMessageContent { message_id, .. }) => {
                assert!(
                    reasoning.contains(message_id),
                    "reasoning content without start: {message_id}"
                );
            }
            EngineEvent::Agui(AgUiEvent::ReasoningMessageEnd { message_id }) => {
                assert!(
                    reasoning.remove(message_id),
                    "reasoning end without start: {message_id}"
                );
            }
            _ => {}
        }
    }
    assert!(text.is_empty(), "text left open: {text:?}");
    assert!(reasoning.is_empty(), "reasoning left open: {reasoning:?}");

    let starts: BTreeSet<String> = events
        .iter()
        .filter_map(|event| {
            id_of(event, |event| match event {
                AgUiEvent::ToolCallStart { tool_call_id, .. } => Some(tool_call_id.as_str()),
                _ => None,
            })
        })
        .collect();
    let ends: BTreeSet<String> = events
        .iter()
        .filter_map(|event| {
            id_of(event, |event| match event {
                AgUiEvent::ToolCallEnd { tool_call_id } => Some(tool_call_id.as_str()),
                _ => None,
            })
        })
        .collect();
    assert_eq!(
        starts, ends,
        "a started tool call did not end, or ended twice"
    );
}

#[test]
fn simple_replays_to_reasoning_then_text_then_usage() {
    let events = replay("simple");
    assert_eq!(
        trace(&events),
        [
            "reasoning.start:R1",
            "reasoning.content:R1=\"I will respond with the exact text \\\"fixture simple ok\\\".\"",
            "reasoning.end:R1",
            "text.start:T1",
            "text.content:T1=\"fixture simple ok\"",
            "text.end:T1",
            "usage:17993",
        ]
    );
    assert_eq!(text(&events), "fixture simple ok");
    assert_eq!(
        reasoning(&events),
        "I will respond with the exact text \"fixture simple ok\"."
    );
    assert_balanced(&events);
}

#[test]
fn tool_call_read_only_replays_two_reads_and_both_text_segments() {
    let events = replay("tool_call_read_only");
    assert_eq!(
        trace(&events),
        [
            "reasoning.start:R1",
            "reasoning.content:R1=<93 chars>",
            "reasoning.end:R1",
            "text.start:T1",
            "text.content:T1=\"Reading both files now.\\n\\n\\n\"",
            "text.end:T1",
            "tool.start:K1:read",
            "tool.args:K1",
            "tool.start:K2:read",
            "tool.args:K2",
            "tool.end:K1",
            "tool.outcome:K1:success",
            "tool.result:K1",
            "tool.end:K2",
            "tool.outcome:K2:success",
            "tool.result:K2",
            "reasoning.start:R2",
            "reasoning.content:R2=<95 chars>",
            "reasoning.end:R2",
            "text.start:T2",
            "text.content:T2=\"read only tool fixture complete\"",
            "text.end:T2",
            "usage:32843",
        ]
    );
    assert_eq!(
        text(&events),
        "Reading both files now.\n\n\nread only tool fixture complete"
    );
    // Two text segments, each with one id shared by its deltas.
    let text_ids: Vec<String> = events
        .iter()
        .filter_map(|event| {
            id_of(event, |event| match event {
                AgUiEvent::TextMessageStart { message_id, .. } => Some(message_id.as_str()),
                _ => None,
            })
        })
        .collect();
    assert_eq!(text_ids.len(), 2);
    assert_ne!(text_ids[0], text_ids[1]);
    let results: Vec<String> = events
        .iter()
        .filter_map(|event| match event {
            EngineEvent::Agui(AgUiEvent::ToolCallResult { content, .. }) => Some(content.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(results.len(), 2);
    assert!(results[0].contains("cursor-read-only-fixture"));
    assert!(results[1].contains("compilerOptions"));
    assert_balanced(&events);
}

#[test]
fn todo_list_replays_the_same_plan_activity_as_the_other_drivers() {
    let events = replay("todo_list");
    assert_eq!(
        trace(&events),
        [
            "reasoning.start:R1",
            "reasoning.content:R1=<221 chars>",
            "reasoning.end:R1",
            "text.start:T1",
            "text.content:T1=\"Checking for the `update_plan` tool, then reading the config files.\\n\"",
            "text.end:T1",
            "tool.start:K1:read",
            "tool.args:K1",
            "tool.start:K2:read",
            "tool.args:K2",
            "tool.end:K1",
            "tool.outcome:K1:success",
            "tool.result:K1",
            "tool.end:K2",
            "tool.outcome:K2:success",
            "tool.result:K2",
            "reasoning.start:R2",
            "reasoning.content:R2=<141 chars>",
            "reasoning.end:R2",
            "tool.start:K3:grep",
            "tool.args:K3",
            "tool.end:K3",
            "tool.outcome:K3:success",
            "tool.result:K3",
            "reasoning.start:R3",
            "reasoning.content:R3=<240 chars>",
            "reasoning.end:R3",
            "tool.start:K4:glob",
            "tool.args:K4",
            "tool.end:K4",
            "tool.outcome:K4:success",
            "tool.result:K4",
            "reasoning.start:R4",
            "reasoning.content:R4=<205 chars>",
            "reasoning.end:R4",
            "tool.start:K5:read",
            "tool.args:K5",
            "tool.end:K5",
            "tool.outcome:K5:success",
            "tool.result:K5",
            "reasoning.start:R5",
            "reasoning.content:R5=<327 chars>",
            "reasoning.end:R5",
            "text.start:T2",
            "text.content:T2=<94 chars>",
            "text.end:T2",
            "plan:inspect package.json=completed",
            "plan:inspect package.json=completed,inspect tsconfig.json=completed",
            "plan:inspect package.json=completed,inspect tsconfig.json=completed,report completion=completed",
            "tool.start:K6:updateTodos",
            "tool.args:K6",
            "tool.end:K6",
            "tool.outcome:K6:success",
            "tool.result:K6",
            "reasoning.start:R6",
            "reasoning.content:R6=\"I will answer exactly: \\\"todo list fixture complete\\\".\"",
            "reasoning.end:R6",
            "text.start:T3",
            "text.content:T3=\"todo list fixture complete\"",
            "text.end:T3",
            "usage:156708",
        ]
    );
    let plans: Vec<&AgUiEvent> = agui(&events)
        .into_iter()
        .filter(|event| matches!(event, AgUiEvent::ActivitySnapshot { .. }))
        .collect();
    assert_eq!(plans.len(), 3);
    let AgUiEvent::ActivitySnapshot {
        message_id,
        activity_type,
        content,
        replace,
    } = plans[2]
    else {
        panic!("not a plan");
    };
    assert_eq!(message_id, "plan");
    assert_eq!(activity_type, "plan");
    assert_eq!(replace, &Some(true));
    assert_eq!(
        content,
        &json!({"steps": [
            {"title": "inspect package.json", "status": "completed"},
            {"title": "inspect tsconfig.json", "status": "completed"},
            {"title": "report completion", "status": "completed"},
        ]})
    );
    assert!(text(&events).ends_with("todo list fixture complete"));
    assert_balanced(&events);
}

#[test]
fn subagent_replays_as_a_task_tool_call_and_keeps_inner_text_out() {
    let events = replay("subagent");
    assert_eq!(
        trace(&events),
        [
            "reasoning.start:R1",
            "reasoning.content:R1=\"Spawning two subagents in parallel to read package.json and tsconfig.json.\"",
            "reasoning.end:R1",
            "text.start:T1",
            "text.content:T1=\"Spawning two subagents in parallel to read `package.json` and `tsconfig.json`.\\n\"",
            "text.end:T1",
            "tool.start:K1:task",
            "tool.args:K1",
            "tool.start:K2:task",
            "tool.args:K2",
            "subagent.activity:K1:tool-call-started:read",
            "subagent.activity:K1:tool-call-completed:read",
            "subagent.activity:K2:tool-call-started:read",
            "subagent.activity:K2:tool-call-completed:read",
            "tool.end:K1",
            "tool.outcome:K1:success",
            "tool.result:K1",
            "tool.end:K2",
            "tool.outcome:K2:success",
            "tool.result:K2",
            "reasoning.start:R2",
            "reasoning.content:R2=\"Both subagents finished successfully.\"",
            "reasoning.end:R2",
            "text.start:T2",
            "text.content:T2=<669 chars>",
            "text.end:T2",
            "usage:36097",
        ]
    );
    assert_eq!(text(&events), fixture_text("subagent"));
    assert!(
        !text(&events).contains("Reading `package.json` for its contents"),
        "subagent text leaked into the parent turn"
    );
    let results: Vec<&str> = agui(&events)
        .into_iter()
        .filter_map(|event| match event {
            AgUiEvent::ToolCallResult { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(results.len(), 2);
    assert!(results[0].contains("cursor-read-only-fixture"));
    assert!(results[1].contains("compilerOptions"));
    assert_balanced(&events);
}

#[test]
fn proposed_plan_replays_one_proposed_plan_message_and_its_tool_call() {
    let events = replay("proposed_plan");
    assert_eq!(
        trace(&events),
        [
            "text.start:T1",
            "text.content:T1=<92 chars>",
            "text.end:T1",
            "tool.start:K1:task",
            "tool.args:K1",
            "tool.start:K2:glob",
            "tool.args:K2",
            "tool.start:K3:grep",
            "tool.args:K3",
            "tool.end:K3",
            "tool.outcome:K3:success",
            "tool.result:K3",
            "tool.end:K2",
            "tool.outcome:K2:success",
            "tool.result:K2",
            "tool.end:K1",
            "tool.outcome:K1:success",
            "tool.result:K1",
            "reasoning.start:R1",
            "reasoning.content:R1=<106 chars>",
            "reasoning.end:R1",
            "text.start:T2",
            "text.content:T2=\"Checking agent transcripts for prior context on this task.\\n\"",
            "text.end:T2",
            "tool.start:K4:glob",
            "tool.args:K4",
            "tool.start:K5:read",
            "tool.args:K5",
            "tool.end:K5",
            "tool.outcome:K5:success",
            "tool.result:K5",
            "reasoning.start:R2",
            "reasoning.content:R2=<160 chars>",
            "reasoning.end:R2",
            "text.start:T3",
            "text.content:T3=<128 chars>",
            "text.end:T3",
            "text.start:T4",
            "text.content:T4=<4934 chars>",
            "text.end:T4",
            "tool.start:K6:createPlan",
            "tool.args:K6",
            "tool.end:K6",
            "tool.outcome:K6:success",
            "tool.result:K6",
            "usage:73658",
            "tool.end:K4",
        ]
    );
    let visible = text(&events);
    assert!(visible.starts_with(&fixture_text("proposed_plan")));
    let recorded = fixture("proposed_plan");
    let plan = recorded
        .iter()
        .filter(|update| {
            update
                .get("toolCall")
                .and_then(|call| call.get("type"))
                .and_then(Value::as_str)
                == Some("createPlan")
        })
        .filter_map(|update| {
            update
                .pointer("/toolCall/args/plan")
                .and_then(Value::as_str)
        })
        .max_by_key(|plan| plan.len())
        .expect("the fixture records a plan");
    assert_eq!(visible.matches("<proposed_plan>").count(), 1);
    assert!(visible.contains(plan.trim()), "the whole plan is visible");
    assert_balanced(&events);
}

#[test]
fn turn_interrupt_mid_tool_closes_the_shell_call_at_finish() {
    let events = replay("turn_interrupt_mid_tool");
    assert_eq!(
        trace(&events),
        [
            "reasoning.start:R1",
            "reasoning.content:R1=<86 chars>",
            "reasoning.end:R1",
            "text.start:T1",
            "text.content:T1=\"Running the command and waiting for it to finish.\\n\"",
            "text.end:T1",
            "tool.start:K1:shell",
            "tool.args:K1",
            "tool.end:K1",
            "usage:79",
        ]
    );
    assert_eq!(
        text(&events),
        "Running the command and waiting for it to finish.\n"
    );
    assert_balanced(&events);
}

#[test]
fn token_deltas_count_and_unknown_updates_do_nothing() {
    let mut mapper = CursorMapper::default();
    assert!(
        mapper
            .map(&json!({"type": "token-delta", "tokens": 7}))
            .is_empty()
    );
    for update in [
        json!({"type": "no-such-update", "text": "never shown"}),
        json!({"type": "text-delta"}),
        json!({"type": "thinking-delta"}),
        json!({"type": "tool-call-started"}),
        json!({"type": "tool-call-delta"}),
        json!({"type": "partial-tool-call"}),
        json!({}),
        json!("not an object"),
    ] {
        assert!(mapper.map(&update).is_empty(), "{update}");
    }
    assert!(
        mapper
            .map(&json!({"type": "summary-completed", "failed": false}))
            .is_empty()
    );
    let events = mapper.finish();
    assert_eq!(
        events
            .iter()
            .filter_map(|event| match event {
                EngineEvent::Usage(usage) => usage.total_tokens,
                _ => None,
            })
            .collect::<Vec<_>>(),
        [7]
    );
    assert!(
        !events.iter().any(|event| matches!(
            event,
            EngineEvent::Agui(AgUiEvent::TextMessageContent { .. })
        )),
        "token deltas and unknown updates say nothing"
    );
}

#[test]
fn long_tool_results_are_capped_and_never_show_raw_json() {
    let mut mapper = CursorMapper::default();
    let long = "x".repeat(40_000);
    let mut events = mapper.map(&json!({
        "type": "tool-call-started",
        "callId": "call-1",
        "toolCall": {"type": "read", "args": {"path": "a.txt"}},
    }));
    events.extend(mapper.map(&json!({
        "type": "tool-call-completed",
        "callId": "call-1",
        "toolCall": {
            "type": "read",
            "args": {"path": "a.txt"},
            "result": {"status": "success", "value": {"content": long}},
        },
    })));
    let content = events
        .iter()
        .find_map(|event| match event {
            EngineEvent::Agui(AgUiEvent::ToolCallResult { content, .. }) => Some(content),
            _ => None,
        })
        .expect("the tool result");
    assert_eq!(content.chars().count(), super::MAX_RESULT_CHARS);
    assert!(!content.contains("workspaceResults"));
    assert_balanced(&events);
}

#[test]
fn every_started_id_is_stable_and_every_message_id_pairs_once() {
    for name in [
        "simple",
        "tool_call_read_only",
        "todo_list",
        "subagent",
        "proposed_plan",
        "turn_interrupt_mid_tool",
    ] {
        let events = replay(name);
        let starts: Vec<String> = events
            .iter()
            .filter_map(|event| id_of(event, text_start))
            .collect();
        let ends: Vec<String> = events
            .iter()
            .filter_map(|event| {
                id_of(event, |event| match event {
                    AgUiEvent::TextMessageEnd { message_id } => Some(message_id.as_str()),
                    _ => None,
                })
            })
            .collect();
        assert_eq!(starts, ends, "{name}: text starts and ends pair in order");
        assert!(!starts.is_empty(), "{name}: the fixture has visible text");
        assert!(
            starts.iter().all(|id| !id.is_empty()),
            "{name}: no empty message id"
        );
        assert_balanced(&events);
    }
}
