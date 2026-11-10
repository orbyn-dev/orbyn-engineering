use serde_json::{Value, json};

use super::CodexState;
use crate::shared::{has, mcp_call, record_launch, send};

fn codex_note(state: &CodexState, method: &str, mut params: Value) {
    params["threadId"] = json!(state.thread);

    params["turnId"] = json!(state.turn);

    send(&json!({ "method": method, "params": params }));
}

pub(super) fn codex_finish(state: &CodexState, status: &str) {
    let error = (status == "failed").then(|| json!({ "message": "fake failure" }));

    send(&json!({ "method": "turn/completed", "params": {

        "threadId": state.thread,

        "turn": { "id": state.turn, "items": [], "status": status, "error": error },

    }}));
}

/// Sends a server request and waits for its answer, as app-server does.
fn codex_request(method: &str, params: Value, lines: &mut impl Iterator<Item = Value>) -> Value {
    send(&json!({ "id": 900, "method": method, "params": params }));

    for message in lines.by_ref() {
        if message.get("id") == Some(&json!(900)) && message.get("method").is_none() {
            return message;
        }
    }

    std::process::exit(0);
}

pub(super) fn codex_turn(state: &CodexState, lines: &mut impl Iterator<Item = Value>) {
    let prompt = &state.prompt;

    // Sub-agent traffic shares the stream and must not end this turn.

    send(&json!({ "method": "turn/completed", "params": {

        "threadId": "subagent-thread",

        "turn": { "id": "turn-x", "items": [], "status": "completed" },

    }}));

    codex_note(
        state,
        "item/started",
        json!({

            "startedAtMs": 1, "item": { "type": "reasoning", "id": "rs1" },

        }),
    );

    codex_note(
        state,
        "item/reasoning/summaryTextDelta",
        json!({

            "itemId": "rs1", "summaryIndex": 0, "delta": "thinking",

        }),
    );

    codex_note(
        state,
        "item/completed",
        json!({

            "completedAtMs": 2, "item": { "type": "reasoning", "id": "rs1" },

        }),
    );

    codex_note(
        state,
        "turn/plan/updated",
        json!({

            "explanation": null, "plan": [{ "step": "do it", "status": "inProgress" }],

        }),
    );

    if has(prompt, "plan-item") {
        codex_note(
            state,
            "item/completed",
            json!({

                "completedAtMs": 2, "item": { "type": "plan", "id": "plan1", "text": "# Ship\n1. a" },

            }),
        );
    }

    let mut answer = "hello from codex".to_owned();

    if has(prompt, "unknown-request") {
        let reply = codex_request("item/tool/call", json!({ "itemId": "x" }), lines);

        answer = format!(
            "refused {}",
            reply.pointer("/error/code").unwrap_or(&Value::Null)
        );
    }

    if has(prompt, "token-refresh") {
        let reply = codex_request(
            "account/chatgptAuthTokens/refresh",
            json!({ "reason": "unauthorized" }),
            lines,
        );

        let code = reply.pointer("/error/code").unwrap_or(&Value::Null);

        let result = reply.get("result").unwrap_or(&Value::Null);

        answer = format!("token refresh error {code} result {result}");
    }

    if has(prompt, "outside-write") {
        let target = std::env::var("ORBYN_FAKE_WRITE_PATH").expect("write path");

        codex_note(
            state,
            "item/started",
            json!({

                "startedAtMs": 3,

                "item": {

                    "type": "fileChange", "id": "fc1", "status": "inProgress",

                    "changes": [{ "path": target, "kind": { "type": "update" }, "diff": "" }],

                },

            }),
        );

        let reply = codex_request(
            "item/fileChange/requestApproval",
            json!({

                "itemId": "fc1", "threadId": state.thread, "turnId": state.turn, "startedAtMs": 3,

            }),
            lines,
        );

        let decision = reply
            .pointer("/result/decision")
            .and_then(Value::as_str)
            .unwrap_or("none");

        answer = format!("write {decision}");
    }

    // Writing is how a turn changes the workspace; `[write:<name>]` names the file.

    if let Some(rest) = prompt.split("[write:").nth(1)
        && let Some(name) = rest.split(']').next()
    {
        let checkout = std::env::var("ORBYN_FAKE_CHECKOUT").expect("checkout");

        let item = json!({

            "type": "fileChange", "id": format!("write-{name}"), "status": "inProgress",

            "changes": [{ "path": name, "kind": { "type": "add" }, "diff": "" }],

        });

        codex_note(state, "item/started", json!({ "item": item }));

        std::fs::write(
            std::path::Path::new(&checkout).join(name),
            "written by turn\n",
        )
        .expect("write");

        codex_note(state, "item/completed", json!({ "item": item }));
    }

    // An MCP App tool call, in the shape Codex 0.166.0 reported for a descriptor with

    // `_meta.ui.resourceUri`; the driver then reads the page from this process.

    if has(prompt, "mcp-app") {
        let item = json!({

            "type": "mcpToolCall", "id": "app-call-1", "server": "demo", "tool": "show_widget",

            "status": "completed", "arguments": { "text": "hello" },

            "appContext": null, "mcpAppResourceUri": "ui://demo/widget.html", "mcpAppUi": null,

            "result": { "content": [{ "type": "text", "text": "shown: hello" }],

                        "structuredContent": { "text": "hello" }, "_meta": null },

            "error": null, "durationMs": 4,

        });

        codex_note(state, "item/started", json!({ "item": item }));

        codex_note(state, "item/completed", json!({ "item": item }));

        for message in lines.by_ref() {
            if message["method"] == "mcpServer/resource/read" {
                record_launch("codex-app-read", &[message["params"].to_string()]);

                let html = if has(prompt, "mcp-app-huge") {
                    "x".repeat(600 * 1024)
                } else {
                    "<!doctype html><p id=v>demo widget</p><img src=\"secret.png\">".to_owned()
                };

                send(&json!({ "id": message["id"], "result": { "contents": [{

                    "uri": "ui://demo/widget.html", "mimeType": "text/html;profile=mcp-app",

                    "text": html, "_meta": { "ui": { "prefersBorder": true } },

                }], "originCallId": null }}));

                break;
            }
        }
    }

    // `[mcp-call:<tool>]<json args>[/mcp-call]` calls one tool and answers with it.

    if let Some(rest) = prompt.split("[mcp-call:").nth(1)
        && let Some((name, rest)) = rest.split_once(']')
        && let Some((arguments, _)) = rest.split_once("[/mcp-call]")
    {
        let url = state.mcp_url.as_deref().expect("mcp url");

        let arguments: Value = serde_json::from_str(arguments).expect("tool arguments");

        let (_, reply) = mcp_call(
            url,
            1,
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        );

        answer = format!(
            "tool error={} {}",
            reply.pointer("/result/isError").is_some(),
            reply
                .pointer("/result/content/0/text")
                .and_then(Value::as_str)
                .unwrap_or("none"),
        );
    }

    if has(prompt, "mcp-tools") {
        let url = state.mcp_url.as_deref().expect("mcp url");

        let (status, reply) = mcp_call(url, 1, "tools/list", json!({}));

        let tools = reply
            .pointer("/result/tools")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);

        answer = format!("mcp {status} tools {tools}");
    }

    if has(prompt, "question") {
        let reply = codex_request(
            "item/tool/requestUserInput",
            json!({

                "threadId": state.thread, "turnId": state.turn, "itemId": "ask1",

                "questions": [{

                    "id": "q1", "header": "Token", "question": "Paste it", "isOther": true,

                    "isSecret": true, "options": null,

                }],

            }),
            lines,
        );

        answer = format!("answers {}", reply["result"]["answers"]);
    }

    if has(prompt, "approve") || has(prompt, "crash-in-approval") {
        codex_note(
            state,
            "item/started",
            json!({

                "startedAtMs": 3,

                "item": {

                    "type": "commandExecution", "id": "cmd1", "command": "ls", "cwd": "/repo",

                    "commandActions": [], "status": "inProgress",

                },

            }),
        );

        if has(prompt, "crash-in-approval") {
            send(
                &json!({ "id": 901, "method": "item/commandExecution/requestApproval", "params": {

                    "itemId": "cmd1", "threadId": state.thread, "turnId": state.turn,

                    "startedAtMs": 3, "command": "ls",

                }}),
            );

            std::thread::sleep(std::time::Duration::from_millis(300));

            std::process::exit(7);
        }

        let reply = codex_request(
            "item/commandExecution/requestApproval",
            json!({

                "itemId": "cmd1", "threadId": state.thread, "turnId": state.turn,

                "startedAtMs": 3, "command": "ls",

            }),
            lines,
        );

        let decision = reply
            .pointer("/result/decision")
            .and_then(Value::as_str)
            .unwrap_or("none");

        codex_note(
            state,
            "item/commandExecution/outputDelta",
            json!({

                "itemId": "cmd1", "delta": "listing",

            }),
        );

        codex_note(
            state,
            "item/completed",
            json!({

                "completedAtMs": 4,

                "item": {

                    "type": "commandExecution", "id": "cmd1", "command": "ls", "cwd": "/repo",

                    "commandActions": [], "status": "completed", "aggregatedOutput": "listing",

                "exitCode": 0,

                },

            }),
        );

        codex_note(
            state,
            "turn/diff/updated",
            json!({

                "diff": "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n",

            }),
        );

        answer = format!("decision {decision}");
    }

    if has(prompt, "hang") {
        for message in lines.by_ref() {
            if message["method"] == "turn/interrupt" {
                send(&json!({ "id": message["id"], "result": {} }));

                codex_finish(state, "interrupted");

                return;
            }

            if message["method"] == "turn/steer" {
                send(&json!({ "id": message["id"], "result": { "turnId": state.turn } }));

                answer = format!(
                    "steered: {}",
                    message["params"]["input"][0]["text"]
                        .as_str()
                        .unwrap_or_default()
                );

                break;
            }
        }

        if !answer.starts_with("steered") {
            return;
        }
    }

    codex_note(
        state,
        "item/started",
        json!({

            "startedAtMs": 5, "item": { "type": "agentMessage", "id": "msg1", "text": "" },

        }),
    );

    codex_note(
        state,
        "item/agentMessage/delta",
        json!({ "itemId": "msg1", "delta": answer }),
    );

    codex_note(
        state,
        "item/completed",
        json!({

            "completedAtMs": 6, "item": { "type": "agentMessage", "id": "msg1", "text": answer },

        }),
    );

    codex_note(
        state,
        "thread/tokenUsage/updated",
        json!({

            "tokenUsage": {

                "total": { "totalTokens": 120 }, "last": { "totalTokens": 20 },

                "modelContextWindow": 1000,

            },

        }),
    );

    if has(prompt, "fail") {
        codex_note(
            state,
            "error",
            json!({

                "error": { "message": "fake failure" }, "willRetry": false,

            }),
        );

        codex_finish(state, "failed");
    } else {
        codex_finish(state, "completed");
    }

    if has(prompt, "late") {
        codex_note(
            state,
            "item/agentMessage/delta",
            json!({ "itemId": "msg1", "delta": "too late" }),
        );
    }
}
