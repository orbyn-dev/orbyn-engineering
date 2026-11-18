//! The reviewer backtest over HTTP (slice 4n PR 3): the Preview gate, who may read and who may start or stop, the shape of a
//! request, one run at a time and what a response holds. The rules, the budgets and the scoring are tested in `orbyn-core`
//! (`crew/tests/backtest_api.rs`, `crew/backtest/*`). No engine runs: a start is refused until a replay engine is connected,
//! and these tests connect a scripted one.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use orbyn_core::crew::backtest::ScriptedRunner;
use orbyn_core::crew::changes::{READS, STATE_CHANGING, changes_crew_state};
use orbyn_core::crew::fixtures;
use orbyn_core::store::BacktestState;
use orbyn_core::{Core, CoreConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::router;
use crate::http::AppState;
use crate::http::access::Access;

const START: &str = "/api/crew/backtest/start";

struct Served {
    port: u16,
    core: Arc<Core>,
    project: String,
    dir: PathBuf,
}

async fn serve(name: &str, preview: bool, remote: bool) -> Served {
    let dir = std::env::temp_dir().join(format!("orbynd-backtest-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut config = CoreConfig::new(dir.clone(), "http://127.0.0.1:1".into());
    config.crew_preview = preview;
    config.crew_confine_workers = false;
    let core = Arc::new(Core::open(config).unwrap());
    let project = fixtures::add_project(&core, "/project");
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
    Served {
        port,
        core,
        project,
        dir,
    }
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

    async fn get(&self, path: &str) -> (u16, Value) {
        self.call("GET", path, None, true).await
    }

    async fn post(&self, path: &str, body: Value) -> (u16, Value) {
        self.call("POST", path, Some(body), true).await
    }

    fn request(&self) -> Value {
        json!({
            "project": self.project,
            "lanes": [{ "lane": "security", "engine": "a", "model": "m", "kind": "review" }],
            "budgets": [{ "engine": "a", "tokens": 100 }],
        })
    }

    fn estimate_path(&self) -> String {
        format!(
            "/api/crew/backtest/estimate?project={}&lane=security&engine=a&model=m",
            self.project
        )
    }
}

#[tokio::test]
async fn with_the_flag_off_the_routes_are_not_there() {
    let off = serve("off", false, false).await;
    for (method, path) in [
        ("GET", off.estimate_path()),
        ("GET", "/api/crew/backtest/run-1/report".to_owned()),
        ("POST", START.to_owned()),
        ("POST", "/api/crew/backtest/run-1/stop".to_owned()),
    ] {
        let (status, body) = off.call(method, &path, Some(off.request()), true).await;
        assert_eq!(
            (status, &body["code"]),
            (404, &json!("not_found")),
            "{method} {path}"
        );
    }
}

#[tokio::test]
async fn the_credential_is_needed_and_a_start_or_stop_is_a_persons_not_an_agents() {
    let app = serve("auth", true, false).await;
    for (method, path) in [
        ("GET", app.estimate_path()),
        ("POST", START.to_owned()),
        ("POST", "/api/crew/backtest/run-1/stop".to_owned()),
    ] {
        let (status, _) = app.call(method, &path, Some(app.request()), false).await;
        assert_eq!(status, 401, "{method} {path}");
    }
    // An agent's request to a start or a stop is refused by path, as any state change is; the reads are not.
    for path in ["/api/crew/backtest/start", "/api/crew/backtest/run-1/stop"] {
        assert!(changes_crew_state(path), "{path}");
    }
    for path in [
        "/api/crew/backtest/estimate",
        "/api/crew/backtest/run-1/report",
    ] {
        assert!(!changes_crew_state(path), "{path}");
    }
    assert!(READS.contains(&("GET", "/api/crew/backtest/estimate")));
    assert!(READS.contains(&("GET", "/api/crew/backtest/{id}/report")));
    assert!(STATE_CHANGING.contains(&("POST", "/api/crew/backtest/start")));
    assert!(STATE_CHANGING.contains(&("POST", "/api/crew/backtest/{id}/stop")));
}

#[tokio::test]
async fn a_remote_viewer_reads_and_cannot_start_or_stop() {
    let remote = serve("remote", true, true).await;
    let (status, body) = remote.get(&remote.estimate_path()).await;
    assert_eq!(status, 200, "{body}");
    let (status, _) = remote.get("/api/crew/backtest/run-1/report").await;
    assert_eq!(status, 404, "an unknown run is not found, not forbidden");
    for path in [START, "/api/crew/backtest/run-1/stop"] {
        let (status, body) = remote.post(path, remote.request()).await;
        assert_eq!(
            (status, &body["code"]),
            (403, &json!("local_only")),
            "{path}"
        );
    }
    assert_eq!(remote.core.store.crew_backtest_running().unwrap(), None);
}

#[tokio::test]
async fn the_estimate_is_read_only_and_says_outcomes_are_not_measured() {
    let app = serve("estimate", true, false).await;
    let (status, body) = app.get(&app.estimate_path()).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["project"], json!(app.project));
    assert_eq!(
        (&body["history_days"], &body["outcome_days"]),
        (&json!(180), &json!(30))
    );
    assert_eq!(body["eligible_prs"], 0);
    assert_eq!(body["outcomes_measured"], false);
    assert_eq!(body["running"], Value::Null);
    assert_eq!(body["lanes"][0]["replays"], 0);
    // Nothing was written.
    assert_eq!(
        app.core
            .store
            .crew_backtest_runs_of(&app.project)
            .unwrap()
            .len(),
        0
    );
    // The query is read strictly.
    for path in [
        "/api/crew/backtest/estimate".to_owned(),
        format!("/api/crew/backtest/estimate?project={}&lane=c", app.project),
        format!(
            "/api/crew/backtest/estimate?project={}&engine=a",
            app.project
        ),
        format!(
            "/api/crew/backtest/estimate?project={}&lane=c&engine=a&engine=b&model=m",
            app.project
        ),
        format!(
            "/api/crew/backtest/estimate?project={}&lane=builder&engine=a&model=m",
            app.project
        ),
        format!("/api/crew/backtest/estimate?project={}&cost=5", app.project),
    ] {
        let (status, _) = app.get(&path).await;
        assert_eq!(status, 400, "{path}");
    }
    let (status, _) = app.get("/api/crew/backtest/estimate?project=nope").await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn a_start_is_refused_until_a_replay_engine_is_connected_and_writes_nothing() {
    let app = serve("noengine", true, false).await;
    let (status, body) = app.post(START, app.request()).await;
    assert_eq!(
        (status, &body["code"]),
        (422, &json!("unsupported")),
        "{body}"
    );
    assert_eq!(
        app.core
            .store
            .crew_backtest_runs_of(&app.project)
            .unwrap()
            .len(),
        0
    );
}

#[tokio::test]
async fn a_start_refuses_money_a_builder_unknown_fields_and_a_missing_budget() {
    let app = serve("shape", true, false).await;
    app.core
        .crew_backtest_use(Arc::new(ScriptedRunner::default()), None);
    let with = |edit: &dyn Fn(&mut Value)| {
        let mut body = app.request();
        edit(&mut body);
        body
    };
    let bad = [
        with(&|b| b["max_cost_usd"] = json!(5)),
        with(&|b| b["lanes"][0]["cost"] = json!(1)),
        with(&|b| b["budgets"][0]["usd"] = json!(1)),
        with(&|b| b["lanes"][0]["kind"] = json!("builder")),
        with(&|b| b["lanes"][0]["lane"] = json!("builder")),
        with(&|b| b["budgets"] = json!([])),
        with(&|b| b["budgets"][0]["tokens"] = json!(0)),
        with(&|b| b["budgets"][0]["tokens"] = json!(-5)),
        with(&|b| b["budgets"][0]["engine"] = json!("price")),
        with(&|b| b["lanes"] = json!([])),
        with(&|b| {
            b["budgets"] = json!([{ "engine": "a", "tokens": 1 }, { "engine": "a", "tokens": 2 }])
        }),
    ];
    for body in bad {
        let (status, response) = app.post(START, body.clone()).await;
        assert_eq!(status, 400, "{body}: {response}");
    }
    let (status, _) = app
        .post(START, with(&|b| b["project"] = json!("nope")))
        .await;
    assert_eq!(status, 404);
    assert_eq!(
        app.core
            .store
            .crew_backtest_runs_of(&app.project)
            .unwrap()
            .len(),
        0
    );
}

#[tokio::test]
async fn one_run_at_a_time_and_a_stop_only_asks() {
    let app = serve("busy", true, false).await;
    app.core
        .crew_backtest_use(Arc::new(ScriptedRunner::default()), None);

    // A run from earlier is still running.
    let budgets: BTreeMap<String, u64> = [("a".to_owned(), 100)].into();
    let lanes = vec![("security".to_owned(), "a".to_owned(), "m".to_owned())];
    app.core
        .store
        .crew_backtest_create(
            "earlier",
            &app.project,
            BacktestState::Running,
            &budgets,
            &lanes,
            1,
        )
        .unwrap();
    let (status, body) = app.post(START, app.request()).await;
    assert_eq!(status, 409, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("already running"),
        "{body}"
    );
    assert_eq!(
        app.core
            .store
            .crew_backtest_runs_of(&app.project)
            .unwrap()
            .len(),
        1
    );
    let (_, estimate) = app.get(&app.estimate_path()).await;
    assert_eq!(estimate["running"], "earlier");

    // A stop only asks: still running, with the reason, so nothing else may start yet.
    let (status, run) = app.post("/api/crew/backtest/earlier/stop", json!({})).await;
    assert_eq!(status, 200, "{run}");
    assert_eq!(run["state"], "running");
    assert_eq!(run["stop_reason"], "stopped by the person");
    let (status, _) = app.post(START, app.request()).await;
    assert_eq!(status, 409);
    let (status, _) = app.post("/api/crew/backtest/missing/stop", json!({})).await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn a_started_run_is_reported_with_each_lane_not_measured_and_no_total() {
    let app = serve("run", true, false).await;
    app.core
        .crew_backtest_use(Arc::new(ScriptedRunner::default()), None);
    let (status, started) = app.post(START, app.request()).await;
    assert_eq!(status, 200, "{started}");
    assert_eq!(started["state"], "running");
    app.core.crew_backtest_join().await;

    let id = started["id"].as_str().unwrap();
    let (status, run) = app.get(&format!("/api/crew/backtest/{id}/report")).await;
    assert_eq!(status, 200, "{run}");
    assert_eq!(run["state"], "finished");
    assert_eq!(run["budgets"], json!([{ "engine": "a", "tokens": 100 }]));
    let lane = &run["lanes"][0];
    assert_eq!(lane["state"], "done");
    assert_eq!(lane["outcome"], "not_measured");
    assert_eq!(lane["score"], Value::Null);
    assert_eq!(lane["findings_n"], 0);
    let text = run.to_string().to_lowercase();
    for word in ["total", "cost", "usd", "price", "money"] {
        assert!(!text.contains(word), "the report holds {word}");
    }
    // Now nothing runs, so the next start is accepted.
    let (status, _) = app.post(START, app.request()).await;
    assert_eq!(status, 200);
    app.core.crew_backtest_join().await;
    let (status, _) = app.get("/api/crew/backtest/..%2Fescape/report").await;
    assert!(status == 400 || status == 404, "{status}");
}
