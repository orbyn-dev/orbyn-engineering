//! The Flow routes over HTTP: three reads that answer empty views, refuse bad input, hide with the flag off, are open to a
//! remote viewer (they only read) and never hold the answer key. The table, credential and flag-off walks of `crew_tests.rs`
//! cover these routes too, since they sit under `/api/crew`.

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
    let dir = std::env::temp_dir().join(format!("orbynd-flow-{name}-{}", std::process::id()));
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
    async fn get(&self, path: &str) -> (u16, Value) {
        let url = format!("http://127.0.0.1:{}{path}", self.port);
        let response = reqwest::Client::new()
            .get(url)
            .bearer_auth("token")
            .send()
            .await
            .unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A task with a stored brief and key, and the text of the key's check.
fn seed_task(core: &Core, label: &str) -> (String, String) {
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
    let key_command = format!("echo key-check-{label}-{}", std::process::id());
    let key = stamper
        .answer_key(
            &json!({
                "checks": [{ "id": "chk-run", "kind": "command", "params": { "run": key_command },
                    "proves": "It runs." }],
                "protected_paths": ["tests/**"]
            }),
            &brief.id,
        )
        .unwrap();
    let task = core
        .store
        .create_crew_task(&brief, &key, &format!("run-{label}"))
        .unwrap();
    (task.id, key_command)
}

#[tokio::test]
async fn the_reads_answer_empty_views_and_refuse_bad_input() {
    let app = serve("empty", true, false).await;
    let (status, crews) = app.get("/api/crew/flow/crews").await;
    assert_eq!((status, crews["total"].clone()), (200, json!(0)));
    let (status, list) = app.get("/api/crew/flow/tasks?crew=nobody").await;
    assert_eq!(status, 200);
    assert_eq!(list["counts"]["all"], 0);
    assert_eq!(list["tasks"], json!([]));
    for path in [
        "/api/crew/flow/crews?limit=0",
        "/api/crew/flow/crews?limit=201",
        "/api/crew/flow/crews?offset=abc",
        "/api/crew/flow/tasks",
        "/api/crew/flow/tasks?crew=..%2Fetc",
        "/api/crew/flow/tasks?crew=a%20b",
        "/api/crew/flow/tasks?crew=crew-1&filter=nothing",
        "/api/crew/flow/tasks?crew=crew-1&engine=..%2Fx",
        "/api/crew/flow/tasks?crew=crew-1&limit=1000",
        "/api/crew/flow/tasks/..%2F..%2Fetc",
    ] {
        let (status, body) = app.get(path).await;
        assert_eq!(status, 400, "{path}");
        assert_eq!(body["code"], "invalid_input", "{path}");
    }
    let (status, _) = app.get("/api/crew/flow/tasks/sha256:abc").await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn a_stored_task_is_listed_and_zoomed_without_its_key() {
    let app = serve("task", true, false).await;
    let (task, key_text) = seed_task(&app.core, "flow");
    let key_id = app.core.store.crew_answer_key(&task).unwrap().unwrap().id;
    let (status, crews) = app.get("/api/crew/flow/crews").await;
    assert_eq!(status, 200);
    assert_eq!(crews["crews"][0]["id"], "crew-1");
    assert_eq!(crews["crews"][0]["tasks"], 1);
    let (status, list) = app.get("/api/crew/flow/tasks?crew=crew-1").await;
    assert_eq!(status, 200);
    assert_eq!(list["tasks"][0]["id"], task.as_str());
    assert_eq!(list["tasks"][0]["name"], "Task flow.");
    assert_eq!(list["tasks"][0]["cells"].as_array().unwrap().len(), 10);
    let (status, detail) = app.get(&format!("/api/crew/flow/tasks/{task}")).await;
    assert_eq!(status, 200);
    assert_eq!(detail["brief"]["files"], json!(["src/**"]));
    for body in [&crews, &list, &detail] {
        let text = body.to_string();
        assert!(
            !text.contains(&key_text),
            "a response holds the key's check"
        );
        assert!(
            !text.contains(key_id.as_str()),
            "a response holds the key's id"
        );
    }
    // Paging walks the list with next_offset.
    let (_, page) = app
        .get("/api/crew/flow/tasks?crew=crew-1&limit=1&offset=1")
        .await;
    assert_eq!(page["tasks"], json!([]));
    assert_eq!(page["total"], 1);
}

#[tokio::test]
async fn a_remote_viewer_may_read_them() {
    let app = serve("remote", true, true).await;
    for path in ["/api/crew/flow/crews", "/api/crew/flow/tasks?crew=crew-1"] {
        let (status, _) = app.get(path).await;
        assert_eq!(status, 200, "{path}");
    }
}

#[tokio::test]
async fn with_the_flag_off_they_are_not_there() {
    let app = serve("off", false, false).await;
    let (_, unknown) = app.get("/api/crew-nothing").await;
    for path in [
        "/api/crew/flow/crews",
        "/api/crew/flow/tasks?crew=crew-1",
        "/api/crew/flow/tasks/sha256:abc",
    ] {
        let (status, body) = app.get(path).await;
        assert_eq!(status, 404, "{path}");
        assert_eq!(body, unknown, "{path} answers as an absent route does");
    }
}
