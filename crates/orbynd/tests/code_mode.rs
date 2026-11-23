//! `POST /api/code-runs/{id}/cancel` through the real service: the Person gate, the Code Mode
//! switch, the body-less shape and the fixed answers. The run's own end is tested in the core,
//! where a test can play a paused script.

mod support;

use serde_json::{Value, json};
use support::{Options, Service, TempDir};

const CANCEL: &str = "/api/code-runs/no-such-run/cancel";

fn options(desktop: bool, code_mode: bool) -> Options {
    Options {
        args: if desktop {
            vec!["--parent-stdin".into()]
        } else {
            Vec::new()
        },
        env: vec![
            ("ORBYN_DEV_DISABLE_PERSON_KEY".into(), "0".into()),
            (
                "ORBYN_CODE_MODE".into(),
                if code_mode { "1" } else { "0" }.into(),
            ),
        ],
    }
}

fn service(dir: &TempDir, desktop: bool, code_mode: bool) -> Service {
    Service::start(&dir.path().join("data"), options(desktop, code_mode))
}

impl Service {
    async fn cancel(&self, with_key: bool, body: Option<String>) -> (u16, Value) {
        let mut request = self.post(CANCEL);
        if with_key && let Some(key) = &self.person_key {
            request = request.header("x-orbyn-person", key);
        }
        if let Some(body) = body {
            request = request
                .header("content-type", "application/json")
                .body(body);
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        (status, response.json().await.unwrap_or(Value::Null))
    }
}

#[tokio::test]
async fn cancel_needs_the_person_key_like_every_person_route() {
    let dir = TempDir::new();
    let service = service(&dir, true, true);
    let (status, answer) = service.cancel(false, None).await;
    assert_eq!(status, 403, "{answer}");
    assert_eq!(answer["code"], "person_required", "{answer}");
    assert_eq!(
        answer["message"],
        "This action needs the person at the desktop app."
    );

    let (status, answer) = service.cancel(true, None).await;
    assert_eq!(status, 404, "{answer}");
    assert_eq!(answer["code"], "not_found", "{answer}");
}

#[tokio::test]
async fn a_shell_less_service_answers_desktop_required() {
    let dir = TempDir::new();
    let service = service(&dir, false, true);
    assert!(service.person_key.is_none());
    let (status, answer) = service.cancel(false, None).await;
    assert_eq!(status, 403, "{answer}");
    assert_eq!(answer["code"], "desktop_required", "{answer}");
    assert_eq!(
        answer["message"],
        "This action is only available in the desktop app."
    );
}

#[tokio::test]
async fn a_body_over_1_kib_is_refused_with_the_fixed_copy() {
    let dir = TempDir::new();
    let service = service(&dir, true, true);
    let body = format!("{{\"pad\":\"{}\"}}", "x".repeat(2 * 1024));
    let (status, answer) = service.cancel(true, Some(body)).await;
    assert_eq!(status, 400, "{answer}");
    assert_eq!(answer["code"], "invalid_input", "{answer}");
    assert_eq!(answer["message"], "This route takes no body.");
}

#[tokio::test]
async fn the_body_limit_is_exactly_1_kib() {
    let dir = TempDir::new();
    let service = service(&dir, true, true);
    let bare = json!({ "pad": "" }).to_string().len();
    let at_limit = json!({ "pad": "x".repeat(1024 - bare) }).to_string();
    assert_eq!(at_limit.len(), 1024);
    let (status, answer) = service.cancel(true, Some(at_limit)).await;
    assert_eq!(status, 404, "1 KiB is read and ignored: {answer}");
    assert_eq!(answer["code"], "not_found", "{answer}");

    let over = json!({ "pad": "x".repeat(1024 - bare + 1) }).to_string();
    assert_eq!(over.len(), 1025);
    let (status, answer) = service.cancel(true, Some(over)).await;
    assert_eq!(status, 400, "{answer}");
    assert_eq!(answer["code"], "invalid_input", "{answer}");
    assert_eq!(answer["message"], "This route takes no body.");
}

#[tokio::test]
async fn with_code_mode_off_the_cancel_route_does_not_exist() {
    let dir = TempDir::new();
    let service = service(&dir, true, false);
    let (status, answer) = service.cancel(true, None).await;
    assert_eq!(status, 404, "{answer}");
    assert_eq!(answer["code"], "not_found", "{answer}");
    // The flag is checked before the body, so an oversized body is still "no such route".
    let (status, answer) = service
        .cancel(
            true,
            Some(json!({ "pad": "x".repeat(2 * 1024) }).to_string()),
        )
        .await;
    assert_eq!(status, 404, "{answer}");
    assert_eq!(answer["code"], "not_found", "{answer}");
}
