use serde_json::{Value, json};

use crate::shared::{record_launch, send, sibling_exists, stdin_lines};

mod turn;

use turn::{codex_finish, codex_turn};
// Codex app-server: JSONL without a `jsonrpc` header.

struct CodexState {
    prompt: String,

    mcp_url: Option<String>,

    thread: String,

    tools_ready: bool,

    /// Each turn gets its own id, as Codex's are, so a revert can name one.
    turn: String,

    loaded: bool,
}

pub(crate) fn codex() {
    let mut lines = stdin_lines();

    let mut state = CodexState {
        prompt: String::new(),

        mcp_url: None,

        thread: "codex-thread-1".to_owned(),

        tools_ready: true,

        turn: "turn-1".to_owned(),

        loaded: false,
    };

    while let Some(message) = lines.next() {
        let method = message["method"].as_str().unwrap_or_default().to_owned();

        let id = message.get("id").cloned().unwrap_or(Value::Null);

        assert!(
            message.get("jsonrpc").is_none(),
            "app-server frames have no header"
        );

        if method.starts_with("account/login") {
            eprintln!("drivers must never call {method}");

            std::process::exit(11);
        }

        match method.as_str() {
            "initialize" => send(&json!({ "id": id, "result": {

                "userAgent": "fake/0.166.0", "codexHome": "/tmp/codex",

                "platformFamily": "unix", "platformOs": "macos",

            }})),

            "initialized" => {}

            "skills/extraRoots/set" => {
                eprintln!("skills come from native discovery");

                std::process::exit(12);
            }

            "thread/resume" if std::env::var_os("ORBYN_FAKE_LOSE_SESSION").is_some() => {
                send(&json!({ "id": id, "error": {

                    "code": -32600, "message": "no rollout found for thread id",

                }}));
            }

            "thread/start" | "thread/resume" => {
                let params = &message["params"];

                // A run states its policy; a bare resume only loads the thread.

                let mode = if params.get("approvalPolicy").is_some() {
                    "codex-thread"
                } else {
                    "codex-load"
                };

                record_launch(mode, &[params.to_string()]);

                if let Some(thread) = params["threadId"].as_str() {
                    state.thread = thread.to_owned();
                }

                state.loaded = true;

                // Real Codex takes the server as a dotted key, which leaves a user's own

                // servers of the same table whole.

                state.mcp_url = params["config"]["mcp_servers.orbyn_engineering"]["url"]
                    .as_str()
                    .map(str::to_owned);

                send(&json!({ "id": id, "result": {

                    "thread": { "id": state.thread }, "cwd": params["cwd"], "model": "fake",

                    "modelProvider": "fake", "approvalPolicy": params["approvalPolicy"],

                    "approvalsReviewer": "user", "sandbox": { "type": "workspaceWrite" },

                }}));

                let slow = std::env::var_os("ORBYN_FAKE_SLOW_TOOLS").is_some();

                state.tools_ready = !slow;
            }

            "mcpServerStatus/list" => {
                if !state.tools_ready {
                    std::thread::sleep(std::time::Duration::from_millis(300));

                    state.tools_ready = true;
                }

                send(&json!({ "id": id, "result": { "nextCursor": null, "data": [

                    { "name": "orbyn_engineering", "tools": { "skill_search": {}, "skill_load": {} }, "authStatus": "unsupported" },

                    { "name": "codex_apps", "tools": { "x": {} }, "authStatus": "bearerToken" },

                    { "name": "filesystem", "tools": { "read": {}, "write": {} }, "authStatus": "unsupported" },

                    { "name": "notion", "tools": {}, "toolsError": "needs login", "authStatus": "notLoggedIn" },

                ] } }));
            }

            // As Codex 0.166.0 answered: a loaded thread is required, and the server's

            // own result comes back unchanged.
            "mcpServer/tool/call" => {
                record_launch("codex-app-tool", &[message["params"].to_string()]);

                let params = &message["params"];

                if params["threadId"].as_str() != Some(state.thread.as_str()) {
                    send(&json!({ "id": id, "error": {

                        "code": -32600, "message": format!("thread not found: {}", params["threadId"]),

                    }}));
                } else {
                    send(&json!({ "id": id, "result": {

                        "content": [{ "type": "text", "text": format!("called {}", params["tool"].as_str().unwrap_or("")) }],

                        "structuredContent": params["arguments"],

                    }}));
                }
            }

            "turn/start" => {
                record_launch("codex-turn", &[message["params"].to_string()]);

                if !state.tools_ready {
                    eprintln!("a turn started while tools were still starting");

                    std::process::exit(13);
                }

                state.prompt = message["params"]["input"][0]["text"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();

                state.turn = format!(
                    "turn-{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |since| since.as_nanos())
                );

                record_launch("codex-turn-id", std::slice::from_ref(&state.turn));

                // A notification ahead of its reply is a legal ordering.

                send(&json!({ "method": "turn/started", "params": {

                    "threadId": state.thread,

                    "turn": { "id": state.turn, "items": [], "status": "inProgress" },

                }}));

                send(&json!({ "id": id, "result": {

                    "turn": { "id": state.turn, "items": [], "status": "inProgress" },

                }}));

                codex_turn(&state, &mut lines);
            }

            "turn/interrupt" => {
                send(&json!({ "id": id, "result": {} }));

                codex_finish(&state, "interrupted");
            }

            "account/rateLimits/read" => {
                send(&json!({ "id": id, "result": {

                    "accountId": "acct-SECRET-MARKER",

                    "rateLimitUpsell": { "text": "SECRET-MARKER upsell" },

                    "rateLimits": {

                        "primary": { "usedPercent": 12, "windowDurationMins": 300, "resetsAt": 1800000000 },

                        "secondary": { "usedPercent": 40, "windowDurationMins": 10080 },

                    },

                }}));
            }

            "account/read" => {
                send(&json!({ "id": id, "result": {

                    "account": { "type": "chatgpt", "email": "x@y.z", "planType": "plus" },

                    "requiresOpenaiAuth": true,

                }}));
            }

            "feedback/upload" => {
                record_launch("codex-feedback", &[message["params"].to_string()]);

                if sibling_exists("feedback-error") {
                    send(&json!({ "id": id, "error": {

                        "code": -32000, "message": "Synthetic feedback failure.",

                    }}));
                } else if sibling_exists("feedback-malformed") {
                    send(&json!({ "id": id, "result": { "promptHash": null } }));
                } else {
                    send(&json!({ "id": id, "result": {

                        "threadId": "fake-feedback-1", "promptHash": null,

                    }}));
                }
            }

            "app/list" => {
                send(&json!({ "id": id, "result": { "nextCursor": null, "data": [

                    { "id": "gh", "name": "GitHub", "description": "Repositories, issues and pull requests", "isAccessible": true, "isEnabled": true },

                    { "id": "gm", "name": "Gmail", "description": "Search and read mail", "isAccessible": false, "isEnabled": true },

                ] } }));
            }

            "plugin/list" => {
                send(
                    &json!({ "id": id, "result": { "marketplaces": [ { "name": "fake", "plugins": [

                    { "name": "orbyn-skills", "installed": true, "enabled": true,

                      "interface": { "displayName": "orbyn-skills", "shortDescription": "3 commands" } },

                ] } ], "marketplaceLoadErrors": [], "featuredPluginIds": [] } }),
                );
            }

            "model/list" if sibling_exists("lost-models") => {
                send(&json!({ "id": id, "error": { "code": -32000, "message": "no list" } }));
            }

            "model/list" => {
                send(&json!({ "id": id, "result": { "nextCursor": null, "data": [

                    {

                        "id": "fake-big", "model": "fake-big", "displayName": "Fake Big",

                        "description": "Large", "hidden": false, "isDefault": true,

                        "defaultReasoningEffort": "medium",

                        "supportedReasoningEfforts": [

                            { "reasoningEffort": "medium", "description": "Balanced" },

                            { "reasoningEffort": "xhigh", "description": "Deep" },

                        ],

                        "serviceTiers": [

                            { "id": "priority", "name": "Fast", "description": "Quicker" },

                        ],

                        "inputModalities": ["text", "image"],

                    },

                    {

                        "id": "fake-small", "model": "fake-small", "displayName": "Fake Small",

                        "description": "", "hidden": false, "isDefault": false,

                        "defaultReasoningEffort": "low",

                        "supportedReasoningEfforts": [

                            { "reasoningEffort": "low", "description": "Quick" },

                        ],

                    },

                    {

                        "id": "fake-old", "model": "fake-old", "displayName": "Fake Old",

                        "description": "", "hidden": true, "isDefault": false,

                        "defaultReasoningEffort": "low", "supportedReasoningEfforts": [],

                    },

                ]}}));
            }

            "config/read" => {
                send(&json!({ "id": id, "result": { "config": {

                    "service_tier": std::env::var("ORBYN_FAKE_CODEX_TIER").ok(),

                }, "origins": {} }}));
            }

            // As Codex 0.166.0 answered: an unloaded thread is not found, and a

            // test can make it refuse.
            "thread/revert" => {
                let params = &message["params"];

                record_launch("codex-revert", &[params.to_string()]);

                if !state.loaded || params["threadId"].as_str() != Some(state.thread.as_str()) {
                    send(&json!({ "id": id, "error": {

                        "code": -32600, "message": format!("thread not found: {}", params["threadId"]),

                    }}));
                } else if sibling_exists("revert-refused") {
                    send(&json!({ "id": id, "error": {

                        "code": -32600, "message": format!("turn not found: {}", params["beforeTurnId"]),

                    }}));
                } else {
                    send(&json!({ "id": id, "result": {

                        "thread": { "id": state.thread, "turns": [] },

                        "turnsBackwardsCursor": null, "itemsBackwardsCursor": null,

                    }}));

                    send(
                        &json!({ "method": "thread/reverted", "params": { "threadId": state.thread } }),
                    );
                }
            }

            "thread/compact/start" => {
                record_launch("codex-compact", &[]);

                send(&json!({ "id": id, "result": {} }));

                codex_finish(&state, "completed");
            }

            "thread/goal/set" => {
                let params = &message["params"];

                record_launch("codex-goal", &[params.to_string()]);

                let goal = json!({

                    "objective": params["objective"].as_str().unwrap_or("kept"),

                    "status": params["status"], "tokensUsed": 5, "timeUsedSeconds": 2,

                });

                send(&json!({ "id": id, "result": { "goal": goal } }));

                send(&json!({ "method": "thread/goal/updated", "params": {

                    "threadId": state.thread, "goal": goal,

                }}));
            }

            "thread/goal/clear" => {
                record_launch("codex-goal", &["clear".to_owned()]);

                send(&json!({ "id": id, "result": { "cleared": true } }));

                send(&json!({ "method": "thread/goal/cleared", "params": {

                    "threadId": state.thread,

                }}));
            }

            _ => {
                if id != Value::Null && !method.is_empty() {
                    send(&json!({ "id": id, "error": { "code": -32601, "message": "unknown" } }));
                }
            }
        }
    }
}
