use super::events::UpdateMapper;
use super::models::{parse_models, parse_session_options, require_offered};
use super::permissions::{classify, permission_outcome};
use super::sign_in::{MAX_SIGN_IN_ARGS, SignInSlot, known_sign_in, terminal_sign_in};
use super::*;
use crate::governance::approvals::Verdict;
use orbyn_contracts::ActionKind;
use orbyn_contracts::AgUiEvent;

#[test]
fn agents_without_http_mcp_get_the_stdio_bridge() {
    let url = "http://127.0.0.1:1/mcp/token";
    let http = json!({"agentCapabilities":{"mcpCapabilities":{"http":true}}});
    let plain = json!({"agentCapabilities":{}});
    let program = || Some(std::path::PathBuf::from("orbyn"));
    assert_eq!(servers_for(&http, Some(url), program)[0]["type"], "http");
    let stdio = servers_for(&plain, Some(url), program);
    assert_eq!(stdio[0]["args"], json!(["acp-mcp-bridge"]));
    assert_eq!(stdio[0]["env"][0]["value"], url);
    assert_eq!(servers_for(&plain, Some(url), || None), json!([]));
    assert_eq!(servers_for(&plain, None, program), json!([]));
}

#[test]
fn tool_updates_report_only_terminal_outcomes() {
    let mut mapper = UpdateMapper::default();
    for (id, status, expected) in [
        ("ok", "completed", Some("success")),
        ("bad", "failed", Some("failure")),
        ("pending", "in_progress", None),
    ] {
        mapper.on_update(&json!({ "sessionUpdate":"tool_call", "toolCallId":id, "title":"Tool" }));
        let events = mapper.on_update(
            &json!({ "sessionUpdate":"tool_call_update", "toolCallId":id, "status":status }),
        );
        let outcomes: Vec<_> = events
            .into_iter()
            .filter_map(|event| match event {
                EngineEvent::Agui(AgUiEvent::Custom { name, value }) if name == "tool.outcome" => {
                    Some(value)
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            outcomes,
            expected
                .map(|outcome| json!({ "toolCallId":id, "outcome":outcome }))
                .into_iter()
                .collect::<Vec<_>>()
        );
    }
}

fn options() -> Value {
    json!({ "options": [
        { "optionId": "yes-always", "name": "Always", "kind": "allow_always" },
        { "optionId": "yes", "name": "Yes", "kind": "allow_once" },
        { "optionId": "no", "name": "No", "kind": "reject_once" },
    ]})
}

#[test]
fn allow_picks_the_narrowest_grant() {
    assert_eq!(
        permission_outcome(&options(), &Verdict::Allow, false),
        json!({ "outcome": "selected", "optionId": "yes" })
    );
}

#[test]
fn deny_picks_reject_and_missing_option_never_allows() {
    assert_eq!(
        permission_outcome(&options(), &Verdict::Deny("x".into()), false),
        json!({ "outcome": "selected", "optionId": "no" })
    );
    let only_allow = json!({ "options": [{ "optionId": "y", "kind": "allow_once" }] });
    assert_eq!(
        permission_outcome(&only_allow, &Verdict::Deny("x".into()), false),
        json!({ "outcome": "cancelled" })
    );
}

#[test]
fn cancellation_answers_cancelled() {
    assert_eq!(
        permission_outcome(&options(), &Verdict::Allow, true),
        json!({ "outcome": "cancelled" })
    );
}

#[test]
fn chunks_form_one_message_until_the_kind_changes() {
    let mut mapper = UpdateMapper::default();
    let chunk = json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "a" } });
    assert_eq!(mapper.on_update(&chunk).len(), 2);
    assert_eq!(mapper.on_update(&chunk).len(), 1);
    let tool = json!({ "sessionUpdate": "tool_call", "toolCallId": "c", "title": "ls" });
    assert_eq!(mapper.on_update(&tool).len(), 2);
    assert_eq!(mapper.close().len(), 1);
}

