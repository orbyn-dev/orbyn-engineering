//! Usage from local history and permission modes through the real service.

mod support;

use serde_json::{Value, json};
use support::{Options, TempDir, setup, text_of};

#[tokio::test]
async fn claude_subscription_windows_use_control_ipc_without_a_prompt_or_account_data() {
    let dir = TempDir::new();
    let controls = dir.path().join("controls");
    let launches = dir.path().join("launches");
    let service = support::Service::start(
        &dir.path().join("data"),
        Options {
            env: vec![
                ("ORBYN_FAKE_CLAUDE_USAGE".into(), "available".into()),
                (
                    "ORBYN_FAKE_CLAUDE_CONTROL_FILE".into(),
                    controls.to_string_lossy().into_owned(),
                ),
                (
                    "ORBYN_FAKE_LAUNCH_FILE".into(),
                    launches.to_string_lossy().into_owned(),
                ),
            ],
            ..Options::default()
        },
    );
    let response = service
        .get("/api/engines/claude/limits")
        .send()
        .await
        .unwrap();
    let code = response.status();
    let limits: Value = response.json().await.unwrap();
    assert_eq!(code, 200, "{limits}");
    assert_eq!(limits["plan"], "pro");
    assert_eq!(limits["windows"][0]["label"], "Session");
    assert_eq!(limits["windows"][0]["used_percent"], 54.0);
    assert_eq!(limits["windows"][0]["window_minutes"], 300);
    assert_eq!(limits["windows"][0]["resets_at"], "2027-01-15T08:00:00Z");
    assert_eq!(limits["windows"][1]["label"], "Weekly");
    assert!(limits["windows"][1]["resets_at"].is_null());
    assert_eq!(limits["windows"][2]["label"], "Weekly · Fable");
    assert_eq!(limits["windows"][3]["label"], "Monthly spend");
    assert_eq!(limits["windows"][3]["used_percent"], 25.0);
    assert!(!limits.to_string().contains("SECRET-MARKER"));
    assert!(!limits.to_string().contains("account"));
    assert!(!limits.to_string().contains("session"));
    let frames = std::fs::read_to_string(&controls).unwrap();
    let frames: Vec<Value> = frames
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(frames.len(), 2);
    assert!(
        frames
            .iter()
            .all(|frame| frame["type"] == "control_request")
    );
    assert_eq!(frames[0]["request"], json!({ "subtype": "initialize" }));
    assert_eq!(frames[1]["request"], json!({ "subtype": "get_usage" }));
    let log = std::fs::read_to_string(&launches).unwrap();
    let probe: Value = log
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|entry| entry["mode"] == "claude-probe")
        .unwrap();
    let cwd = std::path::Path::new(probe["cwd"].as_str().unwrap());
    assert!(
        !cwd.exists(),
        "the probe is reaped and its isolated cwd removed"
    );
    assert_ne!(cwd, dir.path());
    let again = service.json("/api/engines/claude/limits").await;
    for field in ["plan", "windows", "limits"] {
        assert_eq!(again[field], limits[field]);
    }
    assert!(!again.to_string().contains("SECRET-MARKER"));
    assert_eq!(
        std::fs::read_to_string(&controls).unwrap().lines().count(),
        4
    );
    let forced = service.json("/api/engines/claude/limits?force=true").await;
    assert!(!forced.to_string().contains("SECRET-MARKER"));
    assert_eq!(
        std::fs::read_to_string(&controls).unwrap().lines().count(),
        6
    );
    let overview = service.json("/api/limits").await;
    assert!(!overview.to_string().contains("\"claude\""), "{overview}");
    assert_eq!(
        std::fs::read_to_string(&controls).unwrap().lines().count(),
        6,
        "the overview must not probe Claude"
    );
    service.stop().await;
}

