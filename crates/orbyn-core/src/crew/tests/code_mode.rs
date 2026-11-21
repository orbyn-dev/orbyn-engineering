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

async fn run_text(w: &super::harness::World, binding: &McpBinding, args: Value) -> Value {
    parse(w.core.code_tool_text(binding, RUN_TOOL, &args).await).0
}

fn tool_calls(w: &super::harness::World) -> usize {
    w.core
        .store
        .list_audit(Some(&w.lead), 500)
        .unwrap()
        .iter()
        .filter(|event| event.action == "tool.call")
        .count()
}

/// A run that is paused after its first call, then lost with the core's memory.
async fn lost_run(w: &super::harness::World, binding: &McpBinding) -> (String, &'static str) {
    const CODE: &str = "call orbyn_environment_read\nsleep 400\ncall orbyn_environment_read";
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_millis(80));
    let paused = run_text(w, binding, json!({ "code": CODE })).await;
    assert_eq!(paused["status"], "paused", "{paused}");
    let id = paused["runId"].as_str().unwrap().to_owned();
    w.core.code_runs.forget_all();
    w.core.code_runs_recover();
    (id, CODE)
}

#[tokio::test]
async fn a_run_is_recorded_with_its_script_calls_and_result() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_secs(10));
    let lead = lead_binding(&w);
    let done = run_text(&w, &lead, json!({ "code": "call orbyn_environment_read" })).await;
    let id = done["runId"].as_str().unwrap();
    let record = w.core.code_run_record(id).unwrap();
    assert_eq!(record.status, orbyn_contracts::CodeRunStatus::Completed);
    assert_eq!(
        record.script.as_deref(),
        Some("call orbyn_environment_read")
    );
    assert_eq!(record.calls.len(), 1);
    assert_eq!(record.calls[0].tool, "orbyn_environment_read");
    assert_eq!(record.calls[0].status, "ok");
    assert!(
        record.calls[0]
            .preview
            .as_deref()
            .unwrap()
            .contains(&w.lead)
    );
    assert!(record.result.is_some() && record.ended_at.is_some() && record.pending.is_empty());
    let listed = w.core.code_runs_list(&w.lead).unwrap();
    assert_eq!(listed.runs.len(), 1);
    assert_eq!(listed.runs[0].calls, 1);
    assert!(w.core.code_runs_list("nobody").unwrap().runs.is_empty());
}

#[tokio::test]
async fn a_run_id_that_is_not_ours_is_not_found_and_the_flag_gates_every_read() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    for id in ["", "../x", "a b", &"x".repeat(65), "no-such-run"] {
        assert!(w.core.code_run_record(id).is_err(), "{id:?}");
    }
    assert!(w.core.code_mode_setting("bad id!").is_err());
}

#[tokio::test]
async fn a_lost_run_replays_its_recorded_calls_without_running_them_again() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let lead = lead_binding(&w);
    let (id, code) = lost_run(&w, &lead).await;
    let record = w.core.code_run_record(&id).unwrap();
    assert_eq!(record.status, orbyn_contracts::CodeRunStatus::Error);
    assert_eq!(record.error.as_ref().unwrap().kind, "lost");
    // The lost reply, and a resume without the code cannot revive it.
    let lost = run_text(&w, &lead, json!({ "run_id": id })).await;
    assert_eq!(lost["error"]["kind"], "lost", "{lost}");
    let before = tool_calls(&w);
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_secs(10));
    let done = run_text(&w, &lead, json!({ "code": code, "run_id": id })).await;
    assert_eq!(done["status"], "completed", "{done}");
    assert_eq!(done["runId"], json!(id));
    assert_eq!(done["result"].as_array().unwrap().len(), 2, "{done}");
    assert_eq!(
        tool_calls(&w) - before,
        2,
        "the outer call and the one new inner call; the recorded call answered again without running"
    );
    let record = w.core.code_run_record(&id).unwrap();
    assert_eq!(record.status, orbyn_contracts::CodeRunStatus::Completed);
    assert_eq!(record.resumes, 1);
    assert_eq!(record.calls.len(), 2);
    // Finished and out of memory, a resume answers from the record.
    w.core.code_runs.forget_all();
    let again = run_text(&w, &lead, json!({ "run_id": id })).await;
    assert_eq!(again["status"], "completed", "{again}");
}

