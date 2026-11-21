//! The personal library over HTTP: every write is a `Person` route a service token alone is refused on, the lists are bare arrays,
//! an environment value is in no answer, and a run answers `{ terminal_id }` and asks for its grants first.

use std::sync::Arc;

use orbyn_core::credentials::{ENV_DEV_SERVICE, EnvSets, Failure, MemoryBackend};
use orbyn_core::store::{NewProject, NewThread};
use orbyn_core::{Core, CoreConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::router;
use crate::http::AppState;
use crate::http::access::Access;

const KEY: &str = "the-key";
const SECRET: &str = "http-secret-VALUE-4417";

struct Served {
    port: u16,
    core: Arc<Core>,
    memory: Arc<MemoryBackend>,
    thread: String,
    dir: std::path::PathBuf,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

async fn serve(name: &str) -> Served {
    let dir = std::env::temp_dir().join(format!("orbynd-library-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let work = dir.join("work");
    std::fs::create_dir_all(&work).unwrap();
    let memory = Arc::new(MemoryBackend::default());
    let core = Arc::new(
        Core::open(CoreConfig::new(
            dir.join("data"),
            "http://127.0.0.1:1".into(),
        ))
        .unwrap()
        .with_env_sets(EnvSets::new(memory.clone(), ENV_DEV_SERVICE)),
    );
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
            title: "t".into(),
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
        access: Arc::new(Access::new("token", vec![], None).with_person_key(Some(KEY))),
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
        memory,
        thread: thread.id,
        dir,
    }
}

impl Served {
    async fn call(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        person: bool,
    ) -> (u16, Value) {
        let mut request = reqwest::Client::new()
            .request(
                method.parse().unwrap(),
                format!("http://127.0.0.1:{}{path}", self.port),
            )
            .bearer_auth("token");
        if person {
            request = request.header("x-orbyn-person", KEY);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    async fn made(&self, path: &str, body: Value) -> Value {
        let (status, answer) = self.call("POST", path, Some(body), true).await;
        assert_eq!(status, 201, "{answer}");
        answer
    }
}

#[tokio::test]
async fn a_service_token_alone_is_refused_on_every_write_and_nothing_changes() {
    let s = serve("token").await;
    let w = s
        .made(
            "/api/library/workflows",
            json!({ "name": "W", "command": "echo 1" }),
        )
        .await;
    let p = s
        .made("/api/library/prompts", json!({ "name": "P", "body": "hi" }))
        .await;
    let e = s
        .made(
            "/api/library/env-sets",
            json!({ "name": "E", "keys": ["K"] }),
        )
        .await;
    let (wid, pid, eid) = (
        w["id"].as_str().unwrap(),
        p["id"].as_str().unwrap(),
        e["id"].as_str().unwrap(),
    );
    let writes: Vec<(&str, String, Value)> = vec![
        (
            "POST",
            "/api/library/workflows".into(),
            json!({ "name": "X", "command": "x" }),
        ),
        (
            "PATCH",
            format!("/api/library/workflows/{wid}"),
            json!({ "seen_revision": 1, "name": "Hijack" }),
        ),
        (
            "DELETE",
            format!("/api/library/workflows/{wid}"),
            Value::Null,
        ),
        (
            "POST",
            format!("/api/library/workflows/{wid}/run"),
            json!({ "thread_id": s.thread }),
        ),
        (
            "POST",
            "/api/library/prompts".into(),
            json!({ "name": "X", "body": "x" }),
        ),
        (
            "PATCH",
            format!("/api/library/prompts/{pid}"),
            json!({ "seen_revision": 1, "name": "Hijack" }),
        ),
        ("DELETE", format!("/api/library/prompts/{pid}"), Value::Null),
        (
            "POST",
            "/api/library/env-sets".into(),
            json!({ "name": "X" }),
        ),
        (
            "PATCH",
            format!("/api/library/env-sets/{eid}"),
            json!({ "seen_revision": 1, "name": "Hijack" }),
        ),
        (
            "DELETE",
            format!("/api/library/env-sets/{eid}"),
            Value::Null,
        ),
        (
            "PUT",
            format!("/api/library/env-sets/{eid}/keys/K"),
            json!({ "value": SECRET }),
        ),
        (
            "DELETE",
            format!("/api/library/env-sets/{eid}/keys/K"),
            Value::Null,
        ),
        (
            "POST",
            "/api/library/import".into(),
            json!({ "format": "orbyn-library", "version": 1 }),
        ),
        (
            "POST",
            format!("/api/projects/{}/library/suggestions/copy", "p"),
            json!({ "file": "workflows.json", "index": 0 }),
        ),
    ];
    for (method, path, body) in writes {
        let body = (!body.is_null()).then_some(body);
        let (status, answer) = s.call(method, &path, body, false).await;
        assert_eq!(
            (status, answer["code"].as_str()),
            (403, Some("person_required")),
            "{method} {path}"
        );
    }
    let (_, list) = s.call("GET", "/api/library/workflows", None, false).await;
    assert_eq!(list[0]["name"], "W");
    assert!(
        s.memory.accounts().is_empty(),
        "no value reached the keychain"
    );
    assert!(
        s.core
            .store
            .list_audit(None, 500)
            .unwrap()
            .iter()
            .all(|r| r.action != "library.run")
    );
    assert!(s.core.terminals.list(&s.thread).is_empty());
}

#[tokio::test]
async fn lists_are_bare_arrays_and_the_summary_has_the_pinned_limits() {
    let s = serve("lists").await;
    for path in [
        "/api/library/workflows",
        "/api/library/prompts",
        "/api/library/env-sets",
    ] {
        let (status, list) = s.call("GET", path, None, false).await;
        assert_eq!((status, list), (200, json!([])), "{path}");
    }
    let (status, summary) = s.call("GET", "/api/library", None, false).await;
    assert_eq!(status, 200);
    assert_eq!(
        (summary["workflows"].clone(), summary["keychain"].clone()),
        (json!(0), json!("ok"))
    );
    let limits = summary["limits"].as_object().unwrap();
    let mut keys: Vec<&str> = limits.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "env_keys_per_set",
            "env_sets",
            "env_sets_per_workflow",
            "env_value_bytes",
            "import_bytes",
            "params_per_item",
            "prompt_body_bytes",
            "prompts",
            "suggestions_file_bytes",
            "workflow_command_bytes",
            "workflows"
        ]
    );
    assert_eq!(limits["workflow_command_bytes"], 4000);
    assert_eq!(limits["suggestions_file_bytes"], 65536);
}

#[tokio::test]
async fn an_environment_value_is_write_only_over_http() {
    let s = serve("value").await;
    let e = s
        .made(
            "/api/library/env-sets",
            json!({ "name": "E", "keys": ["API_TOKEN"] }),
        )
        .await;
    let id = e["id"].as_str().unwrap();
    assert_eq!(e["status"], "stored");
    let (status, answer) = s
        .call(
            "PUT",
            &format!("/api/library/env-sets/{id}/keys/API_TOKEN"),
            Some(json!({ "value": SECRET })),
            true,
        )
        .await;
    assert_eq!(
        (status, answer.clone()),
        (200, json!({ "name": "API_TOKEN", "present": true }))
    );
    let (_, read) = s
        .call("GET", &format!("/api/library/env-sets/{id}"), None, false)
        .await;
    assert_eq!(
        read["keys"],
        json!([{ "name": "API_TOKEN", "present": true }])
    );
    let (_, list) = s.call("GET", "/api/library/env-sets", None, false).await;
    let (_, export) = s.call("GET", "/api/library/export", None, false).await;
    let (_, summary) = s.call("GET", "/api/library", None, false).await;
    // A body of the wrong shape is refused by field, never echoing the value.
    let (bad_status, bad) = s
        .call(
            "PUT",
            &format!("/api/library/env-sets/{id}/keys/API_TOKEN"),
            Some(json!({ "value": 123456789 })),
            true,
        )
        .await;
    assert_eq!(bad_status, 400);
    let audit = serde_json::to_string(&s.core.store.list_audit(None, 500).unwrap()).unwrap();
    for text in [
        read.to_string(),
        list.to_string(),
        export.to_string(),
        summary.to_string(),
        bad.to_string(),
        audit,
    ] {
        assert!(!text.contains(SECRET) && !text.contains("123456789"));
    }
    let (status, gone) = s
        .call(
            "DELETE",
            &format!("/api/library/env-sets/{id}/keys/API_TOKEN"),
            None,
            true,
        )
        .await;
    assert_eq!(
        (status, gone),
        (200, json!({ "name": "API_TOKEN", "present": false }))
    );
    let (status, refused) = s
        .call(
            "POST",
            "/api/library/env-sets",
            Some(json!({ "name": "Bad", "keys": ["LD_PRELOAD"] })),
            true,
        )
        .await;
    assert_eq!(
        (status, refused["code"].as_str()),
        (409, Some("key_refused"))
    );
}

#[tokio::test]
async fn no_keychain_is_planned_and_a_put_answers_503() {
    let s = serve("planned").await;
    let e = s
        .made(
            "/api/library/env-sets",
            json!({ "name": "E", "keys": ["K"] }),
        )
        .await;
    let id = e["id"].as_str().unwrap();
    s.memory.fail_with(Some(Failure::Unusable));
    let (_, read) = s
        .call("GET", &format!("/api/library/env-sets/{id}"), None, false)
        .await;
    assert_eq!(
        (read["status"].clone(), read["keychain"].clone()),
        (json!("planned"), json!("unavailable"))
    );
    let (status, answer) = s
        .call(
            "PUT",
            &format!("/api/library/env-sets/{id}/keys/K"),
            Some(json!({ "value": SECRET })),
            true,
        )
        .await;
    assert_eq!(
        (status, answer["code"].as_str()),
        (503, Some("keychain_unavailable"))
    );
    s.memory.fail_with(Some(Failure::Blocked));
    let (status, answer) = s
        .call(
            "PUT",
            &format!("/api/library/env-sets/{id}/keys/K"),
            Some(json!({ "value": SECRET })),
            true,
        )
        .await;
    assert_eq!(
        (status, answer["code"].as_str()),
        (503, Some("keychain_blocked"))
    );
    assert!(!answer.to_string().contains(SECRET));
}

#[cfg(unix)]
#[tokio::test]
async fn a_run_answers_the_terminal_id_after_the_grant_and_a_render_runs_nothing() {
    let s = serve("run").await;
    let e = s
        .made(
            "/api/library/env-sets",
            json!({ "name": "Staging", "keys": ["API_TOKEN"] }),
        )
        .await;
    let eid = e["id"].as_str().unwrap().to_owned();
    s.call(
        "PUT",
        &format!("/api/library/env-sets/{eid}/keys/API_TOKEN"),
        Some(json!({ "value": SECRET })),
        true,
    )
    .await;
    let w = s
        .made(
            "/api/library/workflows",
            json!({ "name": "Show", "command": "printenv API_TOKEN; echo {{who}}", "params": [{ "name": "who", "kind": "text", "required": true }], "env_set_ids": [eid] }),
        )
        .await;
    let wid = w["id"].as_str().unwrap();
    let (status, rendered) = s
        .call(
            "POST",
            &format!("/api/library/workflows/{wid}/render"),
            Some(json!({ "values": { "who": "a b; c" } })),
            false,
        )
        .await;
    assert_eq!(
        (status, rendered),
        (
            200,
            json!({ "command": "printenv API_TOKEN; echo 'a b; c'" })
        )
    );
    assert!(s.core.terminals.list(&s.thread).is_empty());

    let body = json!({ "values": { "who": "me" }, "thread_id": s.thread });
    let (status, answer) = s
        .call(
            "POST",
            &format!("/api/library/workflows/{wid}/run"),
            Some(body.clone()),
            true,
        )
        .await;
    assert_eq!(
        (status, answer["code"].as_str()),
        (409, Some("confirm_required"))
    );
    assert_eq!(answer["detail"]["grants"], json!(["env_values"]));
    assert!(!answer.to_string().contains(SECRET));
    assert!(s.core.terminals.list(&s.thread).is_empty());
    let mut acked = body;
    acked["ack_grants"] = json!(["env_values"]);
    let (status, answer) = s
        .call(
            "POST",
            &format!("/api/library/workflows/{wid}/run"),
            Some(acked),
            true,
        )
        .await;
    assert_eq!(status, 200, "{answer}");
    assert_eq!(
        answer.as_object().unwrap().keys().collect::<Vec<_>>(),
        ["terminal_id"]
    );
    assert_eq!(s.core.terminals.list(&s.thread).len(), 1);
}

#[tokio::test]
async fn an_import_over_two_mebibytes_is_refused_and_a_dry_run_writes_nothing() {
    let s = serve("import").await;
    let huge = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{}/api/library/import", s.port))
        .bearer_auth("token")
        .header("x-orbyn-person", KEY)
        .header("content-type", "application/json")
        .body(vec![b' '; 2 * 1024 * 1024 + 1])
        .send()
        .await
        .unwrap();
    assert_eq!(huge.status().as_u16(), 400);
    let doc = json!({ "format": "orbyn-library", "version": 1, "exported_at": "x", "dry_run": true, "prompts": [{ "name": "P", "body": "x" }, 7] });
    let (status, answer) = s.call("POST", "/api/library/import", Some(doc), true).await;
    assert_eq!(status, 200, "{answer}");
    assert_eq!(
        (
            answer["created"].clone(),
            answer["refused"].clone(),
            answer["dry_run"].clone()
        ),
        (json!(1), json!(1), json!(true))
    );
    let (_, prompts) = s.call("GET", "/api/library/prompts", None, false).await;
    assert_eq!(prompts, json!([]));
}
