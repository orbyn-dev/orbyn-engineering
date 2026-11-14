//! `POST /api/crew/tasks/{id}/rerun-checks` over HTTP: the Preview gate, the credential, the remote refusal, the body, the answers
//! for a task that ended, one that did not and one that does not exist, the one-at-a-time claim, the command screen and what the
//! answer never holds (a check's command, output or claim). Checks run for real in temporary git repositories; the stored tasks have
//! no worker.

use std::path::PathBuf;
use std::sync::Arc;

use orbyn_core::crew::fixtures::git;
use orbyn_core::crew::rerun::fixtures::task_in;
use orbyn_core::{Core, CoreConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::router;
use crate::http::AppState;
use crate::http::access::Access;

struct Served {
    port: u16,
    core: Arc<Core>,
    dir: PathBuf,
}

async fn serve(name: &str, preview: bool, remote: bool) -> Served {
    let dir = std::env::temp_dir().join(format!("orbynd-rerun-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut config = CoreConfig::new(dir.join("data"), "http://127.0.0.1:1".into());
    config.crew_preview = preview;
    config.crew_confine_workers = false;
    let core = Arc::new(Core::open(config).unwrap());
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
    Served { port, core, dir }
}

impl Served {
    /// A repository with one commit, as a checkout.
    fn checkout(&self, name: &str) -> PathBuf {
        let repo = self.dir.join(name);
        git(&repo, &["init", "-q"]);
        std::fs::write(repo.join("a.txt"), "a\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", "base"]);
        repo
    }

    async fn rerun_as(&self, id: &str, body: Option<&str>, token: Option<&str>) -> (u16, Value) {
        let url = format!(
            "http://127.0.0.1:{}/api/crew/tasks/{id}/rerun-checks",
            self.port
        );
        let mut request = reqwest::Client::new().post(url);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request
                .header("content-type", "application/json")
                .body(body.to_owned());
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    async fn rerun(&self, id: &str, body: Option<&str>) -> (u16, Value) {
        self.rerun_as(id, body, Some("token")).await
    }

    fn audit(&self, prefix: &str) -> Vec<String> {
        let mut rows = self.core.store.list_audit(None, 200).unwrap();
        rows.reverse();
        rows.into_iter()
            .map(|row| row.action)
            .filter(|action| action.starts_with(prefix))
            .collect()
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[tokio::test]
async fn with_the_flag_off_the_route_is_not_found_and_runs_nothing() {
    let app = serve("off", false, false).await;
    let (status, body) = app.rerun("task-1", None).await;
    assert_eq!((status, body["code"].as_str()), (404, Some("not_found")));
    assert!(app.audit("crew.task").is_empty());
}

#[tokio::test]
async fn the_credential_is_needed_and_a_remote_viewer_is_refused_before_anything_runs() {
    let app = serve("who", true, false).await;
    let repo = app.checkout("repo");
    let id = task_in(&app.core, &repo, &["touch ran"], "stopped").await;
    let (status, _) = app.rerun_as(&id, None, None).await;
    assert_eq!(status, 401);
    let (status, _) = app.rerun_as(&id, None, Some("wrong")).await;
    assert_eq!(status, 401);
    assert!(!repo.join("ran").exists());

    let remote = serve("remote", true, true).await;
    let repo = remote.checkout("repo");
    let id = task_in(&remote.core, &repo, &["touch ran"], "stopped").await;
    let (status, body) = remote.rerun(&id, None).await;
    assert_eq!((status, body["code"].as_str()), (403, Some("local_only")));
    assert!(!repo.join("ran").exists(), "a remote caller ran a command");
    assert!(remote.audit("crew.task").is_empty());
}

#[tokio::test]
async fn an_unknown_task_and_a_bad_body_are_refused_before_anything_is_written() {
    let app = serve("input", true, false).await;
    let (status, body) = app.rerun("no-such-task", None).await;
    assert_eq!((status, body["code"].as_str()), (404, Some("not_found")));
    let repo = app.checkout("repo");
    let id = task_in(&app.core, &repo, &["touch ran"], "stopped").await;
    for body in [
        "not json",
        r#"{"reason": 7}"#,
        r#"{"reason": "ok", "command": "touch ran"}"#,
        r#"{"reason": ""#,
    ] {
        let (status, _) = app.rerun(&id, Some(body)).await;
        assert_eq!(status, 400, "{body}");
    }
    let long = format!(r#"{{"reason":"{}"}}"#, "x".repeat(201));
    assert_eq!(app.rerun(&id, Some(&long)).await.0, 400);
    assert!(!repo.join("ran").exists());
    assert!(app.audit("crew.task").is_empty());
}

#[tokio::test]
async fn a_task_that_ended_is_checked_again_and_the_answer_is_counts_only() {
    let app = serve("ended", true, false).await;
    let repo = app.checkout("repo");
    let command = "echo secret-looking-output-77 && touch ran";
    let id = task_in(&app.core, &repo, &[command], "passed").await;
    let evidence = app.core.store.crew_evidence(&id).unwrap().len();
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let (status, body) = app
        .rerun(&id, Some(r#"{"reason":"after the merge"}"#))
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["task"], id.as_str());
    assert_eq!(body["state"], "passed");
    assert_eq!(
        (
            &body["total"],
            &body["ran"],
            &body["passed"],
            &body["refused"]
        ),
        (&json!(2), &json!(2), &json!(2), &json!(0))
    );
    assert_eq!(body["gate"], "pass");
    assert_eq!(body["evidence_ids"].as_array().unwrap().len(), 2);
    let text = body.to_string();
    for key_text in [
        command,
        "echo",
        "touch",
        "secret-looking-output-77",
        "proves",
        "excerpt",
    ] {
        assert!(!text.contains(key_text), "{key_text}: {text}");
    }
    assert_eq!(
        app.core.store.crew_evidence(&id).unwrap().len(),
        evidence + 2
    );
    assert_eq!(
        app.core.store.crew_task(&id).unwrap().unwrap().state,
        "passed"
    );
    assert_eq!(
        app.audit("crew.task"),
        ["crew.task.rerun_checks", "crew.task.rerun_checks_done"]
    );
    // An empty body is as good as none.
    assert_eq!(app.rerun(&id, Some("  ")).await.0, 200);
}

#[tokio::test]
async fn a_task_that_is_not_final_gets_counts_and_none_of_the_key() {
    let app = serve("leak", true, false).await;
    let repo = app.checkout("repo");
    let drop = "psql -h db.prod.internal -c 'DROP TABLE users' && touch ran-bad";
    let id = task_in(&app.core, &repo, &["touch ran-ok", drop], "failed").await;
    let (status, body) = app.rerun(&id, None).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        (
            &body["ran"],
            &body["passed"],
            &body["refused"],
            &body["gate"]
        ),
        (&json!(2), &json!(2), &json!(1), &json!("fail"))
    );
    assert!(body.get("refused_checks").is_none());
    let text = body.to_string();
    for key_text in ["chk-", "psql", "DROP", "touch", "prod.internal", "blast"] {
        assert!(!text.contains(key_text), "{key_text}: {text}");
    }
    assert!(repo.join("ran-ok").exists());
    assert!(!repo.join("ran-bad").exists(), "a guarded command ran");
}

#[tokio::test]
async fn a_task_that_has_not_ended_is_a_conflict_and_nothing_runs() {
    let app = serve("open", true, false).await;
    let repo = app.checkout("repo");
    for state in ["briefed", "running", "verifying", "cancelled"] {
        let id = task_in(&app.core, &repo, &["touch ran"], state).await;
        let (status, body) = app.rerun(&id, None).await;
        assert_eq!(
            (status, body["code"].as_str()),
            (409, Some("task_busy")),
            "{state}"
        );
        assert!(!body.to_string().contains("touch"), "{state}");
    }
    assert!(!repo.join("ran").exists());
    assert!(app.audit("crew.task").is_empty());
}

#[tokio::test]
async fn a_second_request_while_one_runs_is_busy() {
    let app = serve("busy", true, false).await;
    let repo = app.checkout("repo");
    let wait =
        "touch started; i=0; while [ ! -f go ] && [ $i -lt 400 ]; do sleep 0.05; i=$((i+1)); done";
    let id = task_in(&app.core, &repo, &[wait], "stopped").await;

    let first = {
        let (port, id) = (app.port, id.clone());
        tokio::spawn(async move {
            reqwest::Client::new()
                .post(format!(
                    "http://127.0.0.1:{port}/api/crew/tasks/{id}/rerun-checks"
                ))
                .bearer_auth("token")
                .send()
                .await
                .unwrap()
                .status()
                .as_u16()
        })
    };
    for _ in 0..600 {
        if repo.join("started").exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        repo.join("started").exists(),
        "the first command never started"
    );
    assert_eq!(app.audit("crew.task"), ["crew.task.rerun_checks"]);
    let (status, body) = app.rerun(&id, None).await;
    assert_eq!(
        (status, body["code"].as_str()),
        (409, Some("task_busy")),
        "{body}"
    );
    assert_eq!(app.audit("crew.task"), ["crew.task.rerun_checks"]);

    std::fs::write(repo.join("go"), "go").unwrap();
    assert_eq!(first.await.unwrap(), 200);
    assert_eq!(
        app.audit("crew.task"),
        ["crew.task.rerun_checks", "crew.task.rerun_checks_done"]
    );
}
