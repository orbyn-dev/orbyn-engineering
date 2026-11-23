//! The update channel through the real binary and its real stdin and stdout.

mod support;

use std::io::{BufRead, Write};
use std::time::Duration;

use serde_json::{Value, json};
use support::{Options, Service, TempDir, setup};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn desktop() -> Options {
    Options {
        args: vec!["--parent-stdin".into()],
        ..Options::default()
    }
}

fn say(service: &mut Service, line: &str) {
    let stdin = service.child.stdin.as_mut().unwrap();
    writeln!(stdin, "{line}").unwrap();
    stdin.flush().unwrap();
}

async fn app(service: &Service) -> Value {
    service.json("/api/updates").await["app"].clone()
}

async fn until_can_apply(service: &Service) {
    for _ in 0..500 {
        if app(service).await["can_apply"] == true {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("the shell's message never arrived");
}

#[tokio::test]
async fn shell_channel_survives_junk_and_install_holds_admission() {
    let dir = TempDir::new();
    let (mut service, thread) = setup(&dir, desktop(), "codex").await;
    assert_eq!(app(&service).await["mode"], "desktop");
    say(&mut service, "not json at all");
    say(&mut service, r#"{"type":"from_the_future","x":1}"#);
    say(
        &mut service,
        r#"{"type":"app_update_available","release_id":"r1","version":"99.0.0","notes":"n"}"#,
    );
    until_can_apply(&service).await;
    assert_eq!(app(&service).await["latest"], "99.0.0");

    let run = service.admit(&thread, "[hang]").await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let (code, body) = service
        .send(
            "/api/updates/app/download",
            json!({"release_id":"r1","operation":0}),
        )
        .await;
    assert_eq!(code, 409);
    assert_eq!(body["code"], "runs_active");
    service
        .send(&format!("/api/runs/{run}/cancel"), json!({}))
        .await;
    service.finished(&thread).await;

    let (code, _) = service
        .send(
            "/api/updates/app/download",
            json!({"release_id":"r1","operation":0}),
        )
        .await;
    assert_eq!(code, 202);
    let mut line = String::new();
    while !line.contains("download_app_update") {
        line.clear();
        service.stdout.read_line(&mut line).unwrap();
        assert!(!line.is_empty(), "stdout closed");
    }
    assert_eq!(
        serde_json::from_str::<Value>(&line).unwrap()["type"],
        "download_app_update"
    );
    let (code, body) = service
        .send(
            &format!("/api/threads/{thread}/runs"),
            json!({ "prompt": "x" }),
        )
        .await;
    assert_eq!(code, 503);
    assert_eq!(body["code"], "updating");
}

#[tokio::test]
async fn a_check_failure_cannot_release_an_install_through_the_shell_channel() {
    let dir = TempDir::new();
    let (mut service, thread) = setup(&dir, desktop(), "codex").await;
    say(
        &mut service,
        r#"{"type":"app_update_check_failed","message":"initial check unavailable"}"#,
    );
    wait_update_error(&service, "initial check unavailable").await;
    let run = service.admit(&thread, "[hang]").await;
    service
        .send(&format!("/api/runs/{run}/cancel"), json!({}))
        .await;
    service.finished(&thread).await;
    say(
        &mut service,
        r#"{"type":"app_update_available","release_id":"r1","version":"99.0.0","notes":"new"}"#,
    );
    until_can_apply(&service).await;
    let (code, _) = service
        .send(
            "/api/updates/app/download",
            json!({"release_id":"r1","operation":0}),
        )
        .await;
    assert_eq!(code, 202);
    say(
        &mut service,
        r#"{"type":"app_update_progress","release_id":"r1","operation":1,"downloaded":5,"total":10}"#,
    );
    say(
        &mut service,
        r#"{"type":"app_update_check_failed","message":"check unavailable"}"#,
    );
    let status = wait_update_error(&service, "check unavailable").await;
    let (code, body) = service
        .send(
            &format!("/api/threads/{thread}/runs"),
            json!({ "prompt": "[hang]" }),
        )
        .await;
    assert_eq!(code, 503);
    assert_eq!(body["code"], "updating");
    assert_eq!(status["app"]["downloaded"], 5);
    assert_eq!(status["app"]["total"], 10);
    assert_eq!(status["app"]["latest"], "99.0.0");
    assert_eq!(status["app"]["notes"], "new");

    say(
        &mut service,
        r#"{"type":"app_update_failed","release_id":"r1","operation":1,"message":"install failed"}"#,
    );
    let status = wait_update_error(&service, "install failed").await;
    assert!(status["app"]["downloaded"].is_null());
    assert!(status["app"]["total"].is_null());
    let run = service.admit(&thread, "[hang]").await;
    service
        .send(&format!("/api/runs/{run}/cancel"), json!({}))
        .await;
    service.finished(&thread).await;
}

async fn wait_update_error(service: &Service, error: &str) -> Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let status = service.json("/api/updates").await;
            if status["error"] == error || status["app"]["update_error"] == error {
                return status;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the shell's update failure never arrived")
}

#[tokio::test]
async fn server_compares_a_release_manifest_and_keeps_it_when_it_disappears() {
    let dir = TempDir::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        // A service that never asks for the manifest must fail the test, not hang it.
        let (mut socket, _) = tokio::time::timeout(Duration::from_secs(10), listener.accept())
            .await
            .expect("the service never asked for the update manifest")
            .unwrap();
        let mut buffer = [0u8; 2048];
        let _ = socket.read(&mut buffer).await;
        let body = r#"{"version":"99.0.0","notes":"Big release"}"#;
        let reply = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        socket.write_all(reply.as_bytes()).await.unwrap();
    });
    let service = Service::start(
        &dir.path().join("data"),
        Options {
            args: vec![
                "--update-manifest".into(),
                format!("http://127.0.0.1:{port}/manifest.json"),
            ],
            // The harness runs every service offline, and an offline service makes no automatic update
            // request at all (05324e5b4); this test is about the one request it does make.
            env: vec![("ORBYN_OFFLINE".into(), "0".into())],
        },
    );
    tokio::time::timeout(Duration::from_secs(10), server)
        .await
        .expect("the manifest server never finished")
        .unwrap();
    for _ in 0..500 {
        if app(&service).await["latest"] == "99.0.0" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let status = app(&service).await;
    assert_eq!(status["latest"], "99.0.0");
    assert_eq!(status["notes"], "Big release");
    assert_eq!(status["can_apply"], false, "a server cannot replace itself");
    assert_eq!(status["unconfigured"], false);

    let (code, after) = service.send("/api/updates/check", json!({})).await;
    assert_eq!(code, 200);
    assert_eq!(after["app"]["latest"], "99.0.0");
    assert!(after["error"].as_str().unwrap().contains("manifest"));
    let (code, _) = service
        .send(
            "/api/updates/app/download",
            json!({"release_id":"r1","operation":0}),
        )
        .await;
    assert_eq!(code, 422);
}

#[tokio::test]
async fn update_events_send_the_status_then_each_change() {
    let dir = TempDir::new();
    let (mut service, _) = setup(&dir, desktop(), "codex").await;
    let response = service
        .get("/api/updates/events")
        .header("Accept", "text/event-stream")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let mut body = response.bytes_stream();
    let mut parser = orbyn_core::agents::sse::SseParser::default();
    use futures_util::StreamExt;
    say(
        &mut service,
        r#"{"type":"app_update_available","release_id":"r1","version":"99.0.0"}"#,
    );
    let found = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(Ok(chunk)) = body.next().await {
            for frame in parser.push(&chunk) {
                let status: Value = serde_json::from_str(&frame).unwrap();
                if status["app"]["latest"] == "99.0.0" {
                    return true;
                }
            }
        }
        false
    })
    .await
    .unwrap();
    assert!(found);
}

