use serde_json::{Value, json};

use super::{CodexState, SPENT_PRIMARY_RESET, SPENT_SECONDARY_RESET, USAGE_LIMIT_MESSAGE};
use crate::shared::{has, mcp_call, record_launch, send, sibling_exists, write_sibling};

fn codex_note(state: &CodexState, method: &str, mut params: Value) {
    params["threadId"] = json!(state.thread);

    params["turnId"] = json!(state.turn);

    send(&json!({ "method": method, "params": params }));
}

pub(super) fn codex_finish(state: &CodexState, status: &str) {
    codex_finish_saying(state, status, "fake failure");
}

/// Ends the turn with the engine's own sentence, as a real failure frame does.
pub(super) fn codex_finish_saying(state: &CodexState, status: &str, message: &str) {
    let error = (status == "failed").then(|| json!({ "message": message }));

    send(&json!({ "method": "turn/completed", "params": {

        "threadId": state.thread,

        "turn": { "id": state.turn, "items": [], "status": status, "error": error },

    }}));
}

const AGENT_THREAD: &str = "codex-agent-thread";

/// Scenario tags where the parent's turn completes while a sub-agent still runs.
const BACKGROUND_TAGS: [&str; 8] = [
    "bg-agent-finish",
    "bg-agent-quiet",
    "bg-agent-hang",
    "bg-agent-exit",
    "bg-agent-crash",
    "bg-agent-error",
    "bg-agent-closed",
    "bg-agent-stale",
];

