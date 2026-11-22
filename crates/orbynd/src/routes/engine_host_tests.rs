//! The person's stop and restart of an engine host: person routes only, an audit row for each, the
//! fixed refusals, and nothing the host said that could carry a key. The host is the core's own
//! scripted stand-in behind a tiny launcher, so a real process starts and stops.

use std::sync::Arc;

use orbyn_contracts::{EngineHostCrash, EngineHostErrorKind, EngineHostState, EngineHostStatus};
use orbyn_core::agents::driver::host::{ControlError, HostConfig, Refusal, Timings};
use orbyn_core::agents::engines::host_status::HostControlFailure;
use orbyn_core::{Core, CoreConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::router;
use crate::http::AppState;
use crate::http::access::Access;

const KEY: &str = "sk-live-0123456789abcdef";

struct Served {
    port: u16,
    core: Arc<Core>,
    state: AppState,
    _dir: std::path::PathBuf,
}

fn node() -> Option<String> {
    let found = std::process::Command::new("sh")
        .args(["-c", "command -v node"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .filter(|path| !path.is_empty());
    if found.is_none() {
        eprintln!("SKIPPED: node was not found, so no engine host process can run here");
    }
    found
}

/// A service whose Claude host is the stand-in, started through a launcher script that ignores the
/// family argument the core passes.
async fn serve(name: &str, flag_on: bool) -> Option<Served> {
    let node = node()?;
    let dir = std::env::temp_dir().join(format!("orbynd-enghost-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let standin = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../orbyn-core/src/agents/driver/host/tests/standin.mjs");
    let launcher = dir.join("host.sh");
    std::fs::write(
        &launcher,
        format!(
            "#!/bin/sh\nSTANDIN_FAMILY=claude exec \"{node}\" \"{}\"\n",
            standin.display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut config = CoreConfig::new(dir.clone(), "http://127.0.0.1:1".into());
    config.engines.engine_host = Some(HostConfig {
        program: launcher,
        claude: flag_on,
        cursor: false,
        acp: false,
        timings: Timings::default(),
    });
    let core = Arc::new(Core::open(config).unwrap());
    core.engines.bind_host_core(&core);
    let state = AppState {
        core: core.clone(),
        access: Arc::new(Access::new("token", vec![], None).with_person_key(Some("the-key"))),
        remote: false,
        ui_dir: None,
        images: Arc::new(orbyn_core::browser::remote_images::ImageProxy::default()),
    };
    let app = router(state.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await });
    Some(Served {
        port,
        core,
        state,
        _dir: dir,
    })
}

impl Served {
    async fn call(&self, method: &str, path: &str, body: Value, person: bool) -> (u16, Value) {
        let url = format!("http://127.0.0.1:{}{path}", self.port);
        let mut request = reqwest::Client::new()
            .request(method.parse().unwrap(), url)
            .bearer_auth("token")
            .json(&body);
        if person {
            request = request.header("x-orbyn-person", "the-key");
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    async fn state_of_claude(&self) -> Value {
        let (_, list) = self
            .call("GET", "/api/engine-host", json!(null), false)
            .await;
        list.as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["family"] == "claude")
            .cloned()
            .expect("claude is in the list")
    }
}

#[tokio::test]
async fn stop_and_restart_are_person_routes() {
    let Some(app) = serve("person", true).await else {
        return;
    };
    for action in ["stop", "restart"] {
        let path = format!("/api/engine-host/claude/{action}");
        let (status, reply) = app.call("POST", &path, json!({}), false).await;
        assert_eq!(
            (status, &reply["code"]),
            (403, &json!("person_required")),
            "{action}"
        );
    }
    // Nothing happened.
    assert_eq!(app.state_of_claude().await["state"], "not_started");
    assert!(app.core.store.list_audit(None, 50).unwrap().is_empty());
}

#[tokio::test]
async fn a_restart_starts_the_host_a_stop_ends_it_and_each_leaves_one_audit_row() {
    let Some(app) = serve("flow", true).await else {
        return;
    };
    let (status, started) = app
        .call("POST", "/api/engine-host/claude/restart", json!({}), true)
        .await;
    assert_eq!(status, 200, "{started}");
    assert_eq!(started["state"], "running");
    assert_eq!(started["host_version"], "9.9.9");
    assert_eq!(app.state_of_claude().await["state"], "running");

    let (status, stopped) = app
        .call(
            "POST",
            "/api/engine-host/claude/stop",
            json!({ "stop_background": false }),
            true,
        )
        .await;
    assert_eq!(status, 200, "{stopped}");
    assert_eq!(stopped["state"], "stopped");
    assert_eq!(app.state_of_claude().await["state"], "stopped");

    let rows = app.core.store.list_audit(None, 50).unwrap();
    let mut actions: Vec<_> = rows.iter().map(|row| row.action.as_str()).collect();
    actions.sort_unstable();
    assert_eq!(actions, ["engine_host.restart", "engine_host.stop"]);
    for row in &rows {
        assert_eq!(
            (row.target.as_str(), row.source.as_str()),
            ("claude", "user")
        );
        assert_eq!(row.detail, json!({ "stop_background": false }));
    }
}

#[tokio::test]
async fn an_unknown_family_and_a_family_off_the_host_are_refused_in_fixed_words() {
    let Some(app) = serve("refused", false).await else {
        return;
    };
    let (status, reply) = app
        .call("POST", "/api/engine-host/nope/stop", json!({}), true)
        .await;
    assert_eq!((status, &reply["code"]), (404, &json!("not_found")));
    let (status, reply) = app
        .call("POST", "/api/engine-host/claude/restart", json!({}), true)
        .await;
    assert_eq!((status, &reply["code"]), (409, &json!("not_used")));
    // The attempt is audited before it is tried, so a refusal leaves its row too.
    let rows = app.core.store.list_audit(None, 50).unwrap();
    assert!(
        rows.iter()
            .all(|row| row.action.starts_with("engine_host.")),
        "{rows:?}"
    );
}

async fn failure_of(failure: HostControlFailure) -> (u16, Value) {
    use axum::response::IntoResponse;
    let response = super::engines::host_refusal(failure).into_response();
    let status = response.status().as_u16();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 16)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn a_host_with_background_work_open_is_refused_with_the_count_and_a_way_forward() {
    let (status, reply) = failure_of(HostControlFailure::Control(ControlError::Refused(
        Refusal::BackgroundOpen(2),
    )))
    .await;
    assert_eq!((status, &reply["code"]), (409, &json!("background_open")));
    let message = reply["message"].as_str().unwrap();
    assert!(
        message.starts_with("2 background tasks are still running"),
        "{message}"
    );
    let (_, one) = failure_of(HostControlFailure::Control(ControlError::Refused(
        Refusal::BackgroundOpen(1),
    )))
    .await;
    assert!(
        one["message"]
            .as_str()
            .unwrap()
            .starts_with("1 background task is still running")
    );
    let (status, reply) = failure_of(HostControlFailure::Control(ControlError::Refused(
        Refusal::VersionMismatch,
    )))
    .await;
    assert_eq!((status, &reply["code"]), (409, &json!("version_mismatch")));
}

#[tokio::test]
async fn nothing_the_host_said_leaves_a_key_in_a_stop_or_restart_answer() {
    let Some(app) = serve("redact", true).await else {
        return;
    };
    let leaky = EngineHostStatus {
        family: "claude".into(),
        state: EngineHostState::CantStart,
        host_version: None,
        restart_count: 5,
        last_crash: Some(EngineHostCrash {
            exit_code: Some(1),
            signal: None,
            at: "2026-11-22T00:00:00Z".into(),
            error_kind: EngineHostErrorKind::Exited,
            message: format!("exited with {KEY}"),
            stderr_tail: format!("sign-in failed for {KEY}"),
        }),
        protocol_app: None,
        protocol_host: None,
        not_used: None,
        detail: Some(format!("The host said {KEY}")),
        action: None,
        threads: None,
        last_error_kind: None,
        since: None,
        sdk: Some(orbyn_contracts::EngineHostSdk {
            name: format!("sdk {KEY}"),
            version: format!("version {KEY}"),
        }),
        capabilities: Some(vec![format!("capability {KEY}")]),
        pooled: false,
        restart: None,
        stops_in_window: None,
        has_driver: false,
        sdk_missing: false,
    };
    let clean = serde_json::to_string(&super::engines::redacted(&app.state, leaky)).unwrap();
    assert!(!clean.contains(KEY), "{clean}");
    assert!(clean.contains("[hidden: API key]"), "{clean}");
}
