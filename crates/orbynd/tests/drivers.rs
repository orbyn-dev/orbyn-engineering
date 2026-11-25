//! Each driver against the fake engine, over real subprocess pipes and through
//! the real service, so parsing, approvals and cleanup are all exercised.

mod support;

use std::time::Duration;

use serde_json::{Value, json};
use support::{FAKE, Options, Service, TempDir, setup, text_of, types_of};

fn pid_env(dir: &TempDir) -> (std::path::PathBuf, Options) {
    let file = dir.path().join("pids");
    let options = Options {
        env: vec![(
            "ORBYN_FAKE_PID_FILE".into(),
            file.to_string_lossy().into_owned(),
        )],
        ..Options::default()
    };
    (file, options)
}

/// A freshly built binary can take a moment to exec, so wait for the engine
/// itself rather than for a fixed delay.
async fn engine_started(pid_file: &std::path::Path) {
    for _ in 0..1000 {
        if std::fs::read_to_string(pid_file).is_ok_and(|text| !text.is_empty()) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("the engine never started");
}

#[cfg(unix)]
fn alive(pid: i32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success()
}

#[cfg(unix)]
async fn assert_all_dead(pid_file: &std::path::Path) {
    let text = std::fs::read_to_string(pid_file).unwrap_or_default();
    let pids: Vec<i32> = text.lines().filter_map(|line| line.parse().ok()).collect();
    for _ in 0..300 {
        if pids.iter().all(|pid| !alive(*pid)) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("engine processes survived: {pids:?}");
}

#[tokio::test]
async fn codex_turn_maps_to_events_and_ignores_subagents() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    service.admit(&thread, "say hi").await;
    let events = service.all_events(&thread).await;
    assert_eq!(service.finished(&thread).await, "completed");
    assert_eq!(text_of(&events), "hello from codex");
    let types = types_of(&events);
    assert_eq!(types[0], "RUN_STARTED");
    for wanted in [
        "REASONING_MESSAGE_START",
        "REASONING_MESSAGE_CONTENT",
        "REASONING_MESSAGE_END",
        "ACTIVITY_SNAPSHOT",
        "TEXT_MESSAGE_END",
    ] {
        assert!(types.iter().any(|t| t == wanted), "{wanted} in {types:?}");
    }
    assert_eq!(types.last().unwrap(), "RUN_FINISHED");
}

#[tokio::test]
async fn codex_run_ignores_a_completed_turn_that_is_not_its_own() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    service.admit(&thread, "[stale-turn]").await;
    assert_eq!(service.finished(&thread).await, "completed");
    let events = service.all_events(&thread).await;
    assert_eq!(text_of(&events), "hello from codex");
}

async fn assert_no_reply(service: &Service, thread: &str) {
    assert_eq!(service.finished(thread).await, "failed");
    let events = service.all_events(thread).await;
    let last = events.last().expect("events");
    assert!(
        last["event"]["message"]
            .as_str()
            .is_some_and(|message| message.starts_with("No reply")),
        "{last}"
    );
}

#[tokio::test]
async fn codex_words_of_an_earlier_turn_do_not_make_a_silent_prompt_a_reply() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    service.admit(&thread, "[leftover-turn] [silent]").await;
    assert_no_reply(&service, &thread).await;
    // The earlier turn's words are still shown.
    assert!(text_of(&service.all_events(&thread).await).contains("the old turn spoke"));
}

#[tokio::test]
async fn an_acp_update_for_another_session_does_not_make_a_silent_prompt_a_reply() {
    let dir = TempDir::new();
    let (service, _) = setup(&dir, Options::default(), "codex").await;
    custom_engine(&service, "acp", FAKE, json!(["acp"])).await;
    let projects = service.json("/api/projects").await;
    let thread = service
        .thread(projects[0]["id"].as_str().unwrap(), "custom")
        .await;
    service.admit(&thread, "[stray] [silent]").await;
    assert_no_reply(&service, &thread).await;
    assert!(!text_of(&service.all_events(&thread).await).contains("STRAY"));
}

#[tokio::test]
async fn a_codex_turn_waits_for_its_tools_to_start() {
    let dir = TempDir::new();
    let options = Options {
        env: vec![("ORBYN_FAKE_SLOW_TOOLS".into(), "1".into())],
        ..Options::default()
    };
    let (service, thread) = setup(&dir, options, "codex").await;
    service.admit(&thread, "say hi").await;
    assert_eq!(service.finished(&thread).await, "completed");
    assert_eq!(
        text_of(&service.all_events(&thread).await),
        "hello from codex"
    );
}

