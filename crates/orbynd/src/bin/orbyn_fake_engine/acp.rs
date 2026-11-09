use std::io::Write;

use serde_json::{Value, json};

use crate::shared::{has, record_launch, send, sibling_exists, stdin_lines};

// ACP v1: JSON-RPC 2.0 with the header.

pub(crate) fn acp_send(value: Value) {
    let mut value = value;

    value["jsonrpc"] = json!("2.0");

    send(&value);
}

pub(crate) fn acp_update(session: &str, update: Value) {
    acp_send(
        json!({ "method": "session/update", "params": { "sessionId": session, "update": update } }),
    );
}

/// A test takes a mode away between reading the list and running.
pub(crate) fn acp_lost_mode() -> bool {
    sibling_exists("lost-mode")
}

/// Where a test keeps the agent's own saved sessions, when it has any.
pub(crate) fn native_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("ORBYN_FAKE_ACP_NATIVE").map(std::path::PathBuf::from)
}

pub(crate) fn native_sessions() -> Vec<Value> {
    native_dir()
        .and_then(|dir| std::fs::read_to_string(dir.join("sessions.json")).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub(crate) fn acp() {
    let mut lines = stdin_lines();

    let mut session = "acp-session-1".to_owned();

    let session = &mut session;

    let mut servers = Value::Null;

    while let Some(message) = lines.next() {
        assert_eq!(message["jsonrpc"], "2.0", "ACP frames carry the header");

        let id = message.get("id").cloned().unwrap_or(Value::Null);

        if let Some(list) = message.pointer("/params/mcpServers") {
            servers = list.clone();
        }

        match message["method"].as_str().unwrap_or_default() {
            "initialize" => acp_send(json!({ "id": id, "result": {

                "protocolVersion": 1,

                "agentCapabilities": {

                    "mcpCapabilities": { "http": std::env::var_os("ORBYN_FAKE_ACP_STDIO_MCP").is_none() },

                    "loadSession": native_dir().is_some(),

                    "sessionCapabilities": if native_dir().is_some() {

                        json!({ "list": {}, "delete": {} })

                    } else if std::env::var_os("ORBYN_FAKE_ACP_SESSIONS").is_some() {

                        json!({ "delete": {} })

                    } else {

                        json!({})

                    },

                },

                "authMethods": [],

            }})),

            "session/list" if native_dir().is_some() => {
                record_launch("acp-list", &[message["params"].to_string()]);

                if let Some(ms) = std::env::var("ORBYN_FAKE_ACP_LIST_DELAY_MS")
                    .ok()
                    .and_then(|n| n.parse().ok())
                {
                    std::thread::sleep(std::time::Duration::from_millis(ms));
                }

                let cwd = message["params"]["cwd"].as_str().unwrap_or_default();

                let size: usize = std::env::var("ORBYN_FAKE_ACP_PAGE")
                    .ok()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(2);

                let start: usize = message["params"]["cursor"]
                    .as_str()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0);

                let all: Vec<Value> = native_sessions()
                    .into_iter()
                    .filter(|row| {
                        std::env::var_os("ORBYN_FAKE_ACP_LIST_ALL").is_some()
                            || row["cwd"].as_str() == Some(cwd)
                    })
                    .map(|mut row| {
                        row.as_object_mut().map(|row| row.remove("history"));

                        row
                    })
                    .collect();

                let page: Vec<Value> = all.iter().skip(start).take(size).cloned().collect();

                let next = (start + size < all.len()).then(|| (start + size).to_string());

                acp_send(json!({ "id": id, "result": { "sessions": page, "nextCursor": next } }));
            }

            "session/load" if native_dir().is_some() => {
                record_launch("acp-load", &[message["params"].to_string()]);

                let wanted = message["params"]["sessionId"].as_str().unwrap_or_default();

                match native_sessions()
                    .into_iter()
                    .find(|row| row["sessionId"] == wanted)
                {
                    Some(row) => {
                        *session = wanted.to_owned();

                        for update in row["history"].as_array().cloned().unwrap_or_default() {
                            acp_update(wanted, update);
                        }

                        acp_send(json!({ "id": id, "result": {} }));
                    }

                    None => acp_send(
                        json!({ "id": id, "error": { "code": -32002, "message": "no such session" } }),
                    ),
                }
            }

            "session/delete" => {
                record_launch("acp-delete", &[message["params"].to_string()]);

                if let Some(dir) = native_dir() {
                    let wanted = message["params"]["sessionId"].as_str().unwrap_or_default();

                    let kept: Vec<Value> = native_sessions()
                        .into_iter()
                        .filter(|row| row["sessionId"] != wanted)
                        .collect();

                    std::fs::write(dir.join("sessions.json"), Value::Array(kept).to_string())
                        .expect("sessions file");
                }

                acp_send(json!({ "id": id, "result": {} }));
            }

            "session/new" => acp_send(json!({ "id": id, "result": {

                "sessionId": session,

                "models": {

                    "currentModelId": "acp-small",

                    "availableModels": [

                        { "modelId": "acp-small", "name": "ACP Small" },

                        { "modelId": "acp-large", "name": "ACP Large", "description": "Big" },

                    ],

                },

                "modes": {

                    "currentModeId": "ask",

                    "availableModes": if acp_lost_mode() {

                        json!([{ "id": "ask", "name": "Ask" }])

                    } else {

                        json!([{ "id": "ask", "name": "Ask" }, { "id": "code", "name": "Code" }])

                    },

                },

                "configOptions": [{

                    "id": "think", "name": "Thinking", "category": "thought_level",

                    "type": "select", "currentValue": "off",

                    "options": [{ "value": "off", "name": "Off" }, { "value": "on", "name": "On" }],

                }],

            }})),

            "session/set_model" => {
                record_launch("acp-model", &[message["params"].to_string()]);

                // A large model shows what it offers only once it is chosen.

                let large = message["params"]["modelId"] == "acp-large";

                if large && std::env::var_os("ORBYN_FAKE_ACP_SESSIONS").is_some() {
                    acp_send(json!({ "id": id, "result": { "configOptions": [{

                        "id": "effort", "name": "Effort", "category": "thought_level",

                        "type": "select", "currentValue": "low",

                        "options": [{ "value": "low", "name": "Low" }, { "value": "max", "name": "Max" }],

                    }]}}));
                } else if std::env::var_os("ORBYN_FAKE_ACP_REFUSE_MODEL").is_some() {
                    acp_send(json!({ "id": id, "error": { "code": -32000, "message": "no" } }));
                } else {
                    acp_send(json!({ "id": id, "result": {} }));
                }
            }

            "session/set_mode" | "session/set_config_option" => {
                record_launch("acp-config", &[message.to_string()]);

                acp_send(json!({ "id": id, "result": {} }));
            }

            "session/prompt" => {
                let prompt = message["params"]["prompt"][0]["text"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();

                acp_update(
                    session,
                    json!({

                        "sessionUpdate": "agent_thought_chunk",

                        "content": { "type": "text", "text": "thinking" },

                    }),
                );

                let mut answer = "hello from acp".to_owned();

                if has(&prompt, "bridge") {
                    answer = acp_stdio_bridge(&servers);
                }

                if has(&prompt, "approve") {
                    acp_update(
                        session,
                        json!({

                            "sessionUpdate": "tool_call", "toolCallId": "c1", "title": "List files",

                            "kind": "execute", "status": "pending",

                            "rawInput": { "command": "ls" },

                        }),
                    );

                    acp_send(
                        json!({ "id": 700, "method": "session/request_permission", "params": {

                            "sessionId": session,

                            "toolCall": {

                                "toolCallId": "c1", "title": "List files", "kind": "execute",

                                "rawInput": { "command": "ls" },

                            },

                            "options": [

                                { "optionId": "once", "name": "Allow", "kind": "allow_once" },

                                { "optionId": "no", "name": "Reject", "kind": "reject_once" },

                            ],

                        }}),
                    );

                    let reply = lines
                        .by_ref()
                        .find(|line| line.get("id") == Some(&json!(700)));

                    let outcome = reply
                        .as_ref()
                        .and_then(|reply| reply.pointer("/result/outcome/optionId"))
                        .and_then(Value::as_str)
                        .unwrap_or("cancelled")
                        .to_owned();

                    acp_update(
                        session,
                        json!({

                            "sessionUpdate": "tool_call_update", "toolCallId": "c1",

                            "status": "completed", "rawOutput": "listing",

                        }),
                    );

                    answer = format!("option {outcome}");
                }

                if has(&prompt, "hang") {
                    if lines
                        .by_ref()
                        .any(|line| line["method"] == "session/cancel")
                    {
                        acp_send(json!({ "id": id, "result": { "stopReason": "cancelled" } }));
                    }

                    continue;
                }

                acp_update(
                    session,
                    json!({

                        "sessionUpdate": "agent_message_chunk",

                        "content": { "type": "text", "text": answer },

                    }),
                );

                acp_send(json!({ "id": id, "result": { "stopReason": "end_turn" } }));
            }

            _ => {}
        }
    }
}

/// Uses the stdio MCP server Orbyn offered, as an ACP agent without HTTP MCP would:
/// the bridge for `tools/list`, then `acp-mcp-call` for one tool.
pub(crate) fn acp_stdio_bridge(servers: &Value) -> String {
    let server = &servers[0];

    let Some(program) = server["command"].as_str() else {
        return format!("no stdio server in {servers}");
    };

    let env: Vec<(String, String)> = server["env"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|pair| {
            (
                pair["name"].as_str().unwrap_or_default().to_owned(),
                pair["value"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect();

    let args: Vec<String> = server["args"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|arg| arg.as_str().map(str::to_owned))
        .collect();

    let mut bridge = std::process::Command::new(program)
        .args(&args)
        .envs(env.iter().cloned())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("start the bridge");

    {
        let mut input = bridge.stdin.take().expect("bridge stdin");

        for line in [
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ] {
            writeln!(input, "{line}").expect("write to the bridge");
        }
    }

    let output = bridge.wait_with_output().expect("bridge output");

    let replies: Vec<Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();

    let listed = replies.iter().any(|reply| {
        reply["id"] == 2
            && reply["result"]["tools"]
                .as_array()
                .is_some_and(|tools| tools.iter().any(|tool| tool["name"] == "skill_search"))
    });

    let call = std::process::Command::new(program)
        .args(["acp-mcp-call", "skill_search", "{\"query\":\"\"}"])
        .envs(env.iter().cloned())
        .output()
        .expect("run acp-mcp-call");

    let result: Value = serde_json::from_slice(&call.stdout).unwrap_or(Value::Null);

    let called = call.status.success() && result["isError"] != true && result["content"].is_array();

    let token_on_line = args.iter().any(|arg| arg.contains("/mcp/"));

    format!(
        "bridge {} {}; call {}; token on command line {}",
        output.status.success(),
        if listed {
            "listed tools"
        } else {
            "listed nothing"
        },
        if called { "ok" } else { "failed" },
        if token_on_line { "yes" } else { "no" },
    )
}
