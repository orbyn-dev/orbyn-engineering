//! The desktop shell's presence messages through the real binary's stdin and stdout, and the approve-once route's two refusals
//! (`desktop_required`, `presence_required`). Creating a hold needs a running crew task, which a spawned service cannot be given
//! here; the whole approval, with a hold, is in `routes/crew_held_tests.rs` and the core's `autopilot_held` tests.
//! This file checks what only a real process shows: the lines are read from stdin, answered on stdout, and a service started
//! without `--parent-stdin` has no shell to approve with.

mod support;

use std::io::{BufRead, Write};

use serde_json::{Value, json};
use support::{Options, Service, TempDir};

fn options(desktop: bool, preview: bool) -> Options {
    Options {
        args: if desktop {
            vec!["--parent-stdin".into()]
        } else {
            vec![]
        },
        env: if preview {
            vec![("ORBYN_CREW_PREVIEW".into(), "1".into())]
        } else {
            vec![]
        },
    }
}

/// A nonce as the shell mints one: 16 random bytes, hex, drawn at run time.
fn fresh_nonce() -> String {
    orbyn_core::util::random_token().unwrap()[..32].to_owned()
}

fn say(service: &mut Service, line: &Value) {
    let stdin = service.child.stdin.as_mut().unwrap();
    writeln!(stdin, "{line}").unwrap();
    stdin.flush().unwrap();
}

/// The next presence line the service wrote for the shell; other lines (activity) are passed over.
fn answer(service: &mut Service) -> Value {
    loop {
        let mut line = String::new();
        assert!(
            service.stdout.read_line(&mut line).unwrap() > 0,
            "the service closed its output"
        );
        let value: Value = serde_json::from_str(&line).unwrap();
        if value["type"]
            .as_str()
            .is_some_and(|t| t.starts_with("presence_"))
        {
            return value;
        }
    }
}

#[tokio::test]
async fn the_shell_is_answered_on_stdout_and_a_missing_hold_is_refused() {
    let dir = TempDir::new();
    let mut service = Service::start(&dir.path().join("data"), options(true, true));
    say(
        &mut service,
        &json!({ "type": "presence_describe", "request_id": "r-1", "hold_id": "hold-nope" }),
    );
    let said = answer(&mut service);
    assert_eq!(
        said,
        json!({ "type": "presence_refused", "request_id": "r-1", "code": "not_found" })
    );
    say(
        &mut service,
        &json!({ "type": "presence_grant", "request_id": "r-2", "nonce": fresh_nonce()[..8], "hold_id": "hold-nope",
                "thread_id": "t", "task_id": "k", "command_sha256": "x" }),
    );
    assert_eq!(answer(&mut service)["code"], "invalid");
    say(
        &mut service,
        &json!({ "type": "presence_grant", "request_id": "r-3", "nonce": fresh_nonce(),
                "hold_id": "hold-nope", "thread_id": "t", "task_id": "k", "command_sha256": "x" }),
    );
    assert_eq!(answer(&mut service)["code"], "not_found");

    // The token alone, with no nonce: a desktop service answers presence_required.
    let response = service
        .post("/api/crew/approvals/hold-nope/approve-once")
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["code"], "presence_required");
    service.stop().await;
}

#[tokio::test]
async fn without_the_preview_flag_the_shell_is_told_it_is_unavailable() {
    let dir = TempDir::new();
    let mut service = Service::start(&dir.path().join("data"), options(true, false));
    say(
        &mut service,
        &json!({ "type": "presence_describe", "request_id": "r-1", "hold_id": "hold-nope" }),
    );
    assert_eq!(answer(&mut service)["code"], "unavailable");
    service.stop().await;
}

#[tokio::test]
async fn a_service_started_without_a_shell_answers_desktop_required() {
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), options(false, true));
    let drawn = fresh_nonce();
    for nonce in [None, Some(drawn.as_str())] {
        let mut request = service
            .post("/api/crew/approvals/hold-nope/approve-once")
            .json(&json!({}));
        if let Some(nonce) = nonce {
            request = request.header("x-orbyn-confirm", nonce);
        }
        let response = request.send().await.unwrap();
        assert_eq!(response.status(), 403);
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["code"], "desktop_required", "{nonce:?}");
    }
    service.stop().await;
}
