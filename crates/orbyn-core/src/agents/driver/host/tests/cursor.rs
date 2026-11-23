//! The Cursor family on the engine host: the flag that selects it, and one real run through
//! the stand-in host proving `open`, `send`, `delta` and `runDone` become the Cursor
//! timeline without a permission or question frame (Cursor never asks before acting).

use std::sync::Arc;

use orbyn_contracts::{AgUiEvent, CustomEngine, EngineTier};
use serde_json::json;

use super::{fast, spec, standin_family, start};
use crate::agents::driver::acp::AcpDriver;
use crate::agents::driver::host::delta::DeltaMapper;
use crate::agents::driver::host::{HostConfig, HostDriver, HostHub};
use crate::agents::driver::{EngineDriver, EngineEvent, EngineOutcome};

fn inner() -> Arc<dyn EngineDriver> {
    Arc::new(AcpDriver::new(CustomEngine {
        id: "cursor".into(),
        name: "Cursor".into(),
        tier: EngineTier::Acp,
        target: std::env::current_exe()
            .expect("the test binary has a path")
            .to_string_lossy()
            .into_owned(),
        args: Vec::new(),
    }))
}

fn text(events: &[EngineEvent]) -> String {
    events
        .iter()
        .filter_map(|event| match event {
            EngineEvent::Agui(AgUiEvent::TextMessageContent { delta, .. }) => Some(delta.as_str()),
            _ => None,
        })
        .collect()
}

fn reasoning(events: &[EngineEvent]) -> String {
    events
        .iter()
        .filter_map(|event| match event {
            EngineEvent::Agui(AgUiEvent::ReasoningMessageContent { delta, .. }) => {
                Some(delta.as_str())
            }
            _ => None,
        })
        .collect()
}

#[test]
fn the_cursor_flag_wraps_the_family_and_the_host_maps_it() {
    assert!(
        DeltaMapper::for_family("cursor").is_some(),
        "the core maps Cursor deltas now"
    );
    let rust = inner();
    let off = HostConfig::from_env(Some("/x/orbyn-engine-host".into()), Some("0")).unwrap();
    assert!(!off.cursor);
    let hub = HostHub::new(&off);
    assert!(!hub.knows("cursor"));
    assert!(Arc::ptr_eq(
        &hub.wrap("cursor", rust.clone(), "cursor-agent".into()),
        &rust
    ));
    // The flag wants exactly `1`, as Cursor's own family flag always has.
    assert!(
        !HostConfig::from_env(Some("/x/h".into()), None)
            .unwrap()
            .with_cursor(Some("true"))
            .cursor
    );
    let on = off.with_cursor(Some("1"));
    assert!(on.cursor);
    let hub = HostHub::new(&on);
    assert!(hub.knows("cursor"));
    if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        return;
    }
    assert!(
        !Arc::ptr_eq(
            &hub.wrap("cursor", rust.clone(), "cursor-agent".into()),
            &rust
        ),
        "the flag wraps Cursor"
    );
}

