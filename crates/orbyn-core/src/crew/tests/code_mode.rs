//! Code Mode through a real core: a script's calls go back through the core and a Crew worker's
//! script is held to the worker allow list.

use super::harness::{Turn, brief, key, world};
use super::review_support::lead_binding;
use crate::crew::workers::allowed;
use crate::tools::mcp::McpBinding;
use crate::tools::mcp::code::{DESCRIBE_TOOL, RUN_TOOL, SEARCH_TOOL, fake::Fake};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

fn parse(answer: (String, bool)) -> (Value, bool) {
    (
        serde_json::from_str(&answer.0).unwrap_or(Value::String(answer.0)),
        answer.1,
    )
}

#[tokio::test]
async fn a_script_call_goes_back_through_the_core() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_secs(10));
    let lead = lead_binding(&w);
    let (reply, is_error) = parse(
        w.core
            .code_tool_text(
                &lead,
                RUN_TOOL,
                &json!({ "code": "call orbyn_environment_read" }),
            )
            .await,
    );
    assert!(!is_error, "{reply}");
    assert_eq!(reply["status"], "completed", "{reply}");
    assert_eq!(reply["result"][0]["thread_id"], json!(w.lead), "{reply}");
    assert!(reply["runId"].as_str().is_some());
}

#[tokio::test]
async fn a_script_cannot_call_what_is_not_an_orbyn_tool() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_secs(10));
    let lead = lead_binding(&w);
    for tool in [
        "crew_start",
        "computer_desktop",
        "approve",
        "skill_load",
        RUN_TOOL,
        "orbyn_nope",
    ] {
        let (reply, _) = parse(
            w.core
                .code_tool_text(&lead, RUN_TOOL, &json!({ "code": format!("call {tool}") }))
                .await,
        );
        assert_eq!(reply["status"], "error", "{tool}: {reply}");
        assert_eq!(reply["error"]["kind"], "runtime", "{tool}: {reply}");
        assert!(
            reply["error"]["message"]
                .as_str()
                .unwrap()
                .contains("not available to scripts"),
            "{tool}: {reply}"
        );
    }
}

#[tokio::test]
async fn a_run_is_resumed_by_its_thread_only() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_millis(80));
    let lead = lead_binding(&w);
    let (paused, _) = parse(
        w.core
            .code_tool_text(
                &lead,
                RUN_TOOL,
                &json!({ "code": "sleep 400\ncall orbyn_environment_read" }),
            )
            .await,
    );
    assert_eq!(paused["status"], "paused", "{paused}");
    let run_id = paused["runId"].as_str().unwrap().to_owned();
    let other = McpBinding {
        scope: crate::governance::approvals::RunScope {
            thread_id: "someone-else".into(),
            ..lead.scope.clone()
        },
        ..lead.clone()
    };
    let (lost, _) = parse(
        w.core
            .code_tool_text(&other, RUN_TOOL, &json!({ "run_id": run_id }))
            .await,
    );
    assert_eq!(lost["error"]["kind"], "lost", "{lost}");
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_secs(10));
    let (done, _) = parse(
        w.core
            .code_tool_text(&lead, RUN_TOOL, &json!({ "run_id": run_id }))
            .await,
    );
    assert_eq!(done["status"], "completed", "{done}");
}

#[tokio::test]
async fn search_and_describe_never_show_a_worker_a_tool_it_cannot_call() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (started, _) = w.start(brief(json!({})), key()).await;
    let id = started["task"].as_str().unwrap().to_owned();
    let lead = lead_binding(&w);
    let worker = McpBinding {
        scope: crate::governance::approvals::RunScope {
            thread_id: w.worker(&id),
            run_id: "run-w".into(),
            ..lead.scope.clone()
        },
        ..lead.clone()
    };
    // The three tools are not on the worker allow list yet, so a worker cannot call them at all.
    for tool in [SEARCH_TOOL, DESCRIBE_TOOL, RUN_TOOL] {
        assert!(!allowed(tool), "{tool} is not allowed for workers yet");
        let (text, is_error) = w
            .core
            .code_tool_text(
                &worker,
                tool,
                &json!({ "query": "x", "paths": [], "code": "" }),
            )
            .await;
        assert!(
            is_error && text.contains("A crew worker cannot use"),
            "{tool}: {text}"
        );
    }
    // The lead sees tools; its search never lists a Code Mode tool (a script may not nest).
    let (hits, _) = parse(
        w.core
            .code_tool_text(
                &lead,
                SEARCH_TOOL,
                &json!({ "query": "code", "detail": "name", "limit": 50 }),
            )
            .await,
    );
    assert!(
        hits.as_array()
            .unwrap()
            .iter()
            .all(|h| !h["path"].as_str().unwrap().starts_with("orbyn_code_")),
        "{hits}"
    );
    let (listed, _) = parse(
        w.core
            .code_tool_text(
                &lead,
                DESCRIBE_TOOL,
                &json!({ "paths": ["orbyn_environment_read", "orbyn_code_run", "crew_start"] }),
            )
            .await,
    );
    assert!(
        listed["declarations"]
            .as_str()
            .unwrap()
            .contains("declare function orbyn_environment_read("),
        "{listed}"
    );
    assert_eq!(
        listed["unknown"],
        json!(["orbyn_code_run", "crew_start"]),
        "{listed}"
    );
}
