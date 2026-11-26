//! Code Mode through a real core: a script's calls go back through the core and a Crew worker's
//! script is held to the worker allow list.

use super::harness::{Turn, World, brief, key, world};
use super::review_support::lead_binding;
use crate::crew::workers::allowed;
use crate::store::Admission;
use crate::store::code_runs::{CodeRunEnd, StoredCodeCall};
use crate::tools::mcp::McpBinding;
use crate::tools::mcp::code::{DESCRIBE_TOOL, RUN_TOOL, SEARCH_TOOL, fake::Fake, is_inner_tool};
use async_trait::async_trait;
use orbyn_code_sandbox::{CodeExecutor, Limits, Run, RunError, RunSeed, ToolHost};
use orbyn_contracts::{
    ApprovalState, CodeRunCall, CodeRunRecord, CodeRunStatus, CreateThread, RunState,
};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

fn parse(answer: (String, bool)) -> (Value, bool) {
    (
        serde_json::from_str(&answer.0).unwrap_or(Value::String(answer.0)),
        answer.1,
    )
}

/// A run record's call at script index `index`: the record's key, never the read's row position.
fn recorded_call(record: &CodeRunRecord, index: u32) -> &CodeRunCall {
    record
        .calls
        .iter()
        .find(|call| call.index == index)
        .expect("the recorded call with that script index")
}

/// The stored call with script index `index`, read without relying on its row position.
fn stored_call(w: &World, run_id: &str, index: u32) -> StoredCodeCall {
    w.core
        .store
        .code_calls(run_id)
        .expect("the run's calls read")
        .into_iter()
        .find(|call| call.idx == index)
        .expect("the stored call with that script index")
}

/// A Crew worker's thread and the binding its run would use.
fn worker_binding(w: &World, task: &str) -> (String, McpBinding) {
    let own = w.worker(task);
    let lead = lead_binding(w);
    let binding = McpBinding {
        scope: crate::governance::approvals::RunScope {
            thread_id: own.clone(),
            run_id: "run-w".into(),
            ..lead.scope.clone()
        },
        ..lead.clone()
    };
    (own, binding)
}

/// Every tool name the product can list, from the three tool tables, plus the names the service
/// handles outside a table.
fn all_tool_names() -> Vec<String> {
    let mut names: Vec<String> = crate::tools::mcp_tools::tool_specs(true)
        .chain(crate::tools::mcp::code::specs())
        .chain(crate::crew::tool_specs())
        .filter_map(|spec| spec["name"].as_str().map(str::to_owned))
        .collect();
    names.extend(
        [
            "approve",
            "skill_search",
            "skill_load",
            "computer_browser_navigate",
            "computer_browser_text",
            "computer_browser_screenshot",
            "computer_browser_click",
            "computer_desktop",
            "computer_screenshot",
            "computer_click",
            "computer_type",
        ]
        .into_iter()
        .map(str::to_owned),
    );
    names.sort();
    names.dedup();
    names
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
    // The answers follow the script's call order; this script made one call.
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
    const CODE: &str = "sleep 400\ncall orbyn_environment_read";
    let (paused, _) = parse(
        w.core
            .code_tool_text(&lead, RUN_TOOL, &json!({ "code": CODE }))
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
    for args in [
        json!({ "run_id": run_id }),
        json!({ "code": CODE, "run_id": run_id }),
    ] {
        let (lost, _) = parse(w.core.code_tool_text(&other, RUN_TOOL, &args).await);
        assert_eq!(lost["error"]["kind"], "lost", "{lost}");
    }
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_secs(10));
    let (done, _) = parse(
        w.core
            .code_tool_text(&lead, RUN_TOOL, &json!({ "run_id": run_id }))
            .await,
    );
    assert_eq!(done["status"], "completed", "{done}");
    // A completed run of another thread is not confirmed either, with or without the code.
    for args in [
        json!({ "run_id": run_id }),
        json!({ "code": CODE, "run_id": run_id }),
    ] {
        let (lost, _) = parse(w.core.code_tool_text(&other, RUN_TOOL, &args).await);
        assert_eq!(lost["error"]["kind"], "lost", "{lost}");
    }
    // The owner still gets the completed answer from the record.
    let (again, _) = parse(
        w.core
            .code_tool_text(&lead, RUN_TOOL, &json!({ "run_id": run_id }))
            .await,
    );
    assert_eq!(again["status"], "completed", "{again}");
}

#[tokio::test]
async fn search_and_describe_never_show_a_worker_a_tool_it_cannot_call() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (started, _) = w.start(brief(json!({})), key()).await;
    let id = started["task"].as_str().unwrap().to_owned();
    let (worker_id, worker) = worker_binding(&w, &id);
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_secs(10));
    // The three Code Mode tools are on the worker allow list on purpose, and each inner call is
    // checked again: a worker may start, search and describe a script.
    for tool in [SEARCH_TOOL, DESCRIBE_TOOL, RUN_TOOL] {
        assert!(allowed(tool), "{tool} is allowed to a worker");
        assert!(
            w.core
                .refuse_for_crew_worker(&worker, tool, &json!({}))
                .is_ok(),
            "{tool}"
        );
    }
    let (found, is_error) = w
        .core
        .code_tool_text(
            &worker,
            SEARCH_TOOL,
            &json!({ "query": "worktree", "detail": "name", "limit": 50 }),
        )
        .await;
    assert!(!is_error && found.starts_with('['), "{found}");
    let (listed, is_error) = w
        .core
        .code_tool_text(
            &worker,
            DESCRIBE_TOOL,
            &json!({ "paths": ["orbyn_worktree_status", "orbyn_thread_send"] }),
        )
        .await;
    assert!(!is_error, "{listed}");
    let listed: Value = serde_json::from_str(&listed).unwrap();
    assert!(
        listed["declarations"]
            .as_str()
            .unwrap()
            .contains("declare function orbyn_worktree_status("),
        "{listed}"
    );
    assert_eq!(listed["unknown"], json!(["orbyn_thread_send"]), "{listed}");
    let run = run_text(
        &w,
        &worker,
        json!({ "code": "call orbyn_environment_read" }),
    )
    .await;
    assert_eq!(run["status"], "completed", "{run}");
    // The answers follow the script's call order; this script made one call.
    assert_eq!(run["result"][0]["thread_id"], json!(worker_id), "{run}");
    // The lead sees tools; its search never lists a Code Mode tool (a script may not nest).
    let lead = lead_binding(&w);
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