#[tokio::test]
async fn codex_runs_in_default_collaboration_even_with_plan_mode_saved() {
    let dir = TempDir::new();
    let file = dir.path().join("launches");
    let options = Options {
        env: vec![(
            "ORBYN_FAKE_LAUNCH_FILE".into(),
            file.to_string_lossy().into_owned(),
        )],
        ..Options::default()
    };
    let (service, thread) = setup(&dir, options, "codex").await;
    service.patch(&thread, json!({ "plan_mode": true, "model": "fake-big", "effort": "xhigh", "options": { "service_tier": "priority" } })).await;
    service.admit(&thread, "plan it").await;
    assert_eq!(service.finished(&thread).await, "completed");
    service.patch(&thread, json!({ "plan_mode": false })).await;
    service.admit(&thread, "continue normally").await;
    assert_eq!(service.finished(&thread).await, "completed");
    let entries: Vec<Value> = std::fs::read_to_string(file)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let turns: Vec<Value> = entries
        .iter()
        .filter(|entry| entry["mode"] == "codex-turn")
        .map(|entry| serde_json::from_str(entry["args"][0].as_str().unwrap()).unwrap())
        .collect();
    assert_eq!(turns.len(), 2);
    for (turn, mode) in turns.iter().zip(["default", "default"]) {
        assert_eq!(turn["collaborationMode"]["mode"], mode);
        assert_eq!(turn["collaborationMode"]["settings"]["model"], "fake-big");
        assert_eq!(
            turn["collaborationMode"]["settings"]["reasoning_effort"],
            "xhigh"
        );
        assert!(turn["collaborationMode"]["settings"]["developer_instructions"].is_null());
        assert_eq!(turn["approvalPolicy"], "on-request");
        assert_eq!(turn["serviceTier"], "priority");
    }
    // A saved plan_mode no longer restricts a run (the web dropped plan mode): both turns can write.
    assert_eq!(turns[0]["sandboxPolicy"]["type"], "workspaceWrite");
    assert_eq!(turns[1]["sandboxPolicy"]["type"], "workspaceWrite");
    assert_eq!(turns[0]["threadId"], turns[1]["threadId"]);
    let threads: Vec<Value> = entries
        .iter()
        .filter(|entry| entry["mode"] == "codex-thread")
        .map(|entry| serde_json::from_str(entry["args"][0].as_str().unwrap()).unwrap())
        .collect();
    assert!(threads[0]["threadId"].is_null());
    assert_eq!(threads[1]["threadId"], turns[0]["threadId"]);
    let plan_context = threads[0]["developerInstructions"].as_str().unwrap();
    let ordinary_context = threads[1]["developerInstructions"].as_str().unwrap();
    let plan_instruction = "Plan mode: read and propose a plan. Change nothing.\n";
    assert!(!plan_context.contains(plan_instruction));
    assert!(!ordinary_context.contains(plan_instruction));
    assert!(ordinary_context.starts_with("<orbyn_context>\nCheckout:"));
    assert_eq!(plan_context, ordinary_context);
}

#[tokio::test]
async fn codex_approval_round_trips_and_second_run_resumes() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    service.admit(&thread, "[approve]").await;
    let approval = service.pending().await;
    assert_eq!(approval["kind"], "command");
    assert_eq!(service.run_state(&thread).await, "awaiting_approval");
    let id = approval["id"].as_str().unwrap();
    let (status, decided) = service.decide(id, true).await;
    assert_eq!(status, 200, "{decided}");
    assert_eq!(service.finished(&thread).await, "completed");
    let events = service.all_events(&thread).await;
    assert_eq!(text_of(&events), "decision accept");
    let types = types_of(&events);
    assert!(types.iter().any(|t| t == "TOOL_CALL_START"));
    assert!(types.iter().any(|t| t == "TOOL_CALL_RESULT"));
    let diff = events
        .iter()
        .find(|event| event["event"]["activityType"] == "diff")
        .unwrap();
    assert_eq!(diff["event"]["content"]["files"][0]["path"], "x");
}

#[tokio::test]
async fn codex_denial_reaches_the_engine() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    service.admit(&thread, "[approve]").await;
    let approval = service.pending().await;
    service
        .decide(approval["id"].as_str().unwrap(), false)
        .await;
    assert_eq!(service.finished(&thread).await, "completed");
    assert_eq!(
        text_of(&service.all_events(&thread).await),
        "decision decline"
    );
}

#[tokio::test]
async fn codex_unknown_server_request_is_refused() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    service.admit(&thread, "[unknown-request]").await;
    service.finished(&thread).await;
    assert_eq!(
        text_of(&service.all_events(&thread).await),
        "refused -32601"
    );
}

#[tokio::test]
async fn codex_token_refresh_request_is_refused_over_the_wire() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    service.admit(&thread, "[token-refresh]").await;
    assert_eq!(service.finished(&thread).await, "completed");
    assert_eq!(
        text_of(&service.all_events(&thread).await),
        "token refresh error -32601 result null"
    );
}

#[tokio::test]
async fn codex_cancel_interrupts_and_cleans_up() {
    let dir = TempDir::new();
    let (pids, options) = pid_env(&dir);
    let (service, thread) = setup(&dir, options, "codex").await;
    let run = service.admit(&thread, "[hang]").await;
    engine_started(&pids).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let (status, _) = service
        .send(&format!("/api/runs/{run}/cancel"), json!({}))
        .await;
    assert_eq!(status, 202);
    assert_eq!(service.finished(&thread).await, "cancelled");
    #[cfg(unix)]
    assert_all_dead(&pids).await;
    drop(pids);
}

#[tokio::test]
async fn a_forced_stop_kills_the_engine_and_is_safe_to_repeat() {
    let dir = TempDir::new();
    let (pids, options) = pid_env(&dir);
    let (service, thread) = setup(&dir, options, "codex").await;
    let run = service.admit(&thread, "[hang]").await;
    engine_started(&pids).await;
    let path = format!("/api/threads/{thread}/force-stop");
    let (status, stopped) = service.send(&path, json!({})).await;
    assert_eq!(status, 200, "{stopped}");
    assert_eq!(stopped["run_id"], run);
    assert_eq!(stopped["state"], "cancelled");
    #[cfg(unix)]
    assert_all_dead(&pids).await;
    assert_eq!(service.finished(&thread).await, "cancelled");
    let (_, again) = service.send(&path, json!({})).await;
    assert_eq!(again["run_id"], Value::Null);
    let (status, _) = service
        .send("/api/threads/nothing/force-stop", json!({}))
        .await;
    assert_eq!(status, 404);
    let actions = service.audit(&thread).await;
    assert!(actions.iter().any(|row| row["action"] == "run.force_stop"));
}

