//! A person's two writes about one area over HTTP: the Preview gate, the local-only rule, the credential, the shape of a request and
//! the answers (`stale` is 409). The rules themselves, the trail and the audit row are tested in `orbyn-core`
//! (`crew/tests/autopilot_enforce.rs`, `store/crew_autonomy_tests.rs`).

use std::path::PathBuf;
use std::sync::Arc;

use orbyn_core::crew::fixtures;
use orbyn_core::{Core, CoreConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::router;
use crate::http::AppState;
use crate::http::access::Access;

const OVERRIDE: &str = "/api/crew/autopilot/areas/override";
const FOLLOW: &str = "/api/crew/autopilot/areas/follow";

struct Served {
    port: u16,
    core: Arc<Core>,
    dir: PathBuf,
}

async fn serve(name: &str, preview: bool, remote: bool) -> Served {
    let dir = std::env::temp_dir().join(format!("orbynd-areas-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut config = CoreConfig::new(dir.clone(), "http://127.0.0.1:1".into());
    config.crew_preview = preview;
    config.crew_confine_workers = false;
    let core = Arc::new(Core::open(config).unwrap());
    let app = router(AppState {
        core: core.clone(),
        access: Arc::new(Access::new("token", vec![], None)),
        remote,
        ui_dir: None,
        images: Arc::new(orbyn_core::browser::remote_images::ImageProxy::default()),
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await });
    Served { port, core, dir }
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Served {
    async fn call(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        token: bool,
    ) -> (u16, Value) {
        let url = format!("http://127.0.0.1:{}{path}", self.port);
        let mut request = reqwest::Client::new().request(method.parse().unwrap(), url);
        if token {
            request = request.bearer_auth("token");
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    async fn post(&self, path: &str, body: Value) -> (u16, Value) {
        self.call("POST", path, Some(body), true).await
    }
}

fn choice(project: &str, choice: &str, acknowledged: bool, tripped_at: i64) -> Value {
    json!({
        "project": project,
        "area": "src/a",
        "choice": choice,
        "acknowledged": acknowledged,
        "seen": { "merges": 0, "reverts": 0, "tripped_at": tripped_at },
    })
}

#[tokio::test]
async fn with_the_flag_off_remote_or_without_the_token_nothing_is_done() {
    let off = serve("off", false, false).await;
    for path in [OVERRIDE, FOLLOW] {
        for method in ["POST", "GET", "PUT"] {
            let (status, body) = off.call(method, path, Some(json!({})), true).await;
            assert_eq!(
                (status, &body["code"]),
                (404, &json!("not_found")),
                "{method} {path}"
            );
        }
    }
    let remote = serve("remote", true, true).await;
    for path in [OVERRIDE, FOLLOW] {
        let (status, body) = remote.post(path, json!({})).await;
        assert_eq!(
            (status, &body["code"]),
            (403, &json!("local_only")),
            "{path}"
        );
    }
    let on = serve("token", true, false).await;
    for path in [OVERRIDE, FOLLOW] {
        let (status, _) = on.call("POST", path, Some(json!({})), false).await;
        assert_eq!(status, 401, "{path}");
    }
}

#[tokio::test]
async fn a_choice_is_checked_then_recorded_once_and_followed_back() {
    let app = serve("flow", true, false).await;
    let project = fixtures::add_project(&app.core, app.dir.join("project").to_str().unwrap());
    fixtures::lower_area(&app.core, &project, "src/a", 1_000);

    // Shape: unknown project, no acknowledgement, a choice above the ceiling, a record that is not the current one.
    let (status, _) = app
        .post(OVERRIDE, choice("no-such-project", "off", true, 1_000))
        .await;
    assert_eq!(status, 404);
    let (status, body) = app
        .post(OVERRIDE, choice(&project, "keep_ceiling", false, 1_000))
        .await;
    assert_eq!((status, &body["code"]), (400, &json!("invalid_input")));
    let (status, body) = app
        .post(OVERRIDE, choice(&project, "full", true, 1_000))
        .await;
    assert_eq!((status, &body["code"]), (400, &json!("invalid_input")));
    let (status, body) = app
        .post(OVERRIDE, choice(&project, "keep_ceiling", true, 5))
        .await;
    assert_eq!((status, &body["code"]), (409, &json!("stale")));
    let (status, _) = app.post(OVERRIDE, json!({ "project": project })).await;
    assert!(status == 400 || status == 422, "{status}");

    // The choice: the area as it now stands comes back, and the same choice again records nothing more.
    let (status, row) = app
        .post(OVERRIDE, choice(&project, "keep_ceiling", true, 1_000))
        .await;
    assert_eq!(status, 200, "{row}");
    assert_eq!(row["state"], "overridden");
    assert_eq!(row["override"]["choice"], "keep_ceiling");
    assert_eq!(row["now"], "full");
    let (status, _) = app
        .post(OVERRIDE, choice(&project, "keep_ceiling", true, 1_000))
        .await;
    assert_eq!(status, 200);
    let history = format!("/api/crew/autopilot/areas/history?project={project}&area=src%2Fa");
    let (_, trail) = app.call("GET", &history, None, true).await;
    let kinds: Vec<&Value> = trail["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| &e["kind"])
        .collect();
    assert_eq!(kinds, [&json!("override_set"), &json!("demoted")]);
    assert_eq!(trail["events"][0]["decided_by"], "person");

    // Follow: the override ends, the breaker's lowering stands, and doing it twice records nothing more.
    let body = json!({ "project": project, "area": "src/a" });
    let (status, row) = app.post(FOLLOW, body.clone()).await;
    assert_eq!(status, 200, "{row}");
    assert!(row["override"].is_null());
    assert_eq!(row["now"], "on");
    let (status, _) = app.post(FOLLOW, body).await;
    assert_eq!(status, 200);
    let (_, trail) = app.call("GET", &history, None, true).await;
    assert_eq!(trail["events"].as_array().unwrap().len(), 3);
    let stale =
        json!({ "project": project, "area": "src/a", "seen": { "merges": 9, "reverts": 9 } });
    let (status, body) = app.post(FOLLOW, stale).await;
    assert_eq!((status, &body["code"]), (409, &json!("stale")));
}