/// Claim B, search: a worker's search lists only tools it may call, at every detail, and no field
/// of the reply carries the name of one it may not.
#[tokio::test]
async fn a_workers_search_lists_only_tools_it_may_call() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (started, _) = w.start(brief(json!({})), key()).await;
    let id = started["task"].as_str().unwrap().to_owned();
    let (_worker, worker) = worker_binding(&w, &id);
    let names = all_tool_names();
    let queries = [
        "",
        "orbyn_thread_send",
        "orbyn_thread_launch",
        "crew_start",
        "computer_screenshot",
        "worktree status",
        "attachment",
        "send a message",
    ];
    for detail in ["name", "summary", "signature"] {
        for query in queries {
            let (text, is_error) = w
                .core
                .code_tool_text(
                    &worker,
                    SEARCH_TOOL,
                    &json!({ "query": query, "detail": detail, "limit": 50 }),
                )
                .await;
            assert!(!is_error, "{detail} {query:?}: {text}");
            let hits: Value = serde_json::from_str(&text)
                .unwrap_or_else(|why| panic!("{detail} {query:?}: {why}: {text}"));
            let hits = hits
                .as_array()
                .unwrap_or_else(|| panic!("{detail} {query:?} is not a list: {text}"));
            for hit in hits {
                let path = hit["path"]
                    .as_str()
                    .unwrap_or_else(|| panic!("{detail} {query:?} has no path: {hit}"));
                assert!(allowed(path), "{detail} {query:?}: {path} is not allowed");
                assert!(
                    is_inner_tool(path),
                    "{detail} {query:?}: {path} is not an inner tool"
                );
            }
            for name in &names {
                if !allowed(name) {
                    assert!(
                        !text.contains(name),
                        "{detail} {query:?}: {name} leaked in {text}"
                    );
                }
            }
            // Nesting stays refused too, so a worker never reads a Code Mode name here.
            for name in [SEARCH_TOOL, DESCRIBE_TOOL, RUN_TOOL] {
                assert!(
                    !text.contains(name),
                    "{detail} {query:?}: {name} leaked in {text}"
                );
            }
        }
    }
    // The query that matches everything really lists the worker's tools, including the one whose
    // description names another tool.
    let (text, _) = w
        .core
        .code_tool_text(
            &worker,
            SEARCH_TOOL,
            &json!({ "query": "", "detail": "summary", "limit": 50 }),
        )
        .await;
    assert!(text.contains("\"orbyn_worktree_status\""), "{text}");
    assert!(text.contains("\"orbyn_attachment_add\""), "{text}");
    assert!(!text.contains("orbyn_thread_send"), "{text}");
}

/// Claim B, describe: declarations come back only for tools the worker may call; a name it may not
/// comes back only as its own input under `unknown`, and describe never adds a name the caller
/// did not send.
#[tokio::test]
async fn a_workers_describe_declares_only_tools_it_may_call() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (started, _) = w.start(brief(json!({})), key()).await;
    let id = started["task"].as_str().unwrap().to_owned();
    let (_worker, worker) = worker_binding(&w, &id);
    let paths = json!([
        "orbyn_environment_read",
        "orbyn_attachment_add",
        "orbyn_thread_send",
        "orbyn_thread_launch",
        "orbyn_code_run",
        "crew_start",
        "no_such_tool",
    ]);
    let (text, is_error) = w
        .core
        .code_tool_text(&worker, DESCRIBE_TOOL, &json!({ "paths": paths }))
        .await;
    assert!(!is_error, "{text}");
    let listed: Value = serde_json::from_str(&text).unwrap();
    let declarations = listed["declarations"].as_str().unwrap();
    assert!(
        declarations.contains("declare function orbyn_environment_read("),
        "{declarations}"
    );
    assert!(
        declarations.contains("declare function orbyn_attachment_add("),
        "{declarations}"
    );
    assert_eq!(
        declarations.matches("declare function ").count(),
        2,
        "{declarations}"
    );
    // `unknown` is exactly the caller's own paths: describe adds no name the caller did not send.
    assert_eq!(
        listed["unknown"],
        json!([
            "orbyn_thread_send",
            "orbyn_thread_launch",
            "orbyn_code_run",
            "crew_start",
            "no_such_tool"
        ]),
        "{listed}"
    );
    for name in all_tool_names() {
        if !allowed(&name) {
            assert!(
                !declarations.contains(&name),
                "{name} leaked in {declarations}"
            );
        }
    }
    // A request for no tools declares nothing and has no unknown names.
    let (text, is_error) = w
        .core
        .code_tool_text(&worker, DESCRIBE_TOOL, &json!({ "paths": [] }))
        .await;
    assert!(!is_error, "{text}");
    let listed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(listed["declarations"], "", "{listed}");
    assert_eq!(listed["unknown"], json!([]), "{listed}");
    // Over the max is refused before anything is declared.
    let too_many: Vec<String> = (0..17)
        .map(|_| "orbyn_environment_read".to_owned())
        .collect();
    let (text, is_error) = w
        .core
        .code_tool_text(&worker, DESCRIBE_TOOL, &json!({ "paths": too_many }))
        .await;
    assert!(is_error, "{text}");
    assert!(text.contains("at most 16"), "{text}");
}

/// Claim A: the allow list is checked on every inner call, not only the first. The disallowed name
/// was never shown to the worker, and a replay answers the recorded refusals without running them.
#[tokio::test]
async fn a_workers_script_is_held_to_the_allow_list_on_every_call() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (started, _) = w.start(brief(json!({})), key()).await;
    let id = started["task"].as_str().unwrap().to_owned();
    let (worker, binding) = worker_binding(&w, &id);
    const DISALLOWED: [&str; 2] = ["orbyn_thread_send", "orbyn_thread_launch"];
    for tool in DISALLOWED {
        assert!(!allowed(tool), "{tool} is refused to a worker");
        assert!(is_inner_tool(tool), "{tool} is an ordinary Orbyn tool");
    }
    // Neither name was ever shown to the worker.
    let (found, _) = w
        .core
        .code_tool_text(
            &binding,
            SEARCH_TOOL,
            &json!({ "query": DISALLOWED.join(" "), "detail": "name", "limit": 50 }),
        )
        .await;
    for tool in DISALLOWED {
        assert!(!found.contains(tool), "{tool} was shown: {found}");
    }
    let (described, _) = w
        .core
        .code_tool_text(&binding, DESCRIBE_TOOL, &json!({ "paths": DISALLOWED }))
        .await;
    let described: Value = serde_json::from_str(&described).unwrap();
    assert_eq!(described["declarations"], "", "{described}");
    assert_eq!(described["unknown"], json!(DISALLOWED), "{described}");
    // Two allowed calls, then a refused one as the third call and a never-listed one as the
    // fourth. The pause lets the run be lost and replayed.
    const CODE: &str = "call orbyn_environment_read\ncall orbyn_preview_list\ntry orbyn_thread_send\ntry orbyn_thread_launch\nsleep 1200";
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_millis(500));
    let before = tool_calls_for(&w, &worker);
    let paused = run_text(&w, &binding, json!({ "code": CODE })).await;
    assert_eq!(paused["status"], "paused", "{paused}");
    let run_id = paused["runId"].as_str().unwrap().to_owned();
    assert_eq!(
        tool_calls_for(&w, &worker) - before,
        5,
        "the outer call and four inner calls"
    );
    let record = w.core.code_run_record(&run_id).unwrap();
    assert!(record.crew_worker, "the run belongs to a worker");
    assert_eq!(record.calls.len(), 4, "{record:?}");
    assert_eq!(
        record
            .calls
            .iter()
            .map(|call| call.status.as_str())
            .collect::<Vec<_>>(),
        ["ok", "ok", "failed", "failed"],
        "{record:?}"
    );
    for (call, tool) in record.calls.iter().zip([
        "orbyn_environment_read",
        "orbyn_preview_list",
        "orbyn_thread_send",
        "orbyn_thread_launch",
    ]) {
        assert_eq!(call.tool, tool);
    }
    // The record keeps the script's call order: calls 3 and 4 are its two refusals.
    for (call, tool) in record.calls[2..].iter().zip(DISALLOWED) {
        let preview: Value = serde_json::from_str(call.preview.as_deref().unwrap()).unwrap();
        assert_eq!(
            preview,
            json!({ "code": "invalid", "message": format!("A crew worker cannot use {tool}.") }),
            "the refusal, and no tool output"
        );
    }
    assert!(
        record.pending.is_empty(),
        "nothing waits on the gate: {record:?}"
    );
    assert!(
        w.core.store.pending_for_thread(&worker).unwrap().is_empty(),
        "the effect gate was never reached for a refused call"
    );
    // A replay answers the recorded calls without running them again; both refusals stay refusals.
    w.core.code_runs.forget_all();
    w.core.code_runs_recover();
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_secs(10));
    let before = tool_calls_for(&w, &worker);
    let replayed = run_text(&w, &binding, json!({ "code": CODE, "run_id": run_id })).await;
    assert_eq!(replayed["status"], "completed", "{replayed}");
    let results = replayed["result"].as_array().unwrap();
    assert_eq!(results.len(), 4, "{replayed}");
    // A replay answers the script's calls in the same order.
    assert_eq!(results[0]["thread_id"], json!(worker), "{replayed}");
    assert!(results[1].is_array(), "{replayed}");
    for (result, tool) in results[2..].iter().zip(DISALLOWED) {
        assert_eq!(result["code"], "invalid", "{replayed}");
        let message = result["message"].as_str().unwrap();
        assert!(message.contains("A crew worker cannot use"), "{replayed}");
        assert!(message.contains(tool), "{replayed}");
    }
    assert_eq!(
        tool_calls_for(&w, &worker) - before,
        1,
        "only the outer replay call; recorded calls answer without running"
    );
    let record = w.core.code_run_record(&run_id).unwrap();
    assert_eq!(record.calls.len(), 4, "{record:?}");
    assert_eq!(recorded_call(&record, 3).status, "failed");
    assert_eq!(recorded_call(&record, 4).status, "failed");
    assert_eq!(record.resumes, 1);
    assert!(
        w.core.store.pending_for_thread(&worker).unwrap().is_empty(),
        "the gate was not reached on the replay either"
    );
}

