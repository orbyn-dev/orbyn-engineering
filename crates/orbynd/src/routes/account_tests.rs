//! The account routes: the gate, the remote refusal, and the re-entry guard. The guard
//! test reads the real route table, so a new state-changing account route that is
//! neither listed in `account::OPEN` with its reason nor refused without the password
//! fails here.

use std::sync::Arc;

use orbyn_core::account::{AccountBackendKind, AccountGate};
use orbyn_core::{Core, CoreConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::account::OPEN;
use super::router;
use super::tests::registered;
use crate::http::AppState;
use crate::http::access::Access;

const PASSWORD: &str = "correct horse battery";

struct Served {
    port: u16,
    core: Arc<Core>,
    routes: std::collections::BTreeSet<(String, String)>,
    _dir: std::path::PathBuf,
}

async fn serve(name: &str, gate: AccountGate, remote: bool) -> Served {
    let dir = std::env::temp_dir().join(format!("orbynd-account-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut config = CoreConfig::new(dir.clone(), "http://127.0.0.1:1".into());
    config.account_gate = gate;
    config.account_backend = AccountBackendKind::Development;
    let core = Arc::new(Core::open(config).unwrap());
    let app = router(AppState {
        core: core.clone(),
        access: Arc::new(Access::new("token", vec![], None)),
        remote,
        ui_dir: None,
        images: Arc::new(orbyn_core::browser::remote_images::ImageProxy::default()),
    });
    let routes = registered(&app);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await });
    Served {
        port,
        core,
        routes,
        _dir: dir,
    }
}

impl Served {
    async fn call(&self, method: &str, path: &str, body: Value) -> (u16, Value) {
        let url = format!("http://127.0.0.1:{}{path}", self.port);
        let response = reqwest::Client::new()
            .request(method.parse().unwrap(), url)
            .bearer_auth("token")
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    fn account_routes(&self) -> Vec<(String, String)> {
        self.routes
            .iter()
            .filter(|(_, path)| path.starts_with("/api/account"))
            .cloned()
            .collect()
    }
}

/// A body every account request type accepts, so a route is refused for its gate or its
/// guard and never for a missing field.
fn any_body() -> Value {
    json!({
        "invite": "K7QM-2XWH-9QXA-3C2D",
        "email": "dev@example.com",
        "code": "ABCD-EFGH",
        "password": "x",
        "discard_unsynced": false,
    })
}

fn concrete(path: &str) -> String {
    path.replace("{id}", "dev-mini")
}

#[tokio::test]
async fn the_off_gate_answers_account_disabled_on_every_account_route() {
    let served = serve("off", AccountGate::Off, false).await;
    assert!(served.account_routes().len() >= 14);
    for (method, path) in served.account_routes() {
        let (status, body) = served.call(&method, &concrete(&path), any_body()).await;
        if (method.as_str(), path.as_str()) == ("GET", "/api/account") {
            assert_eq!(status, 200);
            assert_eq!(body["available"], false, "nothing of the account shows");
            continue;
        }
        assert_eq!(
            (status, body["code"].as_str()),
            (403, Some("account_disabled")),
            "{method} {path}"
        );
    }
}

#[tokio::test]
async fn a_remote_viewer_cannot_reach_any_account_route() {
    let served = serve("remote", AccountGate::LocalOnly, true).await;
    for (method, path) in served.account_routes() {
        let (status, body) = served.call(&method, &concrete(&path), any_body()).await;
        assert_eq!(
            (status, body["code"].as_str()),
            (403, Some("local_only")),
            "{method} {path}"
        );
    }
}

#[tokio::test]
async fn every_state_changing_account_route_needs_the_password_again_or_is_open_for_a_reason() {
    let served = serve("reauth", AccountGate::LocalOnly, false).await;
    let (status, _) = served
        .call(
            "POST",
            "/api/account/sign-in",
            json!({ "email": "dev@example.com", "password": PASSWORD }),
        )
        .await;
    assert_eq!(status, 200);
    let open: Vec<&str> = OPEN.iter().map(|(route, _)| *route).collect();
    assert!(OPEN.iter().all(|(_, why)| !why.is_empty()));
    let mut guarded = Vec::new();
    let mut seen_open = Vec::new();
    for (method, path) in served.account_routes() {
        if method == "GET" {
            continue;
        }
        let route = format!("{method} {path}");
        if open.contains(&route.as_str()) {
            seen_open.push(route);
            continue;
        }
        // Signing in just confirmed the password; end that window as ten minutes would.
        served.core.account.forget_confirmation();
        let (status, body) = served.call(&method, &concrete(&path), any_body()).await;
        assert_eq!(
            (status, body["code"].as_str()),
            (403, Some("reauth_required")),
            "{route} changes the account and must need the password again, or be listed in OPEN with a reason"
        );
        guarded.push(route);
    }
    assert!(!guarded.is_empty());
    // The list holds no route that no longer exists.
    for route in &open {
        assert!(
            seen_open.iter().any(|seen| seen == route),
            "{route} is not a route"
        );
    }
    // After "Confirm it's you" the guarded actions run.
    let (status, body) = served
        .call(
            "POST",
            "/api/account/confirm",
            json!({ "password": "wrong" }),
        )
        .await;
    assert_eq!(
        (status, body["code"].as_str()),
        (403, Some("wrong_password"))
    );
    assert!(
        !body.to_string().contains("wrong\""),
        "the password is never echoed"
    );
    let (status, _) = served
        .call(
            "POST",
            "/api/account/confirm",
            json!({ "password": PASSWORD }),
        )
        .await;
    assert_eq!(status, 204);
    let (status, _) = served
        .call("DELETE", "/api/account/devices/dev-mini", json!({}))
        .await;
    assert_eq!(status, 204);
    let (status, _) = served
        .call("POST", "/api/account/sign-out-everywhere", json!({}))
        .await;
    assert_eq!(status, 204);
    let (_, view) = served.call("GET", "/api/account", json!({})).await;
    assert!(view["session"].is_null());
}

#[tokio::test]
async fn the_view_names_the_platform_and_never_a_secret() {
    let served = serve("view", AccountGate::LocalOnly, false).await;
    let (_, view) = served.call("GET", "/api/account", json!({})).await;
    assert_eq!(view["available"], true);
    assert_eq!(view["invite_required"], true);
    assert_eq!(view["platform"], std::env::consts::OS);
    let (status, body) = served
        .call(
            "POST",
            "/api/account/sign-in",
            json!({ "email": "dev@example.com", "password": "wrong-one" }),
        )
        .await;
    assert_eq!(
        (status, body["code"].as_str()),
        (403, Some("wrong_password"))
    );
    assert!(!body.to_string().contains("wrong-one"));
}
