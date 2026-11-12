//! Background crews over HTTP: the whole life of one crew through the real router, what a bad body is refused with, the budget stop
//! seen from the routes, and that nothing a route does reaches a repository, a merge or an engine. The table test in `crew_tests.rs`
//! walks every route with the flag off, checks the marker and the guards, and refuses a remote viewer's change.

use std::path::PathBuf;
use std::sync::Arc;

use orbyn_core::crew::changes::changes_crew_state;
use orbyn_core::crew::fixtures;
use orbyn_core::{Core, CoreConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::router;
use crate::http::AppState;
use crate::http::access::Access;

struct Served {
    port: u16,
    core: Arc<Core>,
    project: String,
    dir: PathBuf,
}

async fn serve(name: &str) -> Served {
    let dir = std::env::temp_dir().join(format!("orbynd-bg-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut config = CoreConfig::new(dir.join("data"), "http://127.0.0.1:1".into());
    config.crew_preview = true;
    let core = Arc::new(Core::open(config).unwrap());
    let project = fixtures::add_project(&core, dir.join("repo").to_str().unwrap());
    let app = router(AppState {
        core: core.clone(),
        access: Arc::new(Access::new("token", vec![], None)),
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
        project,
        dir,
    }
}

impl Served {
    async fn call(&self, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
        let url = format!("http://127.0.0.1:{}{path}", self.port);
        let mut request = reqwest::Client::new()
            .request(method.parse().unwrap(), url)
            .bearer_auth("token");
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    fn body(&self, name: &str) -> Value {
        json!({
            "project_id": self.project,
            "name": name,
            "template": "dependency_bump",
            "trigger": { "kind": "dependency_update", "ecosystem": "cargo" },
            "budget": { "turns": 8, "minutes": 20 }
        })
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[tokio::test]
async fn a_crew_lives_its_whole_life_over_http() {
    let app = serve("life").await;
    let (status, empty) = app.call("GET", "/api/crew/background", None).await;
    assert_eq!((status, empty["total"].clone()), (200, json!(0)));

    let (status, crew) = app
        .call(
            "POST",
            "/api/crew/background/create",
            Some(app.body("Bump")),
        )
        .await;
    assert_eq!(status, 200, "{crew}");
    assert_eq!(crew["enabled"], false);
    assert_eq!(crew["state"], "idle");
    let id = crew["id"].as_str().unwrap().to_owned();

    let (_, listed) = app
        .call(
            "GET",
            &format!("/api/crew/background?project={}", app.project),
            None,
        )
        .await;
    assert_eq!(listed["crews"][0]["id"], id.as_str());
    let (status, got) = app
        .call("GET", &format!("/api/crew/background/{id}"), None)
        .await;
    assert_eq!((status, &got), (200, &crew));

    let (status, renamed) = app
        .call(
            "POST",
            &format!("/api/crew/background/{id}/update"),
            Some(json!({ "name": "Bump deps", "trigger": { "kind": "schedule", "every_hours": 24 } })),
        )
        .await;
    assert_eq!(status, 200, "{renamed}");
    assert_eq!(renamed["name"], "Bump deps");
    assert_eq!(renamed["trigger"]["every_hours"], 24);
    assert_eq!(renamed["budget"], crew["budget"]);

    let (_, on) = app
        .call("POST", &format!("/api/crew/background/{id}/enable"), None)
        .await;
    assert_eq!(on["enabled"], true);

    // Triggers are not fired: nothing runs until the core is told. Drive a run to its budget as slice 4h will.
    let (_, runs) = app
        .call("GET", &format!("/api/crew/background/{id}/runs"), None)
        .await;
    assert_eq!(runs["total"], 0, "enabling starts nothing");
    let run = app.core.crew_background_start_run(&id).unwrap();
    let (status, delete) = app
        .call("POST", &format!("/api/crew/background/{id}/delete"), None)
        .await;
    assert_eq!(status, 409, "{delete}");
    assert_eq!(delete["code"], "runs_active");
    app.core.crew_background_spend(&run.id, 8, 0).unwrap();
    let (_, runs) = app
        .call("GET", &format!("/api/crew/background/{id}/runs"), None)
        .await;
    let first = &runs["runs"][0];
    assert_eq!(first["state"], "stopped");
    assert_eq!(first["stop_reason"], "budget");
    assert_eq!(first["review_waiting"], true);
    assert_eq!(
        (
            first["turns_spent"].clone(),
            first["budget"]["turns"].clone()
        ),
        (json!(8), json!(8))
    );
    let (_, crew) = app
        .call("GET", &format!("/api/crew/background/{id}"), None)
        .await;
    assert_eq!(
        (crew["state"].as_str(), crew["reviews_waiting"].clone()),
        (Some("stopped"), json!(1))
    );

    let (_, off) = app
        .call("POST", &format!("/api/crew/background/{id}/disable"), None)
        .await;
    assert_eq!(off["enabled"], false);
    let (status, gone) = app
        .call("POST", &format!("/api/crew/background/{id}/delete"), None)
        .await;
    assert_eq!((status, gone), (200, json!({ "deleted": true })));
    let (status, _) = app
        .call("GET", &format!("/api/crew/background/{id}"), None)
        .await;
    assert_eq!(status, 404);

    // Every change left an audit row.
    let audit = fixtures::audit_actions(&app.core, "crew.background");
    assert_eq!(
        audit,
        [
            "crew.background.create",
            "crew.background.update",
            "crew.background.enable",
            "crew.background.run.start",
            "crew.background.run.stop",
            "crew.background.disable",
            "crew.background.delete"
        ]
    );
    // Rule 4: nothing was written into the repository.
    assert!(!app.dir.join("repo").exists());
}

#[tokio::test]
async fn a_bad_request_is_refused_and_stores_nothing() {
    let app = serve("bad").await;
    let (status, crew) = app
        .call(
            "POST",
            "/api/crew/background/create",
            Some(app.body("Good")),
        )
        .await;
    assert_eq!(status, 200);
    let id = crew["id"].as_str().unwrap();
    let mut over = app.body("Over");
    over["budget"] = json!({ "turns": 1001, "minutes": 5 });
    let mut unknown_template = app.body("Template");
    unknown_template["template"] = json!("deploy");
    let mut unknown_trigger = app.body("Trigger");
    unknown_trigger["trigger"] = json!({ "kind": "webhook" });
    let mut extra = app.body("Extra");
    extra["enabled"] = json!(true);
    let mut foreign = app.body("Foreign");
    foreign["trigger"] = json!({ "kind": "dependency_update", "label": "x" });
    let mut no_project = app.body("Nowhere");
    no_project["project_id"] = json!("missing");
    for (body, want) in [
        (over, 400),
        (unknown_template, 400),
        (unknown_trigger, 400),
        (extra, 400),
        (foreign, 400),
        (no_project, 404),
        (app.body("good"), 400),
        (app.body(""), 400),
    ] {
        let (status, answer) = app
            .call("POST", "/api/crew/background/create", Some(body.clone()))
            .await;
        assert_eq!(status, want, "{body} gave {answer}");
    }
    // The budget has no place in an update.
    let (status, _) = app
        .call(
            "POST",
            &format!("/api/crew/background/{id}/update"),
            Some(json!({ "budget": { "turns": 999, "minutes": 1 } })),
        )
        .await;
    assert_eq!(status, 400);
    let (status, _) = app
        .call(
            "POST",
            &format!("/api/crew/background/{id}/update"),
            Some(json!({})),
        )
        .await;
    assert_eq!(status, 400);
    for path in [
        "/api/crew/background/..%2Fetc",
        "/api/crew/background/a%20b/runs",
        "/api/crew/background?limit=0",
        "/api/crew/background?limit=201",
        "/api/crew/background?offset=x",
        "/api/crew/background?project=..%2F",
        &format!("/api/crew/background/{id}/runs?limit=0"),
    ] {
        let (status, body) = app.call("GET", path, None).await;
        assert_eq!(status, 400, "{path}");
        assert_eq!(body["code"], "invalid_input");
    }
    for method_path in ["enable", "disable", "delete", "update"] {
        let (status, _) = app
            .call(
                "POST",
                &format!("/api/crew/background/nothing/{method_path}"),
                Some(json!({ "name": "x" })),
            )
            .await;
        assert_eq!(status, 404, "{method_path}");
    }
    let (_, listed) = app.call("GET", "/api/crew/background", None).await;
    assert_eq!(listed["total"], 1, "only the good crew was stored");
    // No client path starts a run, merges or reverts: the routes that exist are the listed ones, and the wrong methods are not routes.
    for (method, path) in [
        ("PUT", format!("/api/crew/background/{id}")),
        ("DELETE", format!("/api/crew/background/{id}")),
        ("POST", format!("/api/crew/background/{id}")),
        ("POST", format!("/api/crew/background/{id}/runs")),
        ("POST", format!("/api/crew/background/{id}/start")),
        ("POST", format!("/api/crew/background/{id}/runs/start")),
        ("POST", "/api/crew/background".to_owned()),
    ] {
        let (status, _) = app.call(method, &path, Some(json!({}))).await;
        assert!(matches!(status, 404 | 405), "{method} {path} gave {status}");
    }
}

#[tokio::test]
async fn a_secret_in_free_text_never_comes_back() {
    let app = serve("secret").await;
    let secret = format!("ghp_{}", "z".repeat(36));
    let mut body = app.body(&format!("Crew {secret}"));
    body["trigger"] = json!({ "kind": "issue_label", "label": format!("x {secret}") });
    let (status, crew) = app
        .call("POST", "/api/crew/background/create", Some(body))
        .await;
    assert_eq!(status, 200);
    assert!(!crew.to_string().contains(&secret));
    let (_, listed) = app.call("GET", "/api/crew/background", None).await;
    assert!(!listed.to_string().contains(&secret));
}

#[test]
fn an_agent_is_refused_every_background_change_by_path() {
    for path in [
        "/api/crew/background/create",
        "/api/crew/background/t-1/update",
        "/api/crew/background/t-1/enable",
        "/api/crew/background/t-1/disable",
        "/api/crew/background/t-1/delete",
        "/api/CREW/background//t-1/./enable",
        "/api/crew/background/x/../t-1/delete",
        "/api/crew/background/t-1/start",
        "/api/crew/background/t-1/runs/start",
    ] {
        assert!(changes_crew_state(path), "{path}");
    }
    for path in [
        "/api/crew/background",
        "/api/crew/background/t-1",
        "/api/crew/background/t-1/runs",
    ] {
        assert!(!changes_crew_state(path), "{path}");
    }
}