#[tokio::test]
async fn codex_exit_during_approval_fails_and_expires() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    service.admit(&thread, "[crash-in-approval]").await;
    let approval = service.pending().await;
    assert_eq!(service.finished(&thread).await, "failed");
    let (status, body) = service.decide(approval["id"].as_str().unwrap(), true).await;
    assert_eq!(status, 409);
    assert_eq!(body["code"], "already_decided");
    assert_eq!(body["detail"]["state"], "expired");
}

#[tokio::test]
async fn codex_failed_turn_reports_its_error_and_late_frames_are_dropped() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    service.admit(&thread, "[fail][late]").await;
    assert_eq!(service.finished(&thread).await, "failed");
    let events = service.all_events(&thread).await;
    let last = events.last().unwrap();
    assert_eq!(last["event"]["type"], "RUN_ERROR");
    assert_eq!(last["event"]["message"], "fake failure");
    assert!(!text_of(&events).contains("too late"));
}

/// The `background.stopped` events a run reported, with their reasons.
fn stopped_reasons(events: &[Value]) -> Vec<String> {
    events
        .iter()
        .filter(|event| event["event"]["name"] == "background.stopped")
        .filter_map(|event| {
            event["event"]["value"]["reason"]
                .as_str()
                .map(str::to_owned)
        })
        .collect()
}

/// Waits for the engine's word, said after its turn completed with an agent
/// running. A run that ended at the completion never delivers it, so seeing it
/// means the run is held; the stream is read, not slept on.
async fn codex_held(service: &Service, thread: &str) {
    service
        .events_until(thread, None, |event| {
            event["event"]["name"] == "engine.warning"
        })
        .await;
    let body = service.json(&format!("/api/threads/{thread}")).await;
    assert!(!body["active_run"].is_null(), "the run ended: {body}");
}

fn codex_options(env: &[(&str, &str)]) -> Options {
    Options {
        env: env
            .iter()
            .map(|(key, value)| ((*key).into(), (*value).into()))
            .collect(),
        ..Options::default()
    }
}

#[tokio::test]
async fn a_codex_sub_agent_keeps_the_run_open_until_it_ends() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    service.admit(&thread, "go [bg-agent-finish]").await;
    // The parent's turn completed with its sub-agent running; the agent's result
    // starts a turn of its own, and closing the engine at the first completion
    // would lose it.
    assert_eq!(service.finished(&thread).await, "completed");
    let events = service.all_events(&thread).await;
    assert_eq!(text_of(&events), "hello from codex and the agent finished");
    assert!(stopped_reasons(&events).is_empty());
}

#[tokio::test]
async fn a_codex_run_ends_when_its_sub_agent_does_and_no_turn_follows() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    service.admit(&thread, "go [bg-agent-quiet]").await;
    assert_eq!(service.finished(&thread).await, "completed");
    let events = service.all_events(&thread).await;
    assert_eq!(text_of(&events), "hello from codex");
    assert!(stopped_reasons(&events).is_empty());
}

#[tokio::test]
async fn cancelling_a_codex_run_with_a_sub_agent_running_says_it_was_stopped() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    let run = service.admit(&thread, "go [bg-agent-hang]").await;
    codex_held(&service, &thread).await;
    let (status, _) = service
        .send(&format!("/api/runs/{run}/cancel"), json!({}))
        .await;
    assert_eq!(status, 202);
    assert_eq!(service.finished(&thread).await, "cancelled");
    let events = service.all_events(&thread).await;
    assert_eq!(stopped_reasons(&events), ["cancelled"]);
}

#[tokio::test]
async fn a_codex_run_gives_up_on_a_sub_agent_that_never_ends_and_says_so() {
    let dir = TempDir::new();
    let options = codex_options(&[("ORBYN_CODEX_BACKGROUND_LIMIT_MS", "800")]);
    let (service, thread) = setup(&dir, options, "codex").await;
    service.admit(&thread, "go [bg-agent-hang]").await;
    // The parent answered, so the run is complete; the agent that was still
    // running is reported, not silently dropped.
    assert_eq!(service.finished(&thread).await, "completed");
    let events = service.all_events(&thread).await;
    assert_eq!(text_of(&events), "hello from codex");
    assert_eq!(stopped_reasons(&events), ["limit"]);
}

#[tokio::test]
async fn a_codex_engine_that_exits_with_a_sub_agent_running_says_it_was_stopped() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    service.admit(&thread, "go [bg-agent-exit]").await;
    assert_eq!(service.finished(&thread).await, "completed");
    let events = service.all_events(&thread).await;
    assert_eq!(stopped_reasons(&events), ["engine exited"]);
}

#[tokio::test]
async fn a_codex_hold_ends_when_nothing_is_heard_from_the_agent() {
    let dir = TempDir::new();
    let options = codex_options(&[
        ("ORBYN_CODEX_BACKGROUND_IDLE_MS", "600"),
        ("ORBYN_CODEX_BACKGROUND_LIMIT_MS", "60000"),
    ]);
    let (service, thread) = setup(&dir, options, "codex").await;
    service.admit(&thread, "go [bg-agent-hang]").await;
    assert_eq!(service.finished(&thread).await, "completed");
    let events = service.all_events(&thread).await;
    assert_eq!(stopped_reasons(&events), ["idle"]);
}