#[test]
fn modes_and_config_options_are_listed_and_the_current_ones_marked() {
    let session = json!({
        "modes": { "currentModeId": "ask", "availableModes": [
            { "id": "ask", "name": "Ask", "description": "Asks first" },
            { "id": "code", "name": "Code" },
        ]},
        "configOptions": [
            { "id": "model", "name": "Model", "category": "model", "currentValue": "b",
              "type": "select", "options": [
                { "group": "one", "name": "One", "options": [{ "value": "a", "name": "A" }] },
                { "value": "b", "name": "B", "description": "Big" },
            ]},
            { "id": "think", "name": "Thinking", "category": "thought_level",
              "currentValue": "on", "type": "select",
              "options": [{ "value": "on", "name": "On" }, { "value": "off", "name": "Off" }] },
            { "id": "broken", "name": "No choices", "options": [] },
        ],
    });
    let models = parse_models(&session);
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, "a");
    assert!(models[1].is_default);
    assert_eq!(models[1].description.as_deref(), Some("Big"));
    let options = parse_session_options(&session);
    assert_eq!(options.len(), 2, "{options:?}");
    assert_eq!(options[0].id, "mode");
    assert!(options[0].values[0].default);
    assert_eq!(options[1].id, "config:think");
    assert!(options[1].values[0].default);
}

#[test]
fn a_mode_the_session_stopped_offering_fails_the_turn() {
    let session = json!({ "modes": { "availableModes": [{ "id": "code" }] } });
    require_offered(&session, MODE, "code").unwrap();
    let refused = require_offered(&session, MODE, "plan").unwrap_err();
    assert_eq!(refused.code(), "unsupported_option");
    assert!(require_offered(&json!({}), MODE, "code").is_err());
}

#[test]
fn execute_targets_the_command_text() {
    let params = json!({ "toolCall": { "title": "List", "kind": "execute", "rawInput": { "command": "ls -a" } } });
    let (kind, target, _) = classify(&params);
    assert_eq!(kind, ActionKind::Command);
    assert_eq!(target, "ls -a");
}

#[test]
fn a_terminal_method_becomes_the_launch_plus_its_arguments() {
    let launch = ["opencode".to_owned(), "acp".to_owned()];
    // OpenCode 2.2.5's reply to a client that can show a terminal sign-in.
    let init = json!({"authMethods": [{"id": "opencode-login", "name": "Login with opencode",
        "type": "terminal", "args": ["--login"]}]});
    assert_eq!(
        terminal_sign_in(&launch, &init).as_deref(),
        Some("opencode acp --login")
    );
    let spaced = ["C:/Program Files/agent.exe".to_owned()];
    let bare = json!({"authMethods": [{"id": "a", "name": "a", "type": "terminal"}]});
    assert_eq!(
        terminal_sign_in(&spaced, &bare).as_deref(),
        Some("\"C:/Program Files/agent.exe\"")
    );
}

#[test]
fn only_a_plain_terminal_method_is_offered() {
    let launch = ["agent".to_owned()];
    for init in [
        json!({}),
        json!({"authMethods": [{"id": "a", "name": "Browser"}]}),
        json!({"authMethods": [{"id": "a", "name": "Key", "type": "env_var", "vars": [{"name": "KEY"}]}]}),
        json!({"authMethods": [{"id": "a", "name": "a", "type": "terminal", "env": {"MODE": "1"}}]}),
        json!({"authMethods": [{"id": "a", "name": "a", "type": "terminal", "args": ["$(id)"]}]}),
        json!({"authMethods": [{"id": "a", "name": "a", "type": "terminal", "args": ["a;b"]}]}),
        json!({"authMethods": [{"id": "a", "name": "a", "type": "terminal", "args": [1]}]}),
        json!({"authMethods": [{"id": "a", "name": "a", "type": "terminal", "args": "--login"}]}),
        json!({"authMethods": [{"id": "a", "name": "a", "type": "terminal",
            "args": vec!["x"; MAX_SIGN_IN_ARGS + 1]}]}),
    ] {
        assert_eq!(terminal_sign_in(&launch, &init), None, "{init}");
    }
}

#[test]
fn a_handshake_records_and_clears_the_engine_sign_in() {
    let slot = SignInSlot {
        id: "sign-in-slot-test".into(),
        launch: vec!["agent".into()],
    };
    slot.record(&json!({"authMethods": [{"id": "a", "name": "a", "type": "terminal"}]}));
    assert_eq!(known_sign_in("sign-in-slot-test").as_deref(), Some("agent"));
    slot.record(&json!({"authMethods": []}));
    assert_eq!(known_sign_in("sign-in-slot-test"), None);
}
