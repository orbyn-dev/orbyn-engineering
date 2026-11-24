//! Cursor's recorded SDK transcripts, replayed through the Cursor mapper: the exact
//! timeline each fixture makes, the text a person sees, and what stays open at the end.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use orbyn_contracts::AgUiEvent;
use serde_json::{Value, json};

use super::CursorMapper;
use crate::agents::driver::EngineEvent;

/// Every `interaction.update` the fixture recorded, in order. A line that does not parse
/// fails the test: a truncated or malformed recording must never silently shrink a replay.
/// The real reader treats an unterminated final line as end of file, so a fixture without
/// its final newline is truncated too.
fn parse_updates(text: &str) -> Vec<Value> {
    if !text.is_empty() && !text.ends_with('\n') {
        panic!("fixture ends in an unterminated final line");
    }
    text.lines()
        .enumerate()
        .map(|(index, line)| {
            serde_json::from_str::<Value>(line)
                .unwrap_or_else(|error| panic!("fixture line {}: {error}", index + 1))
        })
        .filter(|frame| frame.get("type").and_then(Value::as_str) == Some("emit_inbound"))
        .filter_map(|frame| frame.get("frame").cloned())
        .filter(|frame| frame.get("type").and_then(Value::as_str) == Some("interaction.update"))
        .filter_map(|frame| frame.get("update").cloned())
        .collect()
}

