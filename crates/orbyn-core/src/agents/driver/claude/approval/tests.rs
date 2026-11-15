use super::*;
use crate::agents::driver::EngineDriver;
use std::time::{Duration, Instant};

fn spec(prompt: &str) -> ApprovalSpec {
    ApprovalSpec {
        run_id: "r".into(),
        thread_id: "t".into(),
        prompt: prompt.into(),
        family: "claude".into(),
        model: "claude-opus-5-5".into(),
        effort: Some("medium".into()),
        options: std::collections::BTreeMap::from([(FAST_MODE.into(), "off".into())]),
        engine_version: Some("2.1.325".into()),
        deadline: Instant::now() + Duration::from_secs(5),
        max_output_tokens: 1024,
        max_reported_tokens: 16384,
        estimated_spend_stop_usd: 0.05,
    }
}

fn fake(dir: &Path) -> ClaudeDriver {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/agents/driver/claude/fixtures");
    for name in ["approval.mjs", "approval.cmd"] {
        std::fs::copy(source.join(name), dir.join(name)).unwrap();
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            dir.join("approval.mjs"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    let path = dir.join(if cfg!(windows) {
        "approval.cmd"
    } else {
        "approval.mjs"
    });
    ClaudeDriver::new(path.to_string_lossy().into_owned(), Duration::from_secs(5))
}

#[test]
fn version_options_and_limits_are_checked_before_launch() {
    assert!(!version_supported(None));
    assert!(!version_supported(Some("2.1.150")));
    assert!(!version_supported(Some("not-a-version")));
    assert!(version_supported(Some("2.1.295 (Claude Code)")));
    let dir = ProbeDir::new().unwrap();
    let mut request = spec("summary");
    request.options.insert("unknown".into(), "on".into());
    assert!(args(&request, dir.path()).is_err());
    request.options.remove("unknown");
    request.estimated_spend_stop_usd = f64::NAN;
    assert!(args(&request, dir.path()).is_err());
}

#[tokio::test]
async fn synthetic_cli_receives_exact_controls_and_preserves_actual_usage_on_rejection() {
    let dir = crate::testing::TempDir::new();
    let driver = fake(dir.path());
    for prompt in ["summary", "over-budget", "malformed"] {
        let (send, mut receive) = mpsc::channel(16);
        let handle = driver.start_approval(spec(prompt), send).await.unwrap();
        let mut usage = None;
        let mut outcome = None;
        while let Some(event) = receive.recv().await {
            match event {
                EngineEvent::Usage(value) => usage = Some(value),
                EngineEvent::Finished(value) => {
                    outcome = Some(value);
                    break;
                }
                EngineEvent::Session(_) | EngineEvent::Permission(_) | EngineEvent::Question(_) => {
                    panic!("approval leaked a session or tool")
                }
                _ => {}
            }
        }
        handle.task.await.unwrap();
        if prompt == "summary" {
            assert_eq!(outcome, Some(EngineOutcome::Completed));
            assert_eq!(usage.unwrap().total_tokens, Some(20));
        } else {
            assert!(matches!(outcome, Some(EngineOutcome::Failed(_))));
            if prompt == "over-budget" {
                assert_eq!(usage.unwrap().raw["estimated_cost_usd"], 1.0);
            }
        }
    }
}

#[tokio::test]
async fn synthetic_cli_is_owned_through_cancellation_and_deadline() {
    let dir = crate::testing::TempDir::new();
    let driver = fake(dir.path());
    for cancel in [true, false] {
        let mut request = spec("hang");
        if !cancel {
            request.deadline = Instant::now() + Duration::from_millis(500);
        }
        let (send, mut receive) = mpsc::channel(8);
        let handle = driver.start_approval(request, send).await.unwrap();
        if cancel {
            handle.cancel.cancel();
        }
        tokio::time::timeout(Duration::from_secs(3), handle.task)
            .await
            .unwrap()
            .unwrap();
        let event = receive.recv().await.unwrap();
        assert!(if cancel {
            matches!(event, EngineEvent::Finished(EngineOutcome::Cancelled))
        } else {
            matches!(event, EngineEvent::Finished(EngineOutcome::Failed(_)))
        });
    }
}

#[test]
fn aliases_require_authoritative_effective_model_and_receipts_remain_exact() {
    assert_eq!(
        pinned_model("opus", Some("claude-opus-5-5")),
        Some("claude-opus-5-5".into())
    );
    for alias in ["opus", "sonnet", "default", "opusplan"] {
        assert!(pinned_model(alias, None).is_none());
        let mut request = spec("summary");
        request.model = alias.into();
        let dir = ProbeDir::new().unwrap();
        assert!(args(&request, dir.path()).is_err());
    }
    assert!(pinned_model("opus", Some("sonnet")).is_none());
    let mut request = spec("summary");
    request.model = pinned_model("opus", Some("claude-opus-5-5")).unwrap();
    let mut receipt = json!({"type":"result","subtype":"success","is_error":false,"num_turns":1,
        "result":"answer","modelUsage":{"claude-opus-5-5":{}},"total_cost_usd":0.001,
        "usage":{"input_tokens":10,"output_tokens":10,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}});
    assert!(
        parse(&serde_json::to_vec(&receipt).unwrap(), &request)
            .unwrap()
            .2
    );
    receipt["modelUsage"] = json!({"claude-opus-5-5-wrong":{}});
    assert!(
        !parse(&serde_json::to_vec(&receipt).unwrap(), &request)
            .unwrap()
            .2
    );
}

#[test]
fn native_init_retains_only_bounded_effective_model_metadata() {
    use crate::agents::driver::claude::events::StreamMapper;
    for model in ["claude-opus-5-5", "opus", &"x".repeat(1000)] {
        let events = StreamMapper::default().on_frame(&json!({"type":"system","subtype":"init",
            "model":model,"apiKey":"must-not-retain","cwd":"private-path"}));
        let value = events
            .iter()
            .find_map(|event| match event {
                EngineEvent::Agui(AgUiEvent::Custom { name, value }) if name == "engine.model" => {
                    Some(value)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(
            value,
            &json!({"engine":"claude", "model":pinned_model(model, None)})
        );
    }
}

#[test]
fn nested_init_cannot_report_the_leads_effective_model() {
    use crate::agents::driver::claude::events::StreamMapper;
    let events = StreamMapper::default().on_frame(&json!({"type":"system","subtype":"init",
        "parent_tool_use_id":"child-tool","model":"claude-opus-5-5"}));
    assert!(!events.iter().any(|event| matches!(event,
        EngineEvent::Agui(AgUiEvent::Custom { name, .. }) if name == "engine.model")));
}
