//! Background crews over HTTP: the whole life of one crew through the real router, what a bad body is refused with, the budget stop
//! seen from the routes, and that nothing a route does reaches a repository, a merge or an engine. The table test in `crew_tests.rs`
//! walks every route with the flag off, checks the marker and the guards, and refuses a remote viewer's change.

use std::path::PathBuf;
use std::sync::Arc;

use orbyn_contracts as wire;
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
    serve_with(
        name,
        Access::new("token", vec![], None).with_dev_person_override(true),
    )
    .await
}

/// A service whose person key is this launch's only way into a `Person` route, as in a build the person uses.
async fn serve_keyed(name: &str) -> Served {
    serve_with(
        name,
        Access::new("token", vec![], None).with_person_key(Some("the-key")),
    )
    .await
}

async fn serve_with(name: &str, access: Access) -> Served {
    let dir = std::env::temp_dir().join(format!("orbynd-bg-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut config = CoreConfig::new(dir.join("data"), "http://127.0.0.1:1".into());
    config.crew_preview = true;
    let core = Arc::new(Core::open(config).unwrap());
    let project = fixtures::add_project(&core, dir.join("repo").to_str().unwrap());
    let app = router(AppState {
        core: core.clone(),
        access: Arc::new(access),
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
        self.call_as(method, path, body, Some("token"), None).await
    }

    /// One request, with the service token and the person key when each is given.
    async fn call_as(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        token: Option<&str>,
        key: Option<&str>,
    ) -> (u16, Value) {
        let url = format!("http://127.0.0.1:{}{path}", self.port);
        let mut request = reqwest::Client::new().request(method.parse().unwrap(), url);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        if let Some(key) = key {
            request = request.header("x-orbyn-person", key);
        }
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
            "budget": { "turns": 8, "minutes": 20 },
            "daily": { "runs": 2, "turns": 20 },
            "utc_offset_minutes": 600
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
    assert_eq!(renamed["daily"], crew["daily"]);

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
            "crew.background.run.spend",
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
    let mut over_daily = app.body("Over daily");
    over_daily["daily"] = json!({ "runs": 1, "turns": 10_001 });
    let mut far_clock = app.body("Far clock");
    far_clock["utc_offset_minutes"] = json!(841);
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
        (over_daily, 400),
        (far_clock, 400),
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
    // Run now is the one route that starts a run; nothing merges, reverts or spends from here, and the wrong methods are not routes.
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

/// A watch is stored with its command and action, and a command the screen refuses comes back as the rule's sentence, never the
/// command itself, at create and update.
#[tokio::test]
async fn a_watch_is_stored_and_a_refused_command_keeps_the_command_out_of_the_error() {
    let app = serve("watch").await;
    let mut body = app.body("Watch");
    body["trigger"] = json!({
        "kind": "watch", "command": "cargo test", "every_hours": 6, "on_failing": "needs_you"
    });
    let (status, crew) = app
        .call("POST", "/api/crew/background/create", Some(body))
        .await;
    assert_eq!(status, 200, "{crew}");
    assert_eq!(crew["trigger"]["kind"], "watch");
    assert_eq!(crew["trigger"]["command"], "cargo test");
    assert_eq!(crew["trigger"]["every_hours"], 6);
    assert_eq!(crew["trigger"]["on_failing"], "needs_you");

    let mut refused = app.body("Force");
    refused["trigger"] = json!({
        "kind": "watch", "command": "git push --force origin main", "every_hours": 6, "on_failing": "template"
    });
    let (status, answer) = app
        .call("POST", "/api/crew/background/create", Some(refused))
        .await;
    assert_eq!(status, 400, "{answer}");
    assert_eq!(answer["code"], "invalid_input");
    assert_eq!(
        answer["message"],
        "That command is not allowed: force-push, history rewrite or any push to main."
    );
    assert!(!answer.to_string().contains("origin main"));

    let id = crew["id"].as_str().unwrap();
    let (status, answer) = app
        .call(
            "POST",
            &format!("/api/crew/background/{id}/update"),
            Some(json!({
                "trigger": { "kind": "watch", "command": "echo \"unclosed", "every_hours": 6, "on_failing": "needs_you" }
            })),
        )
        .await;
    assert_eq!(status, 400, "{answer}");
    assert_eq!(
        answer["message"],
        "That command is empty or has an open quote."
    );
    assert!(!answer.to_string().contains("unclosed"));
    let (_, stored) = app
        .call("GET", &format!("/api/crew/background/{id}"), None)
        .await;
    assert_eq!(stored["trigger"]["command"], "cargo test");
    let (_, listed) = app.call("GET", "/api/crew/background", None).await;
    assert_eq!(listed["total"], 1, "the refusals stored nothing");
}

/// Run now starts one run by hand: the route hands the core's own refusal and plain reason back, and every refusal leaves no
/// run behind.
#[tokio::test]
async fn run_now_refuses_what_the_core_refuses_with_the_cores_reason() {
    let app = serve("run-now").await;
    let (status, crew) = app
        .call(
            "POST",
            "/api/crew/background/create",
            Some(app.body("Bump")),
        )
        .await;
    assert_eq!(status, 200, "{crew}");
    let id = crew["id"].as_str().unwrap().to_owned();

    // Off: nothing may start it, and the refusal is the core's own sentence.
    let (status, body) = app
        .call("POST", &format!("/api/crew/background/{id}/run-now"), None)
        .await;
    assert_eq!(
        (status, &body["code"]),
        (403, &json!("policy_denied")),
        "{body}"
    );
    assert_eq!(
        body["message"],
        "This background crew is off. Turn it on, then start a run."
    );

    // On, but a dependency update has no listener, so nothing can start it yet.
    assert_eq!(
        app.call("POST", &format!("/api/crew/background/{id}/enable"), None)
            .await
            .0,
        200
    );
    let (status, body) = app
        .call("POST", &format!("/api/crew/background/{id}/run-now"), None)
        .await;
    assert_eq!(
        (status, &body["code"]),
        (403, &json!("policy_denied")),
        "{body}"
    );
    assert_eq!(
        body["message"],
        "A dependency update crew cannot run yet: Orbyn has nothing that can start it."
    );

    // A crew that is not there is refused before anything is read.
    let (status, body) = app
        .call("POST", "/api/crew/background/nothing/run-now", None)
        .await;
    assert_eq!((status, &body["code"]), (404, &json!("not_found")));

    // An enabled crew of a kind that can start reaches the full guard chain, and an untrusted project is refused first.
    let mut body = app.body("Fix CI");
    body["template"] = json!("fix_failing_ci");
    body["trigger"] = json!({ "kind": "ci_failure", "branch": "main" });
    let (status, crew) = app
        .call("POST", "/api/crew/background/create", Some(body))
        .await;
    assert_eq!(status, 200, "{crew}");
    let id = crew["id"].as_str().unwrap().to_owned();
    let (status, _) = app
        .call("POST", &format!("/api/crew/background/{id}/enable"), None)
        .await;
    assert_eq!(status, 200);
    let (status, body) = app
        .call("POST", &format!("/api/crew/background/{id}/run-now"), None)
        .await;
    assert_eq!(
        (status, &body["code"]),
        (403, &json!("folder_untrusted")),
        "{body}"
    );
    // No refusal left a run behind.
    let (_, runs) = app
        .call("GET", &format!("/api/crew/background/{id}/runs"), None)
        .await;
    assert_eq!(runs["total"], 0);
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
        "/api/crew/background/t-1/run-now",
        "/api/crew/background/runs/t-1/acknowledge",
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

/// A crew started by hand and stopped at its budget: one run waits for the person.
async fn waiting_run(app: &Served) -> (String, String) {
    let body: wire::BackgroundCrewCreate = serde_json::from_value(app.body("Settled")).unwrap();
    let crew = app.core.background_create(&body).await.unwrap();
    app.core.background_set_enabled(&crew.id, true).unwrap();
    let run = app.core.crew_background_start_run(&crew.id).unwrap();
    let waiting = app.core.crew_background_spend(&run.id, 8, 0).unwrap();
    assert!(waiting.review_waiting);
    (crew.id, run.id)
}

/// The person settles a waiting review over HTTP: the wait clears, the run keeps its reason, the crew reads nothing waiting,
/// Review drops it, and the same crew starts again.
#[tokio::test]
async fn a_person_acknowledges_a_waiting_review_and_the_crew_may_run_again() {
    let app = serve("acknowledge").await;
    let (crew_id, run_id) = waiting_run(&app).await;
    let crew_path = format!("/api/crew/background/{crew_id}");
    // Unacknowledged: the crew counts it and Review lists it.
    let (_, crew) = app.call("GET", &crew_path, None).await;
    assert_eq!(
        (crew["state"].as_str(), crew["reviews_waiting"].clone()),
        (Some("stopped"), json!(1))
    );
    let (_, queue) = app.call("GET", "/api/crew/autopilot/review", None).await;
    assert_eq!(queue["background_runs"].as_array().unwrap().len(), 1);

    let path = format!("/api/crew/background/runs/{run_id}/acknowledge");
    let (status, settled) = app.call("POST", &path, None).await;
    assert_eq!(status, 200, "{settled}");
    assert_eq!(settled["id"], run_id.as_str());
    assert_eq!(settled["review_waiting"], false);
    assert_eq!(settled["state"], "stopped");
    assert_eq!(settled["stop_reason"], "budget");
    assert!(
        settled["review_reason"]
            .as_str()
            .unwrap()
            .contains("budget"),
        "the run keeps why it waited: {settled}"
    );

    // The crew has nothing waiting, Review drops the run, and the run's own page keeps the reason.
    let (_, crew) = app.call("GET", &crew_path, None).await;
    assert_eq!(
        (crew["state"].as_str(), crew["reviews_waiting"].clone()),
        (Some("stopped"), json!(0))
    );
    let (_, queue) = app.call("GET", "/api/crew/autopilot/review", None).await;
    assert_eq!(queue["background_runs"], json!([]));
    let (_, runs) = app.call("GET", &format!("{crew_path}/runs"), None).await;
    assert_eq!(runs["runs"][0]["review_waiting"], false);
    assert!(
        runs["runs"][0]["review_reason"]
            .as_str()
            .unwrap()
            .contains("budget")
    );
    // The same crew starts again, at once.
    let next = app.core.crew_background_start_run(&crew_id).unwrap();
    assert_eq!(next.state, wire::BackgroundRunState::Running);
    // One person audit row records what was settled.
    let audit = fixtures::audit_actions(&app.core, "crew.background");
    assert_eq!(
        audit
            .iter()
            .filter(|action| action.as_str() == "crew.background.run.acknowledge")
            .count(),
        1,
        "{audit:?}"
    );
}

/// A run that does not wait has nothing to settle: the refusal is plain and nothing changes.
#[tokio::test]
async fn acknowledging_a_run_that_is_not_waiting_is_refused_plainly() {
    let app = serve("acknowledge-refused").await;
    let (status, body) = app
        .call(
            "POST",
            "/api/crew/background/runs/nothing/acknowledge",
            None,
        )
        .await;
    assert_eq!((status, &body["code"]), (404, &json!("not_found")));

    let body: wire::BackgroundCrewCreate = serde_json::from_value(app.body("Running")).unwrap();
    let crew = app.core.background_create(&body).await.unwrap();
    app.core.background_set_enabled(&crew.id, true).unwrap();
    let run = app.core.crew_background_start_run(&crew.id).unwrap();
    let path = format!("/api/crew/background/runs/{}/acknowledge", run.id);
    let (status, body) = app.call("POST", &path, None).await;
    assert_eq!((status, &body["code"]), (400, &json!("invalid_input")));
    assert_eq!(
        body["message"],
        "this run is not waiting for the person's review"
    );
    // Wrong method is not a route.
    let (status, _) = app.call("GET", &path, None).await;
    assert!(matches!(status, 404 | 405), "{status}");
    // Nothing changed and nothing was audited.
    let (_, runs) = app
        .call(
            "GET",
            &format!("/api/crew/background/{}/runs", crew.id),
            None,
        )
        .await;
    assert_eq!(runs["runs"][0]["state"], "running");
    let audit = fixtures::audit_actions(&app.core, "crew.background");
    assert!(
        audit
            .iter()
            .all(|action| action.as_str() != "crew.background.run.acknowledge"),
        "{audit:?}"
    );
}

/// The acknowledgement is a `Person` route: the service token alone, or a wrong key, is refused and changes nothing.
#[tokio::test]
async fn acknowledging_needs_the_person_key_and_changes_nothing_without_it() {
    let app = serve_keyed("acknowledge-key").await;
    let (crew_id, run_id) = waiting_run(&app).await;
    let path = format!("/api/crew/background/runs/{run_id}/acknowledge");
    let (status, _) = app.call_as("POST", &path, None, None, None).await;
    assert_eq!(status, 401, "the service token comes first");
    for key in [None, Some("wrong")] {
        let (status, body) = app.call_as("POST", &path, None, Some("token"), key).await;
        assert_eq!(
            (status, &body["code"]),
            (403, &json!("person_required")),
            "{key:?}"
        );
    }
    // Every refused call left the review waiting and wrote no row.
    let (_, crew) = app
        .call("GET", &format!("/api/crew/background/{crew_id}"), None)
        .await;
    assert_eq!(crew["reviews_waiting"], json!(1));
    assert!(
        fixtures::audit_actions(&app.core, "crew.background")
            .iter()
            .all(|action| action.as_str() != "crew.background.run.acknowledge")
    );
    // The key this launch holds settles it.
    let (status, settled) = app
        .call_as("POST", &path, None, Some("token"), Some("the-key"))
        .await;
    assert_eq!(status, 200, "{settled}");
    assert_eq!(settled["review_waiting"], false);
    let (_, crew) = app
        .call("GET", &format!("/api/crew/background/{crew_id}"), None)
        .await;
    assert_eq!(crew["reviews_waiting"], json!(0));
}