/// A worker may call the three Code Mode tools, but a script may not nest them.
#[tokio::test]
async fn a_workers_script_cannot_nest_code_mode_tools() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (started, _) = w.start(brief(json!({})), key()).await;
    let id = started["task"].as_str().unwrap().to_owned();
    let (_worker, worker) = worker_binding(&w, &id);
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_secs(10));
    for tool in [SEARCH_TOOL, DESCRIBE_TOOL, RUN_TOOL] {
        assert!(allowed(tool), "{tool} is allowed to a worker directly");
        let reply = run_text(&w, &worker, json!({ "code": format!("call {tool}") })).await;
        assert_eq!(reply["status"], "error", "{tool}: {reply}");
        assert_eq!(reply["error"]["kind"], "runtime", "{tool}: {reply}");
        let message = reply["error"]["message"].as_str().unwrap();
        assert!(
            message.contains("not available to scripts"),
            "{tool}: {reply}"
        );
        assert!(
            !message.contains("crew worker cannot use"),
            "{tool}: the outer call passed the allow list and nesting refused it: {reply}"
        );
    }
    // A made-up orbyn_code_ name is not admitted by a prefix rule.
    const MADE_UP: &str = "orbyn_code_made_up";
    assert!(!allowed(MADE_UP), "the three names are explicit");
    let reply = run_text(&w, &worker, json!({ "code": format!("call {MADE_UP}") })).await;
    assert_eq!(reply["status"], "error", "{MADE_UP}: {reply}");
    assert!(
        reply["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not available to scripts"),
        "{MADE_UP}: {reply}"
    );
}

/// The attachment tools' own-thread check still applies inside a worker's script.
#[tokio::test]
async fn an_attachment_tool_in_a_workers_script_stays_on_its_own_thread() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (started, _) = w.start(brief(json!({})), key()).await;
    let id = started["task"].as_str().unwrap().to_owned();
    let (worker, binding) = worker_binding(&w, &id);
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_secs(10));
    let code = format!(
        "try orbyn_attachment_add {{\"thread_id\":\"{worker}\",\"path\":\"src/base.txt\"}}\n\
         try orbyn_attachment_add {{\"thread_id\":\"{}\",\"path\":\"src/base.txt\"}}\n\
         try orbyn_attachment_discard {{\"thread_id\":\"{}\",\"attachment_id\":\"x\"}}",
        w.lead, w.lead
    );
    let reply = run_text(&w, &binding, json!({ "code": code })).await;
    assert_eq!(reply["status"], "completed", "{reply}");
    let results = reply["result"].as_array().unwrap();
    // Its own thread passes the attachment check and reaches the Crew gate, which has no lead to
    // review it in this test. The lead's thread is refused before the gate, for both tools. The
    // answers follow the script's call order: its own thread first, then the lead's two.
    assert_eq!(results[0]["code"], "denied", "{reply}");
    assert!(
        !results[0]["message"]
            .as_str()
            .unwrap()
            .contains("only on its own thread"),
        "{reply}"
    );
    for result in &results[1..] {
        assert_eq!(result["code"], "invalid", "{reply}");
        assert!(
            result["message"]
                .as_str()
                .unwrap()
                .contains("only on its own thread"),
            "{reply}"
        );
    }
}

/// A replay answers from the record only after the allow list is checked again: a recorded call
/// that is no longer allowed is refused, while an allowed one still answers from the record.
#[tokio::test]
async fn a_replay_rechecks_the_allow_list_before_answering_from_the_record() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let project = w.core.store.thread(&w.lead).unwrap().project_id;
    let thread = w
        .core
        .create_thread(CreateThread {
            project_id: Some(project),
            engine: "alpha".into(),
            model: None,
            title: Some("replayed as a worker".into()),
            worktree: None,
            skill_profile: None,
            base_branch: None,
            effort: None,
            options: Default::default(),
        })
        .await
        .unwrap();
    let lead = lead_binding(&w);
    let binding = McpBinding {
        scope: crate::governance::approvals::RunScope {
            thread_id: thread.id.clone(),
            run_id: "run-replay".into(),
            ..lead.scope.clone()
        },
        ..lead.clone()
    };
    // Recorded as a lead run: both calls are allowed then.
    const CODE: &str = "call orbyn_environment_read\ntry orbyn_thread_list\nsleep 2000";
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_millis(800));
    let paused = run_text(&w, &binding, json!({ "code": CODE })).await;
    assert_eq!(paused["status"], "paused", "{paused}");
    let run_id = paused["runId"].as_str().unwrap().to_owned();
    // Lose the run while it is still paused, before any slower setup can let it finish.
    w.core.code_runs.forget_all();
    w.core.code_runs_recover();
    // The thread becomes a Crew worker between the record and the replay.
    let (started, _) = w.start(brief(json!({})), key()).await;
    let task_id = started["task"].as_str().unwrap().to_owned();
    w.core
        .store
        .set_crew_worker(&task_id, &thread.id, &thread.cwd, None)
        .unwrap();
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_secs(10));
    let replayed = run_text(&w, &binding, json!({ "code": CODE, "run_id": run_id })).await;
    assert_eq!(replayed["status"], "completed", "{replayed}");
    let results = replayed["result"].as_array().unwrap();
    // The replay's answers follow the script's call order.
    assert_eq!(
        results[0]["thread_id"],
        json!(thread.id),
        "the allowed recorded call answered from the record: {replayed}"
    );
    assert_eq!(results[1]["code"], "invalid", "{replayed}");
    assert!(
        results[1]["message"]
            .as_str()
            .unwrap()
            .contains("A crew worker cannot use orbyn_thread_list"),
        "{replayed}"
    );
}