fn fixture(name: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../engine-host/test/fixtures/cursor")
        .join(format!("{name}.ndjson"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    parse_updates(&text)
}

fn replay(name: &str) -> Vec<EngineEvent> {
    let mut mapper = CursorMapper::default();
    let mut events = Vec::new();
    for update in fixture(name) {
        events.extend(mapper.map(&update));
    }
    let closed = mapper.finish();
    assert!(
        !closed.is_empty(),
        "{name}: finish reports the run's usage or closes what is still open"
    );
    events.extend(closed);
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
                    // Nested text rows have their own assertions; this compact trace keeps
                    // the tool rows only, so a fixture's expected trace stays readable.
                    let tool = value
                        .get("tool")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if tool == "text" || tool == "thinking" {
                        continue;
                    }
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
                        tool
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
/// Every tool call that started ended. Counts, not sets: a duplicate start paired with a
/// duplicate end is not balanced, and must fail here.
fn assert_balanced(events: &[EngineEvent]) {
    let mut text_started: BTreeMap<String, usize> = BTreeMap::new();
    let mut text_ended: BTreeMap<String, usize> = BTreeMap::new();
    let mut reasoning_started: BTreeMap<String, usize> = BTreeMap::new();
    let mut reasoning_ended: BTreeMap<String, usize> = BTreeMap::new();
    for event in events {
        match event {
            EngineEvent::Agui(AgUiEvent::TextMessageStart { message_id, .. }) => {
                *text_started.entry(message_id.clone()).or_default() += 1;
            }
            EngineEvent::Agui(AgUiEvent::TextMessageContent { message_id, .. }) => {
                assert!(
                    text_started.contains_key(message_id),
                    "text content without start: {message_id}"
                );
            }
            EngineEvent::Agui(AgUiEvent::TextMessageEnd { message_id }) => {
                *text_ended.entry(message_id.clone()).or_default() += 1;
            }
            EngineEvent::Agui(AgUiEvent::ReasoningMessageStart { message_id, .. }) => {
                *reasoning_started.entry(message_id.clone()).or_default() += 1;
            }
            EngineEvent::Agui(AgUiEvent::ReasoningMessageContent { message_id, .. }) => {
                assert!(
                    reasoning_started.contains_key(message_id),
                    "reasoning content without start: {message_id}"
                );
            }
            EngineEvent::Agui(AgUiEvent::ReasoningMessageEnd { message_id }) => {
                *reasoning_ended.entry(message_id.clone()).or_default() += 1;
            }
            _ => {}
        }
    }
    assert_eq!(text_started, text_ended, "text starts and ends do not pair");
    assert_eq!(
        reasoning_started, reasoning_ended,
        "reasoning starts and ends do not pair"
    );
    for (id, count) in text_started.iter().chain(reasoning_started.iter()) {
        assert_eq!(*count, 1, "{id} was started {count} times");
    }
    for (id, count) in text_ended.iter().chain(reasoning_ended.iter()) {
        assert_eq!(*count, 1, "{id} was ended {count} times");
    }

    let mut starts: BTreeMap<String, usize> = BTreeMap::new();
    let mut ends: BTreeMap<String, usize> = BTreeMap::new();
    for event in events {
        match event {
            EngineEvent::Agui(AgUiEvent::ToolCallStart { tool_call_id, .. }) => {
                *starts.entry(tool_call_id.clone()).or_default() += 1;
            }
            EngineEvent::Agui(AgUiEvent::ToolCallEnd { tool_call_id }) => {
                *ends.entry(tool_call_id.clone()).or_default() += 1;
            }
            _ => {}
        }
    }
    assert_eq!(
        starts, ends,
        "a started tool call did not end, or ended twice"
    );
    for (id, count) in starts {
        assert_eq!(count, 1, "{id} was started {count} times");
    }
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
            "usage:11657",
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
            "usage:24651",
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
            "usage:84068",
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
    assert!(
        message_id.starts_with("plan-") && message_id.ends_with("-1"),
        "an occurrence-suffixed plan id: {message_id}"
    );
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
            "usage:25377",
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
            "tool.outcome:K4:failure",
            "tool.result:K4",
            "tool.end:K4",
            "usage:43322",
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
            "tool.outcome:K1:failure",
            "tool.result:K1",
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
    assert!(
        content.starts_with(&"x".repeat(super::MAX_RESULT_CHARS)),
        "the kept prefix is the result cap"
    );
    assert!(content.contains("[truncated:"), "the cut is visible");
    assert!(content.chars().count() > super::MAX_RESULT_CHARS);
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
        "message_steering",
        "multi_turn",
        "provider_thread_resume",
        "queued_turn",
        "skill_invocation",
        "write_edit_delete",
        "grep",
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

fn completed_tool(
    mapper: &mut CursorMapper,
    call_id: &str,
    tool: Value,
    result: Value,
) -> Vec<EngineEvent> {
    let mut events = mapper.map(&json!({
        "type": "tool-call-started",
        "callId": call_id,
        "toolCall": {"type": tool.get("type").cloned().unwrap_or_default(), "args": tool.get("args").cloned().unwrap_or(Value::Null)},
    }));
    let mut call = tool;
    call["result"] = result;
    events.extend(mapper.map(&json!({
        "type": "tool-call-completed",
        "callId": call_id,
        "toolCall": call,
    })));
    events
}

fn tool_result(events: &[EngineEvent]) -> String {
    events
        .iter()
        .find_map(|event| match event {
            EngineEvent::Agui(AgUiEvent::ToolCallResult { content, .. }) => Some(content.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

fn tool_outcome(events: &[EngineEvent]) -> Option<String> {
    events.iter().find_map(|event| match event {
        EngineEvent::Agui(AgUiEvent::Custom { name, value }) if name == "tool.outcome" => value
            .get("outcome")
            .and_then(Value::as_str)
            .map(str::to_owned),
        _ => None,
    })
}

fn custom_events<'a>(events: &'a [EngineEvent], name: &str) -> Vec<&'a Value> {
    events
        .iter()
        .filter_map(|event| match event {
            EngineEvent::Agui(AgUiEvent::Custom { name: kind, value }) if kind == name => {
                Some(value)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn failed_tools_show_the_error_and_a_failure_outcome() {
    let mut mapper = CursorMapper::default();
    let events = completed_tool(
        &mut mapper,
        "call-error",
        json!({"type": "read", "args": {"path": "a.txt"}}),
        json!({"status": "error", "error": {"message": "missing file"}}),
    );
    assert_eq!(tool_result(&events), "missing file");
    assert_eq!(tool_outcome(&events).as_deref(), Some("failure"));
}

#[test]
fn shell_exit_code_and_stderr_are_shown_and_emitted() {
    let mut mapper = CursorMapper::default();
    let events = completed_tool(
        &mut mapper,
        "call-shell",
        json!({"type": "shell", "args": {"command": "false"}}),
        json!({"status": "success", "value": {
            "exitCode": 7, "signal": "", "stdout": "out", "stderr": "boom", "executionTime": 1
        }}),
    );
    let shown = tool_result(&events);
    assert!(shown.contains("out"), "{shown}");
    assert!(shown.contains("boom"), "stderr is shown: {shown}");
    assert_eq!(tool_outcome(&events).as_deref(), Some("failure"));
    let finished = custom_events(&events, "tool.finished");
    assert_eq!(finished.len(), 1, "the exit code is emitted");
    assert_eq!(finished[0]["exitCode"], 7);
}

#[test]
fn mcp_is_error_is_a_failure_even_with_a_success_status() {
    let mut mapper = CursorMapper::default();
    let events = completed_tool(
        &mut mapper,
        "call-mcp",
        json!({"type": "mcp", "args": {"toolName": "search"}}),
        json!({"status": "success", "value": {
            "content": [{"text": {"text": "the server said no"}}], "isError": true
        }}),
    );
    assert_eq!(tool_result(&events), "the server said no");
    assert_eq!(tool_outcome(&events).as_deref(), Some("failure"));
}

#[test]
fn grep_flat_matches_and_files_and_count_outputs_render() {
    let content = json!({
        "type": "grep",
        "args": {"pattern": "needle"},
        "result": {"status": "success", "value": {"workspaceResults": {
            "/w": {"type": "content", "output": {"matches": [
                {"file": "src/a.rs", "lineNumber": 3, "line": "let x = 1;"},
                {"file": "src/b.rs", "line": "no line"}
            ], "totalMatches": 2}}
        }}}
    });
    let mut mapper = CursorMapper::default();
    let mut events = mapper.map(&json!({
        "type": "tool-call-completed", "callId": "call-content", "toolCall": content
    }));
    let shown = tool_result(&events);
    assert!(shown.contains("src/a.rs:3: let x = 1;"), "{shown}");
    assert!(shown.contains("src/b.rs: no line"), "{shown}");

    let files = json!({
        "type": "grep",
        "args": {"pattern": "needle", "outputMode": "files"},
        "result": {"status": "success", "value": {"workspaceResults": {
            "/w": {"type": "files", "output": {"files": ["src/a.rs", "src/b.rs"], "count": 2}}
        }}}
    });
    events = mapper.map(&json!({
        "type": "tool-call-completed", "callId": "call-files", "toolCall": files
    }));
    let shown = tool_result(&events);
    assert!(
        shown.contains("src/a.rs") && shown.contains("src/b.rs"),
        "{shown}"
    );

    let count = json!({
        "type": "grep",
        "args": {"pattern": "needle", "outputMode": "count"},
        "result": {"status": "success", "value": {"workspaceResults": {
            "/w": {"type": "count", "output": {"counts": [{"file": "src/a.rs", "count": 2}], "total": 2}}
        }}}
    });
    events = mapper.map(&json!({
        "type": "tool-call-completed", "callId": "call-count", "toolCall": count
    }));
    let shown = tool_result(&events);
    assert!(shown.contains("src/a.rs: 2"), "{shown}");
}

#[test]
fn huge_text_deltas_are_capped_with_a_visible_marker() {
    let mut mapper = CursorMapper::default();
    let mut events = mapper.map(&json!({"type": "text-delta", "text": "x".repeat(200_000)}));
    events.extend(mapper.finish());
    let shown = text(&events);
    assert!(shown.chars().count() <= 70_000, "{}", shown.chars().count());
    assert!(shown.contains("truncated:"), "the cut is visible");
    assert!(shown.starts_with("xxxx"));
}

#[test]
fn duplicate_and_late_tool_events_do_not_reopen_a_tool() {
    let mut mapper = CursorMapper::default();
    let call = json!({
        "type": "read", "args": {"path": "a.txt"},
        "result": {"status": "success", "value": {"content": "body"}}
    });
    let mut events = mapper.map(&json!({"type": "tool-call-started", "callId": "call-1", "toolCall": {"type": "read", "args": {"path": "a.txt"}}}));
    events.extend(
        mapper.map(&json!({"type": "tool-call-completed", "callId": "call-1", "toolCall": call})),
    );
    events.extend(
        mapper.map(&json!({"type": "tool-call-completed", "callId": "call-1", "toolCall": call})),
    );
    events.extend(mapper.map(&json!({"type": "tool-call-started", "callId": "call-1", "toolCall": {"type": "read", "args": {"path": "a.txt"}}})));
    let starts = events
        .iter()
        .filter(|event| matches!(event, EngineEvent::Agui(AgUiEvent::ToolCallStart { .. })))
        .count();
    let ends = events
        .iter()
        .filter(|event| matches!(event, EngineEvent::Agui(AgUiEvent::ToolCallEnd { .. })))
        .count();
    let results = events
        .iter()
        .filter(|event| matches!(event, EngineEvent::Agui(AgUiEvent::ToolCallResult { .. })))
        .count();
    assert_eq!((starts, ends, results), (1, 1, 1));
    assert_balanced(&events);
}

#[test]
fn empty_todos_replace_the_plan_with_no_steps() {
    let mut mapper = CursorMapper::default();
    let mut first = mapper.map(&json!({
        "type": "tool-call-started", "callId": "call-todo",
        "toolCall": {"type": "updateTodos", "args": {"todos": [
            {"content": "one", "status": "completed"},
            {"content": "two", "status": "pending"}
        ]}}
    }));
    first.extend(mapper.map(&json!({
        "type": "tool-call-completed", "callId": "call-todo",
        "toolCall": {"type": "updateTodos", "args": {"todos": []}, "result": {"status": "success", "value": {}}}
    })));
    let plans: Vec<&AgUiEvent> = agui(&first)
        .into_iter()
        .filter(|event| matches!(event, AgUiEvent::ActivitySnapshot { .. }))
        .collect();
    assert_eq!(plans.len(), 2, "the empty list is a replacement too");
    let AgUiEvent::ActivitySnapshot {
        content, replace, ..
    } = plans[1]
    else {
        panic!("not a plan");
    };
    assert_eq!(content, &json!({"steps": []}));
    assert_eq!(replace, &Some(true));
}

#[test]
fn null_args_are_empty_and_generated_ids_carry_an_occurrence() {
    let mut mapper = CursorMapper::default();
    let mut events = mapper.map(&json!({
        "type": "tool-call-started", "callId": "call-1",
        "toolCall": {"type": "read", "args": null}
    }));
    let args: Vec<&str> = events
        .iter()
        .filter_map(|event| match event {
            EngineEvent::Agui(AgUiEvent::ToolCallArgs { delta, .. }) => Some(delta.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(args, [""], "null args are not the text null");
    events.extend(mapper.map(&json!({
        "type": "tool-call-completed", "callId": "call-1",
        "toolCall": {"type": "read", "args": null, "result": {"status": "success", "value": {"content": "x"}}}
    })));
    let message_id = events
        .iter()
        .find_map(|event| match event {
            EngineEvent::Agui(AgUiEvent::ToolCallResult { message_id, .. }) => {
                Some(message_id.clone())
            }
            _ => None,
        })
        .expect("a result");
    assert!(
        message_id.starts_with("cursor-tool-call-1-") && message_id.ends_with("-1"),
        "an occurrence suffix: {message_id}"
    );
}

#[test]
fn subagent_activity_carries_id_path_and_bounded_nested_text() {
    let mut mapper = CursorMapper::default();
    mapper.map(&json!({
        "type": "tool-call-started", "callId": "call-task",
        "toolCall": {"type": "task", "args": {
            "description": "Read package.json", "agentId": "agent-1", "prompt": "read"
        }}
    }));
    let mut events = mapper.map(&json!({
        "type": "tool-call-delta", "callId": "call-task", "modelCallId": "m",
        "taskUpdate": {"type": "text-delta", "text": "y".repeat(10_000)}
    }));
    events.extend(mapper.map(&json!({
        "type": "tool-call-delta", "callId": "call-task", "modelCallId": "m",
        "taskUpdate": {"type": "tool-call-started", "callId": "nested-1", "modelCallId": "n",
            "toolCall": {"type": "read", "args": {"path": "a.txt"}}}
    })));
    let activities = custom_events(&events, "subagent.activity");
    assert!(!activities.is_empty());
    for activity in &activities {
        assert_eq!(activity["id"], "agent-1");
        assert_eq!(activity["agentPath"], "Read package.json");
    }
    let text = activities
        .iter()
        .find_map(|activity| activity.get("text").and_then(Value::as_str))
        .expect("nested text is surfaced");
    assert!(text.chars().count() <= 4200, "{}", text.chars().count());
    assert!(text.contains("truncated:"));
}

#[test]
fn model_call_usage_is_the_fallback_and_turn_usage_wins() {
    let mut mapper = CursorMapper::default();
    mapper.map(
        &json!({"type": "model-call-completed", "modelCallId": "m", "usage": {
            "inputTokens": 100, "outputTokens": 5, "cacheReadTokens": 90, "cacheWriteTokens": 0
        }}),
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
        [105],
        "model-call usage is input + output"
    );

    let mut mapper = CursorMapper::default();
    mapper.map(
        &json!({"type": "model-call-completed", "modelCallId": "m", "usage": {
            "inputTokens": 100, "outputTokens": 5, "cacheReadTokens": 90, "cacheWriteTokens": 0
        }}),
    );
    mapper.map(&json!({"type": "turn-ended", "usage": {
        "inputTokens": 200, "outputTokens": 10, "cacheReadTokens": 180, "cacheWriteTokens": 0
    }}));
    let events = mapper.finish();
    assert_eq!(
        events
            .iter()
            .filter_map(|event| match event {
                EngineEvent::Usage(usage) => usage.total_tokens,
                _ => None,
            })
            .collect::<Vec<_>>(),
        [210],
        "the turn total wins over the model calls it contains"
    );
}

#[test]
fn shell_output_deltas_become_tool_output() {
    let mut mapper = CursorMapper::default();
    mapper.map(&json!({
        "type": "tool-call-started", "callId": "call-shell",
        "toolCall": {"type": "shell", "args": {"command": "ls"}}
    }));
    let events = mapper.map(&json!({
        "type": "shell-output-delta",
        "event": {"toolCallId": "call-shell", "case": "stdout", "value": {"data": "line\n"}}
    }));
    let output = custom_events(&events, "tool.output");
    assert_eq!(output.len(), 1);
    assert_eq!(output[0]["toolCallId"], "call-shell");
    assert_eq!(output[0]["delta"], "line\n");
}

#[test]
fn usage_math_saturates_instead_of_panicking() {
    let mut mapper = CursorMapper::default();
    mapper.map(&json!({"type": "turn-ended", "usage": {
        "inputTokens": u64::MAX, "outputTokens": u64::MAX,
        "cacheReadTokens": u64::MAX, "cacheWriteTokens": u64::MAX
    }}));
    let events = mapper.finish();
    assert_eq!(
        events
            .iter()
            .filter_map(|event| match event {
                EngineEvent::Usage(usage) => usage.total_tokens,
                _ => None,
            })
            .collect::<Vec<_>>(),
        [u64::MAX]
    );
}

fn usage_totals(events: &[EngineEvent]) -> Vec<u64> {
    events
        .iter()
        .filter_map(|event| match event {
            EngineEvent::Usage(usage) => usage.total_tokens,
            _ => None,
        })
        .collect()
}

fn text_starts(events: &[EngineEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, EngineEvent::Agui(AgUiEvent::TextMessageStart { .. })))
        .count()
}

#[test]
fn message_steering_replays_both_responses_without_the_user_message() {
    let events = replay("message_steering");
    assert_eq!(
        text(&events),
        "steering fixture initial responsesteering fixture observed"
    );
    assert!(!text(&events).contains("Actually, respond with exactly"));
    assert_eq!(text_starts(&events), 2, "the steer starts a new response");
    assert_eq!(usage_totals(&events), [24982]);
    assert_balanced(&events);
}

#[test]
fn multi_turn_replays_both_turns_each_with_its_own_message() {
    let events = replay("multi_turn");
    assert_eq!(
        text(&events),
        "first fixture turn completesecond fixture turn complete"
    );
    assert_eq!(text_starts(&events), 2);
    assert_eq!(usage_totals(&events), [23310]);
    assert_balanced(&events);
}

#[test]
fn provider_thread_resume_replays_the_resumed_turn_separately() {
    let events = replay("provider_thread_resume");
    let shown = text(&events);
    assert_eq!(
        shown
            .matches("provider thread resume fixture first turn complete")
            .count(),
        2,
        "the first answer is replayed and then repeated by the resumed turn"
    );
    assert!(shown.ends_with("provider thread resume fixture second turn complete"));
    assert_eq!(usage_totals(&events), [23521]);
    assert_balanced(&events);
}

#[test]
fn queued_turn_replays_the_second_send_after_a_step_boundary() {
    let events = replay("queued_turn");
    assert_eq!(
        text(&events),
        "first fixture turn completesecond fixture turn complete"
    );
    assert_eq!(text_starts(&events), 2);
    assert_eq!(usage_totals(&events), [23396]);
    assert_balanced(&events);
}

#[test]
fn skill_invocation_replays_both_reads_and_the_final_answer() {
    let events = replay("skill_invocation");
    assert!(text(&events).contains("review skill fixture complete"));
    let results: Vec<String> = events
        .iter()
        .filter_map(|event| match event {
            EngineEvent::Agui(AgUiEvent::ToolCallResult { content, .. }) => Some(content.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(results.len(), 2, "both reads finish");
    assert!(results.iter().any(|result| result.contains("review skill")));
    assert!(
        results
            .iter()
            .any(|result| result.contains("# cursor-agent-sdk"))
    );
    assert_eq!(usage_totals(&events), [28134]);
    assert_balanced(&events);
}

#[test]
fn write_edit_delete_emit_files_only_inside_the_run_cwd() {
    let mut mapper = CursorMapper::default().with_cwd("/workspace/repo/app");
    let mut events = Vec::new();
    for update in fixture("write_edit_delete") {
        events.extend(mapper.map(&update));
    }
    events.extend(mapper.finish());
    let files: Vec<Vec<String>> = events
        .iter()
        .filter_map(|event| match event {
            EngineEvent::Files(paths) => Some(paths.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        files,
        [
            vec!["/workspace/repo/app/src/written.txt".to_owned()],
            vec!["/workspace/repo/app/src/edited.txt".to_owned()],
            vec!["/workspace/repo/app/src/deleted.txt".to_owned()],
        ],
        "the outside write is not a run change"
    );
    let results: Vec<String> = events
        .iter()
        .filter_map(|event| match event {
            EngineEvent::Agui(AgUiEvent::ToolCallResult { content, .. }) => Some(content.clone()),
            _ => None,
        })
        .collect();
    assert!(results[0].contains("written.txt"));
    assert!(results[1].contains("-old\n+new"), "the edit shows its diff");
    assert!(results[2].contains("deleted.txt"));
    assert_eq!(usage_totals(&events), [110]);
    assert_balanced(&events);
}

#[test]
fn grep_fixture_renders_content_files_and_count_outputs() {
    let events = replay("grep");
    let results: Vec<String> = events
        .iter()
        .filter_map(|event| match event {
            EngineEvent::Agui(AgUiEvent::ToolCallResult { content, .. }) => Some(content.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(results.len(), 3);
    assert!(
        results[0].contains("src/a.rs:3: let needle = 1;"),
        "{}",
        results[0]
    );
    assert!(results[0].contains("src/b.rs: // needle"), "{}", results[0]);
    assert!(results[1].contains("src/a.rs") && results[1].contains("src/b.rs"));
    assert!(results[2].contains("src/a.rs: 2"));
    assert_eq!(usage_totals(&events), [220]);
    assert_balanced(&events);
}

#[test]
fn the_fixture_loader_rejects_malformed_and_unterminated_lines() {
    let malformed = std::panic::catch_unwind(|| parse_updates("{not json\n"));
    assert!(malformed.is_err(), "a malformed line is not skipped");
    let truncated =
        std::panic::catch_unwind(|| parse_updates("{\"type\":\"emit_inbound\",\"frame\":{}}"));
    assert!(truncated.is_err(), "an unterminated final line is not read");
    assert!(parse_updates("").is_empty());
}

#[test]
fn missing_tool_identity_is_ignored_deterministically() {
    let mut mapper = CursorMapper::default();
    for update in [
        json!({"type": "tool-call-started", "callId": "", "toolCall": {"type": "read"}}),
        json!({"type": "tool-call-started", "toolCall": {"type": "read"}}),
        json!({"type": "tool-call-completed", "callId": "call-1"}),
        json!({"type": "tool-call-completed", "callId": "call-1", "toolCall": "not an object"}),
    ] {
        assert!(mapper.map(&update).is_empty(), "{update}");
    }
    let events = mapper.finish();
    assert!(
        !events.iter().any(|event| matches!(
            event,
            EngineEvent::Agui(
                AgUiEvent::ToolCallStart { .. }
                    | AgUiEvent::ToolCallEnd { .. }
                    | AgUiEvent::ToolCallResult { .. }
            )
        )),
        "a call with no identity makes no tool events"
    );
}

#[test]
fn every_sdk_tool_type_has_readable_result_text() {
    let mut mapper = CursorMapper::default();
    let mut calls = 0;
    let mut render = |ty: &str, args: Value, value: Value| {
        calls += 1;
        let events = completed_tool(
            &mut mapper,
            &format!("call-{ty}-{calls}"),
            json!({"type": ty, "args": args}),
            json!({"status": "success", "value": value}),
        );
        tool_result(&events)
    };
    let glob = render(
        "glob",
        json!({"globPattern": "**/*"}),
        json!({"files": ["a.rs", "b.rs"], "totalFiles": 2}),
    );
    assert_eq!(glob, "a.rs\nb.rs");
    let ls = render(
        "ls",
        json!({"path": "/w"}),
        json!({"directoryTreeRoot": {
            "absPath": "/w", "childrenDirs": [{"absPath": "/w/src", "childrenDirs": [], "childrenFiles": [{"name": "a.rs"}]}],
            "childrenFiles": [{"name": "b.txt"}]
        }}),
    );
    assert!(
        ls.contains("/w/src") && ls.contains("a.rs") && ls.contains("b.txt"),
        "{ls}"
    );
    let lints = render(
        "readLints",
        json!({"paths": ["src/a.rs"]}),
        json!({"fileDiagnostics": [{"path": "src/a.rs", "diagnosticsCount": 1, "diagnostics": [
            {"severity": "warning", "message": "unused", "source": "rustc", "code": "w1", "range": {"start": {"line": 3}}}
        ]}], "totalFiles": 1, "totalDiagnostics": 1}),
    );
    assert!(
        lints.contains("src/a.rs:3: warning: unused [rustc w1]"),
        "{lints}"
    );
    assert_eq!(
        render(
            "semSearch",
            json!({"query": "x"}),
            json!({"results": "found it"})
        ),
        "found it"
    );
    assert_eq!(
        render(
            "generateImage",
            json!({"description": "x"}),
            json!({"filePath": "/tmp/i.png", "imageData": "..."})
        ),
        "/tmp/i.png"
    );
    assert_eq!(
        render(
            "computerUse",
            json!({"actions": []}),
            json!({"actionCount": 2, "durationMs": 5, "log": "clicked"})
        ),
        "clicked"
    );
    assert_eq!(
        render(
            "readLints",
            json!({"paths": []}),
            json!({"fileDiagnostics": [], "totalFiles": 0, "totalDiagnostics": 0})
        ),
        "No diagnostics"
    );
}
