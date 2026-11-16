use super::catalog::{apply_configured_tier, parse_model};
use super::events::{TurnMapper, plan_content};
use super::protocol::{
    approval_content, approval_response, classify_request, feedback_params, feedback_receipt,
    is_tool_approval,
};
use super::sandbox::{
    approval_policy, collaboration_mode, sandbox_policy, thread_config, tier_param,
};
use super::tools::codex_tools;
use crate::agents::driver::{EngineOutcome, RunKind};
use crate::agents::selection::SERVICE_TIER;
use crate::governance::approvals::Verdict;
use orbyn_contracts::{AgUiEvent, ModelInfo, PermissionMode};
#[test]
fn limits_keep_only_windows_and_the_plan() {
    let kept = super::limit_fields(serde_json::json!({
        "accountId": "acct-SECRET-MARKER",
        "rateLimitUpsell": { "text": "Upgrade" },
        "rateLimitResetCredits": { "available": 1 },
        "planType": "free",
        "rateLimits": { "primary": { "usedPercent": 1 } },
        "rateLimitsByLimitId": { "codex": { "primary": { "usedPercent": 1 } } },
    }));
    assert!(!kept.to_string().contains("SECRET-MARKER"));
    let mut keys: Vec<&String> = kept.as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(keys, ["planType", "rateLimits", "rateLimitsByLimitId"]);
    assert!(super::limit_fields(serde_json::json!("odd")).is_null());
}

use super::*;

#[test]
fn feedback_request_matches_the_app_server_shape() {
    let request = EngineFeedbackRequest {
        reason: Some("  broke  ".into()),
        include_logs: false,
        thread_id: Some(" thread-1 ".into()),
    };
    assert_eq!(
        feedback_params(&request),
        json!({
            "classification": "bug", "includeLogs": false,
            "reason": "broke", "threadId": "thread-1",
        })
    );
    let blank = EngineFeedbackRequest {
        reason: Some("  ".into()),
        include_logs: true,
        thread_id: Some(String::new()),
    };
    assert_eq!(
        feedback_params(&blank),
        json!({ "classification": "bug", "includeLogs": true })
    );
}

#[test]
fn feedback_reply_needs_a_thread_id() {
    assert_eq!(
        feedback_receipt(&json!({ "threadId": "t", "promptHash": null })).unwrap(),
        EngineFeedbackReceipt {
            thread_id: "t".into(),
            prompt_hash: None,
        }
    );
    assert!(matches!(
        feedback_receipt(&json!({ "promptHash": null })),
        Err(CoreError::Internal(_))
    ));
}