#[tokio::test]
async fn a_codex_hold_ends_when_the_agent_fails_or_closes_even_if_a_state_comes_late() {
    for tag in ["bg-agent-error", "bg-agent-closed", "bg-agent-stale"] {
        let dir = TempDir::new();
        // No limit would end these in time: only the agent's own end does.
        let options = codex_options(&[
            ("ORBYN_CODEX_BACKGROUND_IDLE_MS", "60000"),
            ("ORBYN_CODEX_BACKGROUND_LIMIT_MS", "60000"),
        ]);
        let (service, thread) = setup(&dir, options, "codex").await;
        service.admit(&thread, &format!("go [{tag}]")).await;
        assert_eq!(service.finished(&thread).await, "completed", "{tag}");
        let events = service.all_events(&thread).await;
        assert_eq!(text_of(&events), "hello from codex", "{tag}");
        assert!(stopped_reasons(&events).is_empty(), "{tag}");
    }
}

#[tokio::test]
async fn a_failed_codex_turn_with_a_sub_agent_running_says_it_was_stopped() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    service.admit(&thread, "go [bg-agent-hang][fail]").await;
    assert_eq!(service.finished(&thread).await, "failed");
    let events = service.all_events(&thread).await;
    assert_eq!(stopped_reasons(&events), ["turn ended"]);
}

#[tokio::test]
async fn a_codex_engine_that_crashes_after_answering_still_completes_the_run() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    service.admit(&thread, "go [bg-agent-crash]").await;
    assert_eq!(service.finished(&thread).await, "completed");
    let events = service.all_events(&thread).await;
    assert_eq!(text_of(&events), "hello from codex");
    assert_eq!(stopped_reasons(&events), ["engine exited"]);
}

#[tokio::test]
async fn the_engines_own_sub_agent_does_not_hold_the_run() {
    let dir = TempDir::new();
    let options = codex_options(&[("ORBYN_CODEX_BACKGROUND_LIMIT_MS", "60000")]);
    let (service, thread) = setup(&dir, options, "codex").await;
    service.admit(&thread, "go [bg-internal]").await;
    assert_eq!(service.finished(&thread).await, "completed");
    let events = service.all_events(&thread).await;
    assert!(stopped_reasons(&events).is_empty());
}

#[tokio::test]
async fn a_message_sent_during_a_codex_hold_ends_it_and_runs_as_the_next_turn() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    service.admit(&thread, "go [bg-agent-hang]").await;
    codex_held(&service, &thread).await;
    let (code, body) = service
        .send(
            &format!("/api/threads/{thread}/steer"),
            json!({ "prompt": "next please" }),
        )
        .await;
    assert_eq!(code, 202, "{body}");
    // The held turn ended with its agent stopped, and the message became a run.
    for _ in 0..2000 {
        let audit = service.audit(&thread).await;
        let admitted = audit.iter().filter(|row| row["action"] == "run.admit");
        let thread_body = service.json(&format!("/api/threads/{thread}")).await;
        if admitted.count() >= 2 && thread_body["active_run"].is_null() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let audit = service.audit(&thread).await;
    assert!(
        audit
            .iter()
            .filter(|row| row["action"] == "run.admit")
            .count()
            >= 2,
        "the message never ran as a turn"
    );
    let events = service.all_events(&thread).await;
    assert_eq!(stopped_reasons(&events), ["new message"]);
}

#[tokio::test]
async fn codex_gets_the_mcp_server_without_approve() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    service.admit(&thread, "[mcp-tools]").await;
    service.finished(&thread).await;
    let text = text_of(&service.all_events(&thread).await);
    let tools: u32 = text
        .strip_prefix("mcp 200 tools ")
        .and_then(|count| count.parse().ok())
        .unwrap_or_else(|| panic!("{text}"));
    assert!(tools > 30, "{text}");
}

#[tokio::test]
async fn claude_stream_maps_to_events() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "claude").await;
    service.admit(&thread, "say hi").await;
    assert_eq!(service.finished(&thread).await, "completed");
    let events = service.all_events(&thread).await;
    assert_eq!(text_of(&events), "hello from claude");
    assert!(
        types_of(&events)
            .iter()
            .any(|t| t == "REASONING_MESSAGE_CONTENT")
    );
}

#[tokio::test]
async fn plans_reach_the_log_as_steps_and_a_proposed_plan() {
    for (engine, step) in [("claude", "do it"), ("codex", "do it")] {
        let dir = TempDir::new();
        let (service, thread) = setup(&dir, Options::default(), engine).await;
        service.admit(&thread, "[plan-item]").await;
        assert_eq!(service.finished(&thread).await, "completed");
        let events = service.all_events(&thread).await;
        let plan = events
            .iter()
            .find(|event| event["event"]["activityType"] == "plan")
            .unwrap_or_else(|| panic!("{engine}: no plan activity"));
        assert_eq!(plan["event"]["content"]["steps"][0]["title"], step);
        assert_eq!(
            plan["event"]["content"]["steps"][0]["status"],
            "in_progress"
        );
        assert!(
            text_of(&events).contains("<proposed_plan>\n# Ship\n1. a\n</proposed_plan>"),
            "{engine}: {}",
            text_of(&events)
        );
    }
}

