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
            (400, &json!("invalid_request")),
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
