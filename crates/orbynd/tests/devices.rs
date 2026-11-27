//! The device routes through the real service: reads need only the service token,
//! every action needs the person key, and a refused write starts no tool. A fake
//! `adb`, `emulator` and `xcrun` on the service's PATH answer here.

mod support;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine as _;
use serde_json::{Value, json};
use support::{Options, Service, TempDir};

const PNG_CANARY: &str = "ORBYN_SCREEN_CANARY";

/// The one fake tool program the unit device tests run too:
/// `crates/orbynd/tests/fixtures/devices-fake-tool.js`. The shims below pass the tool
/// name and `ORBYN_DEVICES_FAKE_DIR` the way that program expects.
const FAKE: &str = include_str!("fixtures/devices-fake-tool.js");

fn fake(dir: &Path) -> PathBuf {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("devices-fake-tool.cjs"), FAKE).unwrap();
    for tool in ["adb", "emulator", "xcrun"] {
        std::fs::write(
            bin.join(format!("{tool}.cmd")),
            format!(
                "@node \"%~dp0\\devices-fake-tool.cjs\" {tool} \"%ORBYN_DEVICES_FAKE_DIR%\" %*\r\n"
            ),
        )
        .unwrap();
        let unix = bin.join(tool);
        std::fs::write(
            &unix,
            format!(
                "#!/bin/sh\nexec node \"$(dirname \"$0\")/devices-fake-tool.cjs\" {tool} \"$ORBYN_DEVICES_FAKE_DIR\" \"$@\"\n"
            ),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&unix, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    bin
}

fn options(dir: &TempDir, bin: &Path, with_person_key: bool) -> Options {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let path = std::env::join_paths(
        std::iter::once(bin.to_path_buf()).chain(std::env::split_paths(&path)),
    )
    .unwrap();
    Options {
        args: if with_person_key {
            vec!["--parent-stdin".into()]
        } else {
            Vec::new()
        },
        env: vec![
            ("ORBYN_DEV_DISABLE_PERSON_KEY".into(), "0".into()),
            ("ORBYN_LOG".into(), "info".into()),
            (
                "ORBYN_DEVICES_FAKE_DIR".into(),
                dir.path().to_string_lossy().into_owned(),
            ),
            ("PATH".into(), path.to_string_lossy().into_owned()),
        ],
    }
}

fn service(dir: &TempDir) -> Service {
    service_with(dir, true)
}

fn service_with(dir: &TempDir, with_person_key: bool) -> Service {
    let bin = fake(dir.path());
    Service::start(
        &dir.path().join("data"),
        options(dir, &bin, with_person_key),
    )
}

fn calls(dir: &TempDir) -> String {
    std::fs::read_to_string(dir.path().join("calls.ndjson")).unwrap_or_default()
}

fn action_path() -> &'static str {
    "/api/devices/android:Pixel_8/actions"
}

async fn post_without_key(service: &Service, body: Value) -> (u16, Value) {
    let response = service
        .post(action_path())
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    (status, response.json().await.unwrap_or(Value::Null))
}

async fn post_raw(service: &Service, body: &str) -> (u16, Value) {
    let mut request = service
        .post(action_path())
        .header("content-type", "application/json");
    if let Some(key) = &service.person_key {
        request = request.header("x-orbyn-person", key);
    }
    let response = request.body(body.to_owned()).send().await.unwrap();
    let status = response.status().as_u16();
    (status, response.json().await.unwrap_or(Value::Null))
}