#[tokio::test]
async fn claude_permission_goes_through_mcp_approve() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "claude").await;
    service.admit(&thread, "[approve]").await;
    let approval = service.pending().await;
    assert_eq!(approval["kind"], "command");
    assert_eq!(approval["summary"], "Run ls");
    service.decide(approval["id"].as_str().unwrap(), true).await;
    assert_eq!(service.finished(&thread).await, "completed");
    let events = service.all_events(&thread).await;
    assert_eq!(text_of(&events), "permission allow");
    let result = events
        .iter()
        .find(|event| event["event"]["type"] == "TOOL_CALL_RESULT")
        .unwrap();
    assert_eq!(result["event"]["content"], "tool allow");
}

#[tokio::test]
async fn claude_denial_reaches_the_engine() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "claude").await;
    service.admit(&thread, "[approve]").await;
    let approval = service.pending().await;
    service
        .decide(approval["id"].as_str().unwrap(), false)
        .await;
    service.finished(&thread).await;
    assert_eq!(
        text_of(&service.all_events(&thread).await),
        "permission deny"
    );
}

#[tokio::test]
async fn claude_exit_during_approval_fails_and_expires() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "claude").await;
    service.admit(&thread, "[crash-in-approval]").await;
    let approval = service.pending().await;
    assert_eq!(service.finished(&thread).await, "failed");
    let (status, body) = service.decide(approval["id"].as_str().unwrap(), true).await;
    assert_eq!(status, 409);
    assert_eq!(body["detail"]["state"], "expired");
}

#[tokio::test]
async fn claude_error_result_fails_the_run() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "claude").await;
    service.admit(&thread, "[fail]").await;
    assert_eq!(service.finished(&thread).await, "failed");
    let events = service.all_events(&thread).await;
    assert_eq!(events.last().unwrap()["event"]["message"], "fake failure");
}

#[tokio::test]
async fn claude_cancel_closes_the_process() {
    let dir = TempDir::new();
    let (pids, options) = pid_env(&dir);
    let (service, thread) = setup(&dir, options, "claude").await;
    let run = service.admit(&thread, "[hang]").await;
    engine_started(&pids).await;
    service
        .send(&format!("/api/runs/{run}/cancel"), json!({}))
        .await;
    assert_eq!(service.finished(&thread).await, "cancelled");
    #[cfg(unix)]
    assert_all_dead(&pids).await;
    drop(pids);
}

/// The documented run command line, read from the fake's launch record: a run passes the flags
/// a turn and an approval rely on, in order, and never `--bare` or `--plugin-dir`. The unit test
/// `command_line_matches_the_contract_and_never_uses_bare` pins the same contract for one spec;
/// this is the same claim through the real service. Session flags (`--resume`, `--session-id`)
/// are not pinned here.
#[tokio::test]
async fn a_claude_run_launches_with_the_documented_flags_and_never_bare() {
    let dir = TempDir::new();
    let file = dir.path().join("launches");
    let options = Options {
        env: vec![(
            "ORBYN_FAKE_LAUNCH_FILE".into(),
            file.to_string_lossy().into_owned(),
        )],
        ..Options::default()
    };
    let (service, thread) = setup(&dir, options, "claude").await;
    service.admit(&thread, "say hi").await;
    assert_eq!(service.finished(&thread).await, "completed");

    let launches: Vec<Value> = std::fs::read_to_string(&file)
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let run = launches
        .iter()
        .find(|entry| {
            entry["mode"] == "claude"
                && entry["args"]
                    .as_array()
                    .is_some_and(|args| args.iter().any(|arg| arg == "--permission-prompt-tool"))
        })
        .unwrap_or_else(|| panic!("the run's launch was not recorded: {launches:?}"));
    let args: Vec<&str> = run["args"]
        .as_array()
        .expect("args are recorded as an array")
        .iter()
        .map(|arg| arg.as_str().expect("argv entries are strings"))
        .collect();

    // The flags a turn, an approval and a resumed session rely on, in the order they are passed.
    let order = [
        "-p",
        "--output-format",
        "--input-format",
        "--verbose",
        "--include-partial-messages",
        "--replay-user-messages",
        "--permission-mode",
        "--mcp-config",
        "--permission-prompt-tool",
        "--settings",
    ];
    let position = |flag: &str| {
        args.iter()
            .position(|arg| *arg == flag)
            .unwrap_or_else(|| panic!("{flag} is missing from {args:?}"))
    };
    let mut last = 0;
    for flag in order {
        let at = position(flag);
        assert!(at >= last, "{flag} is out of order in {args:?}");
        last = at;
    }
    let value = |flag: &str| args[position(flag) + 1];
    assert_eq!(value("--output-format"), "stream-json");
    assert_eq!(value("--input-format"), "stream-json");
    assert_eq!(value("--permission-mode"), "default");
    assert_eq!(
        value("--permission-prompt-tool"),
        "mcp__orbyn_engineering__approve"
    );
    for (flag, filename) in [("--mcp-config", "mcp.json"), ("--settings", "settings.json")] {
        let path = std::path::Path::new(value(flag));
        assert!(path.is_absolute(), "{flag} is not an absolute path: {args:?}");
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some(filename),
            "{flag} points elsewhere: {args:?}"
        );
    }
    assert!(
        !args.iter().any(|arg| *arg == "--bare" || *arg == "--plugin-dir"),
        "the run took a flag it must never take: {args:?}"
    );
}

async fn custom_engine(service: &Service, tier: &str, target: &str, args: Value) {
    let (status, body) = service
        .send(
            "/api/engines/custom",
            json!({ "id": "custom", "name": "Custom", "tier": tier, "target": target, "args": args }),
        )
        .await;
    assert_eq!(status, 201, "{body}");
}