#[tokio::test]
async fn a_replay_with_other_code_or_another_call_ends_as_lost() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let lead = lead_binding(&w);
    let (id, code) = lost_run(&w, &lead).await;
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_secs(10));
    let changed = run_text(
        &w,
        &lead,
        json!({ "code": format!("{code}\ncall orbyn_environment_read"), "run_id": id }),
    )
    .await;
    assert_eq!(changed["error"]["kind"], "lost", "{changed}");
    assert!(
        changed["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not repeatable")
    );
    // The same code, but the record says the first call was another tool.
    w.core
        .store
        .code_call_tamper(&id, 1, Some("orbyn_thread_list"), None, None)
        .unwrap();
    let before = tool_calls(&w);
    let mismatch = run_text(&w, &lead, json!({ "code": code, "run_id": id })).await;
    assert_eq!(mismatch["error"]["kind"], "lost", "{mismatch}");
    assert_eq!(
        tool_calls(&w),
        before + 1,
        "only the outer call is recorded; nothing ran after the mismatch"
    );
}

#[tokio::test]
async fn a_call_with_an_effect_in_flight_at_the_restart_ends_the_replay_as_lost() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let lead = lead_binding(&w);
    let (id, code) = lost_run(&w, &lead).await;
    w.core
        .store
        .code_call_tamper(&id, 1, None, Some("running"), Some(true))
        .unwrap();
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_secs(10));
    let before = tool_calls(&w);
    let lost = run_text(&w, &lead, json!({ "code": &code, "run_id": id })).await;
    assert_eq!(lost["error"]["kind"], "lost", "{lost}");
    assert!(
        lost["error"]["message"]
            .as_str()
            .unwrap()
            .contains("may have changed something")
    );
    assert_eq!(tool_calls(&w), before + 1, "only the outer call");
    // A read that was in flight is simply run again.
    w.core
        .store
        .code_call_tamper(&id, 1, None, Some("running"), Some(false))
        .unwrap();
    let done = run_text(&w, &lead, json!({ "code": &code, "run_id": id })).await;
    assert_eq!(done["status"], "completed", "{done}");
}

#[tokio::test]
async fn only_the_runs_own_thread_can_replay_it() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let lead = lead_binding(&w);
    let (id, code) = lost_run(&w, &lead).await;
    let other = McpBinding {
        scope: crate::governance::approvals::RunScope {
            thread_id: "someone-else".into(),
            ..lead.scope.clone()
        },
        ..lead.clone()
    };
    let refused = run_text(&w, &other, json!({ "code": code, "run_id": id })).await;
    assert_eq!(refused["error"]["kind"], "lost", "{refused}");
    assert_eq!(
        w.core.code_run_record(&id).unwrap().status,
        orbyn_contracts::CodeRunStatus::Error,
        "still lost"
    );
}

#[tokio::test]
async fn code_mode_is_listed_only_for_an_engine_whose_switch_is_on() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    assert!(!w.core.code_mode_on("claude"), "off by default");
    assert!(!w.core.code_mode_setting("claude").unwrap().enabled);
    assert!(w.core.set_code_mode("claude", true).unwrap().enabled);
    assert!(w.core.code_mode_on("claude"));
    assert!(!w.core.code_mode_on("codex"), "another engine is unchanged");
    w.core.set_code_mode("claude", false).unwrap();
    assert!(!w.core.code_mode_on("claude"));
}

#[tokio::test]
async fn a_record_older_than_seven_days_is_deleted_with_its_calls() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_secs(10));
    let lead = lead_binding(&w);
    let done = run_text(&w, &lead, json!({ "code": "call orbyn_environment_read" })).await;
    let id = done["runId"].as_str().unwrap();
    let now = crate::util::unix_now();
    assert_eq!(w.core.store.code_runs_prune(now + 6 * 86_400).unwrap(), 0);
    assert!(w.core.code_run_record(id).is_ok());
    assert_eq!(w.core.store.code_runs_prune(now + 8 * 86_400).unwrap(), 1);
    assert!(w.core.code_run_record(id).is_err());
    assert!(w.core.store.code_calls(id).unwrap().is_empty());
}