#[tokio::test]
async fn device_reads_are_local_and_every_action_is_person_class() {
    let dir = TempDir::new();
    let service = service(&dir);

    // Every action type, before any read, with the service token alone.
    let actions = [
        json!({ "type": "screenshot" }),
        json!({ "type": "rotate", "value": "portrait" }),
        json!({ "type": "button", "value": "home" }),
        json!({ "type": "power_off" }),
        json!({ "type": "fold" }),
        json!({ "type": "unfold" }),
        json!({ "type": "hinge", "angle": 90 }),
        json!({ "type": "hinge_preset", "preset": "half_open" }),
    ];
    for body in actions {
        let (status, answer) = post_without_key(&service, body).await;
        assert_eq!(status, 403, "{answer}");
        assert_eq!(answer["code"], "person_required", "{answer}");
        assert_eq!(
            answer["message"],
            "This action needs the person at the desktop app."
        );
    }
    assert!(
        calls(&dir).is_empty(),
        "a refused write started a device tool: {}",
        calls(&dir)
    );

    // Reads need no key.
    let list = service.json("/api/devices").await;
    assert_eq!(list["devices"][0]["id"], "android:Pixel_8");
    assert_eq!(list["platforms"][0]["available"], true);
    let detail = service.json("/api/devices/android:Pixel_8").await;
    assert_eq!(detail["id"], "android:Pixel_8");
    assert_eq!(detail["name"], "Pixel_8");
    assert_eq!(detail["actions"][0], "screenshot");
    assert!(detail.get("device").is_none(), "{detail}");
}

#[tokio::test]
async fn a_shell_less_service_answers_desktop_required() {
    let dir = TempDir::new();
    let service = service_with(&dir, false);
    assert!(service.person_key.is_none());
    let (status, answer) = post_without_key(&service, json!({ "type": "screenshot" })).await;
    assert_eq!(status, 403, "{answer}");
    assert_eq!(answer["code"], "desktop_required", "{answer}");
    assert_eq!(
        answer["message"],
        "This action is only available in the desktop app."
    );
    assert!(
        calls(&dir).is_empty(),
        "a refused write started a device tool: {}",
        calls(&dir)
    );
}

#[tokio::test]
async fn device_errors_have_the_fixed_copy() {
    let dir = TempDir::new();
    let service = service(&dir);
    let (status, answer) = service
        .send(
            "/api/devices/android:Nope/actions",
            json!({ "type": "screenshot" }),
        )
        .await;
    assert_eq!(status, 404, "{answer}");
    assert_eq!(answer["code"], "device_not_found");
    assert_eq!(answer["message"], "That device is no longer there.");

    let _ = std::fs::remove_file(dir.path().join("calls.ndjson"));
    let (status, answer) = service
        .send(
            action_path(),
            json!({ "type": "screenshot", "extra": true }),
        )
        .await;
    assert_eq!(status, 400, "{answer}");
    assert_eq!(answer["code"], "invalid_action");
    assert_eq!(answer["message"], "That action is not valid.");

    let (status, answer) = service.send(action_path(), json!({ "type": "tap" })).await;
    assert_eq!(status, 400, "{answer}");
    assert_eq!(answer["code"], "invalid_action");
    assert_eq!(answer["message"], "That action is not valid.");

    let (status, answer) = service
        .send(action_path(), json!({ "type": "hinge", "angle": 0 }))
        .await;
    assert_eq!(status, 400, "{answer}");
    assert_eq!(answer["code"], "invalid_action");

    let long = format!(
        "{{\"type\":\"screenshot\",\"pad\":\"{}\"}}",
        "x".repeat(5 * 1024)
    );
    let (status, answer) = post_raw(&service, &long).await;
    assert_eq!(status, 400, "{answer}");
    assert_eq!(answer["code"], "invalid_action");
    assert_eq!(answer["message"], "That action is not valid.");
    assert!(
        calls(&dir).is_empty(),
        "an invalid action started a tool: {}",
        calls(&dir)
    );

    let (status, answer) = service.send(action_path(), json!({ "type": "fold" })).await;
    assert_eq!(status, 422, "{answer}");
    assert_eq!(answer["code"], "not_foldable");
    assert_eq!(answer["message"], "This device does not fold.");

    let (status, answer) = service
        .send(
            "/api/devices/ios:UDID-1/actions",
            json!({ "type": "rotate", "value": "portrait" }),
        )
        .await;
    assert_eq!(status, 422, "{answer}");
    assert_eq!(answer["code"], "action_unsupported");
    assert_eq!(answer["message"], "This device cannot do that.");

    std::fs::write(dir.path().join("booted"), "0").unwrap();
    let (status, answer) = service
        .send(action_path(), json!({ "type": "screenshot" }))
        .await;
    assert_eq!(status, 409, "{answer}");
    assert_eq!(answer["code"], "device_not_booted");
    assert_eq!(answer["message"], "Start the device first.");
    std::fs::write(dir.path().join("booted"), "1").unwrap();

    std::fs::write(dir.path().join("action_exit"), "2").unwrap();
    let (status, answer) = service
        .send(action_path(), json!({ "type": "button", "value": "home" }))
        .await;
    assert_eq!(status, 502, "{answer}");
    assert_eq!(answer["code"], "tool_failed");
    assert_eq!(
        answer["message"],
        "The device tool could not do that. Try again."
    );
}