#[tokio::test]
async fn acp_agent_runs_and_is_gated() {
    let dir = TempDir::new();
    let (service, _) = setup(&dir, Options::default(), "codex").await;
    custom_engine(&service, "acp", FAKE, json!(["acp"])).await;
    let projects = service.json("/api/projects").await;
    let thread = service
        .thread(projects[0]["id"].as_str().unwrap(), "custom")
        .await;
    service.admit(&thread, "[approve]").await;
    let approval = service.pending().await;
    assert_eq!(approval["kind"], "command");
    service.decide(approval["id"].as_str().unwrap(), true).await;
    assert_eq!(service.finished(&thread).await, "completed");
    let events = service.all_events(&thread).await;
    assert_eq!(text_of(&events), "option once");
    assert!(types_of(&events).iter().any(|t| t == "TOOL_CALL_RESULT"));
}

#[tokio::test]
async fn acp_cancel_reaches_the_agent() {
    let dir = TempDir::new();
    let (service, _) = setup(&dir, Options::default(), "codex").await;
    custom_engine(&service, "acp", FAKE, json!(["acp"])).await;
    let projects = service.json("/api/projects").await;
    let thread = service
        .thread(projects[0]["id"].as_str().unwrap(), "custom")
        .await;
    let run = service.admit(&thread, "[hang]").await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    service
        .send(&format!("/api/runs/{run}/cancel"), json!({}))
        .await;
    assert_eq!(service.finished(&thread).await, "cancelled");
}

#[tokio::test]
async fn an_acp_run_ignores_another_sessions_updates_and_unknown_replies() {
    let dir = TempDir::new();
    let (service, _) = setup(&dir, Options::default(), "codex").await;
    custom_engine(&service, "acp", FAKE, json!(["acp"])).await;
    let projects = service.json("/api/projects").await;
    let thread = service
        .thread(projects[0]["id"].as_str().unwrap(), "custom")
        .await;
    service.admit(&thread, "[stray]").await;
    assert_eq!(service.finished(&thread).await, "completed");
    let events = service.all_events(&thread).await;
    // The run stayed open past the stray frames and kept only its own answer.
    assert_eq!(text_of(&events), "hello from acp");
}

#[tokio::test]
async fn an_acp_turn_that_stopped_early_or_unexplained_is_not_success() {
    for (tag, expected) in [
        ("refusal", "it refused to continue"),
        ("max_tokens", "it ran out of tokens"),
        ("no-stop-reason", "it did not say why"),
    ] {
        let dir = TempDir::new();
        let (service, _) = setup(&dir, Options::default(), "codex").await;
        custom_engine(&service, "acp", FAKE, json!(["acp"])).await;
        let projects = service.json("/api/projects").await;
        let thread = service
            .thread(projects[0]["id"].as_str().unwrap(), "custom")
            .await;
        service.admit(&thread, &format!("[{tag}]")).await;
        assert_eq!(service.finished(&thread).await, "failed", "{tag}");
        let events = service.all_events(&thread).await;
        assert!(
            events
                .iter()
                .any(|event| event.to_string().contains(expected)),
            "{tag}: {events:?}"
        );
    }
}

#[tokio::test]
async fn an_acp_agent_without_http_mcp_reaches_tools_through_the_stdio_bridge() {
    let dir = TempDir::new();
    let options = Options {
        env: vec![("ORBYN_FAKE_ACP_STDIO_MCP".into(), "1".into())],
        ..Options::default()
    };
    let (service, _) = setup(&dir, options, "codex").await;
    custom_engine(&service, "acp", FAKE, json!(["acp"])).await;
    let projects = service.json("/api/projects").await;
    let thread = service
        .thread(projects[0]["id"].as_str().unwrap(), "custom")
        .await;
    service.admit(&thread, "[bridge]").await;
    assert_eq!(service.finished(&thread).await, "completed");
    let events = service.all_events(&thread).await;
    assert_eq!(
        text_of(&events),
        "bridge true listed tools; call ok; token in argv or env no; endpoint file private; live read ok"
    );
    // The endpoint file lives in the run's folder, which goes with the run.
    let runs = dir.path().join("data").join("runs");
    let left: Vec<_> = std::fs::read_dir(&runs)
        .into_iter()
        .flatten()
        .flatten()
        .collect();
    assert!(left.is_empty(), "run folders left behind: {left:?}");
}

/// The fake AG-UI agent, stopped with its test.
struct Agent(std::process::Child);

