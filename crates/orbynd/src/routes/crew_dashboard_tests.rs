//! The Crew dashboard read over HTTP (W12, row G18): the Preview gate, the local-only rule, the credential, the shape of the request
//! and the empty answer for a repository the person has never used Crew in. The figures themselves are tested in `orbyn-core`
//! (`crew/tests/dashboard.rs`), and the route's own documentation row in `crates/orbynd/tests/routes.json`.

use std::path::PathBuf;
use std::sync::Arc;

use orbyn_core::crew::fixtures;
use orbyn_core::{Core, CoreConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::router;
use crate::http::AppState;
use crate::http::access::Access;

const DASHBOARD: &str = "/api/crew/dashboard";
/// The project menu's counts: the same family, one path segment further in.
const PROJECTS: &str = "/api/crew/dashboard/projects";

struct Served {
    port: u16,
    core: Arc<Core>,
    dir: PathBuf,
}

async fn serve(name: &str, preview: bool, remote: bool) -> Served {
    let dir = std::env::temp_dir().join(format!("orbynd-dashboard-{name}-{}", std::process::id()));
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

impl Drop for Served {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Served {
    async fn get(&self, path: &str, token: bool) -> (u16, Value) {
        let url = format!("http://127.0.0.1:{}{path}", self.port);
        let mut request = reqwest::Client::new().request(reqwest::Method::GET, url);
        if token {
            request = request.bearer_auth("token");
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let body = response.json().await.unwrap_or(Value::Null);
        (status, body)
    }
}

#[tokio::test]
async fn the_read_needs_the_flag_the_token_and_reads_from_another_computer() {
    let off = serve("off", false, false).await;
    let (status, body) = off.get(&format!("{DASHBOARD}?project=p"), true).await;
    assert_eq!((status, &body["code"]), (404, &json!("not_found")));

    // A read is not a write: it is not behind `local_only`, so a remote viewer may read it. The unknown project is
    // not found, which proves the request got past the gate and the caller check.
    let remote = serve("remote", true, true).await;
    let (status, body) = remote.get(&format!("{DASHBOARD}?project=p"), true).await;
    assert_eq!((status, &body["code"]), (404, &json!("not_found")));

    let on = serve("token", true, false).await;
    let (status, _) = on.get(&format!("{DASHBOARD}?project=p"), false).await;
    assert_eq!(status, 401);
}

#[tokio::test]
async fn an_empty_repository_answers_zeros_and_no_figures_it_cannot_measure() {
    let app = serve("empty", true, false).await;
    let project = fixtures::add_project(&app.core, app.dir.join("project").to_str().unwrap());
    let (status, answer) = app
        .get(&format!("{DASHBOARD}?project={project}"), true)
        .await;
    assert_eq!(
        (
            status,
            &answer["merges"],
            &answer["unaided"],
            &answer["reverted"],
            &answer["days"]
        ),
        (200, &json!(0), &json!(0), &json!(0), &json!(60))
    );
    assert!(answer["as_of"].as_i64().unwrap_or_default() > 0);
    assert_eq!(
        (&answer["min_rate_sample"], &answer["min_median_sample"]),
        (&json!(10), &json!(5))
    );
    assert!(
        answer["cycle"].is_null(),
        "a zero would be a made-up figure"
    );
    assert_eq!(answer["weeks"], json!([]));
    assert_eq!(answer["truncated"], json!(false));
    // KC3: the live row is served even when nothing waits and nothing works, and it is not a windowed figure.
    assert_eq!(
        answer["live"],
        json!({ "needs_you": [], "working": [], "truncated": false })
    );
    assert_eq!(answer["project"], json!(project));
    // The week's local Monday: a person an hour ahead of UTC gets a start that is a whole hour off UTC midnight.
    let (status, shifted) = app
        .get(
            &format!("{DASHBOARD}?project={project}&utc_offset_minutes=600"),
            true,
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(shifted["weeks"], json!([]));
}

#[tokio::test]
async fn a_store_that_cannot_be_read_answers_history_unreadable() {
    let app = serve("unreadable", true, false).await;
    let project = fixtures::add_project(&app.core, app.dir.join("project").to_str().unwrap());
    // A damaged database is what the page's "could not be read" state is for: the read fails at
    // the merge query, and the answer carries the document's code and status, not `internal`.
    app.core.store.break_crew_dashboard_for_test();
    let (status, body) = app
        .get(&format!("{DASHBOARD}?project={project}"), true)
        .await;
    assert_eq!((status, &body["code"]), (500, &json!("history_unreadable")));
}

#[tokio::test]
async fn the_request_shape_is_checked_before_anything_is_read() {
    let app = serve("shape", true, false).await;
    let project = fixtures::add_project(&app.core, app.dir.join("project").to_str().unwrap());
    for path in [
        DASHBOARD.to_owned(),
        format!("{DASHBOARD}?project={project}&days=45"),
        format!("{DASHBOARD}?project={project}&days=ninety"),
        format!("{DASHBOARD}?project={project}&utc_offset_minutes=900"),
        format!("{DASHBOARD}?project={project}&utc_offset_minutes=ten"),
        format!("{DASHBOARD}?project=..%2F..%2Fetc"),
    ] {
        let (status, body) = app.get(&path, true).await;
        assert_eq!(
            (status, &body["code"]),
            (400, &json!("invalid_input")),
            "{path}"
        );
    }
    let path = format!("{DASHBOARD}?project=project-that-is-not-here");
    let (status, body) = app.get(&path, true).await;
    assert_eq!(
        (status, &body["code"]),
        (404, &json!("not_found")),
        "{path}"
    );
    let (status, _) = app
        .get(&format!("{DASHBOARD}?project={project}&days=90"), true)
        .await;
    assert_eq!(status, 200, "a 90 day window is one of the three");
}

/// The project menu's count (`/api/crew/dashboard/projects`): the same gate, credential and request shape as the
/// dashboard read, and a row for every repository the service lists. The counts themselves are tested in
/// `orbyn-core` (`crew/tests/dashboard.rs`); here it is the route.
#[tokio::test]
async fn the_projects_read_needs_the_flag_the_token_and_answers_a_row_for_every_repository() {
    let off = serve("projects-off", false, false).await;
    let (status, body) = off.get(PROJECTS, true).await;
    assert_eq!((status, &body["code"]), (404, &json!("not_found")));

    let on = serve("projects", true, false).await;
    let (status, _) = on.get(PROJECTS, false).await;
    assert_eq!(status, 401);

    let project = fixtures::add_project(&on.core, on.dir.join("project").to_str().unwrap());
    let (status, answer) = on.get(PROJECTS, true).await;
    assert_eq!(status, 200);
    assert_eq!(
        answer["days"],
        json!(60),
        "30 or 60 or 90; 60 when none is asked"
    );
    assert!(answer["as_of"].as_i64().unwrap_or_default() > 0);
    assert_eq!(answer["truncated"], json!(false));
    assert_eq!(
        answer["projects"],
        json!([{ "project": project, "merges": 0 }]),
        "a repository with no Crew use is a row with no merges, never left out"
    );

    // The request shape: one of the three windows, or nothing.
    for path in [
        format!("{PROJECTS}?days=45"),
        format!("{PROJECTS}?days=ninety"),
    ] {
        let (status, body) = on.get(&path, true).await;
        assert_eq!(
            (status, &body["code"]),
            (400, &json!("invalid_input")),
            "{path}"
        );
    }
    let (status, answer) = on.get(&format!("{PROJECTS}?days=90"), true).await;
    assert_eq!((status, &answer["days"]), (200, &json!(90)));
}