#[tokio::test]
async fn a_screenshot_answers_a_png_and_the_log_keeps_no_body_or_image() {
    let dir = TempDir::new();
    let mut service = service(&dir);
    // The shared fake puts this text in the PNG after its magic bytes, so the answer can
    // be compared whole, and `image` is a string no log line may carry.
    std::fs::write(dir.path().join("screenshot_text"), PNG_CANARY).unwrap();
    let (status, answer) = service
        .send(action_path(), json!({ "type": "screenshot" }))
        .await;
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["ok"], true);
    assert_eq!(answer["device"]["id"], "android:Pixel_8");
    let image = answer["image"].as_str().unwrap().to_owned();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&image)
        .expect("the image is base64");
    let mut expected = [b"\x89PNG\r\n\x1a\n".as_slice(), PNG_CANARY.as_bytes()].concat();
    expected.resize(64, b'A');
    assert_eq!(bytes, expected);

    // A refused body that carries a canary must not reach the log either.
    let (status, _) = service
        .send(
            action_path(),
            json!({ "type": "screenshot", "canary": "ORBYN_BODY_CANARY" }),
        )
        .await;
    assert_eq!(status, 400);

    service.kill();
    let mut log = String::new();
    service
        .child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut log)
        .unwrap();
    assert!(log.contains("screenshot"), "{log}");
    assert!(log.contains("android:Pixel_8"), "{log}");
    assert!(log.contains("duration_ms"), "{log}");
    for secret in [PNG_CANARY, "ORBYN_BODY_CANARY", image.as_str()] {
        assert!(!log.contains(secret), "{secret} leaked into the log");
    }
}

/// Waits for the shared fake's action marker, which it writes after logging its argv
/// and before the delay, so this marks the action running, not a guess about scheduling.
async fn until_action_started(dir: &TempDir) {
    for _ in 0..1000 {
        if dir.path().join("action_started").exists() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the first action never started");
}

#[tokio::test]
async fn a_second_action_while_one_runs_is_device_busy_and_starts_no_tool() {
    let dir = TempDir::new();
    let mut service = service(&dir);
    // The shared fake holds the first action open, so the second meets the service's
    // one-writer-per-device guard instead of racing it.
    std::fs::write(dir.path().join("action_delay_ms"), "2000").unwrap();
    let first = async {
        service
            .send(action_path(), json!({ "type": "button", "value": "home" }))
            .await
    };
    let second = async {
        until_action_started(&dir).await;
        service
            .send(action_path(), json!({ "type": "button", "value": "back" }))
            .await
    };
    let ((first_status, first_body), (second_status, second_body)) = tokio::join!(first, second);
    assert_eq!(first_status, 200, "{first_body}");
    assert_eq!(second_status, 409, "{second_body}");
    assert_eq!(second_body["code"], "device_busy");
    assert_eq!(
        second_body["message"],
        "Another action is running on this device."
    );

    // The refused second action started no tool: only the first keyevent became argv.
    let calls = calls(&dir);
    let keyevents = calls
        .lines()
        .filter(|line| line.contains("\"input\"") && line.contains("\"keyevent\""))
        .count();
    assert_eq!(keyevents, 1, "{calls}");

    // Both attempts are in the action log, the refused one with no exit status.
    service.kill();
    let mut log = String::new();
    service
        .child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut log)
        .unwrap();
    assert_eq!(
        log.matches("device action").count(),
        2,
        "the refused attempt left no action log line: {log}"
    );
}
