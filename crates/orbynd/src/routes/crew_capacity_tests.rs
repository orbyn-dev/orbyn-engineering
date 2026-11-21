//! The capacity routes over HTTP: the two reads answer a local caller and are hidden with the flag off; the two writes also
//! refuse a remote viewer and a request with no token. Rules and storage are tested in `orbyn-core`.

use std::path::PathBuf;
use std::sync::Arc;

use orbyn_core::crew::changes::changes_crew_state;
use orbyn_core::crew::fixtures;
use orbyn_core::{Core, CoreConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::crew_tasks_tests::seed_task;
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
    serve_with(
        name,
        preview,
        remote,
        Access::new("token", vec![], None).with_dev_person_override(true),
    )
    .await
}

/// As [`serve`], with the caller's access: a test of a `Person` route gives it a person key and no development override.
async fn serve_with(name: &str, preview: bool, remote: bool, access: Access) -> Served {
    let dir = std::env::temp_dir().join(format!("orbynd-capacity-{name}-{}", std::process::id()));
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

    /// A POST with the service token, and the person key when one is given: what an engine and the desktop shell send.
    async fn post(&self, path: &str, body: Value, token: bool, key: Option<&str>) -> (u16, Value) {
        let url = format!("http://127.0.0.1:{}{path}", self.port);
        let mut request = reqwest::Client::new().post(url);
        if token {
            request = request.bearer_auth("token");
        }
        if let Some(key) = key {
            request = request.header("x-orbyn-person", key);
        }
        let response = request.json(&body).send().await.unwrap();
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

#[tokio::test]
async fn the_state_read_lists_the_held_starts_and_stays_a_read() {
    let app = serve("held", true, false).await;
    let project = fixtures::add_project(&app.core, app.dir.join("project").to_str().unwrap());
    let path = format!("/api/crew/capacity?project={project}");
    let (status, state) = app.call("GET", &path, None, true).await;
    assert_eq!((status, &state["held"]), (200, &json!([])));
    fixtures::held_start(
        &app.core,
        &project,
        "review_fix",
        "fp-1",
        (9, 8, 100),
        Some(150),
    );
    fixtures::held_start(
        &app.core,
        &project,
        "comment_fix",
        "h-2",
        (11, 8, 200),
        None,
    );
    let (status, state) = app.call("GET", &path, None, true).await;
    assert_eq!(status, 200);
    let held = state["held"].as_array().unwrap();
    assert_eq!(held.len(), 2);
    assert_eq!(
        (
            &held[0]["kind"],
            &held[0]["state"],
            &held[0]["waiting"],
            &held[0]["started_unix"]
        ),
        (
            &json!("comment_fix"),
            &json!("held"),
            &json!(11),
            &Value::Null
        )
    );
    assert_eq!(
        (
            &held[1]["kind"],
            &held[1]["state"],
            &held[1]["started_unix"]
        ),
        (&json!("review_fix"), &json!("started"), &json!(150))
    );
    assert!(
        held.iter()
            .all(|h| h.get("text").is_none() && h.get("prompt").is_none())
    );
}

/// The two capacity writes are `Person` routes: an engine holding only the service token is refused by the person layer,
/// with no key or a wrong one, and nothing it sent is stored; the person's own key does both. Autopilot's refusal of an
/// agent's request by path (N1) is `orbyn-core`'s and is asserted here first, so the two halves meet at this route.
#[tokio::test]
async fn the_capacity_writes_need_the_person_key_and_store_nothing_without_it() {
    for path in [SETTINGS, TASK] {
        assert!(changes_crew_state(path), "{path}");
    }
    assert!(!changes_crew_state("/api/crew/capacity"));
    assert!(!changes_crew_state("/api/crew/capacity/coverage"));

    // No development override: this launch's person key is the only way in, as in the build a person uses.
    let app = serve_with(
        "capacity-key",
        true,
        false,
        Access::new("token", vec![], None).with_person_key(Some("the-key")),
    )
    .await;
    let project = fixtures::add_project(&app.core, app.dir.join("project").to_str().unwrap());
    let (task, _) = seed_task(&app.core, "capacity");
    let task_path = format!("/api/crew/capacity/tasks/{task}");
    let settings_body =
        json!({ "project": project, "limit": 4, "sampling": "one_in_four", "pause_enabled": true });
    let task_body = json!({ "priority": "low", "crew_started": true });
    // The capacity group's `Priority` is not public to this crate, so its Debug form is the readable witness of the stored pair.
    let stored = || format!("{:?}", app.core.store.crew_task_capacity(&task).unwrap());
    assert_eq!(stored(), "(Medium, false)");

    for (path, body) in [
        (SETTINGS, settings_body.clone()),
        (task_path.as_str(), task_body.clone()),
    ] {
        let (status, _) = app.post(path, body.clone(), false, Some("the-key")).await;
        assert_eq!(status, 401, "{path}: the service token comes first");
        for key in [None, Some("wrong")] {
            let (status, reply) = app.post(path, body.clone(), true, key).await;
            assert_eq!(
                (status, &reply["code"]),
                (403, &json!("person_required")),
                "{path}"
            );
        }
    }
    // Every refused call left the stored values alone: no settings saved, and the task as it was.
    let (_, before) = app
        .call(
            "GET",
            &format!("/api/crew/capacity?project={project}"),
            None,
            true,
        )
        .await;
    assert_eq!(
        (
            &before["settings_saved"],
            &before["limit"],
            &before["pause_enabled"]
        ),
        (&json!(false), &json!(8), &json!(false))
    );
    assert_eq!(stored(), "(Medium, false)");

    // With the key, a person's own save and task write work and are stored.
    let (status, saved) = app
        .post(SETTINGS, settings_body, true, Some("the-key"))
        .await;
    assert_eq!(
        (
            status,
            &saved["settings_saved"],
            &saved["limit"],
            &saved["pause_enabled"]
        ),
        (200, &json!(true), &json!(4), &json!(true))
    );
    let (status, answer) = app.post(&task_path, task_body, true, Some("the-key")).await;
    assert_eq!(
        (status, &answer["priority"], &answer["crew_started"]),
        (200, &json!("low"), &json!(true))
    );
    assert_eq!(stored(), "(Low, true)");
}