#[tokio::test]
async fn claude_limits_stay_unknown_for_unavailable_errors_malformed_and_timed_out_replies() {
    for (mode, expected) in [
        ("error", 502),
        ("unavailable", 422),
        ("malformed", 502),
        ("silent", 502),
    ] {
        let dir = TempDir::new();
        let controls = dir.path().join("controls");
        let service = support::Service::start(
            &dir.path().join("data"),
            Options {
                env: vec![
                    ("ORBYN_FAKE_CLAUDE_USAGE".into(), mode.into()),
                    (
                        "ORBYN_FAKE_CLAUDE_CONTROL_FILE".into(),
                        controls.to_string_lossy().into_owned(),
                    ),
                ],
                ..Options::default()
            },
        );
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(8),
            service.get("/api/engines/claude/limits").send(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(response.status(), expected, "{mode}");
        let body: Value = response.json().await.unwrap();
        assert!(
            !body.to_string().contains("SECRET-MARKER"),
            "{mode}: {body}"
        );
        assert!(body.get("windows").is_none(), "{mode}: {body}");
        if mode == "error" {
            let before = std::fs::read_to_string(&controls).unwrap();
            let overview = service.json("/api/limits").await;
            assert!(!overview.to_string().contains("\"claude\""), "{overview}");
            assert!(!overview.to_string().contains("SECRET-MARKER"));
            assert_eq!(
                std::fs::read_to_string(&controls).unwrap(),
                before,
                "the overview must not retry a Claude probe"
            );
        }
        service.stop().await;
    }
}

fn write(path: std::path::PathBuf, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[tokio::test]
async fn usage_comes_from_history_alone_and_never_from_credentials() {
    let dir = TempDir::new();
    let claude = dir.path().join("claude-home");
    let codex = dir.path().join("codex-home");
    write(
        claude.join("projects/p/s.jsonl"),
        r#"{"timestamp":"2026-10-30T01:00:00Z","message":{"id":"m","model":"opus","usage":{"input_tokens":10,"output_tokens":2}}}"#,
    );
    write(
        claude.join(".credentials.json"),
        r#"{"token":"SECRET-MARKER"}"#,
    );
    write(codex.join("auth.json"), r#"{"token":"SECRET-MARKER"}"#);
    let options = Options {
        env: vec![
            (
                "CLAUDE_CONFIG_DIR".into(),
                claude.to_string_lossy().into_owned(),
            ),
            ("CODEX_HOME".into(), codex.to_string_lossy().into_owned()),
        ],
        ..Options::default()
    };
    let (service, _) = setup(&dir, options, "codex").await;
    let report = service.json("/api/usage").await;
    assert!(!report.to_string().contains("SECRET-MARKER"));
    assert_eq!(
        report["host_id"],
        service.json("/api/usage").await["host_id"]
    );
    let engine = |name: &str| {
        report["engines"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["engine"] == name)
            .unwrap()
            .clone()
    };
    assert_eq!(engine("codex")["status"], "no_history");
    let usage = engine("claude");
    assert_eq!(usage["files_scanned"], 1);
    assert_eq!(usage["models"][0]["input_tokens"], 10);
    assert_eq!(usage["models"][0]["estimated_cost"], Value::Null);
    assert_eq!(usage["days"][0]["date"], "2026-10-30");

    // A new price shows on the next read, and a changed file is read again.
    let mut settings = service.json("/api/settings").await;
    settings["model_prices"] = json!([
        { "model": "opus", "input": 1000000.0, "output": 0.0, "cache_read": 0.0, "cache_write": 0.0 }
    ]);
    service
        .client
        .put(format!("{}/api/settings", service.url))
        .bearer_auth(&service.token)
        .json(&settings)
        .send()
        .await
        .unwrap();
    let priced = service.json("/api/usage").await;
    assert_eq!(priced["engines"][0]["models"][0]["estimated_cost"], 10.0);
}

/// One Claude history with a standard and a fast message, through the real service.
async fn history_service(dir: &TempDir) -> support::Service {
    let claude = dir.path().join("claude-home");
    write(
        claude.join("projects/p/s.jsonl"),
        &[
            r#"{"timestamp":"2026-10-30T01:00:00Z","message":{"id":"a","model":"opus","usage":{"input_tokens":1000000,"output_tokens":1000000,"cache_read_input_tokens":2000000,"speed":"standard","service_tier":"standard"}}}"#,
            r#"{"timestamp":"2026-10-31T01:00:00Z","message":{"id":"b","model":"opus","usage":{"input_tokens":1000000,"speed":"fast","service_tier":"standard"}}}"#,
            r#"{"timestamp":"2026-10-31T02:00:00Z","message":{"id":"c","model":"opus","usage":{"input_tokens":500000,"service_tier":"priority"}}}"#,
        ]
        .join("\n"),
    );
    let options = Options {
        env: vec![
            (
                "CLAUDE_CONFIG_DIR".into(),
                claude.to_string_lossy().into_owned(),
            ),
            (
                "CODEX_HOME".into(),
                dir.path().join("codex-home").to_string_lossy().into_owned(),
            ),
        ],
        ..Options::default()
    };
    setup(dir, options, "claude").await.0
}

async fn put_prices(service: &support::Service, prices: Value) {
    let mut settings = service.json("/api/settings").await;
    settings["model_prices"] = prices;
    let response = service
        .client
        .put(format!("{}/api/settings", service.url))
        .bearer_auth(&service.token)
        .json(&settings)
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success(), "{}", response.status());
}

fn claude_of(report: &Value) -> Value {
    report["engines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["engine"] == "claude")
        .unwrap()
        .clone()
}

fn engine_of(report: &Value, name: &str) -> Value {
    report["engines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["engine"] == name)
        .unwrap_or_else(|| panic!("no {name} row in {report}"))
        .clone()
}

/// A Muse `runtime.session` inference as its session log records it: microsecond
/// `recorded_at` and inclusive usage counters.
fn muse_inference(id: &str, inference: &str) -> String {
    format!(
        r#"{{"schema_version":1,"record_type":"event","payload_type":"runtime.session","payload_schema_version":1,"id":"{id}","recorded_at":1785578400123456,"stream":{{"kind":"session","id":"session-1"}},"payload":{{"source_run_record_id":"{inference}","event":{{"kind":"model_completed","model":"muse-wonder-full-1.0","usage":{{"input_tokens":100,"cache_read_tokens":60,"cache_write_tokens":10,"output_tokens":20,"reasoning_tokens":8}}}}}}}}"#
    )
}

#[tokio::test]
async fn muse_history_comes_from_its_session_logs_and_never_fabricates_spend() {
    let dir = TempDir::new();
    let data_home = dir.path().join("xdg-data");
    let session = data_home.join("muse/sessions/2026/08/01/session-1");
    write(
        session.join("session.jsonl"),
        &[
            muse_inference("envelope-1", "inference-1"),
            // The same inference copied into a log again must not double its spend.
            muse_inference("envelope-2", "inference-1"),
            "{not json".to_owned(),
            r#"{"schema_version":2,"record_type":"event","id":"x","recorded_at":1785578400123456}"#
                .to_owned(),
        ]
        .join("\n"),
    );
    write(
        session.join("telemetry.jsonl"),
        r#"{"input_tokens":999999}"#,
    );
    let options = Options {
        env: vec![(
            "XDG_DATA_HOME".into(),
            data_home.to_string_lossy().into_owned(),
        )],
        ..Options::default()
    };
    let service = support::Service::start(&dir.path().join("data"), options);
    let muse = engine_of(&service.json("/api/usage").await, "muse");
    assert_eq!(muse["status"], "scanned", "{muse}");
    assert_eq!(
        muse["files_scanned"], 1,
        "only session.jsonl is a session log"
    );
    let model = &muse["models"][0];
    assert_eq!(model["model"], "muse-wonder-full-1.0");
    assert_eq!(model["input_tokens"], 30, "cached input is moved out");
    assert_eq!(model["cache_read_tokens"], 60);
    assert_eq!(model["cache_write_tokens"], 10);
    assert_eq!(model["output_tokens"], 20);
    assert_eq!(
        muse["days"][0],
        json!({ "date": "2026-08-01", "tokens": 120, "estimated_cost": null }),
        "a copied inference counts once and reasoning is already in output"
    );
    service.stop().await;

    // No Muse home is no history, never zero spend.
    let dir = TempDir::new();
    let empty = dir.path().join("empty");
    let service = support::Service::start(
        &dir.path().join("data"),
        Options {
            env: vec![("XDG_DATA_HOME".into(), empty.to_string_lossy().into_owned())],
            ..Options::default()
        },
    );
    assert_eq!(
        engine_of(&service.json("/api/usage").await, "muse")["status"],
        "no_history"
    );
    service.stop().await;
}

#[tokio::test]
async fn cost_splits_by_token_type_and_tier_and_a_tier_never_borrows_the_standard_rate() {
    let dir = TempDir::new();
    let service = history_service(&dir).await;
    put_prices(
        &service,
        json!([
            { "model": "opus", "input": 1.0, "output": 10.0, "cache_read": 0.5, "cache_write": 0.0 },
            { "model": "opus", "input": 6.0, "output": 60.0, "cache_read": 3.0, "cache_write": 0.0, "tier": "fast" }
        ]),
    )
    .await;
    let model = claude_of(&service.json("/api/usage").await)["models"][0].clone();
    assert_eq!(model["model"], "opus");
    // Standard: 1 + 10 + 1; fast: 6. Priority has no rate of its own.
    assert_eq!(model["estimated_cost"], 18.0);
    assert_eq!(
        model["cost_split"],
        json!({ "input": 7.0, "output": 10.0, "cache_read": 1.0, "cache_write": 0.0 })
    );
    assert_eq!(model["unpriced_tokens"], 500_000);
    let tier = |name: &str| {
        model["tiers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tier| tier["tier"] == name)
            .unwrap()
            .clone()
    };
    assert_eq!(tier("standard")["tokens"], 4_000_000);
    assert_eq!(tier("fast")["estimated_cost"], 6.0);
    assert!(tier("priority")["estimated_cost"].is_null());
    assert_eq!(
        model["days"],
        json!([
            { "date": "2026-10-30", "tokens": 4_000_000, "estimated_cost": 12.0 },
            { "date": "2026-10-31", "tokens": 1_500_000, "estimated_cost": 6.0 }
        ])
    );
    let engine = claude_of(&service.json("/api/usage").await);
    assert_eq!(
        engine["days"], model["days"],
        "one model, so the engine's days match"
    );
    service.stop().await;
}

#[tokio::test]
async fn limits_come_from_the_engine_and_only_where_it_reports_them() {
    let dir = TempDir::new();
    let launches = dir.path().join("launches");
    let (service, _) = setup(
        &dir,
        Options {
            env: vec![(
                "ORBYN_FAKE_LAUNCH_FILE".into(),
                launches.to_string_lossy().into_owned(),
            )],
            ..Options::default()
        },
        "codex",
    )
    .await;
    let reads = || {
        std::fs::read_to_string(&launches)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .filter(|entry| entry["mode"] == "codex")
            .count()
    };
    let limits = service.json("/api/engines/codex/limits").await;
    assert_eq!(limits["limits"]["rateLimits"]["primary"]["usedPercent"], 12);
    assert!(
        !limits.to_string().contains("SECRET-MARKER"),
        "no account ID or upsell leaves the driver: {limits}"
    );
    let before = reads();
    let again = service.json("/api/engines/codex/limits").await;
    assert_eq!(again["limits"], limits["limits"]);
    assert_eq!(
        reads(),
        before + 1,
        "each explicit read asks a fresh fake app-server"
    );
    assert_eq!(limits["plan"], "plus");
    assert_eq!(limits["windows"][0]["label"], "5 hours");
    assert_eq!(limits["windows"][1]["label"], "7 days");
    assert_eq!(limits["windows"][0]["resets_at"], "2027-01-15T08:00:00Z");
    let forced = service.json("/api/engines/codex/limits?force=true").await;
    assert_eq!(forced["windows"], limits["windows"]);
    assert_eq!(reads(), before + 2);
    assert!(!forced.to_string().contains("SECRET-MARKER"));
    let overview = service.json("/api/limits").await;
    assert_eq!(overview["pooled"][0]["used_percent"], 12.0);
    assert_eq!(overview["pooled"][0]["engines"][0], "codex");
    let refreshed = service
        .post("/api/usage/refresh?days=7")
        .send()
        .await
        .unwrap();
    assert_eq!(refreshed.status(), 200);
    let claude = service
        .get("/api/engines/claude/limits")
        .send()
        .await
        .unwrap();
    assert_eq!(claude.status(), 422);
}

#[tokio::test]
async fn a_turn_can_set_the_mode_and_full_access_never_widens_codex_writes() {
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
    let (code, _) = service
        .send(
            &format!("/api/threads/{thread}/runs"),
            json!({ "prompt": "[approve]", "permission_mode": "full_access" }),
        )
        .await;
    assert_eq!(code, 202);
    assert_eq!(service.finished(&thread).await, "completed");
    assert_eq!(
        text_of(&service.all_events(&thread).await),
        "decision accept"
    );
    let current = service.json(&format!("/api/threads/{thread}")).await;
    assert_eq!(current["lifecycle"]["permission_mode"], "full_access");
    let log = std::fs::read_to_string(&file).unwrap();
    let turn = log
        .lines()
        .find(|l| l.contains(r#""mode":"codex-turn""#))
        .unwrap();
    let entry: Value = serde_json::from_str(turn).unwrap();
    let params: Value = serde_json::from_str(entry["args"][0].as_str().unwrap()).unwrap();
    assert_eq!(params["approvalPolicy"], "never");
    assert_eq!(params["sandboxPolicy"]["type"], "workspaceWrite");
    assert_eq!(params["sandboxPolicy"]["networkAccess"], true);
    let root = dir.path().join("library").join("repo").join("project");
    assert_eq!(params["sandboxPolicy"]["writableRoots"], json!([root]));
}

#[tokio::test]
async fn claude_stays_on_the_gate_in_every_mode() {
    let dir = TempDir::new();
    let file = dir.path().join("launches");
    let target = dir.path().join("library/AGENTS.md");
    let options = Options {
        env: vec![
            (
                "ORBYN_FAKE_LAUNCH_FILE".into(),
                file.to_string_lossy().into_owned(),
            ),
            (
                "ORBYN_FAKE_WRITE_PATH".into(),
                target.to_string_lossy().into_owned(),
            ),
        ],
        ..Options::default()
    };
    let (service, thread) = setup(&dir, options, "claude").await;
    service
        .patch(&thread, json!({ "permission_mode": "full_access" }))
        .await;
    service.admit(&thread, "[approve]").await;
    assert_eq!(service.finished(&thread).await, "completed");
    let log = std::fs::read_to_string(&file).unwrap();
    assert!(log.contains(r#""--permission-mode","default""#));
    assert!(log.contains("--permission-prompt-tool"));
    assert!(!log.contains("bypassPermissions"));
    assert!(!log.contains("acceptEdits"));
    assert_eq!(
        text_of(&service.all_events(&thread).await),
        "permission allow"
    );
}