#[tokio::test]
async fn a_link_waits_for_the_window_and_arrives_as_a_route() {
    let dir = TempDir::new();
    let (mut service, _thread) = setup(&dir, desktop(), "codex").await;
    let status = service.json("/api/shell").await;
    assert_eq!(status["attached"], true);
    assert_eq!(status["windows"], 0);
    say(
        &mut service,
        r#"{"type":"open_link","url":"orbyn://run/start"}"#,
    );
    say(
        &mut service,
        r#"{"type":"open_link","url":"orbyn://thread/t1"}"#,
    );
    say(&mut service, r#"{"type":"app_update_none"}"#);
    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut stream = service.get("/api/shell/events").send().await.unwrap();
    let mut seen = String::new();
    while !seen.contains("\n\n") {
        let chunk = tokio::time::timeout(Duration::from_secs(5), stream.chunk())
            .await
            .expect("the held link never arrived")
            .unwrap()
            .expect("the stream closed");
        seen.push_str(&String::from_utf8_lossy(&chunk));
    }
    assert!(
        seen.contains(r##"{"type":"navigate","href":"#/threads/t1"}"##),
        "{seen}"
    );
    assert_eq!(service.json("/api/shell").await["windows"], 1);
}

#[tokio::test]
async fn a_window_reaches_the_shell_only_through_listed_actions() {
    let dir = TempDir::new();
    let (mut service, _thread) = setup(&dir, desktop(), "codex").await;
    let (code, _) = service
        .send("/api/shell/actions", json!({ "action": "zoom_in" }))
        .await;
    assert_eq!(code, 202);
    let mut line = String::new();
    while !line.contains("shell_action") {
        line.clear();
        service.stdout.read_line(&mut line).unwrap();
        assert!(!line.is_empty(), "stdout closed");
    }
    assert!(line.contains("zoom_in"), "{line}");
    let (code, _) = service
        .send("/api/shell/actions", json!({ "action": "open_anything" }))
        .await;
    assert_eq!(code, 400);

    let dir = TempDir::new();
    let (server, _thread) = setup(&dir, Options::default(), "codex").await;
    let (code, _) = server
        .send("/api/shell/actions", json!({ "action": "zoom_in" }))
        .await;
    assert_eq!(code, 422);
    assert_eq!(server.json("/api/shell").await["attached"], false);
}

#[tokio::test]
async fn a_window_can_ask_the_shell_to_restart_the_service_and_show_its_log() {
    let dir = TempDir::new();
    let (mut service, _thread) = setup(&dir, desktop(), "codex").await;
    for action in ["restart_service", "show_log"] {
        let (code, _) = service
            .send("/api/shell/actions", json!({ "action": action }))
            .await;
        assert_eq!(code, 202, "{action}");
        let mut line = String::new();
        while !line.contains("shell_action") {
            line.clear();
            service.stdout.read_line(&mut line).unwrap();
            assert!(!line.is_empty(), "stdout closed");
        }
        assert!(line.contains(action), "{line}");
    }
}

#[tokio::test]
async fn the_shell_hears_when_work_starts_and_ends() {
    let dir = TempDir::new();
    let (mut service, thread) = setup(&dir, desktop(), "codex").await;
    let activity = async |service: &mut Service, running: u64| {
        let mut line = String::new();
        loop {
            line.clear();
            service.stdout.read_line(&mut line).unwrap();
            assert!(!line.is_empty(), "stdout closed");
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if value["type"] == "activity" && value["running"] == running {
                return value;
            }
        }
    };
    let run = service.admit(&thread, "[hang]").await;
    let seen = activity(&mut service, 1).await;
    assert_eq!(seen["keep_awake"], true);
    assert_eq!(seen["waiting"], 0);
    service
        .send(&format!("/api/runs/{run}/cancel"), json!({}))
        .await;
    service.finished(&thread).await;
    let seen = activity(&mut service, 0).await;
    assert_eq!(seen["keep_awake"], false);
    assert_eq!(seen["unread"], 0);

    // A thread marked unread reaches the Dock badge's count.
    let code = service
        .client
        .patch(format!("{}/api/threads/{thread}", service.url))
        .bearer_auth(&service.token)
        .json(&json!({ "unread": true }))
        .send()
        .await
        .unwrap()
        .status();
    assert!(code.is_success(), "{code}");
    let mut line = String::new();
    loop {
        line.clear();
        service.stdout.read_line(&mut line).unwrap();
        assert!(!line.is_empty(), "stdout closed");
        if let Ok(value) = serde_json::from_str::<Value>(&line)
            && value["type"] == "activity"
            && value["unread"] == 1
        {
            break;
        }
    }
}

#[tokio::test]
async fn only_verified_readiness_allows_install_and_a_new_run_can_still_refuse_it() {
    let dir = TempDir::new();
    let (mut service, thread) = setup(&dir, desktop(), "codex").await;
    say(
        &mut service,
        r#"{"type":"app_update_available","release_id":"r1","version":"99.0.0"}"#,
    );
    until_can_apply(&service).await;
    let action = json!({"release_id":"r1","operation":0});
    assert_eq!(
        service
            .send("/api/updates/app/apply", action.clone())
            .await
            .0,
        422
    );
    assert_eq!(
        service
            .send("/api/updates/app/download", action.clone())
            .await
            .0,
        202
    );
    // Duplicate consent is accepted without another transfer; bytes do not prove readiness.
    assert_eq!(
        service.send("/api/updates/app/download", action).await.0,
        202
    );
    say(
        &mut service,
        r#"{"type":"app_update_progress","release_id":"r1","operation":1,"downloaded":100,"total":100}"#,
    );
    say(
        &mut service,
        r#"{"type":"app_update_preparing","release_id":"r1","operation":1}"#,
    );
    let action = json!({"release_id":"r1","operation":1});
    assert_eq!(
        service
            .send("/api/updates/app/apply", action.clone())
            .await
            .0,
        422
    );
    say(
        &mut service,
        r#"{"type":"app_update_ready","release_id":"r1","operation":1}"#,
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while app(&service).await["phase"] != "ready" {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let run = service.admit(&thread, "[hang]").await;
    assert_eq!(
        service
            .send("/api/updates/app/apply", action.clone())
            .await
            .0,
        409
    );
    service
        .send(&format!("/api/runs/{run}/cancel"), json!({}))
        .await;
    service.finished(&thread).await;
    assert_eq!(service.send("/api/updates/app/apply", action).await.0, 202);
    let mut commands = Vec::new();
    loop {
        let mut line = String::new();
        service.stdout.read_line(&mut line).unwrap();
        assert!(!line.is_empty());
        let message: Value = serde_json::from_str(&line).unwrap();
        commands.push(message["type"].as_str().unwrap().to_owned());
        if commands.last().unwrap() == "install_app_update" {
            break;
        }
    }
    assert_eq!(
        commands
            .iter()
            .filter(|kind| kind.as_str() == "download_app_update")
            .count(),
        1
    );
}

#[tokio::test]
async fn download_consent_requires_this_launch_person_key() {
    let dir = TempDir::new();
    let mut options = desktop();
    options
        .env
        .push(("ORBYN_DEV_DISABLE_PERSON_KEY".into(), "0".into()));
    let (mut service, _thread) = setup(&dir, options, "codex").await;
    say(
        &mut service,
        r#"{"type":"app_update_available","release_id":"r1","version":"99.0.0"}"#,
    );
    until_can_apply(&service).await;
    let body = json!({"release_id":"r1","operation":0});
    for key in [None, Some("wrong-launch-key")] {
        let mut request = service
            .client
            .post(format!("{}/api/updates/app/download", service.url))
            .bearer_auth(&service.token)
            .json(&body);
        if let Some(key) = key {
            request = request.header("x-orbyn-person", key);
        }
        let response = request.send().await.unwrap();
        assert_eq!(
            response.status(),
            403,
            "a service token or another launch's key cannot reserve update admission"
        );
    }
    assert_eq!(service.send("/api/updates/app/download", body).await.0, 202);
}

#[tokio::test]
async fn complete_query_preserves_person_key_and_correlated_lifecycle() {
    let dir = TempDir::new();
    let mut options = desktop();
    options
        .env
        .push(("ORBYN_DEV_DISABLE_PERSON_KEY".into(), "0".into()));
    let (mut service, _thread) = setup(&dir, options, "codex").await;
    say(
        &mut service,
        r#"{"type":"app_update_available","release_id":"r1","version":"99.0.0"}"#,
    );
    until_can_apply(&service).await;
    let route = "/api/updates/app/apply?complete=true";
    let body = json!({"release_id":"r1","operation":0});
    for key in [None, Some("wrong-launch-key")] {
        let mut request = service.post(route).json(&body);
        if let Some(key) = key {
            request = request.header("x-orbyn-person", key);
        }
        assert_eq!(request.send().await.unwrap().status(), 403);
    }
    assert_eq!(
        service
            .send(route, json!({"release_id":"stale","operation":0}))
            .await
            .0,
        422
    );
    assert_eq!(
        service
            .send(route, json!({"release_id":"r1","operation":42}))
            .await
            .0,
        422
    );
    assert_eq!(
        service.send("/api/updates/app/apply", body.clone()).await.0,
        422
    );
    assert_eq!(
        service
            .send("/api/updates/app/apply?complete=false", body.clone())
            .await
            .0,
        422
    );
    assert_eq!(service.send(route, body.clone()).await.0, 202);
    assert_eq!(
        service
            .send("/api/updates/app/apply?complete=invalid", body.clone())
            .await
            .0,
        400
    );
    assert_eq!(service.send(route, body).await.0, 202);
    say(
        &mut service,
        r#"{"type":"app_update_progress","release_id":"r1","operation":1,"downloaded":10,"total":10}"#,
    );
    say(
        &mut service,
        r#"{"type":"app_update_preparing","release_id":"r1","operation":1}"#,
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while app(&service).await["phase"] != "preparing" {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_ne!(app(&service).await["phase"], "installing");
    say(
        &mut service,
        r#"{"type":"app_update_ready","release_id":"r1","operation":1}"#,
    );
    let mut downloads = Vec::new();
    let install = loop {
        let mut line = String::new();
        service.stdout.read_line(&mut line).unwrap();
        assert!(!line.is_empty(), "stdout closed");
        let message: Value = serde_json::from_str(&line).unwrap();
        if message["type"] == "download_app_update" {
            downloads.push(message.clone());
        }
        if message["type"] == "install_app_update" {
            break message;
        }
    };
    assert_eq!(
        downloads,
        [json!({"type":"download_app_update","release_id":"r1","operation":1})]
    );
    assert_eq!(
        install,
        json!({"type":"install_app_update","release_id":"r1","operation":1})
    );
}

#[tokio::test]
async fn complete_query_refuses_a_competing_plain_download_without_installing() {
    let dir = TempDir::new();
    let (mut service, _thread) = setup(&dir, desktop(), "codex").await;
    say(
        &mut service,
        r#"{"type":"app_update_available","release_id":"r1","version":"99.0.0"}"#,
    );
    until_can_apply(&service).await;
    assert_eq!(
        service
            .send(
                "/api/updates/app/download",
                json!({"release_id":"r1","operation":0})
            )
            .await
            .0,
        202
    );
    for operation in [0, 1] {
        assert_eq!(
            service
                .send(
                    "/api/updates/app/apply?complete=true",
                    json!({"release_id":"r1","operation":operation})
                )
                .await
                .0,
            422
        );
    }
    say(
        &mut service,
        r#"{"type":"app_update_ready","release_id":"r1","operation":1}"#,
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while app(&service).await["phase"] != "ready" {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(app(&service).await["phase"], "ready");
}