impl Drop for Agent {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn_agui(dir: &TempDir) -> (Agent, String) {
    let port_file = dir.path().join("agui-port");
    let agent = Agent(
        std::process::Command::new(FAKE)
            .args(["agui", port_file.to_str().unwrap()])
            .spawn()
            .unwrap(),
    );
    for _ in 0..500 {
        if let Ok(port) = std::fs::read_to_string(&port_file)
            && !port.is_empty()
        {
            return (agent, format!("http://127.0.0.1:{port}/"));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("agui fake did not start");
}

#[tokio::test]
async fn agui_agent_streams_and_keeps_unknown_events_visible() {
    let dir = TempDir::new();
    let (_agent, url) = spawn_agui(&dir);
    let (service, _) = setup(&dir, Options::default(), "codex").await;
    custom_engine(&service, "agui", &url, json!([])).await;
    let projects = service.json("/api/projects").await;
    let thread = service
        .thread(projects[0]["id"].as_str().unwrap(), "custom")
        .await;
    service.admit(&thread, "say hi").await;
    assert_eq!(service.finished(&thread).await, "completed");
    let events = service.all_events(&thread).await;
    assert_eq!(text_of(&events), "hello from agui");
    assert!(types_of(&events).iter().any(|t| t == "RAW"));
}

#[tokio::test]
async fn agui_interrupt_becomes_an_approval_and_resumes() {
    let dir = TempDir::new();
    let (_agent, url) = spawn_agui(&dir);
    let (service, _) = setup(&dir, Options::default(), "codex").await;
    custom_engine(&service, "agui", &url, json!([])).await;
    let projects = service.json("/api/projects").await;
    let thread = service
        .thread(projects[0]["id"].as_str().unwrap(), "custom")
        .await;
    service.admit(&thread, "[approve]").await;
    let approval = service.pending().await;
    assert_eq!(approval["summary"], "Deploy now?");
    service
        .decide(approval["id"].as_str().unwrap(), false)
        .await;
    assert_eq!(service.finished(&thread).await, "completed");
    let events = service.all_events(&thread).await;
    assert_eq!(text_of(&events), "resumed approved=false");
}

#[tokio::test]
async fn an_agui_run_ignores_another_runs_end_frames() {
    let dir = TempDir::new();
    let (_agent, url) = spawn_agui(&dir);
    let (service, _) = setup(&dir, Options::default(), "codex").await;
    custom_engine(&service, "agui", &url, json!([])).await;
    let projects = service.json("/api/projects").await;
    let thread = service
        .thread(projects[0]["id"].as_str().unwrap(), "custom")
        .await;
    service.admit(&thread, "[stray]").await;
    assert_eq!(service.finished(&thread).await, "completed");
    let events = service.all_events(&thread).await;
    assert_eq!(text_of(&events), "hello from agui");
}

#[tokio::test]
async fn agui_run_error_fails_the_run() {
    let dir = TempDir::new();
    let (_agent, url) = spawn_agui(&dir);
    let (service, _) = setup(&dir, Options::default(), "codex").await;
    custom_engine(&service, "agui", &url, json!([])).await;
    let projects = service.json("/api/projects").await;
    let thread = service
        .thread(projects[0]["id"].as_str().unwrap(), "custom")
        .await;
    service.admit(&thread, "[fail]").await;
    assert_eq!(service.finished(&thread).await, "failed");
}

#[tokio::test]
async fn terminal_tier_is_labelled_unmanaged_and_runs_in_a_pty() {
    let dir = TempDir::new();
    let (service, _) = setup(&dir, Options::default(), "codex").await;
    custom_engine(&service, "terminal", FAKE, json!(["pty"])).await;
    let engines = service.json("/api/engines").await;
    let custom = engines
        .as_array()
        .unwrap()
        .iter()
        .find(|engine| engine["id"] == "custom")
        .unwrap();
    assert_eq!(custom["capabilities"]["managed"], false);
    assert_eq!(custom["capabilities"]["approvals"], false);
    let projects = service.json("/api/projects").await;
    let thread = service
        .thread(projects[0]["id"].as_str().unwrap(), "custom")
        .await;
    service.admit(&thread, "exit").await;
    assert_eq!(service.finished(&thread).await, "completed");
    let events = service.all_events(&thread).await;
    let notice = events
        .iter()
        .find(|event| event["event"]["activityType"] == "unmanaged");
    assert!(notice.is_some());
}

#[tokio::test]
async fn probes_read_version_and_login_without_a_session() {
    let dir = TempDir::new();
    let (service, _) = setup(&dir, Options::default(), "codex").await;
    let engines = service.json("/api/engines").await;
    let codex = engines
        .as_array()
        .unwrap()
        .iter()
        .find(|engine| engine["id"] == "codex")
        .unwrap();
    assert_eq!(codex["installed"], true);
    assert_eq!(codex["version"], "0.166.0");
    assert_eq!(codex["support"], "tested");
    assert_eq!(codex["auth"], "logged_in");
    assert_eq!(codex["installer"], "unknown");
    assert!(codex["update_command"].is_null());
}

/// Claude run launches so far, leaving out capability probes.
fn claude_run_launches(file: &std::path::Path) -> usize {
    std::fs::read_to_string(file)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|entry| {
            entry["mode"] == "claude"
                && entry["args"]
                    .as_array()
                    .is_some_and(|args| args.iter().any(|arg| arg == "--permission-prompt-tool"))
        })
        .count()
}

async fn claude_launches_reach(file: &std::path::Path, wanted: usize) {
    for _ in 0..2000 {
        if claude_run_launches(file) >= wanted {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("{} launches, wanted {wanted}", claude_run_launches(file));
}

/// What the run's own child saw as `CLAUDE_CODE_ENABLE_TODO_TOOLS`, for a service started
/// with `extra` in its environment. The capability probe and the one-shot title call are
/// Claude launches too; the opt-in belongs to runs alone, so they are checked as well.
async fn task_tools_seen_by_a_run(extra: &[(&str, &str)]) -> Value {
    let dir = TempDir::new();
    let file = dir.path().join("launches");
    let mut env = vec![(
        "ORBYN_FAKE_LAUNCH_FILE".to_owned(),
        file.to_string_lossy().into_owned(),
    )];
    env.extend(
        extra
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
    );
    let options = Options {
        env,
        ..Options::default()
    };
    let (service, thread) = setup(&dir, options, "claude").await;
    service.admit(&thread, "say hi").await;
    assert_eq!(service.finished(&thread).await, "completed");
    let launches: Vec<Value> = std::fs::read_to_string(&file)
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|entry| entry["mode"] == "claude")
        .collect();
    let (runs, others): (Vec<&Value>, Vec<&Value>) = launches.iter().partition(|entry| {
        entry["args"]
            .as_array()
            .is_some_and(|args| args.iter().any(|arg| arg == "--permission-prompt-tool"))
    });
    assert_eq!(runs.len(), 1, "{launches:?}");
    assert!(!others.is_empty(), "{launches:?}");
    for entry in others {
        assert_ne!(
            entry["task_tools"], "1",
            "a non-run launch took the driver's opt-in: {entry}"
        );
    }
    runs[0]["task_tools"].clone()
}

/// Off unless a person turns it on: Orbyn sets no task-tool flag by default.
#[tokio::test]
async fn a_claude_run_sets_no_task_tools_flag_by_default() {
    assert_eq!(task_tools_seen_by_a_run(&[]).await, Value::Null);
}

/// A person's `ORBYN_CLAUDE_TASK_TOOLS=1` on the service makes its unconfined runs ask
/// the CLI for the modern task tools the plan row projects.
#[tokio::test]
async fn a_persons_opt_in_makes_a_claude_run_carry_the_task_tools_flag() {
    let seen = task_tools_seen_by_a_run(&[("ORBYN_CLAUDE_TASK_TOOLS", "1")]).await;
    assert_eq!(seen, "1");
}

/// A value the person already set for the CLI wins, `0` included, even with the opt-in.
#[tokio::test]
async fn a_persons_own_task_tools_value_is_never_overridden() {
    let seen = task_tools_seen_by_a_run(&[
        ("ORBYN_CLAUDE_TASK_TOOLS", "1"),
        ("CLAUDE_CODE_ENABLE_TODO_TOOLS", "0"),
    ])
    .await;
    assert_eq!(seen, "0");
}

/// The thread's events through the end of its `runs`th run.
async fn events_through(service: &Service, thread: &str, runs: usize) -> Vec<Value> {
    let ended = std::cell::Cell::new(0);
    service
        .events_until(thread, None, |event| {
            if matches!(
                event["event"]["type"].as_str(),
                Some("RUN_FINISHED" | "RUN_ERROR")
            ) {
                ended.set(ended.get() + 1);
            }
            ended.get() >= runs
        })
        .await
}

fn warm_adoptions(events: &[Value]) -> usize {
    events
        .iter()
        .filter(|event| event["event"]["name"] == "engine.warm")
        .count()
}

#[tokio::test]
async fn the_next_claude_turn_adopts_a_process_started_for_it_only_when_inputs_match() {
    let dir = TempDir::new();
    let file = dir.path().join("launches");
    let options = Options {
        env: vec![(
            "ORBYN_FAKE_LAUNCH_FILE".into(),
            file.to_string_lossy().into_owned(),
        )],
        ..Options::default()
    };
    let (service, thread) = setup(&dir, options, "claude").await;
    let started = std::time::SystemTime::now();
    let response = service
        .client
        .put(format!("{}/api/preferences", service.url))
        .bearer_auth(&service.token)
        .json(&json!({ "values": { "warm_next_turn": true } }))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    service.admit(&thread, "say hi").await;
    assert_eq!(service.finished(&thread).await, "completed");
    // The completed turn starts the next one's process, which waits unprompted.
    claude_launches_reach(&file, 2).await;
    // The waiting process's token file is under the runs folder, where the confined
    // worker profile denies other runs, and in no shared temporary folder.
    let waiting: Vec<_> = std::fs::read_dir(dir.path().join("data").join("runs"))
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("warm-"))
        .filter_map(|entry| std::fs::read_to_string(entry.path().join("mcp.json")).ok())
        .collect();
    assert!(
        waiting.iter().any(|text| text.contains("/mcp/")),
        "no warm token file under runs: {waiting:?}"
    );
    for entry in std::fs::read_dir(std::env::temp_dir()).unwrap().flatten() {
        // Only folders made by this test: older ones are left by earlier builds.
        let fresh = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .is_ok_and(|time| time >= started);
        if fresh
            && entry
                .file_name()
                .to_string_lossy()
                .starts_with("orbyn-probe-")
        {
            let text = std::fs::read_to_string(entry.path().join("mcp.json")).unwrap_or_default();
            assert!(!text.contains("/mcp/"), "token in {:?}", entry.path());
        }
    }
    service.admit(&thread, "say hi again").await;
    assert_eq!(service.finished(&thread).await, "completed");
    let events = events_through(&service, &thread, 2).await;
    assert_eq!(warm_adoptions(&events), 1);
    assert_eq!(text_of(&events), "hello from claudehello from claude");
    // The adopted turn launched nothing of its own; its end starts another.
    claude_launches_reach(&file, 3).await;
    assert_eq!(claude_run_launches(&file), 3);

    // A changed mode is a different launch: the waiting process is not used.
    let (status, _) = service
        .patch(&thread, json!({ "permission_mode": "full_access" }))
        .await;
    assert_eq!(status, 200);
    service.admit(&thread, "and again").await;
    assert_eq!(service.finished(&thread).await, "completed");
    assert_eq!(
        warm_adoptions(&events_through(&service, &thread, 3).await),
        1
    );
    claude_launches_reach(&file, 5).await;
}
