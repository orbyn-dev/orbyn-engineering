//! `POST /api/crew/tasks/{id}/cancel` over HTTP: the Preview gate, the remote refusal, the body, and the answers for a task that
//! exists, one already cancelled, one that ended and one that does not exist. The engine side (stopping a worker's run) is tested in
//! the core on a fake engine; these tasks have no worker.

use std::sync::Arc;

use orbyn_contracts as wire;
use orbyn_core::crew::Stamper;
use orbyn_core::{Core, CoreConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::router;
use crate::http::AppState;
use crate::http::access::Access;

struct Served {
    port: u16,
    core: Arc<Core>,
    dir: std::path::PathBuf,
}

async fn serve(name: &str, preview: bool, remote: bool) -> Served {
    let dir = std::env::temp_dir().join(format!("orbynd-cancel-{name}-{}", std::process::id()));
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

impl Served {
    async fn cancel(&self, id: &str, body: Option<&str>) -> (u16, Value) {
        let url = format!("http://127.0.0.1:{}/api/crew/tasks/{id}/cancel", self.port);
        let mut request = reqwest::Client::new().post(url).bearer_auth("token");
        if let Some(body) = body {
            request = request
                .header("content-type", "application/json")
                .body(body.to_owned());
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    fn task(&self, label: &str) -> String {
        let stamper = Stamper {
            crew_id: wire::CrewId::new("crew-1").unwrap(),
            engine: wire::CrewToken::new("claude").unwrap(),
            member: wire::CrewMember::new("lead-01").unwrap(),
            now: wire::CrewTimestamp::new("2026-11-12T10:00:00Z").unwrap(),
        };
        let brief = stamper
            .brief(&json!({
                "kind": "implement",
                "outcome": format!("Task {label}."),
                "inputs": [],
                "files": [{ "path": "src/**", "mode": "write" }],
                "allowed_effects": ["read_repo", "write_leased", "run_checks"],
                "stop_when": ["Done."],
                "budget": { "turns": 20, "minutes": 30 }
            }))
            .unwrap();
        let key = stamper
            .answer_key(
                &json!({
                    "checks": [{ "id": "chk-run", "kind": "command",
                        "params": { "run": "echo ok" }, "proves": "It runs." }],
                    "protected_paths": ["tests/**"]
                }),
                &brief.id,
            )
            .unwrap();
        self.core
            .store
            .create_crew_task(&brief, &key, &format!("run-{label}"))
            .unwrap()
            .id
    }

    fn audit(&self, action: &str) -> usize {
        self.core
            .store
            .list_audit(None, 100)
            .unwrap()
            .iter()
            .filter(|row| row.action == action)
            .count()
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[tokio::test]
async fn a_cancel_answers_the_state_and_a_repeat_says_already() {
    let app = serve("ok", true, false).await;
    let id = app.task("a");
    let (status, body) = app.cancel(&id, None).await;
    assert_eq!(
        (status, &body),
        (
            200,
            &json!({ "task": id, "state": "cancelled", "already": false })
        )
    );
    assert_eq!(
        app.core.store.crew_task(&id).unwrap().unwrap().state,
        "cancelled"
    );
    let (status, body) = app.cancel(&id, Some("{}")).await;
    assert_eq!(
        (status, &body),
        (
            200,
            &json!({ "task": id, "state": "cancelled", "already": true })
        )
    );
    assert_eq!(app.audit("crew.task.cancel"), 1);
    assert_eq!(app.audit("crew.task.cancelled"), 1);
}

#[tokio::test]
async fn the_body_is_empty_or_a_bounded_reason() {
    let app = serve("body", true, false).await;
    let id = app.task("b");
    for bad in [r#"{"reason":1}"#, r#"{"other":"x"}"#, "not json"] {
        let (status, body) = app.cancel(&id, Some(bad)).await;
        assert_eq!(status, 400, "{bad}");
        assert_eq!(body["code"], "invalid_input");
    }
    let (status, _) = app
        .cancel(&id, Some(&json!({ "reason": "x".repeat(201) }).to_string()))
        .await;
    assert_eq!(status, 400);
    assert_eq!(
        app.core.store.crew_task(&id).unwrap().unwrap().state,
        "briefed"
    );
    assert_eq!(app.audit("crew.task.cancel"), 0);
    let (status, _) = app.cancel(&id, Some(r#"{"reason":"wrong brief"}"#)).await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn an_unknown_task_is_404_and_an_ended_one_is_409() {
    let app = serve("states", true, false).await;
    let (status, body) = app.cancel("no-such-task", None).await;
    assert_eq!((status, body["code"].as_str()), (404, Some("not_found")));
    let id = app.task("c");
    app.core
        .store
        .move_crew_task(&id, "needs_person", None)
        .unwrap();
    app.core
        .store
        .move_crew_task(&id, "stopped", Some("budget"))
        .unwrap();
    let (status, body) = app.cancel(&id, None).await;
    assert_eq!((status, body["code"].as_str()), (409, Some("task_ended")));
    assert_eq!(body["message"], "This task already ended.");
    assert_eq!(
        app.core.store.crew_task(&id).unwrap().unwrap().state,
        "stopped"
    );
    assert_eq!(app.audit("crew.task.cancel"), 0);
}

#[tokio::test]
async fn with_the_flag_off_the_route_is_not_there() {
    let app = serve("off", false, false).await;
    let (status, body) = app.cancel("task-1", None).await;
    assert_eq!((status, body["code"].as_str()), (404, Some("not_found")));
}

#[tokio::test]
async fn a_remote_viewer_cannot_cancel_and_nothing_is_written() {
    let app = serve("remote", true, true).await;
    let id = app.task("d");
    let (status, body) = app.cancel(&id, None).await;
    assert_eq!((status, body["code"].as_str()), (403, Some("local_only")));
    assert_eq!(
        app.core.store.crew_task(&id).unwrap().unwrap().state,
        "briefed"
    );
    assert_eq!(app.audit("crew.task.cancel"), 0);
}
