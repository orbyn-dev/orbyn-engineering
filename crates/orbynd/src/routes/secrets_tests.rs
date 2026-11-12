//! Settings › Secrets routes: the remote refusal on every write, the credential, and that the route table and the core's
//! change marker (`redaction::changes`) agree, so an agent holding the token is refused whichever route it names.

use std::collections::BTreeSet;
use std::sync::Arc;

use orbyn_core::redaction::changes::{READS, STATE_CHANGING, changes_redaction_state};
use orbyn_core::{Core, CoreConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::router;
use super::tests::registered;
use crate::http::AppState;
use crate::http::access::Access;

struct Served {
    port: u16,
    routes: BTreeSet<(String, String)>,
    dir: std::path::PathBuf,
}

async fn serve(name: &str, remote: bool) -> Served {
    let dir = std::env::temp_dir().join(format!("orbynd-secrets-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let core =
        Arc::new(Core::open(CoreConfig::new(dir.clone(), "http://127.0.0.1:1".into())).unwrap());
    let app = router(AppState {
        core,
        access: Arc::new(Access::new("token", vec![], None)),
        remote,
        ui_dir: None,
        images: Arc::new(orbyn_core::browser::remote_images::ImageProxy::default()),
    });
    let routes = registered(&app);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await });
    Served { port, routes, dir }
}

impl Served {
    async fn call(&self, method: &str, path: &str, token: Option<&str>) -> (u16, Value) {
        let url = format!("http://127.0.0.1:{}{path}", self.port);
        let mut request = reqwest::Client::new()
            .request(method.parse().unwrap(), url)
            .json(&json!({}));
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        (
            status,
            serde_json::from_str(&response.text().await.unwrap()).unwrap_or(Value::Null),
        )
    }

    fn ours(&self) -> Vec<(String, String)> {
        self.routes
            .iter()
            .filter(|(_, path)| path.starts_with("/api/settings/secrets"))
            .cloned()
            .collect()
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn concrete(path: &str) -> String {
    path.replace("{id}", "x")
}

#[tokio::test]
async fn a_remote_viewer_cannot_change_any_redaction_setting() {
    let app = serve("remote", true).await;
    for (method, path) in STATE_CHANGING {
        let (status, body) = app.call(method, &concrete(path), Some("token")).await;
        assert_eq!(
            (status, body["code"].as_str()),
            (403, Some("local_only")),
            "{method} {path}"
        );
    }
    let (status, _) = app
        .call("GET", "/api/settings/secrets", Some("token"))
        .await;
    assert_eq!(status, 200, "a remote viewer reads");
}

#[tokio::test]
async fn every_route_needs_the_credential() {
    let app = serve("token", false).await;
    for (method, path) in app.ours() {
        let (status, _) = app.call(&method, &concrete(&path), None).await;
        assert_eq!(status, 401, "{method} {path}");
    }
}

#[tokio::test]
async fn the_routes_and_the_core_marker_list_the_same_routes() {
    let app = serve("table", false).await;
    let served: BTreeSet<String> = app.ours().iter().map(|(m, p)| format!("{m} {p}")).collect();
    let listed: BTreeSet<String> = STATE_CHANGING
        .iter()
        .chain(READS)
        .map(|(m, p)| format!("{m} {p}"))
        .collect();
    assert_eq!(
        served, listed,
        "a route under /api/settings/secrets is in neither list or in both"
    );
    for (_, path) in STATE_CHANGING {
        assert!(changes_redaction_state(&concrete(path)), "{path}");
    }
}
