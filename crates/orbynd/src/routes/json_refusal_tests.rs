//! The shared JSON extractor over a real route: a wrong-typed value under a map key names the map's
//! field only, so the caller's key and value stay out of the 400 body. The formatting itself is
//! covered in `http::json`.

use std::path::PathBuf;
use std::sync::Arc;

use orbyn_core::{Core, CoreConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::router;
use crate::http::AppState;
use crate::http::access::Access;

struct Served {
    port: u16,
    dir: PathBuf,
}

async fn serve(name: &str) -> Served {
    let dir =
        std::env::temp_dir().join(format!("orbynd-json-refusal-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut config = CoreConfig::new(dir.clone(), "http://127.0.0.1:1".into());
    config.crew_confine_workers = false;
    let core = Arc::new(Core::open(config).unwrap());
    let app = router(AppState {
        core,
        access: Arc::new(Access::new("token", vec![], None).with_dev_person_override(true)),
        remote: false,
        ui_dir: None,
        images: Arc::new(orbyn_core::browser::remote_images::ImageProxy::default()),
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await });
    Served { port, dir }
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Served {
    async fn post(&self, path: &str, body: Value) -> (u16, Value) {
        let response = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{}{path}", self.port))
            .bearer_auth("token")
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }
}

/// `POST /api/questions/{id}/answer` takes `AnswerQuestion`, whose `answers` is a map. A wrong-typed
/// value under a key must name `answers` only: the key and the value are canaries.
#[tokio::test]
async fn a_refused_map_value_names_the_field_and_not_the_key() {
    let app = serve("map-key").await;
    let caller_key = "caller-chosen-key";
    let (status, body) = app
        .post(
            "/api/questions/q/answer",
            json!({ "answers": { "caller-chosen-key": 48190273 } }),
        )
        .await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["code"], "invalid_input", "{body}");
    let message = body["message"].as_str().unwrap_or_default();
    assert!(message.contains("`answers`"), "{message}");
    assert!(
        !message.contains(caller_key),
        "the answer echoes the key: {message}"
    );
    assert!(
        !body.to_string().contains("48190273"),
        "the answer echoes a value: {body}"
    );
}
