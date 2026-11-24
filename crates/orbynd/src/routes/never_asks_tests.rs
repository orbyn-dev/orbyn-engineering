//! The routes of an engine that never asks (Cursor on the host): the typed 409 bodies, the one-turn override and the per-project
//! notice, with their person key and audit rows. The engine is a stand-in named `cursor`; no real Cursor turn runs.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use orbyn_contracts::{CreateThread, PermissionMode, UpdateThread};
use orbyn_core::agents::engines::stand_in::StandIn;
use orbyn_core::{Core, CoreConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::router;
use crate::http::AppState;
use crate::http::access::Access;

/// A minimal skills library the workspace can adopt.
fn library(root: &Path) {
    std::fs::create_dir_all(root.join("skills/how")).unwrap();
    std::fs::write(root.join("AGENTS.md"), "shared policy\n").unwrap();
    std::fs::write(root.join(".gitignore"), "repo/\n").unwrap();
    std::fs::write(
        root.join("skills/catalog.json"),
        r#"{"skills":[{"name":"how","category":"Engineering practice"}]}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("skills/how/SKILL.md"),
        "---\nname: how\ndescription: \"Trace behavior\"\n---\n\n# how\n",
    )
    .unwrap();
    for host in [".claude", ".agents"] {
        let links = root.join(host).join("skills");
        std::fs::create_dir_all(&links).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("../../skills/how", links.join("how")).unwrap();
    }
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.email", "t@example.com"],
        &["config", "user.name", "T"],
        &["add", "."],
        &["commit", "-q", "-m", "library"],
    ] {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(root)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }
}