#[tokio::test]
async fn a_cursor_run_maps_open_send_deltas_and_run_done_through_the_host() {
    let Some(rig) = standin_family("cursor", &[], fast()) else {
        return;
    };
    let driver = HostDriver::new(inner(), rig.pool.clone(), "cursor-agent".into());
    let (events, outcome) = start(&driver, spec("[cursor]"))
        .await
        .expect("the host accepted the Cursor run")
        .outcome()
        .await;
    assert_eq!(outcome, EngineOutcome::Completed);

    assert_eq!(text(&events), "hello world");
    assert_eq!(reasoning(&events), "Looking");
    assert!(
        !text(&events).contains("must not show"),
        "an unknown update says nothing"
    );
    assert!(
        !text(&events).contains("echo:"),
        "the stand-in's fake-family text never appears"
    );

    let starts: Vec<(&str, &str)> = events
        .iter()
        .filter_map(|event| match event {
            EngineEvent::Agui(AgUiEvent::ToolCallStart {
                tool_call_id,
                tool_call_name,
            }) => Some((tool_call_id.as_str(), tool_call_name.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(
        starts,
        [("call-read", "read"), ("call-todo", "updateTodos")]
    );
    let ends: Vec<&str> = events
        .iter()
        .filter_map(|event| match event {
            EngineEvent::Agui(AgUiEvent::ToolCallEnd { tool_call_id }) => {
                Some(tool_call_id.as_str())
            }
            _ => None,
        })
        .collect();
    assert_eq!(ends, ["call-read", "call-todo"]);
    let results: Vec<&str> = events
        .iter()
        .filter_map(|event| match event {
            EngineEvent::Agui(AgUiEvent::ToolCallResult { content, .. }) => Some(content.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(results, ["file body", ""]);

    assert!(
        events.iter().any(|event| matches!(
            event,
            EngineEvent::Agui(AgUiEvent::ActivitySnapshot { content, .. })
                if content == &json!({"steps": [{"title": "one", "status": "completed"}]})
        )),
        "the Cursor todo list becomes the plan activity"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, EngineEvent::Session(id) if id.starts_with("cursor-"))),
        "the accepted engine session is named"
    );
    let usage: Vec<u64> = events
        .iter()
        .filter_map(|event| match event {
            EngineEvent::Usage(usage) => usage.total_tokens,
            _ => None,
        })
        .collect();
    assert_eq!(
        usage,
        [12],
        "turn-ended total is input + output; cache is already in input"
    );

    let requests: Vec<String> = rig
        .log()
        .iter()
        .filter_map(|request| request.get("type").and_then(|kind| kind.as_str()))
        .map(str::to_owned)
        .collect();
    assert!(requests.iter().any(|kind| kind == "open"));
    assert!(requests.iter().any(|kind| kind == "send"));
    assert!(!requests.iter().any(|kind| kind == "permission"));
    assert!(!requests.iter().any(|kind| kind == "question"));
    rig.pool.shutdown().await;
}

fn tool_results(events: &[EngineEvent]) -> Vec<(&str, &str)> {
    events
        .iter()
        .filter_map(|event| match event {
            EngineEvent::Agui(AgUiEvent::ToolCallResult {
                tool_call_id,
                content,
                ..
            }) => Some((tool_call_id.as_str(), content.as_str())),
            _ => None,
        })
        .collect()
}

fn tool_outcomes(events: &[EngineEvent]) -> Vec<(&str, &str)> {
    events
        .iter()
        .filter_map(|event| match event {
            EngineEvent::Agui(AgUiEvent::Custom { name, value }) if name == "tool.outcome" => {
                Some((
                    value.get("toolCallId")?.as_str()?,
                    value.get("outcome")?.as_str()?,
                ))
            }
            _ => None,
        })
        .collect()
}

fn custom<'a>(events: &'a [EngineEvent], name: &str) -> Vec<&'a serde_json::Value> {
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

/// The disabled CLI/provider EngineDriver::start_run forwards to the Rust driver: its
/// model and mode must reach the host's open and send requests, with no secret in either.
#[tokio::test]
async fn cursor_open_and_send_carry_model_mode_and_mcp() {
    let Some(rig) = standin_family("cursor", &[], fast()) else {
        return;
    };
    let driver = HostDriver::new(inner(), rig.pool.clone(), "cursor-agent".into());
    let mut run = spec("[cursor]");
    run.model = Some("composer-2.5".into());
    let (_, outcome) = start(&driver, run).await.unwrap().outcome().await;
    assert_eq!(outcome, EngineOutcome::Completed);
    let log = rig.log();
    let open = log
        .iter()
        .find(|line| line["type"] == "open")
        .expect("open was sent");
    let send = log
        .iter()
        .find(|line| line["type"] == "send")
        .expect("send was sent");
    for frame in [open, send] {
        assert_eq!(frame["options"]["model"]["id"], "composer-2.5");
        assert_eq!(frame["options"]["mode"], "agent");
        assert_eq!(
            frame["options"]["mcpServers"]["orbyn_engineering"]["type"],
            "http"
        );
        let text = frame["options"].to_string();
        assert!(!text.contains("apiKey"), "no key is sent: {text}");
    }

    let mut plan = spec("[cursor]");
    plan.model = Some("composer-2.5".into());
    plan.plan = true;
    let (_, outcome) = start(&driver, plan).await.unwrap().outcome().await;
    assert_eq!(outcome, EngineOutcome::Completed);
    let log = rig.log();
    for kind in ["open", "send"] {
        let frame = log
            .iter()
            .rev()
            .find(|line| line["type"] == kind)
            .expect("the plan run sent it");
        assert_eq!(frame["options"]["mode"], "plan");
    }
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn cursor_failed_tools_show_errors_stderr_and_exit_codes() {
    let Some(rig) = standin_family("cursor", &[], fast()) else {
        return;
    };
    let driver = HostDriver::new(inner(), rig.pool.clone(), "cursor-agent".into());
    let (events, outcome) = start(&driver, spec("[cursor-fail]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(outcome, EngineOutcome::Completed);
    let results = tool_results(&events);
    let result = |id: &str| {
        results
            .iter()
            .find_map(|(call, content)| (*call == id).then_some(*content))
            .unwrap_or_default()
    };
    assert!(result("call-shell").contains("out"));
    assert!(result("call-shell").contains("boom"));
    assert_eq!(result("call-error"), "missing file");
    assert_eq!(result("call-mcp"), "the server said no");
    let outcomes = tool_outcomes(&events);
    for id in ["call-shell", "call-error", "call-mcp"] {
        assert!(outcomes.contains(&(id, "failure")), "{id}: {outcomes:?}");
    }
    let finished = custom(&events, "tool.finished");
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0]["toolCallId"], "call-shell");
    assert_eq!(finished[0]["exitCode"], 1);
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn cursor_interrupt_mid_tool_reports_an_interrupted_result() {
    let Some(rig) = standin_family("cursor", &[], fast()) else {
        return;
    };
    let driver = HostDriver::new(inner(), rig.pool.clone(), "cursor-agent".into());
    let mut started = start(&driver, spec("[cursor-hang]")).await.unwrap();
    let wait = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            match started.events.recv().await {
                Some(EngineEvent::Agui(AgUiEvent::ToolCallStart { .. })) => return,
                Some(_) => {}
                None => panic!("the run ended before the tool started"),
            }
        }
    })
    .await;
    assert!(wait.is_ok(), "the shell tool never started");
    started.handle.cancel.cancel();
    let (events, outcome) = started.outcome().await;
    assert_eq!(outcome, EngineOutcome::Cancelled);
    let results = tool_results(&events);
    assert!(
        results
            .iter()
            .any(|(_, content)| content.contains("Interrupted before a result.")),
        "{results:?}"
    );
    assert!(
        tool_outcomes(&events).contains(&("call-shell", "failure")),
        "the interrupted tool is not a success"
    );
    let result_at = events
        .iter()
        .position(|event| matches!(event, EngineEvent::Agui(AgUiEvent::ToolCallResult { .. })))
        .expect("a result");
    let end_at = events
        .iter()
        .position(|event| matches!(event, EngineEvent::Agui(AgUiEvent::ToolCallEnd { .. })))
        .expect("an end");
    assert!(result_at < end_at, "the result precedes the end");
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn cursor_duplicate_completions_and_late_starts_are_ignored() {
    let Some(rig) = standin_family("cursor", &[], fast()) else {
        return;
    };
    let driver = HostDriver::new(inner(), rig.pool.clone(), "cursor-agent".into());
    let (events, outcome) = start(&driver, spec("[cursor-dup]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(outcome, EngineOutcome::Completed);
    let count = |want: fn(&AgUiEvent) -> bool| {
        events
            .iter()
            .filter(|event| matches!(event, EngineEvent::Agui(inner) if want(inner)))
            .count()
    };
    assert_eq!(
        count(|event| matches!(event, AgUiEvent::ToolCallStart { .. })),
        1
    );
    assert_eq!(
        count(|event| matches!(event, AgUiEvent::ToolCallArgs { .. })),
        1
    );
    assert_eq!(
        count(|event| matches!(event, AgUiEvent::ToolCallEnd { .. })),
        1
    );
    assert_eq!(
        count(|event| matches!(event, AgUiEvent::ToolCallResult { .. })),
        1
    );
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn cursor_huge_text_deltas_are_capped() {
    let Some(rig) = standin_family("cursor", &[], fast()) else {
        return;
    };
    let driver = HostDriver::new(inner(), rig.pool.clone(), "cursor-agent".into());
    let (events, outcome) = start(&driver, spec("[cursor-huge]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(outcome, EngineOutcome::Completed);
    let shown = text(&events);
    assert!(shown.chars().count() <= 70_000, "{}", shown.chars().count());
    assert!(shown.contains("truncated:"), "the cut is visible");
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn cursor_plan_and_todos_become_the_plan_activity() {
    let Some(rig) = standin_family("cursor", &[], fast()) else {
        return;
    };
    let driver = HostDriver::new(inner(), rig.pool.clone(), "cursor-agent".into());
    let (events, outcome) = start(&driver, spec("[cursor-plan]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(outcome, EngineOutcome::Completed);
    assert!(text(&events).contains("<proposed_plan>"));
    assert!(text(&events).contains("Ship the cursor fix."));
    assert!(!text(&events).contains("this summary is not shown"));
    assert!(
        events.iter().any(|event| matches!(
            event,
            EngineEvent::Agui(AgUiEvent::ActivitySnapshot { content, .. })
                if content["steps"][0]["title"] == "write the test"
        )),
        "the todo list is the plan activity"
    );
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn cursor_subagent_activity_carries_an_id_path_and_bounded_text() {
    let Some(rig) = standin_family("cursor", &[], fast()) else {
        return;
    };
    let driver = HostDriver::new(inner(), rig.pool.clone(), "cursor-agent".into());
    let (events, outcome) = start(&driver, spec("[cursor-subagent]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(outcome, EngineOutcome::Completed);
    let activities = custom(&events, "subagent.activity");
    assert!(!activities.is_empty());
    for activity in &activities {
        assert_eq!(activity["id"], "agent-1");
        assert_eq!(activity["agentPath"], "Read package.json");
    }
    let nested = activities
        .iter()
        .find_map(|activity| activity.get("text").and_then(|text| text.as_str()))
        .expect("nested text is surfaced");
    assert!(nested.chars().count() <= 4200);
    assert!(nested.contains("truncated:"));
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn cursor_result_usage_wins_over_turn_ended_usage() {
    let Some(rig) = standin_family("cursor", &[], fast()) else {
        return;
    };
    let driver = HostDriver::new(inner(), rig.pool.clone(), "cursor-agent".into());
    let (events, outcome) = start(&driver, spec("[cursor-usage]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(outcome, EngineOutcome::Completed);
    let totals: Vec<u64> = events
        .iter()
        .filter_map(|event| match event {
            EngineEvent::Usage(usage) => usage.total_tokens,
            _ => None,
        })
        .collect();
    assert_eq!(totals, [220], "one usage event, from the run result");
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn cursor_credentials_reach_the_host_and_never_a_frame_or_a_log() {
    let Some(rig) =
        super::standin_family_with("cursor", &[], fast(), Some(super::secrets::source()))
    else {
        return;
    };
    let driver = HostDriver::new(inner(), rig.pool.clone(), "cursor-agent".into());
    let (events, outcome) = start(&driver, spec("[cursor]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(outcome, EngineOutcome::Completed);
    let spawn = rig
        .log()
        .into_iter()
        .find(|line| line["type"] == "spawn")
        .expect("the host logged its spawn");
    assert_eq!(spawn["keyInEnv"], true);
    assert_eq!(spawn["keyInArgv"], false);
    assert!(
        !rig.raw().contains(super::secrets::KEY),
        "the key was sent in a frame"
    );
    assert!(
        !serde_json::to_string(&rig.log())
            .unwrap()
            .contains(super::secrets::KEY),
        "the key reached the request log"
    );
    assert!(!format!("{events:?}").contains(super::secrets::KEY));
    rig.pool.shutdown().await;
}
