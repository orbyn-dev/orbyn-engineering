//! The per-project Worker provider over HTTP: a bad value is a 400 that stores nothing, a person-key write is read back, and
//! the default is read when nothing is stored. The 403 without the key is covered for every Person route by
//! `tests/person_key.rs`; the routing rules are tested in `orbyn-core`.

use std::path::PathBuf;
use std::sync::Arc;

use orbyn_core::crew::fixtures;
use orbyn_core::{Core, CoreConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::router;
use crate::http::AppState;
use crate::http::access::Access;

struct Served {
    port: u16,
    core: Arc<Core>,
    dir: PathBuf,
}

async fn serve() -> Served {
    let dir = std::env::temp_dir().join(format!("orbynd-worker-provider-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut config = CoreConfig::new(dir.clone(), "http://127.0.0.1:1".into());
    config.crew_preview = true;
    config.crew_confine_workers = false;
    let core = Arc::new(Core::open(config).unwrap());
    let app = router(AppState {
        core: core.clone(),
        access: Arc::new(Access::new("token", vec![], None).with_person_key(Some("person-key"))),
        remote: false,
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
    async fn call(&self, method: &str, path: &str, body: Option<Value>, key: bool) -> (u16, Value) {
        let url = format!("http://127.0.0.1:{}{path}", self.port);
        let mut request = reqwest::Client::new()
            .request(method.parse().unwrap(), url)
            .bearer_auth("token");
        if key {
            request = request.header("x-orbyn-person", "person-key");
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

#[tokio::test]
async fn a_bad_value_is_refused_and_a_person_write_is_read_back() {
    let served = serve().await;
    let project = fixtures::add_project(&served.core, "/project");
    let read = format!("/api/crew/projects/{project}/worker-provider");
    let write = format!("{read}/set");

    let (status, body) = served.call("GET", &read, None, false).await;
    assert_eq!(status, 200);
    assert_eq!(body["worker_provider"], "same_as_lead");

    let (status, _) = served
        .call(
            "POST",
            &write,
            Some(json!({ "worker_provider": "any" })),
            false,
        )
        .await;
    assert_eq!(status, 403, "no person key, no write");

    let (status, body) = served
        .call(
            "POST",
            &write,
            Some(json!({ "worker_provider": "everyone" })),
            true,
        )
        .await;
    assert_eq!(status, 400);
    assert_eq!(body["code"], "invalid_input");
    let (_, body) = served.call("GET", &read, None, false).await;
    assert_eq!(
        body["worker_provider"], "same_as_lead",
        "nothing was stored"
    );

    let (status, body) = served
        .call(
            "POST",
            &write,
            Some(json!({ "worker_provider": "any" })),
            true,
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(body["worker_provider"], "any");
    let (_, body) = served.call("GET", &read, None, false).await;
    assert_eq!(body["worker_provider"], "any");

    let (status, _) = served
        .call(
            "POST",
            &write,
            Some(json!({ "worker_provider": "same_as_lead" })),
            true,
        )
        .await;
    assert_eq!(status, 200);
    let (_, body) = served.call("GET", &read, None, false).await;
    assert_eq!(body["worker_provider"], "same_as_lead");
}
