//! Code Mode through a real core: a script's calls go back through the core and a Crew worker's
//! script is held to the worker allow list.

use super::harness::{Turn, World, brief, key, world};
use super::review_support::lead_binding;
use crate::crew::workers::allowed;
use crate::tools::mcp::McpBinding;
use crate::tools::mcp::code::{DESCRIBE_TOOL, RUN_TOOL, SEARCH_TOOL, fake::Fake, is_inner_tool};
use orbyn_contracts::CreateThread;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

fn parse(answer: (String, bool)) -> (Value, bool) {
    (
        serde_json::from_str(&answer.0).unwrap_or(Value::String(answer.0)),
        answer.1,
    )
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
    let mut names: Vec<String> = crate::tools::mcp_tools::tool_specs()
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
    assert_eq!(record.calls[2].status, "failed");
    assert_eq!(record.calls[3].status, "failed");
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
    // review it in this test. The lead's thread is refused before the gate, for both tools.
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
