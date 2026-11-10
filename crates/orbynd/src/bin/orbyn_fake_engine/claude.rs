use serde_json::{Value, json};

use crate::shared::{
    WORKFLOW_SCRIPT, has, mcp_call, record_claude_control, record_launch, send, stdin_lines,
};

// Claude Code stream-json.

pub(crate) fn claude(args: &[String]) {
    if args.iter().any(|arg| arg == "--bare") {
        eprintln!("--bare ignores OAuth login");

        std::process::exit(9);
    }

    let flag = |name: &str| {
        args.iter()
            .position(|arg| arg == name)
            .and_then(|index| args.get(index + 1))
            .cloned()
    };

    let mut lines = stdin_lines();

    let Some(mut first) = lines.next() else {
        return;
    };

    if first["type"] == "control_request" {
        assert_eq!(first["request"]["subtype"], "initialize");

        record_claude_control(&first);

        if std::env::var_os("ORBYN_FAKE_NO_INITIALIZE").is_some() {
            return;
        }

        record_launch("claude-probe", args);

        send(&json!({ "type": "control_response", "response": {

            "subtype": "success", "request_id": first["request_id"], "response": {

                "models": [

                    { "value": "default", "displayName": "Default", "supportsEffort": true,

                      "supportedEffortLevels": ["low", "high"] },

                    { "value": "fable", "displayName": "Fable", "supportsEffort": true,

                      "supportedEffortLevels": ["low"], "supportsFastMode": true },

                    { "value": "haiku", "displayName": "Haiku" },

                ],

                "commands": [{ "name": "review", "description": "Review a change" }],

                "agents": [{ "name": "Explore", "description": "Search the code" }],

                "available_output_styles": ["default", "Learning"],

                "output_style": "default",

                "account": { "email": "person@example.com" },

            },

        }}));

        let Some(next) = lines.next() else { return };

        first = next;

        record_claude_control(&first);

        if first["type"] == "control_request" && first["request"]["subtype"] == "get_usage" {
            assert!(args.iter().any(|arg| arg == "--no-session-persistence"));

            assert!(args.iter().any(|arg| arg == "--strict-mcp-config"));

            let mcp: Value =
                serde_json::from_slice(&std::fs::read(flag("--mcp-config").unwrap()).unwrap())
                    .unwrap();

            let settings: Value =
                serde_json::from_slice(&std::fs::read(flag("--settings").unwrap()).unwrap())
                    .unwrap();

            assert_eq!(mcp, json!({ "mcpServers": {} }));

            assert_eq!(settings, json!({ "disableAllHooks": true }));

            assert_eq!(
                std::env::var("ENABLE_CLAUDEAI_MCP_SERVERS").unwrap(),
                "false"
            );

            assert_eq!(std::env::var("CLAUDE_CODE_AUTO_CONNECT_IDE").unwrap(), "0");

            let mode = std::env::var("ORBYN_FAKE_CLAUDE_USAGE").unwrap_or_default();

            if mode == "silent" {
                // EOF is the probe's cleanup path; this is not a model turn.

                for _ in lines {}

                return;
            }

            if mode == "error" {
                send(&json!({ "type": "control_response", "response": {

                    "subtype": "error", "request_id": first["request_id"], "error": "SECRET-MARKER failure",

                }}));

                return;
            }

            let response = if mode == "malformed" {
                json!({ "account": "SECRET-MARKER" })
            } else if mode == "unavailable" || mode.is_empty() {
                json!({ "rate_limits_available": false, "rate_limits": null })
            } else {
                json!({ "rate_limits_available": true, "subscription_type": "pro",

                    "account": { "token": "SECRET-MARKER" }, "session": { "private": "SECRET-MARKER" },

                    "rate_limits": {

                        "five_hour": { "utilization": 54.0, "resets_at": "2027-01-15T08:00:00Z" },

                        "seven_day": { "utilization": 18.4, "resets_at": null },

                        "model_scoped": [{ "display_name": "Fable", "utilization": 73.0, "resets_at": null }],

                        "extra_usage": { "is_enabled": true, "monthly_limit": 100.0, "used_credits": 25.0 }

                    }

                })
            };

            send(&json!({ "type": "control_response", "response": {

                "subtype": "success", "request_id": first["request_id"], "response": response,

            }}));

            return;
        }
    }

    let prompt = first
        .pointer("/message/content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();

    let resume = flag("--resume");

    if let (Some(session), true) = (
        &resume,
        std::env::var_os("ORBYN_FAKE_LOSE_SESSION").is_some(),
    ) {
        send(&json!({

            "type": "result", "subtype": "error_during_execution", "is_error": true,

            "errors": [format!("No conversation found with session ID: {session}")],

        }));

        return;
    }

    if std::env::var_os("ORBYN_FAKE_NOT_LOGGED_IN").is_some() {
        send(&json!({

            "type": "result", "subtype": "error_during_execution", "is_error": true,

            "errors": ["Not logged in · Please run /login\nkey sk-ant-do-not-show"],

        }));

        return;
    }

    let session = resume.unwrap_or_else(|| "claude-session-1".to_owned());

    send(&json!({

        "type": "system", "subtype": "init", "session_id": session, "model": "fake",

        "slash_commands": ["compact", "review", "fake-command"],

        // The frame a real run reports: servers (only the dynamic one when --mcp-config is

        // passed), the person's plugins, and the tools by name.

        "mcp_servers": [{ "name": "orbyn_engineering", "status": "connected", "source": "dynamic" }],

        "plugins": [

            { "name": "orbyn-skills", "path": "/home/me/.claude/plugins/orbyn-skills" },

            { "name": "cc-plugin-telemetry", "path": "builtin" },

        ],

        "tools": ["Bash", "Read", "Edit", "mcp__orbyn_engineering__skill_search", "mcp__orbyn_engineering__skill_load"],

        "skills": ["tdd", "architect", "how"],

    }));

    let block = |index: u64, block: Value| {
        send(&json!({

            "type": "stream_event",

            "event": { "type": "content_block_start", "index": index, "content_block": block },

        }));
    };

    let delta = |index: u64, delta: Value| {
        send(&json!({

            "type": "stream_event",

            "event": { "type": "content_block_delta", "index": index, "delta": delta },

        }));
    };

    let stop = |index: u64| {
        send(&json!({

            "type": "stream_event",

            "event": { "type": "content_block_stop", "index": index },

        }));
    };

    block(0, json!({ "type": "thinking", "thinking": "" }));

    delta(
        0,
        json!({ "type": "thinking_delta", "thinking": "considering" }),
    );

    stop(0);

    if has(&prompt, "plan-item") {
        send(&json!({

            "type": "assistant", "parent_tool_use_id": null,

            "message": { "content": [{

                "type": "tool_use", "id": "todo1", "name": "TodoWrite",

                "input": { "todos": [{ "content": "do it", "status": "in_progress" }] },

            }]},

        }));

        send(&json!({

            "type": "assistant", "parent_tool_use_id": null,

            "message": { "content": [{

                "type": "tool_use", "id": "exit1", "name": "ExitPlanMode",

                "input": { "plan": "# Ship\n1. a" },

            }]},

        }));
    }

    let mut answer = "hello from claude".to_owned();

    let mut lines = lines;

    if has(&prompt, "approve") || has(&prompt, "crash-in-approval") {
        block(
            1,
            json!({ "type": "tool_use", "id": "toolu_1", "name": "Bash" }),
        );

        delta(
            1,
            json!({ "type": "input_json_delta", "partial_json": "{\"command\":\"ls\"}" }),
        );

        stop(1);

        let config: Value = serde_json::from_str(
            &std::fs::read_to_string(flag("--mcp-config").expect("mcp config")).expect("read"),
        )
        .expect("mcp json");

        let url = config["mcpServers"]["orbyn_engineering"]["url"]
            .as_str()
            .expect("url")
            .to_owned();

        mcp_call(
            &url,
            1,
            "initialize",
            json!({ "protocolVersion": "2025-06-18" }),
        );

        if has(&prompt, "crash-in-approval") {
            let call = url.clone();

            std::thread::spawn(move || {
                mcp_call(
                    &call,
                    2,
                    "tools/call",
                    json!({ "name": "approve", "arguments": {

                        "tool_name": "Bash", "input": { "command": "ls" }, "tool_use_id": "toolu_1"

                    }}),
                );
            });

            std::thread::sleep(std::time::Duration::from_millis(300));

            std::process::exit(7);
        }

        let (_, reply) = mcp_call(
            &url,
            2,
            "tools/call",
            json!({ "name": "approve", "arguments": {

                "tool_name": "Bash", "input": { "command": "ls" }, "tool_use_id": "toolu_1"

            }}),
        );

        let text = reply
            .pointer("/result/content/0/text")
            .and_then(Value::as_str)
            .unwrap_or("{}");

        let decision: Value = serde_json::from_str(text).unwrap_or(Value::Null);

        let behavior = decision["behavior"].as_str().unwrap_or("none").to_owned();

        if let Some(words) = decision["message"].as_str() {
            record_launch("claude-denied", &[words.to_owned()]);
        }

        send(&json!({

            "type": "user",

            "message": { "role": "user", "content": [{

                "type": "tool_result", "tool_use_id": "toolu_1",

                "content": [{ "type": "text", "text": format!("tool {behavior}") }],

            }]},

        }));

        answer = format!("permission {behavior}");
    }

    if has(&prompt, "question") {
        let config: Value = serde_json::from_str(
            &std::fs::read_to_string(flag("--mcp-config").expect("mcp config")).expect("read"),
        )
        .expect("mcp json");

        let url = config["mcpServers"]["orbyn_engineering"]["url"]
            .as_str()
            .expect("url")
            .to_owned();

        mcp_call(
            &url,
            1,
            "initialize",
            json!({ "protocolVersion": "2025-06-18" }),
        );

        let (_, reply) = mcp_call(
            &url,
            2,
            "tools/call",
            json!({ "name": "approve", "arguments": {

                "tool_name": "AskUserQuestion", "tool_use_id": "toolu_2", "input": { "questions": [{

                    "question": "Which color?", "header": "Color", "multiSelect": true,

                    "options": [{ "label": "Red", "description": "Warm" }],

                }]},

            }}),
        );

        let decision = reply
            .pointer("/result/content/0/text")
            .and_then(Value::as_str)
            .unwrap_or("{}");

        let decision: Value = serde_json::from_str(decision).unwrap_or(Value::Null);

        answer = format!(
            "question {} {}",
            decision["behavior"].as_str().unwrap_or("none"),
            decision["updatedInput"]["answers"]["Which color?"]
                .as_str()
                .unwrap_or("-"),
        );
    }

    if has(&prompt, "hang") {
        // Stays alive until stdin closes, like the real CLI between turns.

        match lines.next() {
            Some(steer) => {
                answer = format!(
                    "steered: {}",
                    steer
                        .pointer("/message/content")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                );
            }

            None => return,
        }
    }

    let workflow = has(&prompt, "workflow") || has(&prompt, "workflow-finish");

    if workflow {
        // Claude Code 2.1.318's Workflow tool: an inline script, launched in the

        // background, with the task's handles in the structured tool result.

        let input = json!({ "script": WORKFLOW_SCRIPT });

        block(
            1,
            json!({ "type": "tool_use", "id": "toolu_wf", "name": "Workflow" }),
        );

        delta(
            1,
            json!({ "type": "input_json_delta", "partial_json": input.to_string() }),
        );

        stop(1);

        send(&json!({

            "type": "assistant", "parent_tool_use_id": null,

            "message": { "content": [{

                "type": "tool_use", "id": "toolu_wf", "name": "Workflow", "input": input,

            }]},

        }));

        send(&json!({

            "type": "system", "subtype": "task_started", "task_id": "wtask1",

            "tool_use_id": "toolu_wf", "task_type": "local_workflow", "workflow_name": "probe",

        }));

        send(&json!({

            "type": "user", "parent_tool_use_id": null,

            "message": { "role": "user", "content": [{

                "type": "tool_result", "tool_use_id": "toolu_wf",

                "content": "Workflow launched in background. Task ID: wtask1",

            }]},

            "tool_use_result": {

                "status": "async_launched", "taskId": "wtask1", "taskType": "local_workflow",

                "workflowName": "probe", "runId": "wf_1", "summary": "probe",

            },

        }));

        answer = "launched".to_owned();
    }

    record_launch("claude-prompt", std::slice::from_ref(&prompt));

    block(2, json!({ "type": "text", "text": "" }));

    delta(2, json!({ "type": "text_delta", "text": answer }));

    stop(2);

    if has(&prompt, "fail") {
        send(&json!({

            "type": "result", "subtype": "error_during_execution", "is_error": true,

            "errors": ["fake failure"], "session_id": session,

        }));
    } else {
        send(&json!({

            "type": "result", "subtype": "success", "is_error": false,

            "result": answer, "session_id": session,

        }));
    }

    if has(&prompt, "late") {
        block(3, json!({ "type": "text", "text": "" }));

        delta(3, json!({ "type": "text_delta", "text": "too late" }));
    }

    if has(&prompt, "workflow-finish") {
        // A finished workflow notifies the session, which starts a follow-up turn.

        std::thread::sleep(std::time::Duration::from_millis(300));

        send(&json!({

            "type": "system", "subtype": "task_notification", "task_id": "wtask1",

            "tool_use_id": "toolu_wf", "status": "completed", "output_file": "", "summary": "probe",

        }));

        send(&json!({

            "type": "system", "subtype": "init", "session_id": session, "model": "fake",

            "slash_commands": [],

        }));

        block(4, json!({ "type": "text", "text": "" }));

        delta(
            4,
            json!({ "type": "text_delta", "text": " workflow finished" }),
        );

        stop(4);

        send(&json!({

            "type": "result", "subtype": "success", "is_error": false,

            "result": "workflow finished", "session_id": session,

        }));
    } else if workflow {
        // Runs until stopped, as the CLI does; a stop starts no follow-up turn.

        for frame in lines.by_ref() {
            if frame["type"] == "control_request" && frame["request"]["subtype"] == "stop_task" {
                record_launch("claude-stop-task", &[frame.to_string()]);

                send(&json!({

                    "type": "system", "subtype": "task_notification",

                    "task_id": frame["request"]["task_id"], "tool_use_id": "toolu_wf",

                    "status": "stopped", "output_file": "", "summary": "probe",

                }));

                send(&json!({ "type": "control_response", "response": {

                    "subtype": "success", "request_id": frame["request_id"], "response": {},

                }}));
            }
        }

        return;
    }

    for _ in lines {}
}