struct Served {
    port: u16,
    core: Arc<Core>,
    project: String,
    thread: String,
    dir: PathBuf,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

async fn serve() -> Served {
    let dir = std::env::temp_dir().join(format!(
        "orbynd-never-asks-{}-{}",
        std::process::id(),
        orbyn_core::util::new_id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    // Temp folders sit behind a symlink on macOS, and the workspace refuses those.
    let dir = std::fs::canonicalize(&dir).unwrap();
    let root = dir.join("library");
    library(&root);
    let mut config = CoreConfig::new(dir.join("data"), "http://127.0.0.1:1".into());
    config.engines.lookup_latest = false;
    let core = Arc::new(Core::open(config).unwrap());
    core.engines.register(StandIn::new(
        "cursor",
        vec![PermissionMode::NoApprovals],
        Arc::new(Mutex::new(Vec::new())),
    ));
    core.workspace
        .setup(orbyn_contracts::WorkspaceSetup {
            url: None,
            adopt_path: Some(root.to_string_lossy().into_owned()),
        })
        .await
        .unwrap();
    let project = core.new_project("project", false, None).await.unwrap();
    core.store.trust_project(&project.id, "t").unwrap();
    let thread = core
        .create_thread(CreateThread {
            project_id: Some(project.id.clone()),
            engine: "cursor".into(),
            model: None,
            title: None,
            worktree: None,
            skill_profile: None,
            base_branch: None,
            effort: None,
            options: std::collections::BTreeMap::new(),
        })
        .await
        .unwrap();
    let state = AppState {
        core: core.clone(),
        access: Arc::new(Access::new("token", vec![], None).with_person_key(Some("the-key"))),
        remote: false,
        ui_dir: None,
        images: Arc::new(orbyn_core::browser::remote_images::ImageProxy::default()),
    };
    let app = router(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await });
    Served {
        port,
        core,
        project: project.id,
        thread: thread.id,
        dir,
    }
}

impl Served {
    async fn call(&self, method: &str, path: &str, body: Value, person: bool) -> (u16, Value) {
        let url = format!("http://127.0.0.1:{}{path}", self.port);
        let mut request = reqwest::Client::new()
            .request(method.parse().unwrap(), url)
            .bearer_auth("token")
            .json(&body);
        if person {
            request = request.header("x-orbyn-person", "the-key");
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    fn notice_path(&self, engine: &str) -> String {
        format!("/api/projects/{}/engines/{engine}/notice", self.project)
    }

    fn audits(&self, action: &str) -> usize {
        self.core
            .store
            .list_audit(None, 100)
            .unwrap()
            .iter()
            .filter(|row| row.action == action)
            .count()
    }

    async fn idle(&self) {
        for _ in 0..2000 {
            if self
                .core
                .store
                .thread(&self.thread)
                .unwrap()
                .active_run
                .is_none()
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        panic!("the run never ended");
    }
}

#[tokio::test]
async fn the_notice_reads_for_anyone_and_is_acknowledged_by_the_person_once() {
    let app = serve().await;
    let path = app.notice_path("cursor");
    let (status, body) = app.call("GET", &path, json!(null), false).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body,
        json!({
            "engine": "cursor",
            "project_id": app.project,
            "acknowledged": false,
            "acknowledged_at": null,
        })
    );
    // An engine with no notice answers 404.
    let (status, _) = app
        .call("GET", &app.notice_path("claude"), json!(null), false)
        .await;
    assert_eq!(status, 404);

    let ack = format!("{path}/acknowledge");
    let (status, body) = app.call("POST", &ack, json!(null), false).await;
    assert_eq!((status, &body["code"]), (403, &json!("person_required")));
    assert_eq!(app.audits("engine_notice.acknowledge"), 0);

    let (status, first) = app.call("POST", &ack, json!(null), true).await;
    assert_eq!(status, 200, "{first}");
    assert_eq!(first["acknowledged"], true);
    assert!(first["acknowledged_at"].is_string());
    let (status, again) = app.call("POST", &ack, json!(null), true).await;
    assert_eq!((status, &again), (200, &first));
    let (_, read) = app.call("GET", &path, json!(null), false).await;
    assert_eq!(read, first);
    assert_eq!(app.audits("engine_notice.acknowledge"), 1);
    // Nothing secret is in it: the only fields are the five of the contract.
    let mut keys: Vec<_> = first.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    assert_eq!(
        keys,
        ["acknowledged", "acknowledged_at", "engine", "project_id"]
    );
}

#[tokio::test]
async fn a_refused_start_is_409_with_the_fields_beside_code_and_message() {
    let app = serve().await;
    let runs = format!("/api/threads/{}/runs", app.thread);
    let (status, body) = app
        .call("POST", &runs, json!({ "prompt": "go" }), false)
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["code"], "engine_notice_required");
    assert_eq!(body["engine"], "cursor");
    assert_eq!(body["project_id"], json!(app.project));
    assert!(body["message"].as_str().unwrap().ends_with('.'));

    app.call(
        "POST",
        &format!("{}/acknowledge", app.notice_path("cursor")),
        json!(null),
        true,
    )
    .await;
    let (status, body) = app
        .call("POST", &runs, json!({ "prompt": "go" }), false)
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["code"], "engine_mode_refused");
    assert_eq!(body["engine"], "cursor");
    assert_eq!(body["thread_id"], json!(app.thread));
    assert_eq!(body["mode"], "supervised");
    assert_eq!(body["allowed"], json!(["no_approvals"]));
    assert!(body["message"].as_str().unwrap().ends_with('.'));
    assert!(body.get("detail").is_none());
}

#[tokio::test]
async fn send_without_approvals_is_the_persons_runs_one_turn_and_stores_nothing() {
    let app = serve().await;
    app.call(
        "POST",
        &format!("{}/acknowledge", app.notice_path("cursor")),
        json!(null),
        true,
    )
    .await;
    let once = format!("/api/threads/{}/send-without-approvals", app.thread);
    let (status, body) = app
        .call("POST", &once, json!({ "prompt": "go" }), false)
        .await;
    assert_eq!((status, &body["code"]), (403, &json!("person_required")));
    assert_eq!(app.audits("thread.send_without_approvals"), 0);

    // A mode in the body would be stored: it is refused, not ignored.
    let (status, _) = app
        .call(
            "POST",
            &once,
            json!({ "prompt": "go", "permission_mode": "no_approvals" }),
            true,
        )
        .await;
    assert_eq!(status, 400);

    let (status, run) = app
        .call("POST", &once, json!({ "prompt": "go" }), true)
        .await;
    assert_eq!(status, 202, "{run}");
    assert_eq!(app.audits("thread.send_without_approvals"), 1);
    app.idle().await;
    let saved = app
        .core
        .store
        .thread(&app.thread)
        .unwrap()
        .lifecycle
        .unwrap();
    assert_eq!(saved.permission_mode, PermissionMode::Supervised);
    // The next plain send is refused again.
    let runs = format!("/api/threads/{}/runs", app.thread);
    let (status, body) = app
        .call("POST", &runs, json!({ "prompt": "again" }), false)
        .await;
    assert_eq!(
        (status, &body["code"]),
        (409, &json!("engine_mode_refused"))
    );
}

#[tokio::test]
async fn a_cursor_thread_saved_in_no_approvals_sends_after_the_notice() {
    let app = serve().await;
    app.core
        .store
        .update_thread(
            &app.thread,
            &UpdateThread {
                permission_mode: Some(PermissionMode::NoApprovals),
                ..Default::default()
            },
            "t",
        )
        .unwrap();
    app.call(
        "POST",
        &format!("{}/acknowledge", app.notice_path("cursor")),
        json!(null),
        true,
    )
    .await;
    let runs = format!("/api/threads/{}/runs", app.thread);
    let (status, run) = app
        .call("POST", &runs, json!({ "prompt": "go" }), false)
        .await;
    assert_eq!(status, 202, "{run}");
}
