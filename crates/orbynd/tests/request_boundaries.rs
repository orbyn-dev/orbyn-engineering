//! Request-boundary regressions for CodeQL findings 10, 13, 15 and 16.
//! Collectors, repositories and engines are owned local fixtures.

mod support;

use std::sync::{Arc, Mutex};

use axum::body::to_bytes;
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use serde_json::{Value, json};
use support::{Options, TempDir, run_git, setup};

type CapturedRequests = Vec<(String, Vec<u8>)>;

struct Collector {
    url: String,
    requests: Arc<Mutex<CapturedRequests>>,
    task: tokio::task::JoinHandle<()>,
}

impl Collector {
    async fn new(status: StatusCode, location: String) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/v1/traces", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = requests.clone();
        let app = axum::Router::new().fallback(move |request: Request<axum::body::Body>| {
            let seen = seen.clone();
            let location = location.clone();
            async move {
                let method = request.method().to_string();
                let body = to_bytes(request.into_body(), 1024).await.unwrap();
                seen.lock().unwrap().push((method, body.to_vec()));
                (status, [("location", location)]).into_response()
            }
        });
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            url,
            requests,
            task,
        }
    }
}

impl Drop for Collector {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[tokio::test]
async fn codeql_10_otlp_posts_to_the_chosen_local_collector_without_following_redirects() {
    let dir = TempDir::new();
    let (service, _) = setup(&dir, Options::default(), "codex").await;
    let destination = Collector::new(StatusCode::NO_CONTENT, String::new()).await;
    let (_, direct) = service
        .send("/api/diagnostics/otlp", json!({ "url": destination.url }))
        .await;
    assert_eq!(direct["ok"], true, "{direct}");
    assert_eq!(direct["status"], 204);
    assert_eq!(
        *destination.requests.lock().unwrap(),
        [("POST".into(), b"{}".to_vec())]
    );
    for status in [
        StatusCode::TEMPORARY_REDIRECT,
        StatusCode::PERMANENT_REDIRECT,
    ] {
        let redirect = Collector::new(status, destination.url.clone()).await;
        let (_, result) = service
            .send("/api/diagnostics/otlp", json!({ "url": redirect.url }))
            .await;
        assert_eq!(result["status"], status.as_u16(), "{result}");
        assert_eq!(result["ok"], false);
        assert_eq!(redirect.requests.lock().unwrap().len(), 1);
    }
    assert_eq!(
        destination.requests.lock().unwrap().len(),
        1,
        "redirect changed the authorized destination"
    );
    service.stop().await;
}

#[tokio::test]
async fn codeql_10_otlp_rejects_ambiguous_urls_before_any_request() {
    let dir = TempDir::new();
    let (service, _) = setup(&dir, Options::default(), "codex").await;
    let destination = Collector::new(StatusCode::NO_CONTENT, String::new()).await;
    for url in [
        format!("{}#other-destination", destination.url),
        destination.url.replace("http://", "http://user:password@"),
        format!("{}\n", destination.url),
        "file:///collector".into(),
        "http://".into(),
    ] {
        let (_, result) = service
            .send("/api/diagnostics/otlp", json!({ "url": url }))
            .await;
        assert_eq!(result["ok"], false, "{result}");
        assert!(result["status"].is_null(), "{result}");
        assert!(
            result["message"]
                .as_str()
                .is_some_and(|text| !text.contains("password"))
        );
    }
    assert!(destination.requests.lock().unwrap().is_empty());
    service.stop().await;
}

#[tokio::test]
async fn codeql_13_batch_limit_rejects_the_whole_request_before_creating_threads() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    let project = service.json(&format!("/api/threads/{thread}")).await["project_id"].clone();
    let entry = |engine: &str| {
        json!({ "prompt": "", "thread": {
            "project_id": project, "engine": engine, "model": null, "title": null,
            "worktree": null, "skill_profile": null,
        }})
    };
    let before = service.json("/api/threads").await;
    let mut oversized = vec![entry("nothing"); 101];
    oversized[0] = entry("codex");
    let (code, result) = service
        .send("/api/threads/batch", json!({ "threads": oversized }))
        .await;
    assert_eq!(code, 400, "{result}");
    assert_eq!(result["code"], "invalid_input");
    assert_eq!(
        service.json("/api/threads").await,
        before,
        "oversized batches must have no partial effects"
    );
    let (code, result) = service
        .send(
            "/api/threads/batch",
            json!({ "threads": vec![entry("nothing"); 100] }),
        )
        .await;
    assert_eq!(code, 200, "{result}");
    assert_eq!(result.as_array().unwrap().len(), 100);
    assert!(
        result
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["error"].is_string())
    );
    let (code, result) = service
        .send("/api/threads/batch", json!({ "threads": [] }))
        .await;
    assert_eq!(code, 200);
    assert_eq!(result, json!([]));
    service.stop().await;
}

#[tokio::test]
async fn codeql_15_16_git_operands_refuse_options_and_preserve_literal_messages_and_search() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;
    let body = service.json(&format!("/api/threads/{thread}")).await;
    let cwd = std::path::PathBuf::from(body["cwd"].as_str().unwrap());
    assert!(cwd.starts_with(dir.path()));
    run_git(&cwd, &["config", "user.name", "Test"]);
    run_git(&cwd, &["config", "user.email", "test@example.com"]);
    std::fs::write(cwd.join("-literal.txt"), "--help; literal\n").unwrap();
    run_git(&cwd, &["add", "--", "-literal.txt"]);
    run_git(&cwd, &["commit", "-qm", "initial"]);
    for body in [
        json!({ "name": "--detach" }),
        json!({ "name": "safe", "start": "--orphan" }),
    ] {
        let (code, result) = service
            .send(&format!("/api/threads/{thread}/branches"), body)
            .await;
        assert_eq!(code, 400, "{result}");
    }
    let (code, result) = service
        .send(
            &format!("/api/threads/{thread}/branches/switch"),
            json!({ "name": "--detach" }),
        )
        .await;
    assert_eq!(code, 400, "{result}");
    for (scope, key) in [("branch", "base"), ("commit", "sha")] {
        let response = service
            .get(&format!("/api/threads/{thread}/diff"))
            .query(&[("scope", scope), (key, "--output=unwanted")])
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
    }
    assert!(!cwd.join("unwanted").exists());
    let (code, result) = service
        .send(
            &format!("/api/threads/{thread}/branches"),
            json!({ "name": "feature/safe", "start": "main" }),
        )
        .await;
    assert_eq!(code, 201, "{result}");
    std::fs::write(cwd.join("-literal.txt"), "--help; literal\nchanged\n").unwrap();
    let message = "--help; $(literal) 'quoted'";
    let (code, result) = service
        .send(
            &format!("/api/threads/{thread}/git/action"),
            json!({ "commit_message": message }),
        )
        .await;
    assert_eq!(code, 200, "{result}");
    assert_eq!(result["phases"][0]["ok"], true, "{result}");
    let output = std::process::Command::new("git")
        .args(["log", "-1", "--format=%s"])
        .current_dir(&cwd)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), message);
    let response = service
        .get(&format!("/api/threads/{thread}/files/search"))
        .query(&[("q", "--help; literal"), ("content", "true")])
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    let hits: Value = response.json().await.unwrap();
    assert!(
        hits.as_array()
            .unwrap()
            .iter()
            .any(|hit| hit["path"] == "-literal.txt")
    );
    service.stop().await;
}
