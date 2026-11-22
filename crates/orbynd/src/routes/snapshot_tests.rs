//! Snap Shot source over HTTP: `PUT /api/attachments/{id}/source` keeps the source on the
//! attachment, refuses a non-image, an unknown or already sent attachment, and an over-limit
//! value; the body is capped at 64 KiB.

use std::path::PathBuf;
use std::sync::Arc;

use orbyn_contracts::AgUiEvent;
use orbyn_core::store::{NewProject, NewThread};
use orbyn_core::{Core, CoreConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::router;
use crate::http::AppState;
use crate::http::access::Access;

const TOKEN: &str = "token";
const KEY: &str = "person-key";

struct Served {
    port: u16,
    core: Arc<Core>,
    thread: String,
    dir: PathBuf,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

async fn serve(name: &str) -> Served {
    let dir = std::env::temp_dir().join(format!("orbynd-snapshot-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let work = dir.join("work");
    std::fs::create_dir_all(&work).unwrap();
    let mut config = CoreConfig::new(dir.join("data"), "http://127.0.0.1:1".into());
    config.engines.lookup_latest = false;
    let core = Arc::new(Core::open(config).unwrap());
    let project = core
        .store
        .add_project(NewProject {
            path: &work.to_string_lossy(),
            name: "p",
            is_git: false,
            managed: false,
            scratch: false,
        })
        .unwrap();
    core.store.trust_project(&project.id, "user").unwrap();
    let thread = core
        .store
        .create_thread(NewThread {
            project_id: project.id,
            title: "Snap Shot".into(),
            engine: "claude".into(),
            model: None,
            cwd: work.to_string_lossy().into_owned(),
            worktree: None,
            skill_profile: None,
            parent_thread_id: None,
            relation: None,
            scratch: false,
        })
        .unwrap();
    let app = router(AppState {
        core: core.clone(),
        access: Arc::new(Access::new(TOKEN, vec![], None).with_person_key(Some(KEY))),
        remote: false,
        ui_dir: None,
        images: Arc::new(orbyn_core::browser::remote_images::ImageProxy::default()),
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await });
    Served {
        port,
        core,
        thread: thread.id,
        dir,
    }
}

impl Served {
    async fn put(&self, id: &str, body: Value) -> (u16, Value) {
        self.put_as(id, body, true).await
    }

    async fn put_as(&self, id: &str, body: Value, person: bool) -> (u16, Value) {
        let mut request = reqwest::Client::new()
            .put(format!(
                "http://127.0.0.1:{}/api/attachments/{id}/source",
                self.port
            ))
            .bearer_auth(TOKEN)
            .json(&body);
        if person {
            request = request.header("x-orbyn-person", KEY);
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        (status, response.json().await.unwrap_or(Value::Null))
    }

    async fn attachments(&self) -> Value {
        let response = reqwest::Client::new()
            .get(format!(
                "http://127.0.0.1:{}/api/threads/{}/attachments",
                self.port, self.thread
            ))
            .bearer_auth(TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 200);
        response.json().await.unwrap()
    }

    fn image(&self) -> orbyn_contracts::Attachment {
        self.core
            .add_attachment(&self.thread, "shot.png", "image/png", b"png")
            .unwrap()
    }
}

fn source(app_name: &str) -> Value {
    json!({
        "kind": "snap_shot",
        "captured_at": "2026-11-22T10:00:00Z",
        "app_name": app_name,
        "window_title": "Weekly plan",
        "app_id": "com.example.app",
        "accessible_text": "buy milk",
    })
}

#[tokio::test]
async fn the_source_is_saved_listed_and_replaced() {
    let served = serve("save").await;
    let image = served.image();
    let (code, answer) = served.put(&image.id, source("Notes")).await;
    assert_eq!(code, 200, "{answer}");
    assert_eq!(answer["id"], json!(image.id.clone()));
    assert_eq!(answer["source"]["kind"], "snap_shot");
    assert_eq!(answer["source"]["app_name"], "Notes");
    let listed = served.attachments().await;
    assert_eq!(listed[0]["source"]["accessible_text"], "buy milk");

    let (code, answer) = served.put(&image.id, source("Browser")).await;
    assert_eq!(code, 200, "{answer}");
    assert_eq!(answer["source"]["app_name"], "Browser");
    assert_eq!(
        served
            .core
            .store
            .attachment_source(&image.id)
            .unwrap()
            .unwrap(),
        orbyn_contracts::AttachmentSource::SnapShot {
            captured_at: "2026-11-22T10:00:00Z".into(),
            app_name: "Browser".into(),
            window_title: "Weekly plan".into(),
            app_id: Some("com.example.app".into()),
            accessible_text: Some("buy milk".into()),
        }
    );
}

#[tokio::test]
async fn only_an_image_takes_a_source() {
    let served = serve("type").await;
    let file = served
        .core
        .add_attachment(&served.thread, "notes.txt", "text/plain", b"hi")
        .unwrap();
    let (code, answer) = served.put(&file.id, source("Notes")).await;
    assert_eq!(code, 422, "{answer}");
    assert_eq!(answer["code"], "unsupported_type");
}

#[tokio::test]
async fn an_unknown_or_already_sent_attachment_is_not_found() {
    let served = serve("missing").await;
    let (code, answer) = served.put("missing", source("Notes")).await;
    assert_eq!(code, 404, "{answer}");
    assert_eq!(answer["code"], "not_found");

    let image = served.image();
    served
        .core
        .store
        .append_thread_event(
            &served.thread,
            &AgUiEvent::Custom {
                name: "user.attachments".into(),
                value: json!({ "attachments": [{ "id": image.id.clone() }] }),
            },
        )
        .unwrap();
    let (code, answer) = served.put(&image.id, source("Notes")).await;
    assert_eq!(code, 404, "{answer}");
    assert_eq!(answer["code"], "not_found");
}

#[tokio::test]
async fn an_over_limit_field_is_source_too_large() {
    let served = serve("large").await;
    let image = served.image();
    let mut body = source(&"a".repeat(256));
    let (code, answer) = served.put(&image.id, body.clone()).await;
    assert_eq!(code, 413, "{answer}");
    assert_eq!(answer["code"], "source_too_large");

    body = source("Notes");
    body["accessible_text"] = json!("a".repeat(32_001));
    let (code, answer) = served.put(&image.id, body).await;
    assert_eq!(code, 413, "{answer}");
    assert_eq!(answer["code"], "source_too_large");
}

#[tokio::test]
async fn an_unknown_kind_is_an_invalid_body() {
    let served = serve("kind").await;
    let image = served.image();
    let (code, answer) = served.put(&image.id, json!({ "kind": "future" })).await;
    assert_eq!(code, 400, "{answer}");
    assert_eq!(answer["code"], "invalid_input");
}

#[tokio::test]
async fn a_body_over_64_kib_is_refused() {
    let served = serve("body").await;
    let image = served.image();
    let mut body = source("Notes");
    body["accessible_text"] = json!("a".repeat(70 * 1024));
    let (code, answer) = served.put(&image.id, body).await;
    assert_eq!(code, 400, "{answer}");
    assert_eq!(answer["code"], "invalid_input");
}

#[tokio::test]
async fn deleting_the_attachment_removes_its_source() {
    let served = serve("delete").await;
    let image = served.image();
    assert_eq!(served.put(&image.id, source("Notes")).await.0, 200);
    let response = reqwest::Client::new()
        .delete(format!(
            "http://127.0.0.1:{}/api/attachments/{}",
            served.port, image.id
        ))
        .bearer_auth(TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 204);
    assert_eq!(
        served.core.store.attachment_source(&image.id).unwrap(),
        None
    );
}

#[tokio::test]
async fn a_service_token_alone_gets_403_and_the_attachment_keeps_no_source() {
    let served = serve("person-only").await;
    let image = served.image();
    let (status, answer) = served.put_as(&image.id, source("Notes"), false).await;
    assert_eq!(status, 403, "{answer}");
    assert_eq!(
        served.core.store.attachment_source(&image.id).unwrap(),
        None
    );
    let (status, _) = served.put(&image.id, source("Notes")).await;
    assert_eq!(status, 200);
}