async fn run_text(w: &super::harness::World, binding: &McpBinding, args: Value) -> Value {
    parse(w.core.code_tool_text(binding, RUN_TOOL, &args).await).0
}

fn tool_calls_for(w: &super::harness::World, thread: &str) -> usize {
    w.core
        .store
        .list_audit(Some(thread), 500)
        .unwrap()
        .iter()
        .filter(|event| event.action == "tool.call")
        .count()
}

fn tool_calls(w: &super::harness::World) -> usize {
    tool_calls_for(w, &w.lead)
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
    let call = recorded_call(&record, 1);
    assert_eq!(call.tool, "orbyn_environment_read");
    assert_eq!(call.status, "ok");
    assert!(call.preview.as_deref().unwrap().contains(&w.lead));
    assert!(record.result.is_some() && record.ended_at.is_some() && record.pending.is_empty());
    let listed = w.core.code_runs_list(&w.lead).unwrap();
    assert_eq!(listed.runs.len(), 1);
    let row = listed
        .runs
        .iter()
        .find(|row| row.run_id == id)
        .expect("the run's row in the list");
    assert_eq!(row.calls, 1);
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

/// The rows the tool wrote at its effect boundary: attachment rows on the lead thread. The audit
/// rows a call writes are a separate record and never stand in for the effect.
fn attachments(w: &World) -> Vec<orbyn_contracts::Attachment> {
    w.core
        .store
        .list_attachments(&w.lead)
        .expect("the lead thread's attachments read")
        .into_iter()
        .map(|(attachment, _)| attachment)
        .collect()
}

#[tokio::test]
async fn a_run_replayed_after_a_real_restart_does_not_repeat_its_effect() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_millis(80));
    let lead = effect_binding(&w);
    // The first call is an effect tool, so its row is the effect boundary. The five seconds after it
    // keep the first run live through a real restart; the same line runs on the replay, which then
    // answers the recorded call from the record and finishes.
    let code = format!(
        "call orbyn_attachment_add {{\"thread_id\":\"{}\",\"path\":\"src/base.txt\"}}\nsleep 5000",
        w.lead
    );
    let paused = run_text(&w, &lead, json!({ "code": &code })).await;
    assert_eq!(paused["status"], "paused", "{paused}");
    let run_id = paused["runId"]
        .as_str()
        .expect("a paused run names its id")
        .to_owned();
    assert_eq!(attachments(&w).len(), 0, "nothing ran before the approval");
    let approval = next_pending_for(&w, &w.lead, &run_id).await;
    w.core
        .gate
        .decide_with(&approval.id, true, false, None, "test-person")
        .expect("the person allows the script's call");
    w.until("the approved effect is recorded", |w| {
        stored_call(w, &run_id, 1).status == "ok"
    })
    .await;

    let written = attachments(&w);
    assert_eq!(
        written.len(),
        1,
        "the tool wrote exactly one attachment row"
    );
    let attachment_id = written
        .into_iter()
        .next()
        .expect("the one attachment row")
        .id;
    let record = w
        .core
        .code_run_record(&run_id)
        .expect("the paused run's record reads");
    assert_eq!(record.status, CodeRunStatus::Paused, "{record:?}");
    assert_eq!(record.calls.len(), 1, "{record:?}");
    let before = stored_call(&w, &run_id, 1);
    assert_eq!(before.tool, "orbyn_attachment_add");
    assert_eq!(before.status, "ok");
    assert!(before.effect, "the recorded call is an effect call");
    let preview = before.preview;
    let result = before.result.expect("a small result is kept whole");
    let calls = tool_calls(&w);

    // A real restart: the process's core goes away and a new one opens the same data folder.
    let w = w.restart().await;
    let recovered = w
        .core
        .code_run_record(&run_id)
        .expect("the lost run's record reads");
    assert_eq!(recovered.status, CodeRunStatus::Error, "{recovered:?}");
    assert_eq!(
        recovered.error.as_ref().map(|error| error.kind.as_str()),
        Some("lost"),
        "{recovered:?}"
    );
    assert_eq!(recovered.calls.len(), 1, "the record kept the call");
    let call = recorded_call(&recovered, 1);
    assert_eq!(call.tool, "orbyn_attachment_add");
    assert_eq!(call.status, "ok");
    assert_eq!(call.preview, preview, "the same preview");
    assert_eq!(attachments(&w).len(), 1, "the effect is still the one row");

    // The replay gets the same code: the recorded call answers from the record, nothing runs it
    // again, and the effect boundary stays at one.
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_secs(10));
    let lead = lead_binding(&w);
    let replayed = run_text(&w, &lead, json!({ "code": &code, "run_id": &run_id })).await;
    assert_eq!(replayed["status"], "completed", "{replayed}");
    assert_eq!(replayed["runId"], json!(run_id));
    let recorded: Value = serde_json::from_str(&result).expect("the stored result is JSON");
    assert_eq!(replayed["result"], json!([recorded]), "{replayed}");
    let written = attachments(&w);
    assert_eq!(written.len(), 1, "the replay repeated no effect");
    assert!(
        written.iter().any(|row| row.id == attachment_id),
        "the row is the first run's"
    );
    assert_eq!(
        tool_calls(&w),
        calls + 1,
        "only the outer orbyn_code_run call ran; the recorded inner call did not"
    );
    let record = w
        .core
        .code_run_record(&run_id)
        .expect("the replayed run's record reads");
    assert_eq!(record.status, CodeRunStatus::Completed, "{record:?}");
    assert_eq!(record.resumes, 1, "{record:?}");
    assert_eq!(record.calls.len(), 1, "the replay added no call row");
    assert_eq!(
        recorded_call(&record, 1).preview,
        preview,
        "the same result answer"
    );
    let after = stored_call(&w, &run_id, 1);
    assert_eq!(after.result, Some(result), "the stored result is unchanged");
}

