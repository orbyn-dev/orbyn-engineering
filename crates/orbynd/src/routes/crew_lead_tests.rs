//! The lead-thread routes over HTTP (KC2): the live read the close confirm shows, the close itself, and the thread route that
//! runs the close when a lead thread settles or archives. The engine side (stopping a worker's run) is tested in the core on
//! a fake engine; these tasks have no worker.

use std::sync::Arc;

use orbyn_core::store::{NewProject, NewThread};
use orbyn_core::{Core, CoreConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::crew_tasks_tests::seed_task;
use super::router;
use crate::http::AppState;
use crate::http::access::Access;

struct Served {
    port: u16,
    core: Arc<Core>,
    dir: std::path::PathBuf,
}

async fn serve(name: &str, preview: bool, remote: bool) -> Served {
    let dir = std::env::temp_dir().join(format!("orbynd-lead-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("work")).unwrap();
    let mut config = CoreConfig::new(dir.join("data"), "http://127.0.0.1:1".into());
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
    fn thread(&self, title: &str) -> String {
        let work = self.dir.join("work");
        let project = self
            .core
            .store
            .add_project(NewProject {
                path: &work.to_string_lossy(),
                name: "p",
                is_git: false,
                managed: false,
                scratch: false,
            })
            .unwrap();
        self.core
            .store
            .create_thread(NewThread {
                project_id: project.id,
                title: title.into(),
                engine: "claude".into(),
                model: None,
                cwd: work.to_string_lossy().into_owned(),
                worktree: None,
                skill_profile: None,
                parent_thread_id: None,
                relation: None,
                scratch: false,
            })
            .unwrap()
            .id
    }

    /// A task of `thread`, `state` or briefed: the close's own tasks have no worker.
    fn task(&self, label: &str, thread: &str, state: Option<&str>) -> String {
        let (task, _) = seed_task(&self.core, label);
        self.core
            .store
            .add_crew_run(&task, thread, "member-01")
            .unwrap();
        if let Some(state) = state {
            self.core.store.move_crew_task(&task, state, None).unwrap();
        }
        task
    }

    async fn call(&self, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
        let url = format!("http://127.0.0.1:{}{path}", self.port);
        let mut request = reqwest::Client::new().request(method.parse().unwrap(), url);
        request = request.bearer_auth("token");
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    fn live_path(&self, thread: &str) -> String {
        format!("/api/crew/lead/{thread}/live")
    }

    fn close_path(&self, thread: &str) -> String {
        format!("/api/crew/lead/{thread}/close")
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[tokio::test]
async fn the_live_read_is_exactly_what_the_close_cancels() {
    let app = serve("agree", true, false).await;
    let thread = app.thread("Lead");
    let first = app.task("live-a", &thread, None);
    let second = app.task("live-b", &thread, Some("routed"));
    let ended = app.task("ended", &thread, Some("needs_person"));
    app.core
        .store
        .move_crew_task(&ended, "stopped", Some("budget"))
        .unwrap();
    // A task another thread leads is never this thread's live work.
    let elsewhere = app.task("elsewhere", "another-lead", Some("routed"));

    let (status, live) = app.call("GET", &app.live_path(&thread), None).await;
    assert_eq!(status, 200, "{live}");
    assert_eq!(
        live,
        json!({ "tasks": [
            { "id": first, "name": "Task live-a.", "state": "briefed" },
            { "id": second, "name": "Task live-b.", "state": "routed" },
        ] })
    );

    let (status, closed) = app.call("POST", &app.close_path(&thread), None).await;
    assert_eq!(status, 200, "{closed}");
    assert_eq!(closed, json!({ "closed": [first, second], "failed": [] }));
    assert_eq!(
        app.core.store.crew_task(&first).unwrap().unwrap().state,
        "cancelled"
    );
    assert_eq!(
        app.core.store.crew_task(&second).unwrap().unwrap().state,
        "cancelled"
    );
    assert_eq!(
        app.core.store.crew_task(&ended).unwrap().unwrap().state,
        "stopped"
    );
    assert_eq!(
        app.core.store.crew_task(&elsewhere).unwrap().unwrap().state,
        "routed"
    );
    let (status, live) = app.call("GET", &app.live_path(&thread), None).await;
    assert_eq!(status, 200);
    assert_eq!(live, json!({ "tasks": [] }));

    // The thread's own settle is the person's route and still lands.
    let path = format!("/api/threads/{thread}");
    let (status, settled) = app
        .call("PATCH", &path, Some(json!({ "settled": true })))
        .await;
    assert_eq!(status, 200, "{settled}");
    assert!(settled["lifecycle"]["settled_at"].as_str().is_some());
}

/// `PATCH /api/threads/{id}` is not a Person route, so a settle or an archive through it must never stop a task: only the
/// confirmed `POST .../close` (a Person route) does.
#[tokio::test]
async fn the_thread_route_alone_cancels_nothing() {
    let app = serve("thread", true, false).await;
    let thread = app.thread("Lead");
    let first = app.task("archive-a", &thread, None);
    let second = app.task("archive-b", &thread, Some("routed"));
    let path = format!("/api/threads/{thread}");

    let (status, archived) = app
        .call("PATCH", &path, Some(json!({ "archived": true })))
        .await;
    assert_eq!(status, 200, "{archived}");
    assert!(archived["lifecycle"]["archived_at"].as_str().is_some());
    for task in [&first, &second] {
        assert_ne!(
            app.core.store.crew_task(task).unwrap().unwrap().state,
            "cancelled",
            "{task}"
        );
    }
    let cancelled = app
        .core
        .store
        .list_audit(None, 100)
        .unwrap()
        .into_iter()
        .filter(|row| row.action.starts_with("crew.task.cancel"))
        .count();
    assert_eq!(cancelled, 0, "the patch wrote no cancel");
}

#[tokio::test]
async fn with_the_flag_off_both_routes_are_not_there() {
    let app = serve("off", false, false).await;
    let (_, unknown) = app.call("GET", "/api/crew-nothing", None).await;
    for (method, path) in [
        ("GET", "/api/crew/lead/t-1/live"),
        ("POST", "/api/crew/lead/t-1/close"),
    ] {
        let (status, body) = app.call(method, path, None).await;
        assert_eq!(status, 404, "{method} {path}");
        assert_eq!(
            body, unknown,
            "{method} {path} answers as an absent route does"
        );
    }
}

#[tokio::test]
async fn a_remote_viewer_reads_the_live_tasks_and_cannot_close() {
    let app = serve("remote", true, true).await;
    let thread = app.thread("Lead");
    app.task("remote-a", &thread, None);
    let (status, live) = app.call("GET", &app.live_path(&thread), None).await;
    assert_eq!(status, 200, "{live}");
    assert_eq!(live["tasks"].as_array().unwrap().len(), 1);
    let (status, body) = app.call("POST", &app.close_path(&thread), None).await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["code"], "local_only");
}

#[tokio::test]
async fn a_bad_thread_id_is_refused() {
    let app = serve("ids", true, false).await;
    let (status, body) = app.call("GET", "/api/crew/lead/a%20b/live", None).await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["code"], "invalid_input");
    let (status, body) = app.call("POST", "/api/crew/lead/a%20b/close", None).await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["code"], "invalid_input");
}