/// A page whose own inline script drives the host bridge, served when the test writes an
/// `mcp-app-scripted` file beside the launch log: it asks for `ui/initialize`, reports the
/// tool-input and tool-result notifications, and turns its buttons into the requests a real
/// app makes (a tool call, model-context notes, a resource read and an unsupported request).
/// The web spec drives it through the real render route and sandbox.
const SCRIPTED_APP_PAGE: &str = r#"<!doctype html>
<meta charset="utf-8">
<title>demo widget</title>
<style>body{font:13px system-ui;margin:8px}pre{white-space:pre-wrap;margin:2px 0}button{margin:2px 4px 6px 0}</style>
<p id=v>demo widget</p>
<button id=initialize>Initialize</button>
<button id=initialized>Initialized</button>
<button id=call>Call tool</button>
<button id=note>Send note</button>
<button id=read>Read the page's resource</button>
<button id=unknown>Ask for a link</button>
<pre id=init>init: pending</pre>
<pre id=notifications>notifications: pending</pre>
<pre id=call-result>call: pending</pre>
<pre id=note-result>note: pending</pre>
<pre id=read-result>read: pending</pre>
<pre id=unknown-result>unknown: pending</pre>
<script>
(() => {
  let next = 1;
  const pending = new Map();
  const show = (id, value) => { document.getElementById(id).textContent = value; };
  const request = (method, params) => new Promise((resolve) => {
    const id = next++;
    pending.set(id, resolve);
    window.parent.postMessage({ jsonrpc: "2.0", id, method, params }, "*");
  });
  const notify = (method, params) => window.parent.postMessage({ jsonrpc: "2.0", method, params }, "*");
  window.addEventListener("message", (event) => {
    const message = event.data;
    if (!message || message.jsonrpc !== "2.0") return;
    if (message.id !== undefined && pending.has(message.id)) {
      pending.get(message.id)(message);
      pending.delete(message.id);
      return;
    }
    if (message.method === "ui/notifications/tool-input") {
      show("notifications", "input " + JSON.stringify(message.params.arguments));
    } else if (message.method === "ui/notifications/tool-result") {
      show("notifications", document.getElementById("notifications").textContent + " result " + JSON.stringify(message.params));
    }
  });
  document.getElementById("initialize").onclick = async () => {
    // The descriptor read lands after the frame starts; ask again until the host has it.
    for (let attempt = 0; attempt < 50; attempt++) {
      const reply = await request("ui/initialize", { hostContext: { toolInfo: { tool: {
        name: "spoofed", description: "spoofed", inputSchema: { type: "string" } } } } });
      const context = reply.result && reply.result.hostContext;
      if (context && context.toolInfo) {
        show("init", "protocol " + reply.result.protocolVersion + " theme " + context.theme
          + " tool " + JSON.stringify(context.toolInfo));
        return;
      }
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    show("init", "tool-info never arrived");
  };
  document.getElementById("initialized").onclick = () => notify("ui/notifications/initialized", {});
  document.getElementById("call").onclick = async () => {
    const reply = await request("tools/call", { name: "show_widget", arguments: { text: "from app" },
      server: "evil", tool: "evil", confirmed: true });
    show("call-result", JSON.stringify(reply));
  };
  document.getElementById("note").onclick = async () => {
    const reply = await request("ui/update-model-context", {
      content: [{ type: "text", text: "selected: blue" }], structuredContent: { color: "blue" } });
    show("note-result", JSON.stringify(reply));
  };
  document.getElementById("read").onclick = async () => {
    const reply = await request("resources/read", { uri: "ui://demo/widget.html" });
    show("read-result", JSON.stringify(reply));
  };
  document.getElementById("unknown").onclick = async () => {
    const reply = await request("ui/open-link", { url: "https://evil.example/" });
    show("unknown-result", JSON.stringify(reply));
  };
})();
</script>"#;

fn agent_status(status: &str) {
    send(&json!({ "method": "thread/status/changed", "params": {
        "threadId": AGENT_THREAD, "status": { "type": status, "activeFlags": [] },
    }}));
}

/// The parent spawns a sub-agent thread that is running when the parent's turn ends.
fn codex_spawn_agent(state: &CodexState) {
    send(&json!({ "method": "thread/started", "params": { "thread": {
        "id": AGENT_THREAD, "parentThreadId": state.thread,
        "status": { "type": "active", "activeFlags": [] },
        "source": { "subAgent": { "thread_spawn": {
            "parent_thread_id": state.thread, "depth": 1, "agent_path": null,
            "agent_nickname": "probe", "agent_role": null,
        }}},
    }}}));

    codex_note(
        state,
        "item/started",
        json!({ "item": {
            "type": "collabAgentToolCall", "id": "collab1", "tool": "spawnAgent",
            "status": "inProgress", "senderThreadId": state.thread,
            "receiverThreadIds": [AGENT_THREAD], "prompt": "probe",
            "agentsStates": { AGENT_THREAD: { "status": "running", "message": null } },
        }}),
    );
}

/// What the engine does after the parent's turn has completed with the agent running.
fn codex_background_end(state: &CodexState, lines: &mut impl Iterator<Item = Value>) {
    let prompt = &state.prompt;

    // Said after the turn completed: a driver that held the run reads it, and one
    // that ended the run never does.
    send(&json!({ "method": "warning", "params": { "message": "agent still running" } }));

    if has(prompt, "bg-agent-exit") || has(prompt, "bg-agent-crash") {
        std::thread::sleep(std::time::Duration::from_millis(200));

        std::process::exit(if has(prompt, "bg-agent-crash") { 3 } else { 0 });
    }

    if has(prompt, "bg-agent-hang") {
        // The agent never ends: only a cancel or the engine's exit gets us out.
        for message in lines.by_ref() {
            if message["method"] == "turn/interrupt" {
                send(&json!({ "id": message["id"], "result": {} }));
            }
        }

        std::process::exit(0);
    }

    std::thread::sleep(std::time::Duration::from_millis(300));

    if has(prompt, "bg-agent-error") {
        agent_status("systemError");
    } else if has(prompt, "bg-agent-closed") {
        send(&json!({ "method": "thread/closed", "params": { "threadId": AGENT_THREAD } }));
    } else {
        agent_status("idle");
    }

    if has(prompt, "bg-agent-stale") {
        // A call's state, delivered after the thread's own status said it ended.
        codex_note(
            state,
            "item/completed",
            json!({ "item": {
                "type": "collabAgentToolCall", "id": "collab1", "tool": "spawnAgent",
                "status": "completed", "senderThreadId": state.thread,
                "receiverThreadIds": [AGENT_THREAD], "prompt": "probe",
                "agentsStates": { AGENT_THREAD: { "status": "running", "message": null } },
            }}),
        );
    }

    if has(prompt, "bg-agent-finish") {
        // The agent's result reaches the parent, which answers in a turn of its own.
        let follow = CodexState {
            prompt: String::new(),
            mcp_url: None,
            thread: state.thread.clone(),
            tools_ready: true,
            turn: "turn-2".to_owned(),
            loaded: true,
        };

        send(&json!({ "method": "turn/started", "params": {
            "threadId": follow.thread, "turn": { "id": follow.turn, "items": [], "status": "inProgress" },
        }}));

        codex_note(
            &follow,
            "item/agentMessage/delta",
            json!({ "itemId": "msg2", "delta": " and the agent finished" }),
        );

        codex_finish(&follow, "completed");
    }
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

    if has(prompt, "usage-limited") {
        // Both windows at their limit, with the later reset on the secondary: the
        // driver keeps the latest spent window, so a person waits for the longest one.
        send(
            &json!({ "method": "account/rateLimits/updated", "params": { "rateLimits": {

                "primary": { "usedPercent": 100, "windowDurationMins": 300, "resetsAt": SPENT_PRIMARY_RESET },

                "secondary": { "usedPercent": 100, "windowDurationMins": 10080, "resetsAt": SPENT_SECONDARY_RESET },

            }}}),
        );

        // A fresh app-server session (the limits read) reports the same spent account.
        write_sibling("rate-limits-spent", "1");

        codex_note(
            state,
            "error",
            json!({

                "error": { "message": USAGE_LIMIT_MESSAGE, "codexErrorInfo": "usageLimitExceeded" },

                "willRetry": false,

            }),
        );

        codex_finish_saying(state, "failed", USAGE_LIMIT_MESSAGE);

        return;
    }

    // A previous or engine-started turn on this thread finishing must not end
    // the run for the person's prompt either.
    if has(prompt, "stale-turn") {
        send(&json!({ "method": "turn/completed", "params": {
            "threadId": state.thread,
            "turn": {
                "id": "turn-previous", "items": [], "status": "failed",
                "error": { "message": "previous turn failed" },
            },
        }}));
    }

    // Words of an earlier turn on this thread that reach us after this prompt was sent.
    if has(prompt, "leftover-turn") {
        let earlier = CodexState {
            prompt: String::new(),
            mcp_url: None,
            thread: state.thread.clone(),
            tools_ready: true,
            turn: "turn-previous".to_owned(),
            loaded: true,
        };

        codex_note(
            &earlier,
            "item/agentMessage/delta",
            json!({ "itemId": "old-msg", "delta": "the old turn spoke" }),
        );
    }

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

    if !has(prompt, "silent") {
        codex_note(
            state,
            "turn/plan/updated",
            json!({

                "explanation": null, "plan": [{ "step": "do it", "status": "inProgress" }],

            }),
        );
    }

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
                } else if sibling_exists("mcp-app-scripted") {
                    SCRIPTED_APP_PAGE.to_owned()
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

    if !has(prompt, "silent") {
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
    }

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

    let background = BACKGROUND_TAGS.iter().any(|tag| has(prompt, tag));

    if background {
        codex_spawn_agent(state);
    }

    if has(prompt, "bg-internal") {
        // An agent the engine runs for itself, still active when the turn ends.
        send(&json!({ "method": "thread/started", "params": { "thread": {
            "id": "codex-review-thread", "parentThreadId": state.thread,
            "status": { "type": "active", "activeFlags": [] },
            "source": { "subAgent": "review" },
        }}}));
    }

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

    if background {
        codex_background_end(state, lines);
    }

    if has(prompt, "late") {
        codex_note(
            state,
            "item/agentMessage/delta",
            json!({ "itemId": "msg1", "delta": "too late" }),
        );
    }
}