/// Only built with the `code-mode` feature (a workspace test run turns it on through `orbynd`).
/// The same restart, with the real QuickJS executor instead of the scripted one: a replay of real
/// JavaScript must reach the same calls in the same order, so the recorded first call answers from the
/// record, the in-flight second call ends the run as lost, and the first effect is never written twice.
#[cfg(feature = "code-mode")]
#[tokio::test]
async fn real_javascript_replay_reaches_the_in_flight_call_without_repeating_the_first() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    w.core.code_runs.set_executor(
        Arc::new(orbyn_code_sandbox::QuickJsExecutor),
        Duration::from_millis(300),
    );
    let lead = effect_binding(&w);
    let code = format!(
        "const first = await tools.orbyn_attachment_add({{ thread_id: \"{thread}\", path: \"src/base.txt\" }});\n\
         await tools.orbyn_thread_organize({{ thread_id: \"{thread}\", pinned: true }});\n\
         return first;",
        thread = w.lead
    );
    let paused = run_text(&w, &lead, json!({ "code": &code })).await;
    assert_eq!(paused["status"], "paused", "{paused}");
    let run_id = paused["runId"]
        .as_str()
        .expect("a paused run names its id")
        .to_owned();
    let first = next_pending_for(&w, &w.lead, &run_id).await;
    w.core
        .gate
        .decide_with(&first.id, true, false, None, "test-person")
        .expect("the person allows the first call");
    // The second call now waits for the person: the run is live, with one finished call. Bounded on
    // the harness clock so a second call that never pends fails by name instead of spinning the
    // test binary.
    let second = tokio::time::timeout(super::harness::WAIT_LIMIT, async {
        loop {
            let next = next_pending_for(&w, &w.lead, &run_id).await;
            if next.id != first.id {
                break next;
            }
        }
    })
    .await
    .expect("the second call became pending for the person");
    assert_eq!(second.via.as_ref().map(|via| via.call), Some(2));
    assert_eq!(attachments(&w).len(), 1, "the first effect ran once");
    let recorded = stored_call(&w, &run_id, 1);
    assert_eq!(recorded.tool, "orbyn_attachment_add");
    assert_eq!(recorded.status, "ok");

    let w = w.restart().await;
    w.core.code_runs.set_executor(
        Arc::new(orbyn_code_sandbox::QuickJsExecutor),
        Duration::from_millis(300),
    );
    let lead = lead_binding(&w);
    let replayed = run_text(&w, &lead, json!({ "code": &code, "run_id": &run_id })).await;
    // The first call answered from the record, so the replay reached the second call in the same order
    // with the same arguments; that call was in flight with an effect, so the run ends as lost for that
    // reason (a changed order or argument would end it as "not repeatable" instead).
    assert_eq!(replayed["status"], "error", "{replayed}");
    assert_eq!(replayed["error"]["kind"], "lost", "{replayed}");
    assert!(
        replayed["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("may have changed something")),
        "{replayed}"
    );
    assert_eq!(attachments(&w).len(), 1, "the replay repeated no effect");
    let after = stored_call(&w, &run_id, 1);
    assert_eq!(
        after.result, recorded.result,
        "the first answer is the recorded one"
    );
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

/// A script line that asks the gate for one thread change, so a test has a pending approval.
fn organize(w: &World, pinned: bool) -> String {
    format!(
        "call orbyn_thread_organize {{\"thread_id\":\"{}\",\"pinned\":{pinned}}}",
        w.lead
    )
}

/// The lead's binding with a real, non-terminal conversation run, which the gate needs to record an
/// approval of a script's inner call.
fn effect_binding(w: &World) -> McpBinding {
    let lead = lead_binding(w);
    let run = w
        .core
        .store
        .admit_run(Admission {
            thread_id: &w.lead,
            prompt: "script",
            actor: "test",
        })
        .unwrap();
    McpBinding {
        scope: crate::governance::approvals::RunScope {
            run_id: run.id,
            ..lead.scope.clone()
        },
        ..lead
    }
}

/// A fake whose `prepare` waits for a permit, so a test can cancel while a start is still starting.
struct GatePrepare {
    gate: Arc<Semaphore>,
}

#[async_trait]
impl CodeExecutor for GatePrepare {
    async fn prepare(
        &self,
        source: &str,
        limits: Limits,
        seed: RunSeed,
        host: Arc<dyn ToolHost>,
    ) -> Result<Box<dyn Run>, RunError> {
        self.gate.acquire().await.unwrap().forget();
        Fake.prepare(source, limits, seed, host).await
    }
}

/// Polls `done` on the harness clock: a row another task writes must appear, and a loaded machine
/// must not decide whether it appears in time. A test's wait is not a product budget (#1063, tdd 3.7);
/// the panic still names the wait, so a real hang fails by name.
async fn until<T>(what: &str, mut done: impl FnMut() -> Option<T>) -> T {
    let give_up = std::time::Instant::now() + super::harness::WAIT_LIMIT;
    loop {
        if let Some(value) = done() {
            return value;
        }
        assert!(
            std::time::Instant::now() < give_up,
            "never happened in {:?}: {what}",
            super::harness::WAIT_LIMIT
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

async fn next_pending_for(w: &World, thread: &str, run_id: &str) -> orbyn_contracts::Approval {
    until(&format!("an approval became pending for {run_id}"), || {
        w.core
            .store
            .pending_for_thread(thread)
            .unwrap()
            .into_iter()
            .find(|approval| {
                approval
                    .via
                    .as_ref()
                    .is_some_and(|via| via.code_run == run_id)
            })
    })
    .await
}

async fn next_pending(w: &World, thread: &str) -> orbyn_contracts::Approval {
    until(&format!("an approval became pending for {thread}"), || {
        w.core.store.pending_for_thread(thread).unwrap().pop()
    })
    .await
}

fn cancelled_error(record: &CodeRunRecord) {
    assert_eq!(record.status, CodeRunStatus::Error, "{record:?}");
    let error = record.error.as_ref().expect("a cancelled run has an error");
    assert_eq!(error.kind, "cancelled", "{record:?}");
    assert_eq!(error.message, "The run was cancelled.", "{record:?}");
}

#[tokio::test]
async fn a_paused_run_with_a_pending_approval_is_cancelled_and_the_approval_is_denied() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_millis(50));
    let lead = effect_binding(&w);
    let code = organize(&w, true);
    let paused = run_text(&w, &lead, json!({ "code": code })).await;
    assert_eq!(paused["status"], "paused", "{paused}");
    let run_id = paused["runId"].as_str().unwrap().to_owned();
    let approval = next_pending(&w, &w.lead).await;
    assert_eq!(
        approval.via.as_ref().map(|via| via.code_run.as_str()),
        Some(run_id.as_str())
    );

    let record = w.core.code_run_cancel(&run_id).await.unwrap();
    cancelled_error(&record);
    assert_eq!(record.calls.len(), 1, "{record:?}");
    let call = recorded_call(&record, 1);
    assert_eq!(call.status, "failed", "{record:?}");
    assert!(
        call.preview
            .as_deref()
            .unwrap()
            .contains("The run was cancelled."),
        "{record:?}"
    );
    assert!(record.pending.is_empty(), "{record:?}");
    assert!(
        w.core.store.pending_for_thread(&w.lead).unwrap().is_empty(),
        "the approval is gone from the queue"
    );
    let decided = w.core.store.approval(&approval.id).unwrap();
    assert_eq!(decided.state, ApprovalState::Denied);
    assert_eq!(decided.decided_by.as_deref(), Some("user"));
    let audits = w.core.store.list_audit(Some(&w.lead), 100).unwrap();
    assert_eq!(
        audits
            .iter()
            .filter(|row| row.action == "code_run.cancel" && row.target == run_id)
            .count(),
        1
    );

    // Cancelling twice answers the same record and writes nothing new.
    let again = w.core.code_run_cancel(&run_id).await.unwrap();
    assert_eq!(again, record);
    assert_eq!(
        w.core
            .store
            .list_audit(Some(&w.lead), 100)
            .unwrap()
            .iter()
            .filter(|row| row.action == "code_run.cancel")
            .count(),
        1
    );
}

#[tokio::test]
async fn a_running_run_is_cancelled_while_its_outer_call_is_still_waiting() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_secs(10));
    let lead = lead_binding(&w);
    let core = w.core.clone();
    let binding = lead.clone();
    let outer = tokio::spawn(async move {
        core.code_tool_text(&binding, RUN_TOOL, &json!({ "code": "sleep 60000" }))
            .await
    });
    let run_id = until("the run is still running inside its outer call", || {
        w.core
            .code_runs_list(&w.lead)
            .unwrap()
            .runs
            .iter()
            .find(|row| row.status == CodeRunStatus::Running)
            .map(|row| row.run_id.clone())
    })
    .await;
    let record = w.core.code_run_cancel(&run_id).await.unwrap();
    cancelled_error(&record);
    // The outer call returns promptly with the same cancelled reply a resume gives; a run that was
    // not aborted would hold the call until its 10 s slice passed.
    let answer = tokio::time::timeout(Duration::from_secs(2), outer)
        .await
        .expect("the aborted run ends its outer call")
        .unwrap();
    let reply: Value = serde_json::from_str(&answer.0).unwrap();
    assert_eq!(reply["status"], "error", "{reply}");
    assert_eq!(reply["error"]["kind"], "cancelled", "{reply}");
    cancelled_error(&w.core.code_run_record(&run_id).unwrap());
}

