//! The blast-radius guard's routes: the two lists on this device, the read-only test, tightening without a grant and loosening
//! refused without the desktop app's grant. The table test in `crew_tests.rs` already walks every route with the flag off (404 for
//! every method) and checks the marker and the guards; this file drives the answers. The guard's own rules are tested in
//! `orbyn-core` (`crew/tests/autopilot_guard.rs`), where a grant can be recorded; no grant can be minted from here.

use std::path::PathBuf;
use std::sync::Arc;

use orbyn_core::{Core, CoreConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use super::router;
use crate::http::AppState;
use crate::http::access::Access;

struct Served {
    port: u16,
    dir: PathBuf,
}

async fn serve(name: &str, remote: bool, desktop: bool) -> Served {
    let dir = std::env::temp_dir().join(format!("orbynd-guard-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut config = CoreConfig::new(dir.clone(), "http://127.0.0.1:1".into());
    config.crew_preview = true;
    config.crew_confine_workers = false;
    let core = Arc::new(Core::open(config).unwrap());
    if desktop {
        let (lines, answers) = mpsc::unbounded_channel();
        core.shell.attach(lines);
        std::mem::forget(answers);
    }
    let app = router(AppState {
        core,
        access: Arc::new(Access::new("token", vec![], None)),
        remote,
        ui_dir: None,
        images: Arc::new(orbyn_core::browser::remote_images::ImageProxy::default()),
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await });
    Served { port, dir }
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
        body: Value,
        nonce: Option<&str>,
    ) -> (u16, Value) {
        let url = format!("http://127.0.0.1:{}{path}", self.port);
        let mut request = reqwest::Client::new()
            .request(method.parse().unwrap(), url)
            .bearer_auth("token");
        if method == "POST" {
            request = request.json(&body);
        }
        if let Some(nonce) = nonce {
            request = request.header("x-orbyn-confirm", nonce);
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    async fn add(&self, list: &str, pattern: &str) -> (u16, Value) {
        self.call(
            "POST",
            "/api/crew/guard/targets/add",
            json!({ "list": list, "pattern": pattern, "kind": "host" }),
            None,
        )
        .await
    }
}

/// A nonce as the shell mints one, drawn at run time.
fn fresh_nonce() -> String {
    orbyn_core::util::random_token().unwrap()[..32].to_owned()
}

#[tokio::test]
async fn tightening_works_and_loosening_is_refused_without_the_desktop_grant() {
    let app = serve("lists", false, true).await;
    let (status, body) = app
        .call("GET", "/api/crew/guard/targets", json!({}), None)
        .await;
    assert_eq!(
        (status, &body["items"], &body["max_entries"]),
        (200, &json!([]), &json!(200))
    );

    // Adding a prod entry tightens: no grant.
    let (status, prod) = app.add("prod", "DB.prod.internal").await;
    assert_eq!(status, 200, "{prod}");
    assert_eq!(prod["pattern"], "db.prod.internal");
    assert_eq!(prod["list"], "prod");

    // Adding a safe entry, and removing a prod one, loosen: the desktop's grant is needed, and no grant exists.
    let (status, body) = app.add("safe", "stage.internal").await;
    assert_eq!((status, &body["code"]), (403, &json!("presence_required")));
    let (status, body) = app
        .call(
            "POST",
            &format!(
                "/api/crew/guard/targets/{}/remove",
                prod["id"].as_str().unwrap()
            ),
            json!({}),
            Some(&fresh_nonce()),
        )
        .await;
    assert_eq!((status, &body["code"]), (403, &json!("presence_required")));
    let (_, list) = app
        .call("GET", "/api/crew/guard/targets", json!({}), None)
        .await;
    assert_eq!(
        list["items"].as_array().unwrap().len(),
        1,
        "nothing changed"
    );

    // A bad pattern is a 400.
    let (status, _) = app.add("prod", "https://user:pw@db.example.com").await;
    assert_eq!(status, 400);
    let (status, _) = app
        .call(
            "POST",
            "/api/crew/guard/targets/add",
            json!({ "list": "prod", "pattern": "x", "kind": "host", "extra": 1 }),
            None,
        )
        .await;
    assert!(status >= 400, "an unknown field is refused");
}

#[tokio::test]
async fn without_a_desktop_shell_loosening_answers_desktop_required() {
    let app = serve("noshell", false, false).await;
    let (status, body) = app.add("safe", "stage.internal").await;
    assert_eq!((status, &body["code"]), (403, &json!("desktop_required")));
    // Adding a prod entry tightens, even with no shell.
    let (status, _) = app.add("prod", "p.internal").await;
    assert_eq!(status, 200);
    let (_, list) = app
        .call("GET", "/api/crew/guard/targets", json!({}), None)
        .await;
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn the_test_route_says_what_the_guard_says_and_runs_nothing() {
    let app = serve("test", false, false).await;
    let query = |command: &str| {
        format!(
            "/api/crew/guard/test?command={}",
            command.replace(' ', "%20").replace('\'', "%27")
        )
    };
    let (status, body) = app
        .call(
            "GET",
            &query("psql -h db.prod.internal -c 'DROP TABLE users'"),
            json!({}),
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["verdict"], "guarded");
    assert_eq!(body["category"], "database");
    assert_eq!(body["target"], "db.prod.internal");
    assert_eq!(body["production_linked"], true);
    let (_, body) = app
        .call(
            "GET",
            &query("psql -h localhost -c 'DROP TABLE users'"),
            json!({}),
            None,
        )
        .await;
    assert_eq!(
        (&body["verdict"], &body["local"]),
        (&json!("local"), &json!(true))
    );
    let (_, body) = app.call("GET", &query("ls -la"), json!({}), None).await;
    assert_eq!(body["verdict"], "not_destructive");
    let (status, _) = app
        .call("GET", "/api/crew/guard/test", json!({}), None)
        .await;
    assert_eq!(status, 400, "a command is required");
}

#[tokio::test]
async fn a_remote_viewer_reads_and_changes_nothing() {
    let app = serve("remote", true, false).await;
    let (status, _) = app
        .call("GET", "/api/crew/guard/targets", json!({}), None)
        .await;
    assert_eq!(status, 200);
    let (status, body) = app.add("prod", "db.prod.internal").await;
    assert_eq!((status, &body["code"]), (403, &json!("local_only")));
}
