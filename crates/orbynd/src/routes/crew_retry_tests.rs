//! `POST /api/crew/tasks/{id}/retry-elsewhere` over HTTP, and the dismiss of a task that could not start (the existing cancel): the
//! Preview gate, the remote refusal, the answers for a task that does not exist or did start, and a waiting task that cancels. The
//! engine side (another engine, the lead's message) is tested in the core on fake engines.

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
    let dir = std::env::temp_dir().join(format!("orbynd-retry-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut config = CoreConfig::new(dir.clone(), "http://127.0.0.1:1".into());
    config.crew_preview = preview;
    config.crew_confine_workers = false;
    let core = Arc::new(Core::open(config).unwrap());
    let app = router(AppState {
        core: core.clone(),
        access: Arc::new(Access::new("token", vec![], None).with_dev_person_override(true)),
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
    async fn cancel(&self, id: &str) -> (u16, Value) {
        self.post(id, "cancel").await
    }

    async fn retry(&self, id: &str) -> (u16, Value) {
        self.post(id, "retry-elsewhere").await
    }

    async fn post(&self, id: &str, what: &str) -> (u16, Value) {
        let url = format!("http://127.0.0.1:{}/api/crew/tasks/{id}/{what}", self.port);
        let response = reqwest::Client::new()
            .post(url)
            .bearer_auth("token")
            .send()
            .await
            .unwrap();
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
async fn an_unknown_task_is_404_and_a_task_that_did_start_is_409() {
    let app = serve("states", true, false).await;
    let (status, body) = app.retry("no-such-task").await;
    assert_eq!((status, body["code"].as_str()), (404, Some("not_found")));
    let id = app.task("a");
    let (status, body) = app.retry(&id).await;
    assert_eq!((status, body["code"].as_str()), (409, Some("task_busy")));
    assert_eq!(
        app.core.store.crew_task(&id).unwrap().unwrap().state,
        "briefed"
    );
    assert_eq!(app.audit("crew.task.cancel"), 0);
}

#[tokio::test]
async fn with_the_flag_off_the_route_is_not_there() {
    let app = serve("off", false, false).await;
    let (status, body) = app.retry("task-1").await;
    assert_eq!((status, body["code"].as_str()), (404, Some("not_found")));
}

#[tokio::test]
async fn a_remote_viewer_cannot_retry_and_nothing_is_written() {
    let app = serve("remote", true, true).await;
    let id = app.task("b");
    let (status, body) = app.retry(&id).await;
    assert_eq!((status, body["code"].as_str()), (403, Some("local_only")));
    assert_eq!(app.audit("crew.task.cancel"), 0);
}

#[tokio::test]
async fn a_waiting_task_is_dismissed_by_the_existing_cancel_with_its_audit_rows() {
    let app = serve("dismiss", true, false).await;
    let id = app.task("c");
    app.core
        .store
        .move_crew_task(&id, "needs_person", None)
        .unwrap();
    let (status, body) = app.cancel(&id).await;
    assert_eq!(
        (status, &body),
        (
            200,
            &json!({ "task": id, "state": "cancelled", "already": false })
        )
    );
    assert_eq!(app.audit("crew.task.cancel"), 1);
    assert_eq!(app.audit("crew.task.cancelled"), 1);
}
