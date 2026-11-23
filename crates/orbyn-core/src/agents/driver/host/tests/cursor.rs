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
    assert_eq!(usage, [13], "turn-ended counts every token kind once");

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
