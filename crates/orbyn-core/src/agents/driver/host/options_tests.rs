use std::path::Path;

use serde_json::json;

use super::claude;
use crate::agents::driver::host::support::run_spec;

#[test]
fn claude_options_carry_model_effort_mode_mcp_and_context() {
    let spec = run_spec();
    let options = claude(&spec, Some(Path::new("/usr/local/bin/claude")));
    let sdk = &options["sdk"];
    assert_eq!(sdk["model"], "sonnet");
    assert_eq!(sdk["effort"], "high");
    assert_eq!(sdk["permissionMode"], "default");
    assert_eq!(
        sdk["mcpServers"]["orbyn_engineering"],
        json!({"type":"http","url":"http://127.0.0.1:1/mcp/x"})
    );
    assert_eq!(sdk["systemPrompt"]["append"], "context");
    assert_eq!(options["claudePath"], "/usr/local/bin/claude");
}

#[test]
fn plan_mode_is_the_sdks_plan_and_nothing_ever_skips_asking() {
    let mut spec = run_spec();
    spec.plan = true;
    assert_eq!(claude(&spec, None)["sdk"]["permissionMode"], "plan");
    for mode in orbyn_contracts::PermissionMode::ALL.iter().copied() {
        spec.mode = mode;
        spec.plan = false;
        assert_eq!(claude(&spec, None)["sdk"]["permissionMode"], "default");
    }
}

#[test]
fn absent_choices_are_left_out_and_extra_folders_are_the_library() {
    let mut spec = run_spec();
    spec.model = None;
    spec.effort = None;
    spec.mcp_url = None;
    spec.context = "  ".into();
    let options = claude(&spec, None);
    let sdk = options["sdk"].as_object().unwrap();
    for key in ["model", "effort", "mcpServers", "systemPrompt"] {
        assert!(!sdk.contains_key(key), "{key} should be absent");
    }
    assert_eq!(sdk["additionalDirectories"], json!(["/workspace"]));
    assert!(options.get("claudePath").is_none());
}