#[tokio::test]
async fn a_cancel_during_prepare_never_spawns_the_script_and_releases_the_slot() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let gate = Arc::new(Semaphore::new(0));
    w.core.code_runs.set_executor(
        Arc::new(GatePrepare { gate: gate.clone() }),
        Duration::from_secs(10),
    );
    let lead = lead_binding(&w);
    let core = w.core.clone();
    let binding = lead.clone();
    let outer = tokio::spawn(async move {
        core.code_tool_text(&binding, RUN_TOOL, &json!({ "code": "return 1" }))
            .await
    });
    let run_id = until("the row is written before prepare", || {
        w.core
            .code_runs_list(&w.lead)
            .unwrap()
            .runs
            .iter()
            .find(|row| row.status == CodeRunStatus::Running)
            .map(|row| row.run_id.clone())
    })
    .await;

    let record = w.core.code_run_cancel(&run_id).await.unwrap();
    cancelled_error(&record);
    gate.add_permits(1);
    let answer = tokio::time::timeout(Duration::from_secs(2), outer)
        .await
        .expect("the cancelled prepare returns promptly")
        .unwrap();
    let reply: Value = serde_json::from_str(&answer.0).unwrap();
    assert_eq!(reply["error"]["kind"], "cancelled", "{reply}");
    cancelled_error(&w.core.code_run_record(&run_id).unwrap());

    // The preparing start released its reservation: the thread can start another run.
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_secs(10));
    let next = run_text(&w, &lead, json!({ "code": "return 1" })).await;
    assert_eq!(next["status"], "completed", "{next}");
}

#[tokio::test]
async fn two_concurrent_cancels_answer_the_record_once() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_millis(50));
    let lead = effect_binding(&w);
    let paused = run_text(&w, &lead, json!({ "code": organize(&w, true) })).await;
    assert_eq!(paused["status"], "paused", "{paused}");
    let run_id = paused["runId"].as_str().unwrap().to_owned();
    let _approval = next_pending_for(&w, &w.lead, &run_id).await;

    let (first, second) = tokio::join!(
        w.core.code_run_cancel(&run_id),
        w.core.code_run_cancel(&run_id)
    );
    cancelled_error(&first.unwrap());
    cancelled_error(&second.unwrap());
    assert_eq!(
        w.core
            .store
            .list_audit(Some(&w.lead), 100)
            .unwrap()
            .iter()
            .filter(|row| row.action == "code_run.cancel")
            .count(),
        1,
        "only the cancel that did the work audits"
    );
    assert!(
        w.core.store.pending_for_thread(&w.lead).unwrap().is_empty(),
        "the queued approval is denied"
    );
}

#[tokio::test]
async fn a_crew_workers_run_is_cancelled_by_the_person() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (started, _) = w.start(brief(json!({})), key()).await;
    let id = started["task"].as_str().unwrap().to_owned();
    let (_worker, binding) = worker_binding(&w, &id);
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_millis(50));
    let paused = run_text(&w, &binding, json!({ "code": "sleep 60000" })).await;
    assert_eq!(paused["status"], "paused", "{paused}");
    let run_id = paused["runId"].as_str().unwrap().to_owned();
    let record = w.core.code_run_cancel(&run_id).await.unwrap();
    cancelled_error(&record);
    assert!(record.crew_worker, "{record:?}");
}

#[tokio::test]
async fn a_late_call_answer_after_a_cancel_changes_nothing() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_millis(50));
    let lead = effect_binding(&w);
    let paused = run_text(&w, &lead, json!({ "code": organize(&w, true) })).await;
    let run_id = paused["runId"].as_str().unwrap().to_owned();
    let _approval = next_pending(&w, &w.lead).await;
    let record = w.core.code_run_cancel(&run_id).await.unwrap();
    cancelled_error(&record);
    assert!(
        !w.core
            .store
            .code_call_begin(&run_id, 2, "orbyn_environment_read", "digest", false)
            .unwrap(),
        "no call can begin after the run ended"
    );

    // A call's own end and the task's own finish both arrive late: neither flips the record.
    w.core
        .store
        .code_call_end(&run_id, 1, "ok", "{\"late\":true}", 1)
        .unwrap();
    let end = CodeRunEnd {
        status: "completed",
        ended_at: crate::util::now(),
        error: None,
        result: Some("{\"late\":true}".into()),
        logs: vec!["late".into()],
        logs_total: 1,
        truncated: false,
    };
    w.core.store.code_run_end(&run_id, &end).unwrap();
    let record = w.core.code_run_record(&run_id).unwrap();
    cancelled_error(&record);
    assert_eq!(recorded_call(&record, 1).status, "failed", "{record:?}");
    assert_eq!(record.result, None, "{record:?}");
    assert_eq!(record.logs, Vec::<String>::new(), "{record:?}");
}

#[tokio::test]
async fn a_finished_run_is_not_cancellable_and_unknown_or_lost_runs_are_not() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_secs(10));
    let lead = lead_binding(&w);
    let done = run_text(&w, &lead, json!({ "code": "call orbyn_environment_read" })).await;
    let run_id = done["runId"].as_str().unwrap().to_owned();
    let error = w.core.code_run_cancel(&run_id).await.unwrap_err();
    assert_eq!((error.status(), error.code()), (409, "run_finished"));
    assert_eq!(error.public_message(), "This run already ended.");

    for id in ["", "../x", "a b", &"x".repeat(65), "no-such-run"] {
        let error = w.core.code_run_cancel(id).await.unwrap_err();
        assert_eq!((error.status(), error.code()), (404, "not_found"), "{id:?}");
    }

    // A lost run is not live and cannot be cancelled; it keeps its lost record.
    let (lost_id, _) = lost_run(&w, &lead).await;
    let error = w.core.code_run_cancel(&lost_id).await.unwrap_err();
    assert_eq!((error.status(), error.code()), (409, "run_finished"));
    assert_eq!(
        w.core
            .code_run_record(&lost_id)
            .unwrap()
            .error
            .unwrap()
            .kind,
        "lost"
    );
}