#[test]
fn tool_items_report_known_outcomes_only() {
    for (item, expected) in [
        (
            json!({ "type":"mcpToolCall", "id":"ok", "status":"completed" }),
            Some("success"),
        ),
        (
            json!({ "type":"mcpToolCall", "id":"bad", "status":"failed" }),
            Some("failure"),
        ),
        (
            json!({ "type":"mcpToolCall", "id":"pending", "status":"inProgress" }),
            None,
        ),
        (
            json!({ "type":"commandExecution", "id":"zero", "status":"failed", "exitCode":0 }),
            Some("failure"),
        ),
        (
            json!({ "type":"commandExecution", "id":"nonzero", "status":"completed", "exitCode":1 }),
            Some("failure"),
        ),
        (
            json!({ "type":"commandExecution", "id":"success", "status":"completed", "exitCode":0 }),
            Some("success"),
        ),
        (json!({ "type":"commandExecution", "id":"unknown" }), None),
    ] {
        let id = item["id"].as_str().unwrap();
        let mut mapper = TurnMapper::new("thread".into());
        let step = mapper.on_notification(
            "item/completed",
            &json!({ "threadId":"thread", "item":item }),
        );
        let outcomes: Vec<_> = step
            .events
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

fn mapper() -> TurnMapper {
    TurnMapper::new("thread".into())
}

fn listed(extra: Value) -> Value {
    let mut model = json!({
        "id": "m", "model": "m", "displayName": "M", "description": "", "hidden": false,
        "isDefault": true, "defaultReasoningEffort": "low",
        "supportedReasoningEfforts": [
            { "reasoningEffort": "low", "description": "Quick" },
            { "reasoningEffort": "high", "description": "" },
        ],
    });
    model
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    model
}

/// Two real entries from a 0.166.0 `model/list`.
const RECORDED: &str = include_str!("../fixtures/codex-0.166.0-model-list.json");

#[test]
fn a_recorded_list_parses_as_the_live_one_will() {
    let page: Value = serde_json::from_str(RECORDED).unwrap();
    let models: Vec<ModelInfo> = page["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(parse_model)
        .collect();
    assert_eq!(models.len(), 2);
    for model in &models {
        let effort = model.options.iter().find(|option| option.id == "effort");
        let effort = effort.unwrap();
        assert_eq!(
            effort.values.iter().filter(|value| value.default).count(),
            1
        );
        assert!(!model.input_modalities.is_empty());
    }
}

/// `cargo test -p orbyn-core live_ -- --ignored`. Needs the CLI, spends no usage.
#[tokio::test]
#[ignore = "needs a real Codex"]
async fn live_catalog_is_the_engines_own() {
    let driver = CodexDriver::new("codex".into(), "workspace-write".into());
    let catalog = driver.models().await.unwrap().unwrap();
    assert!(!catalog.models.is_empty());
}

#[test]
fn a_model_lists_its_efforts_tiers_and_modalities() {
    let model = parse_model(&listed(json!({
        "serviceTiers": [
            { "id": "priority", "name": "Fast", "description": "Quicker, costs more" },
        ],
        "inputModalities": ["text", "image"],
        "upgrade": "m2",
    })))
    .unwrap();
    assert_eq!(model.input_modalities, ["text", "image"]);
    assert_eq!(model.upgrade.as_deref(), Some("m2"));
    let effort = &model.options[0];
    assert_eq!(effort.values[0].description.as_deref(), Some("Quick"));
    assert!(effort.values[0].default);
    assert_eq!(effort.values[1].description, None);
    let tiers = &model.options[1];
    assert_eq!(tiers.id, "service_tier");
    assert_eq!(tiers.values[0].id, "default");
    assert!(tiers.values[0].default);
    assert_eq!(tiers.values[1].id, "priority");
}

#[test]
fn a_catalog_default_tier_wins_and_old_speed_tiers_still_count() {
    let model = parse_model(&listed(json!({
        "serviceTiers": [{ "id": "flex", "name": "Flex", "description": "" }],
        "defaultServiceTier": "flex",
    })))
    .unwrap();
    assert!(model.options[1].values[1].default);
    let old = parse_model(&listed(json!({ "additionalSpeedTiers": ["fast"] }))).unwrap();
    assert_eq!(old.options[1].values[1].label, "Fast");
    let none = parse_model(&listed(json!({}))).unwrap();
    assert_eq!(none.options.len(), 1);
}

#[test]
fn the_configured_tier_becomes_the_default_where_it_is_offered() {
    let priced = json!({ "serviceTiers": [
        { "id": "priority", "name": "Fast", "description": "" },
    ]});
    let mut models = vec![
        parse_model(&listed(priced)).unwrap(),
        parse_model(&listed(json!({}))).unwrap(),
    ];
    apply_configured_tier(&mut models, "fast");
    let values = &models[0].options[1].values;
    assert!(!values[0].default);
    assert!(values[1].default, "config's fast is the catalog's priority");
    apply_configured_tier(&mut models, "unknown");
    assert!(models[0].options[1].values[1].default);
}

#[test]
fn standard_clears_the_tier_and_an_unread_default_leaves_it_alone() {
    let mut spec = turn_spec();
    assert_eq!(tier_param(&spec), None);
    spec.options.insert(SERVICE_TIER.into(), "default".into());
    assert_eq!(tier_param(&spec), Some(Value::Null));
    spec.options.insert(SERVICE_TIER.into(), "priority".into());
    assert_eq!(tier_param(&spec), Some(json!("priority")));
}

#[test]
fn real_login_status_is_recognized() {
    assert_eq!(
        parse_login(0, "Logged in using ChatGPT\n"),
        AuthState::LoggedIn
    );
    assert_eq!(parse_login(1, "Not logged in\n"), AuthState::LoggedOut);
}

#[test]
fn other_threads_cannot_finish_this_turn() {
    let mut mapper = mapper();
    let step = mapper.on_notification(
        "turn/completed",
        &json!({ "threadId": "subagent", "turn": { "status": "completed" } }),
    );
    assert!(step.outcome.is_none());
}

#[test]
fn completed_without_started_still_produces_a_whole_message() {
    let mut mapper = mapper();
    let step = mapper.on_notification(
        "item/completed",
        &json!({ "threadId": "thread", "item": { "type": "agentMessage", "id": "m", "text": "hi" } }),
    );
    // The raw item for the inspector, then start, content and end.
    assert_eq!(step.events.len(), 4);
    assert!(mapper.open.is_empty());
}

#[test]
fn completed_commands_keep_their_exit_code_and_raw_item() {
    let mut mapper = mapper();
    let step = mapper.on_notification(
        "item/completed",
        &json!({ "threadId": "thread", "item": {
            "type": "commandExecution", "id": "c", "command": "false",
            "status": "failed", "exitCode": 1, "aggregatedOutput": "boom",
        }}),
    );
    assert!(matches!(
        &step.events[0],
        EngineEvent::Item(item) if item.exit_code == Some(1) && item.kind == "commandExecution"
    ));
    assert!(step.events.iter().any(|event| matches!(
        event,
        EngineEvent::Agui(AgUiEvent::Custom { name, .. }) if name == "tool.finished"
    )));
}

#[test]
fn usage_reports_totals_and_the_window() {
    let mut mapper = mapper();
    let step = mapper.on_notification(
        "thread/tokenUsage/updated",
        &json!({ "threadId": "thread", "tokenUsage": {
            "total": { "totalTokens": 120 }, "modelContextWindow": 1000,
        }}),
    );
    assert!(step.events.iter().any(|event| matches!(
        event,
        EngineEvent::Usage(usage)
            if usage.total_tokens == Some(120) && usage.context_window == Some(1000)
    )));
}

#[test]
fn deltas_are_not_repeated_by_the_completed_item() {
    let mut mapper = mapper();
    let params = json!({ "threadId": "thread", "itemId": "m", "delta": "hi", "turnId": "t" });
    assert_eq!(
        mapper
            .on_notification("item/agentMessage/delta", &params)
            .events
            .len(),
        2
    );
    let done = mapper.on_notification(
        "item/completed",
        &json!({ "threadId": "thread", "item": { "type": "agentMessage", "id": "m", "text": "hi" } }),
    );
    // The raw item and the end, with no second copy of the text.
    assert_eq!(done.events.len(), 2);
}

#[test]
fn a_usage_limit_stop_carries_its_reset() {
    let mut mapper = mapper();
    mapper.on_notification(
        "account/rateLimits/updated",
        &json!({ "rateLimits": { "primary": { "usedPercent": 100, "resetsAt": 500 } } }),
    );
    let step = mapper.on_notification(
        "error",
        &json!({ "threadId": "thread", "willRetry": false, "error": { "message": "Out", "codexErrorInfo": "usageLimitExceeded" } }),
    );
    assert!(step.events.iter().any(|event| matches!(
        event,
        EngineEvent::Limited {
            resets_at: Some(500)
        }
    )));
}

#[test]
fn failed_turn_reports_the_last_error() {
    let mut mapper = mapper();
    mapper.on_notification(
        "error",
        &json!({ "threadId": "thread", "willRetry": false, "error": { "message": "quota" } }),
    );
    let step = mapper.on_notification(
        "turn/completed",
        &json!({ "threadId": "thread", "turn": { "status": "failed", "error": null } }),
    );
    assert_eq!(step.outcome, Some(EngineOutcome::Failed("quota".into())));
}

#[test]
fn approval_replies_match_each_schema() {
    let allow = Verdict::Allow;
    let deny = Verdict::Deny("no".into());
    let params = json!({ "permissions": { "network": { "enabled": true } } });
    assert_eq!(
        approval_response("item/commandExecution/requestApproval", &params, &allow),
        json!({ "decision": "accept" })
    );
    assert_eq!(
        approval_response("item/fileChange/requestApproval", &params, &deny),
        json!({ "decision": "decline" })
    );
    assert_eq!(
        approval_response("execCommandApproval", &params, &allow),
        json!({ "decision": "approved" })
    );
    assert_eq!(
        approval_response("applyPatchApproval", &params, &deny),
        json!({ "decision": "denied" })
    );
    assert_eq!(
        approval_response("item/permissions/requestApproval", &params, &allow),
        json!({ "permissions": { "network": { "enabled": true } }, "scope": "turn" })
    );
    assert_eq!(
        approval_response("item/permissions/requestApproval", &params, &deny),
        json!({ "permissions": {}, "scope": "turn" })
    );
}

fn turn_spec() -> RunSpec {
    RunSpec {
        run_id: "r".into(),
        thread_id: "t".into(),
        cwd: "/workspace".into(),
        checkout: "/workspace/repo/app".into(),
        library: "/workspace".into(),
        prompt: "p".into(),
        user_prompt: "p".into(),
        context: "<orbyn_context>c</orbyn_context>".into(),
        kind: RunKind::Turn,
        mode: PermissionMode::Supervised,
        plan: false,
        model: None,
        effort: None,
        options: std::collections::BTreeMap::new(),
        resume: None,
        mcp_url: Some("http://127.0.0.1:1/mcp/x".into()),
        run_dir: "/run".into(),
        confine: None,
    }
}

#[test]
fn mcp_server_name_cannot_merge_with_a_user_orbyn_entry() {
    let config = thread_config(&turn_spec());
    assert!(config["mcp_servers.orbyn"].is_null());
    assert_eq!(
        config["mcp_servers.orbyn_engineering"]["url"],
        "http://127.0.0.1:1/mcp/x"
    );
    // A dotted key, never a nested table that would replace the user's own servers.
    assert!(config.get("mcp_servers").is_none(), "{config}");
    assert_eq!(config["mcp_optional_startup_grace_ms"], 3_000);
}

#[test]
fn without_a_url_only_the_startup_grace_is_set() {
    let mut spec = turn_spec();
    spec.mcp_url = None;
    assert_eq!(
        thread_config(&spec),
        json!({ "mcp_optional_startup_grace_ms": 3_000 })
    );
}

#[test]
fn collaboration_uses_effective_settings_without_inventing_defaults() {
    let mut spec = turn_spec();
    spec.plan = true;
    let thread = json!({ "model": "returned-model", "reasoningEffort": "medium" });
    let mode = collaboration_mode(&spec, &thread).unwrap();
    assert_eq!(
        mode,
        json!({ "mode": "plan", "settings": {
            "model": "returned-model", "reasoning_effort": "medium", "developer_instructions": null,
        }})
    );
    assert!(collaboration_mode(&spec, &json!({})).is_err());
    assert!(collaboration_mode(&spec, &json!({ "model": "" })).is_err());
    spec.model = Some("selected-model".into());
    spec.effort = Some("high".into());
    spec.plan = false;
    let mode = collaboration_mode(&spec, &thread).unwrap();
    assert_eq!(mode["mode"], "default");
    assert_eq!(mode["settings"]["model"], "selected-model");
    assert_eq!(mode["settings"]["reasoning_effort"], "high");
    spec.effort = None;
    assert!(
        collaboration_mode(&spec, &json!({})).unwrap()["settings"]["reasoning_effort"].is_null()
    );
}

#[test]
fn turns_write_only_to_the_selected_checkout() {
    let checkout = std::path::Path::new("/w/repo/app");
    let policy = sandbox_policy("workspace-write", checkout, PermissionMode::Supervised);
    assert_eq!(policy["type"], "workspaceWrite");
    assert_eq!(policy["writableRoots"], json!(["/w/repo/app"]));
    assert_eq!(policy["networkAccess"], false);
    assert_eq!(
        sandbox_policy("read-only", checkout, PermissionMode::FullAccess)["type"],
        "readOnly"
    );
    let full = sandbox_policy("workspace-write", checkout, PermissionMode::FullAccess);
    assert_eq!(
        full["writableRoots"],
        json!(["/w/repo/app"]),
        "full access keeps the scope"
    );
    assert_eq!(full["networkAccess"], true);
    assert_eq!(approval_policy(PermissionMode::FullAccess), "never");
    assert_eq!(approval_policy(PermissionMode::Auto), "on-request");
}

fn elicitation(message: &str) -> Value {
    json!({
        "serverName": "codex_apps", "threadId": "t", "mode": "form",
        "_meta": { "codex_approval_kind": "mcp_tool_call" },
        "message": message,
        "requestedSchema": { "type": "object", "properties": {} },
    })
}

#[test]
fn a_connector_tool_approval_goes_through_our_approval_flow() {
    let params = elicitation("Allow Gmail to run tool \"send_email\"?");
    let request = classify_request("mcpServer/elicitation/request", &params, &mapper()).unwrap();
    assert_eq!(request.target, "codex_apps/send_email");
    assert!(request.summary.contains("send_email"));
    let allow = Verdict::Allow;
    let deny = Verdict::Deny("no".into());
    assert_eq!(
        approval_response("mcpServer/elicitation/request", &params, &allow),
        json!({ "action": "accept", "content": null })
    );
    assert_eq!(
        approval_response("mcpServer/elicitation/request", &params, &deny)["action"],
        "decline"
    );
}

#[test]
fn remembering_one_tool_does_not_cover_another_tool_or_server() {
    let target = |params: Value| {
        classify_request("mcpServer/elicitation/request", &params, &mapper())
            .unwrap()
            .target
    };
    let send = target(elicitation("Allow Gmail to run tool \"send_email\"?"));
    let read = target(elicitation("Allow Gmail to run tool \"read_email\"?"));
    assert_ne!(send, read);
    let mut other_server = elicitation("Allow Gmail to run tool \"send_email\"?");
    other_server["serverName"] = json!("github");
    assert_ne!(target(other_server), send);
    // The meta names the tool when the message does not.
    let mut meta = elicitation("Allow this?");
    meta["_meta"]["tool_name"] = json!("create_issue");
    assert_eq!(target(meta), "codex_apps/create_issue");
    // A request that names no tool is keyed on its whole message, not on the server.
    assert_eq!(target(elicitation("Allow this?")), "codex_apps/Allow this?");
}

#[test]
fn a_url_request_is_never_answered_as_a_tool_approval() {
    let url = json!({
        "serverName": "x", "mode": "url", "message": "Sign in", "url": "https://evil.example/p",
        "_meta": { "codex_approval_kind": "mcp_tool_call" },
    });
    assert!(!is_tool_approval(&url));
    assert!(classify_request("mcpServer/elicitation/request", &url, &mapper()).is_none());
    // No schema and no approval marker is not a plain yes/no either.
    assert!(!is_tool_approval(
        &json!({ "serverName": "x", "message": "m" })
    ));
}

#[test]
fn required_fields_are_filled_from_defaults_or_the_request_is_declined() {
    let with_default = json!({
        "requestedSchema": { "type": "object", "required": ["scope"],
            "properties": { "scope": { "type": "string", "default": "once" } } },
    });
    assert_eq!(
        approval_content(&with_default),
        Some(json!({ "scope": "once" }))
    );
    let without = json!({
        "requestedSchema": { "type": "object", "required": ["reason"],
            "properties": { "reason": { "type": "string" } } },
    });
    assert_eq!(approval_content(&without), None);
    // An optional field with no default is left out.
    let optional = json!({
        "requestedSchema": { "type": "object", "properties": { "note": { "type": "string" } } },
    });
    assert_eq!(approval_content(&optional), Some(json!({})));
    let named = json!({ "_meta": { "codex_approval_kind": "mcp_tool_call" }, "requestedSchema": without["requestedSchema"] });
    assert_eq!(
        approval_response("mcpServer/elicitation/request", &named, &Verdict::Allow)["action"],
        "decline"
    );
}

#[test]
fn a_form_that_asks_the_person_for_data_is_not_an_approval() {
    let form = json!({
        "serverName": "x", "message": "Your email?", "mode": "form",
        "requestedSchema": { "type": "object", "properties": { "email": { "type": "string" } } },
    });
    assert!(!is_tool_approval(&form));
    assert!(classify_request("mcpServer/elicitation/request", &form, &mapper()).is_none());
}

#[test]
fn codex_tools_reduce_three_big_lists_to_names_and_states() {
    use orbyn_contracts::ToolState;
    let servers = json!({ "data": [
        { "name": "orbyn_engineering", "tools": { "a": {}, "b": {} }, "authStatus": "unsupported" },
        { "name": "codex_apps", "tools": { "x": {} }, "authStatus": "bearerToken" },
        { "name": "filesystem", "tools": { "read": {}, "write": {} }, "authStatus": "unsupported" },
        { "name": "computer-use", "tools": {}, "toolsError": "MCP startup failed: No such file", "authStatus": "unsupported" },
        { "name": "notion", "tools": {}, "toolsError": "needs login", "authStatus": "notLoggedIn" },
    ] });
    let apps = json!({ "data": [
        { "name": "GitHub", "description": "Repos and issues. More.", "isAccessible": true, "isEnabled": true },
        { "name": "Gmail", "isAccessible": false, "isEnabled": true },
        { "name": "Off", "isAccessible": true, "isEnabled": false },
    ] });
    let plugins = json!({ "marketplaces": [ { "plugins": [
        { "name": "documents", "installed": true, "enabled": true, "interface": { "displayName": "Documents", "shortDescription": "Edit documents" } },
        { "name": "other", "installed": false, "enabled": false },
    ] } ] });
    let tools = codex_tools(Some(&servers), Some(&apps), Some(&plugins));
    let find = |id: &str| tools.groups.iter().find(|one| one.id == id).unwrap();
    assert_eq!(find("orbyn").count, 2);
    let mcp = find("mcp");
    let names: Vec<_> = mcp.items.iter().map(|one| one.name.as_str()).collect();
    assert_eq!(names, ["filesystem", "computer-use", "notion"]);
    assert_eq!(mcp.items[0].state, ToolState::Ready);
    assert_eq!(mcp.items[0].tools, Some(2));
    assert_eq!(mcp.items[1].state, ToolState::Failed);
    assert_eq!(mcp.items[2].state, ToolState::NeedsSignIn);
    let connectors = find("connectors");
    assert_eq!(
        connectors.items.len(),
        1,
        "only the apps the person can reach"
    );
    assert_eq!(
        connectors.items[0].description.as_deref(),
        Some("Repos and issues")
    );
    assert_eq!(find("plugins").items[0].name, "Documents");
    // Codex reports neither built-in tools nor skills here.
    assert_eq!(tools.not_reported, ["builtin", "skills"]);
}

#[test]
fn a_list_codex_did_not_answer_leaves_its_group_out() {
    let tools = codex_tools(None, Some(&json!({ "data": [] })), None);
    let ids: Vec<_> = tools.groups.iter().map(|one| one.id.as_str()).collect();
    assert_eq!(ids, ["connectors"]);
    assert!(tools.not_reported.contains(&"mcp".to_owned()));
}

#[test]
fn unknown_requests_are_not_classified() {
    let mapper = mapper();
    for method in [
        "account/chatgptAuthTokens/refresh",
        "item/tool/call",
        "attestation/generate",
        "anything/new",
    ] {
        assert!(
            classify_request(method, &json!({}), &mapper).is_none(),
            "{method}"
        );
    }
}

#[test]
fn plan_steps_use_the_activity_shape() {
    let content = plan_content(
        &json!({ "plan": [{ "step": "a", "status": "inProgress" }], "explanation": null }),
    );
    assert_eq!(
        content["steps"],
        json!([{ "title": "a", "status": "in_progress" }])
    );
}
