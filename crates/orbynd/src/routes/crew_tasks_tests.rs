//! The Flow routes over HTTP: four reads that answer empty views, refuse bad input, hide with the flag off, are open to a
//! remote viewer (they only read) and never hold the answer key. One task's sample read (KC5 slice 4) sits beside
//! them under `/api/crew/tasks`: it answers the record the store wrote and never the key either. The table, credential
//! and flag-off walks of `crew_tests.rs` cover these routes too, since they sit under `/api/crew`.

use std::sync::Arc;

use orbyn_contracts as wire;
use orbyn_contracts::{CrewObject, object_id};
use orbyn_core::crew::Stamper;
use orbyn_core::{Core, CoreConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::router;
use crate::http::AppState;
use crate::http::access::Access;

pub(super) struct Served {
    pub(super) port: u16,
    pub(super) core: Arc<Core>,
    dir: std::path::PathBuf,
}

pub(super) async fn serve(name: &str, preview: bool, remote: bool) -> Served {
    serve_with(
        name,
        preview,
        remote,
        Access::new("token", vec![], None).with_dev_person_override(true),
    )
    .await
}

/// As [`serve`], with the caller's access: a test of a `Person` route gives it a person key and no development override.
pub(super) async fn serve_with(name: &str, preview: bool, remote: bool, access: Access) -> Served {
    let dir = std::env::temp_dir().join(format!("orbynd-flow-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut config = CoreConfig::new(dir.clone(), "http://127.0.0.1:1".into());
    config.crew_preview = preview;
    config.crew_confine_workers = false;
    let core = Arc::new(Core::open(config).unwrap());
    let app = router(AppState {
        core: core.clone(),
        access: Arc::new(access),
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
    pub(super) async fn get(&self, path: &str) -> (u16, Value) {
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
pub(super) fn seed_task(core: &Core, label: &str) -> (String, String) {
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
        "/api/crew/flow/tasks/..%2F..%2Fetc/run",
    ] {
        let (status, body) = app.get(path).await;
        assert_eq!(status, 400, "{path}");
        assert_eq!(body["code"], "invalid_input", "{path}");
    }
    let (status, _) = app.get("/api/crew/flow/tasks/sha256:abc").await;
    assert_eq!(status, 404);
    let (status, _) = app.get("/api/crew/flow/tasks/sha256:abc/run").await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn a_lane_is_read_by_name_and_one_that_did_not_run_is_not_there() {
    let app = serve("lane", true, false).await;
    let (task, key_text) = seed_task(&app.core, "lane");
    let key_id = app.core.store.crew_answer_key(&task).unwrap().unwrap().id;
    // No review ran on this task, so its lanes are not there: the read refuses a missing name, a name that is not a
    // lane, a task that is not one and a task that does not exist.
    let base = format!("/api/crew/flow/tasks/{task}/lanes");
    for path in [
        base.clone(),
        format!("{base}?lane=a%20b"),
        format!("{base}?lane=Correctness"),
        "/api/crew/flow/tasks/..%2F..%2Fetc/lanes?lane=correctness".to_owned(),
    ] {
        let (status, body) = app.get(&path).await;
        assert_eq!(status, 400, "{path}");
        assert_eq!(body["code"], "invalid_input", "{path}");
    }
    for path in [
        format!("{base}?lane=correctness"),
        "/api/crew/flow/tasks/sha256:abc/lanes?lane=correctness".to_owned(),
    ] {
        let (status, body) = app.get(&path).await;
        assert_eq!(status, 404, "{path}");
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
    assert_eq!(list["tasks"][0]["stop_detail"], json!(null));
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

    // The Run page: the prompt of a task that never ran, no check yet, and the key's id still absent.
    let (status, run) = app.get(&format!("/api/crew/flow/tasks/{task}/run")).await;
    assert_eq!(status, 200);
    assert_eq!(run["task"], task.as_str());
    assert_eq!(run["finished"], false);
    assert_eq!(run["prompt"]["text"], "Task flow.");
    assert_eq!(run["checks"], json!([]));
    assert_eq!(run["not_verified"], json!([]));
    assert_eq!(run["pull"], Value::Null);
    assert!(run["commit"].is_null());
    assert!(
        !run.to_string().contains(key_id.as_str()),
        "the Run page holds the key's id"
    );
}

#[tokio::test]
async fn a_stopped_task_names_its_typed_stop_detail_on_the_flow_and_the_board() {
    let app = serve("stop-detail", true, false).await;
    let (task, _) = seed_task(&app.core, "stop-detail");
    for to in ["routed", "running"] {
        app.core.store.move_crew_task(&task, to, None).unwrap();
    }
    let stopped = app
        .core
        .store
        .stop_crew_after_restart(&task, "already_resumed")
        .unwrap()
        .unwrap();
    assert_eq!(stopped.stop_detail.as_deref(), Some("worker_stopped_twice"));

    let (status, list) = app.get("/api/crew/flow/tasks?crew=crew-1").await;
    assert_eq!(status, 200);
    let row = &list["tasks"][0];
    assert_eq!(row["id"], task.as_str());
    assert_eq!(row["state"], "stopped");
    assert_eq!(row["stop_detail"], "worker_stopped_twice");

    let (status, board) = app.get("/api/crew/flow/board?crew=crew-1").await;
    assert_eq!(status, 200);
    let card = board["columns"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|column| column["cards"].as_array().unwrap())
        .find(|card| card["id"] == task.as_str())
        .expect("the task's card");
    assert_eq!(card["stop_reason"], "engine_error");
    assert_eq!(card["stop_detail"], "worker_stopped_twice");
}

#[tokio::test]
async fn a_worker_token_reads_a_running_task_without_its_check_text_and_gets_it_once_final() {
    // The service token alone, no person key and no development override: the route is Local, as it must stay.
    let app = serve_with(
        "worker-run",
        true,
        false,
        Access::new("token", vec![], None),
    )
    .await;
    let (task, key_text) = seed_task(&app.core, "worker-run");
    let row = app.core.store.crew_task(&task).unwrap().unwrap();
    let hash = |byte: &str| format!("sha256:{}", byte.repeat(64));
    let commit = "a".repeat(40);

    let mut evidence = json!({
        "v": 0, "type": "evidence", "id": "",
        "crew_id": row.crew_id.clone(),
        "author": { "role": "core", "engine": "orbyn" },
        "created_at": "2026-11-12T10:00:00Z",
        "run_id": row.run_id.clone(),
        "check": "chk-run",
        "command": key_text.clone(),
        "outcome": "pass",
        "exit_code": 0,
        "duration_ms": 5,
        "commit": commit.clone(),
        "log_hash": hash("b"),
        "excerpt": "",
        "artifacts": [],
        "proves": "It runs.",
        "does_not_prove": "Anything else."
    });
    evidence["id"] = json!(object_id(&evidence).unwrap());
    let evidence_id = evidence["id"].as_str().unwrap().to_owned();
    app.core
        .store
        .add_crew_object(&task, &CrewObject::parse(&evidence).unwrap())
        .unwrap();

    let mut gate = json!({
        "v": 0, "type": "gate", "id": "",
        "crew_id": row.crew_id.clone(),
        "author": { "role": "core", "engine": "orbyn" },
        "created_at": "2026-11-12T10:00:01Z",
        "run_id": row.run_id.clone(),
        "stage": "checks",
        "subject": task.clone(),
        "decision": "pass",
        "decided_by": "code",
        "evidence": [evidence_id.clone()],
        "reason": "All checks passed."
    });
    gate["id"] = json!(object_id(&gate).unwrap());
    app.core
        .store
        .add_crew_object(&task, &CrewObject::parse(&gate).unwrap())
        .unwrap();

    let mut result = json!({
        "v": 0, "type": "result", "id": "",
        "crew_id": row.crew_id.clone(),
        "author": { "role": "core", "engine": "orbyn" },
        "created_at": "2026-11-12T10:00:02Z",
        "run_id": row.run_id.clone(),
        "brief": task.clone(),
        "state": "passed",
        "final": true,
        "summary": "1 check passed.",
        "branch": Value::Null,
        "commit": commit.clone(),
        "checks": [{ "check": "chk-run", "evidence": evidence_id.clone(), "outcome": "pass" }],
        "not_verified": [],
        "attempts": 1,
        "usage": {},
        "route": Value::Null,
        "report": Value::Null
    });
    result["id"] = json!(object_id(&result).unwrap());
    app.core
        .store
        .add_crew_object(&task, &CrewObject::parse(&result).unwrap())
        .unwrap();

    app.core
        .store
        .move_crew_task(&task, "routed", None)
        .unwrap();
    app.core
        .store
        .move_crew_task(&task, "running", None)
        .unwrap();
    let (status, running) = app.get(&format!("/api/crew/flow/tasks/{task}/run")).await;
    assert_eq!(status, 200);
    assert_eq!(running["finished"], false);
    assert_eq!(running["checks"].as_array().unwrap().len(), 1);
    assert_eq!(running["checks"][0]["label"], "");
    assert_eq!(running["checks"][0]["proves"], "");
    assert!(
        !running.to_string().contains(&key_text),
        "a worker token read a running task's check text"
    );

    app.core
        .store
        .move_crew_task(&task, "verifying", None)
        .unwrap();
    app.core
        .store
        .pass_crew_task(&task, &[evidence_id])
        .unwrap();
    let (status, finished) = app.get(&format!("/api/crew/flow/tasks/{task}/run")).await;
    assert_eq!(status, 200);
    assert_eq!(finished["finished"], true);
    assert_eq!(finished["checks"][0]["label"], key_text.as_str());
    assert_eq!(finished["checks"][0]["proves"], "It runs.");
}

#[tokio::test]
async fn the_artifacts_read_answers_one_task_and_keeps_the_key_out() {
    let app = serve("artifacts", true, false).await;
    let (task, key_text) = seed_task(&app.core, "artifacts");
    let key_id = app.core.store.crew_answer_key(&task).unwrap().unwrap().id;

    let (status, view) = app
        .get(&format!("/api/crew/flow/tasks/{task}/artifacts"))
        .await;
    assert_eq!(status, 200);
    assert_eq!(view["task"], task.as_str());
    assert_eq!(view["crew"], "crew-1");
    // The seeded task has no run, so no check ran and there is no checkout to read a diff from.
    assert_eq!(view["checks"], json!([]));
    assert!(view["diff"].is_null());
    assert_eq!(view["diff_absent"], "no_checkout");
    assert_eq!(view["truncated"], false);
    let text = view.to_string();
    assert!(
        !text.contains(&key_text),
        "a response holds the key's check"
    );
    assert!(
        !text.contains(key_id.as_str()),
        "a response holds the key's id"
    );
    // A bad id is invalid input, and an unknown task is not there.
    let (status, body) = app
        .get("/api/crew/flow/tasks/..%2F..%2Fetc/artifacts")
        .await;
    assert_eq!(status, 400);
    assert_eq!(body["code"], "invalid_input");
    let (status, _) = app.get("/api/crew/flow/tasks/sha256:abc/artifacts").await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn a_remote_viewer_may_read_them() {
    let app = serve("remote", true, true).await;
    let (task, _) = seed_task(&app.core, "remote");
    for path in [
        "/api/crew/flow/crews".to_owned(),
        "/api/crew/flow/tasks?crew=crew-1".to_owned(),
        format!("/api/crew/flow/tasks/{task}/artifacts"),
        format!("/api/crew/tasks/{task}/samples"),
    ] {
        let (status, _) = app.get(&path).await;
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
        "/api/crew/flow/tasks/sha256:abc/artifacts",
        "/api/crew/flow/tasks/sha256:abc/run",
        "/api/crew/tasks/sha256:abc/samples",
    ] {
        let (status, body) = app.get(path).await;
        assert_eq!(status, 404, "{path}");
        assert_eq!(body, unknown, "{path} answers as an absent route does");
    }
}

#[tokio::test]
async fn a_task_sample_is_read_by_task_and_an_unknown_task_is_not_there() {
    let app = serve("samples", true, false).await;
    let (task, key_text) = seed_task(&app.core, "samples");
    let key_id = app.core.store.crew_answer_key(&task).unwrap().unwrap().id;
    let path = format!("/api/crew/tasks/{task}/samples");
    let (status, body) = app.get(&path).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["task_id"], task.as_str());
    let samples = body["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 1, "one task records exactly one sample");
    assert_eq!(samples[0]["task_id"], task.as_str());
    assert_eq!(samples[0]["kind"], "implement");
    assert_eq!(samples[0]["had_lessons"], false);
    assert_eq!(samples[0]["lesson_ids"], json!([]));
    assert_eq!(samples[0]["left_out_ids"], json!([]));
    assert_eq!(samples[0]["outcome"], Value::Null);
    assert_eq!(samples[0]["stop_reason"], Value::Null);
    assert_eq!(samples[0]["finished_at"], Value::Null);
    for text in [key_text.as_str(), key_id.as_str()] {
        assert!(!body.to_string().contains(text), "the read holds the key");
    }

    // The store's own state move writes the one outcome, and the read answers it with its finish time.
    app.core
        .store
        .move_crew_task(&task, "cancelled", None)
        .unwrap();
    let (status, body) = app.get(&path).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["samples"][0]["outcome"], "cancelled");
    assert!(body["samples"][0]["finished_at"].is_string());

    // A task nothing stored is not found.
    let (status, body) = app.get("/api/crew/tasks/sha256:none/samples").await;
    assert_eq!(
        (status, &body["code"]),
        (404, &json!("not_found")),
        "{body}"
    );
}