#[tokio::test]
async fn a_cancelled_run_cannot_be_resumed_or_replayed_even_after_a_restart() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_millis(50));
    let lead = effect_binding(&w);
    let code = organize(&w, true);
    let paused = run_text(&w, &lead, json!({ "code": code })).await;
    let run_id = paused["runId"].as_str().unwrap().to_owned();
    let _approval = next_pending(&w, &w.lead).await;
    w.core.code_run_cancel(&run_id).await.unwrap();
    assert!(
        !w.core.store.code_run_restart(&run_id).unwrap(),
        "a cancelled run cannot be restarted by a replay"
    );

    for restart in [false, true] {
        if restart {
            w.core.code_runs.forget_all();
            w.core.code_runs_recover();
        }
        for args in [
            json!({ "run_id": run_id }),
            json!({ "code": code, "run_id": run_id }),
        ] {
            let reply = run_text(&w, &lead, args).await;
            assert_eq!(reply["status"], "error", "restart {restart}: {reply}");
            assert_eq!(
                reply["error"]["kind"], "cancelled",
                "restart {restart}: {reply}"
            );
            assert_eq!(
                reply["error"]["message"], "The run was cancelled.",
                "restart {restart}: {reply}"
            );
        }
    }
}

#[tokio::test]
async fn a_cancel_releases_the_slot_for_a_new_start_at_the_cap() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_millis(50));
    let lead = lead_binding(&w);
    let mut ids = Vec::new();
    for _ in 0..4 {
        let paused = run_text(&w, &lead, json!({ "code": "sleep 60000" })).await;
        assert_eq!(paused["status"], "paused", "{paused}");
        ids.push(paused["runId"].as_str().unwrap().to_owned());
    }
    // Any one of the four runs frees its slot; which of them it is makes no difference.
    w.core
        .code_run_cancel(ids.first().expect("four runs at the cap"))
        .await
        .unwrap();
    let next = run_text(&w, &lead, json!({ "code": "sleep 60000" })).await;
    assert_eq!(next["status"], "paused", "{next}");
}

#[tokio::test]
async fn no_tool_or_script_can_cancel_a_run() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    assert!(
        !crate::tools::mcp::code::specs()
            .iter()
            .any(|spec| spec["name"].as_str().unwrap_or_default().contains("cancel"))
    );
    assert!(!is_inner_tool("orbyn_code_run_cancel"));
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_secs(10));
    let lead = lead_binding(&w);
    let reply = run_text(&w, &lead, json!({ "code": "call orbyn_code_run_cancel" })).await;
    assert_eq!(reply["status"], "error", "{reply}");
    assert!(
        reply["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not available to scripts"),
        "{reply}"
    );
}

#[tokio::test]
async fn waited_ms_sums_two_approvals_and_survives_a_pause() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_millis(50));
    let lead = effect_binding(&w);
    let code = format!("{}\n{}", organize(&w, true), organize(&w, false));
    let paused = run_text(&w, &lead, json!({ "code": code })).await;
    assert_eq!(paused["status"], "paused", "{paused}");
    let run_id = paused["runId"].as_str().unwrap().to_owned();
    let first = next_pending(&w, &w.lead).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    w.core
        .gate
        .decide_with(&first.id, true, false, None, "test-person")
        .unwrap();
    let paused_again = run_text(&w, &lead, json!({ "run_id": run_id })).await;
    assert_eq!(paused_again["status"], "paused", "{paused_again}");
    let mid = w.core.code_run_record(&run_id).unwrap();
    assert!(mid.waited_ms.unwrap() >= 100, "{mid:?}");
    let second = next_pending(&w, &w.lead).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    w.core
        .gate
        .decide_with(&second.id, true, false, None, "test-person")
        .unwrap();
    let done = run_text(&w, &lead, json!({ "run_id": run_id })).await;
    assert_eq!(done["status"], "completed", "{done}");
    let record = w.core.code_run_record(&run_id).unwrap();
    assert_eq!(record.status, CodeRunStatus::Completed);
    let waited = record.waited_ms.expect("a waited run has a number");
    assert!((200..=2_000).contains(&waited), "{record:?}");
    let summary = w
        .core
        .code_runs_list(&w.lead)
        .unwrap()
        .runs
        .into_iter()
        .find(|run| run.run_id == run_id)
        .unwrap();
    assert_eq!(summary.waited_ms, Some(waited));
}

#[tokio::test]
async fn waited_ms_counts_an_approval_that_expires_with_its_run() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_millis(50));
    let lead = effect_binding(&w);
    let run_id = lead.scope.run_id.clone();
    let paused = run_text(&w, &lead, json!({ "code": organize(&w, true) })).await;
    assert_eq!(paused["status"], "paused", "{paused}");
    let code_run = paused["runId"].as_str().unwrap().to_owned();
    let approval = next_pending_for(&w, &w.lead, &code_run).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    let (_, expired) = w
        .core
        .store
        .finish_run(&run_id, RunState::Cancelled, None)
        .unwrap();
    assert_eq!(expired.len(), 1);
    assert_eq!(
        w.core.store.approval(&approval.id).unwrap().state,
        ApprovalState::Expired
    );
    let waited = w
        .core
        .code_run_record(&code_run)
        .unwrap()
        .waited_ms
        .expect("an expiry records the wait");
    assert!((100..=2_000).contains(&waited), "{waited}");
}

#[tokio::test]
async fn waited_ms_counts_only_the_approvals_of_the_run_that_asked() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_millis(50));
    let lead = effect_binding(&w);
    let first = run_text(&w, &lead, json!({ "code": organize(&w, true) })).await;
    let second = run_text(&w, &lead, json!({ "code": organize(&w, false) })).await;
    assert_eq!(first["status"], "paused", "{first}");
    assert_eq!(second["status"], "paused", "{second}");
    let first_id = first["runId"].as_str().unwrap().to_owned();
    let second_id = second["runId"].as_str().unwrap().to_owned();
    let first_approval = next_pending_for(&w, &w.lead, &first_id).await;
    let second_approval = next_pending_for(&w, &w.lead, &second_id).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    w.core
        .gate
        .decide_with(&first_approval.id, true, false, None, "test-person")
        .unwrap();
    let first_record = w.core.code_run_record(&first_id).unwrap();
    let first_waited = first_record.waited_ms.expect("the first run waited");
    assert!((100..=2_000).contains(&first_waited), "{first_record:?}");
    assert_eq!(
        w.core.code_run_record(&second_id).unwrap().waited_ms,
        Some(0),
        "the second run's pending approval is not counted under the first"
    );
    w.core
        .gate
        .decide_with(&second_approval.id, true, false, None, "test-person")
        .unwrap();
    let second_waited = w
        .core
        .code_run_record(&second_id)
        .unwrap()
        .waited_ms
        .expect("the second run waited");
    assert!((100..=2_000).contains(&second_waited), "{second_waited}");
    assert_eq!(
        w.core.code_run_record(&first_id).unwrap().waited_ms,
        Some(first_waited)
    );
}

