//! The capacity routes over HTTP: the two reads answer a local caller and are hidden with the flag off; the two writes also
//! refuse a remote viewer and a request with no token. Rules and storage are tested in `orbyn-core`.

use std::path::PathBuf;
use std::sync::Arc;

use orbyn_core::crew::fixtures;
use orbyn_core::{Core, CoreConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::router;
use crate::http::AppState;
use crate::http::access::Access;

const SETTINGS: &str = "/api/crew/capacity/settings";
const TASK: &str = "/api/crew/capacity/tasks/t-1";

struct Served {
    port: u16,
    core: Arc<Core>,
    dir: PathBuf,
}

async fn serve(name: &str, preview: bool, remote: bool) -> Served {
    let dir = std::env::temp_dir().join(format!("orbynd-capacity-{name}-{}", std::process::id()));
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
}

const READS: [&str; 2] = [
    "/api/crew/capacity?project=p",
    "/api/crew/capacity/coverage?project=p",
];

#[tokio::test]
async fn the_writes_need_the_flag_a_local_caller_and_the_token_and_the_reads_need_the_flag() {
    let off = serve("off", false, false).await;
    for path in [SETTINGS, TASK] {
        let (status, body) = off.call("POST", path, Some(json!({})), true).await;
        assert_eq!(
            (status, &body["code"]),
            (404, &json!("not_found")),
            "{path}"
        );
    }
    for path in READS {
        let (status, _) = off.call("GET", path, None, true).await;
        assert_eq!(status, 404, "{path}");
    }
    let remote = serve("remote", true, true).await;
    for path in [SETTINGS, TASK] {
        let (status, body) = remote.call("POST", path, Some(json!({})), true).await;
        assert_eq!(
            (status, &body["code"]),
            (403, &json!("local_only")),
            "{path}"
        );
    }
    let on = serve("token", true, false).await;
    for path in [SETTINGS, TASK] {
        let (status, _) = on.call("POST", path, Some(json!({})), false).await;
        assert_eq!(status, 401, "{path}");
    }
}

#[tokio::test]
async fn a_person_saves_settings_and_reads_them_back() {
    let app = serve("flow", true, false).await;
    let project = fixtures::add_project(&app.core, app.dir.join("project").to_str().unwrap());
    let (status, state) = app
        .call(
            "GET",
            &format!("/api/crew/capacity?project={project}"),
            None,
            true,
        )
        .await;
    assert_eq!(
        (status, &state["pause_enabled"], &state["limit"]),
        (200, &json!(false), &json!(8))
    );
    let body =
        json!({ "project": project, "limit": 4, "sampling": "one_in_four", "pause_enabled": true });
    let (status, saved) = app.call("POST", SETTINGS, Some(body), true).await;
    assert_eq!(
        (status, &saved["limit"], &saved["pause_enabled"]),
        (200, &json!(4), &json!(true))
    );
    let (status, _) = app
        .call("POST", SETTINGS, Some(json!({ "project": project, "limit": 0, "sampling": "off", "pause_enabled": false })), true)
        .await;
    assert_eq!(status, 400);
    let (status, cover) = app
        .call(
            "GET",
            &format!("/api/crew/capacity/coverage?project={project}"),
            None,
            true,
        )
        .await;
    assert_eq!((status, &cover["merges"]), (200, &json!(0)));
    assert!(cover["began_unix"].is_null());
}