#[tokio::test]
async fn an_old_null_run_that_gets_an_approval_stays_null() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_millis(50));
    let lead = effect_binding(&w);
    let paused = run_text(&w, &lead, json!({ "code": organize(&w, true) })).await;
    assert_eq!(paused["status"], "paused", "{paused}");
    let run_id = paused["runId"].as_str().unwrap().to_owned();
    let approval = next_pending_for(&w, &w.lead, &run_id).await;
    w.core.store.code_run_set_waited(&run_id, None).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    w.core
        .gate
        .decide_with(&approval.id, true, false, None, "test-person")
        .unwrap();
    assert_eq!(w.core.code_run_record(&run_id).unwrap().waited_ms, None);
}

#[tokio::test]
async fn waited_ms_is_capped_and_a_failed_wait_write_never_rolls_back_the_decision() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_millis(50));
    let lead = effect_binding(&w);
    let paused = run_text(&w, &lead, json!({ "code": organize(&w, true) })).await;
    assert_eq!(paused["status"], "paused", "{paused}");
    let run_id = paused["runId"].as_str().unwrap().to_owned();
    let approval = next_pending_for(&w, &w.lead, &run_id).await;
    let cap = (i64::MAX / 2) as u64;
    w.core
        .store
        .code_run_set_waited(&run_id, Some(cap - 1))
        .unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;
    w.core
        .gate
        .decide_with(&approval.id, true, false, None, "test-person")
        .unwrap();
    assert_eq!(
        w.core.code_run_record(&run_id).unwrap().waited_ms,
        Some(cap),
        "the column is capped below i64::MAX and stays an integer"
    );

    // A broken wait update is logged but the approval decision still stands.
    let paused = run_text(&w, &lead, json!({ "code": organize(&w, true) })).await;
    assert_eq!(paused["status"], "paused", "{paused}");
    let run_id = paused["runId"].as_str().unwrap().to_owned();
    let approval = next_pending_for(&w, &w.lead, &run_id).await;
    w.core.store.code_run_break_waits().unwrap();
    w.core
        .gate
        .decide_with(&approval.id, true, false, None, "test-person")
        .unwrap();
    assert_eq!(
        w.core.store.approval(&approval.id).unwrap().state,
        ApprovalState::Allowed,
        "the decision is committed even when the derived wait fails"
    );
    assert_eq!(w.core.code_run_record(&run_id).unwrap().waited_ms, Some(0));
}

#[tokio::test]
async fn a_lost_run_is_restarted_once_and_a_cancelled_run_is_never_revived() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_millis(80));
    let lead = lead_binding(&w);
    let (run_id, _) = lost_run(&w, &lead).await;
    // Replay A reads the lost row and restarts it. The person then cancels A.
    assert!(w.core.store.code_run_restart(&run_id).unwrap());
    w.core.store.code_run_mark_cancelled(&run_id).unwrap();
    // Replay B had read the lost row too; its restart must not revive the cancelled run.
    assert!(
        !w.core.store.code_run_restart(&run_id).unwrap(),
        "a slower replay cannot take a cancelled run"
    );
    let record = w.core.code_run_record(&run_id).unwrap();
    cancelled_error(&record);
}

#[tokio::test]
async fn a_replay_that_lost_the_restart_answers_cancelled_or_lost() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_millis(50));
    let lead = effect_binding(&w);
    let paused = run_text(&w, &lead, json!({ "code": organize(&w, true) })).await;
    let cancelled_id = paused["runId"].as_str().unwrap().to_owned();
    let _approval = next_pending_for(&w, &w.lead, &cancelled_id).await;
    w.core.code_run_cancel(&cancelled_id).await.unwrap();
    let cancelled: Value = serde_json::from_str(&w.core.code_replay_taken(&cancelled_id)).unwrap();
    assert_eq!(cancelled["error"]["kind"], "cancelled", "{cancelled}");

    let (lost_id, _) = lost_run(&w, &lead).await;
    let lost: Value = serde_json::from_str(&w.core.code_replay_taken(&lost_id)).unwrap();
    assert_eq!(lost["error"]["kind"], "lost", "{lost}");
}

#[tokio::test]
async fn a_run_that_never_waited_is_zero_and_an_old_row_stays_null() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_secs(10));
    let lead = lead_binding(&w);
    let done = run_text(&w, &lead, json!({ "code": "call orbyn_environment_read" })).await;
    let run_id = done["runId"].as_str().unwrap().to_owned();
    let record = w.core.code_run_record(&run_id).unwrap();
    assert_eq!(record.waited_ms, Some(0));
    let summary = w
        .core
        .code_runs_list(&w.lead)
        .unwrap()
        .runs
        .into_iter()
        .find(|run| run.run_id == run_id)
        .unwrap();
    assert_eq!(summary.waited_ms, Some(0));
    w.core.store.code_run_set_waited(&run_id, None).unwrap();
    assert_eq!(w.core.code_run_record(&run_id).unwrap().waited_ms, None);
    let summary = w
        .core
        .code_runs_list(&w.lead)
        .unwrap()
        .runs
        .into_iter()
        .find(|run| run.run_id == run_id)
        .unwrap();
    assert_eq!(summary.waited_ms, None);
}

#[tokio::test]
async fn waited_ms_survives_a_replay() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_millis(50));
    let lead = effect_binding(&w);
    let code = format!(
        "{}\nsleep 1000\ncall orbyn_environment_read",
        organize(&w, true)
    );
    let paused = run_text(&w, &lead, json!({ "code": &code })).await;
    assert_eq!(paused["status"], "paused", "{paused}");
    let run_id = paused["runId"].as_str().unwrap().to_owned();
    let approval = next_pending(&w, &w.lead).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    w.core
        .gate
        .decide_with(&approval.id, true, false, None, "test-person")
        .unwrap();
    // The call's own answer must be recorded before the restart, or the replay would see an
    // effect in flight and end as lost instead of answering from the record.
    until("the call's answer is stored before the restart", || {
        (stored_call(&w, &run_id, 1).status == "ok").then_some(())
    })
    .await;
    w.core.code_runs.forget_all();
    w.core.code_runs_recover();
    let lost = w.core.code_run_record(&run_id).unwrap();
    let waited = lost
        .waited_ms
        .expect("the wait was recorded before the loss");
    assert!(waited >= 100, "{lost:?}");
    assert_eq!(lost.error.unwrap().kind, "lost");

    w.core
        .code_runs
        .set_executor(Arc::new(Fake), Duration::from_secs(10));
    let replayed = run_text(&w, &lead, json!({ "code": code, "run_id": run_id })).await;
    assert_eq!(replayed["status"], "completed", "{replayed}");
    let record = w.core.code_run_record(&run_id).unwrap();
    assert_eq!(record.status, CodeRunStatus::Completed);
    assert_eq!(record.waited_ms, Some(waited));
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
