//! The six Snap Shot commands. Every one checks that the caller is the bundled app's main window before it does
//! anything else.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::Deserialize;
use serde_json::{Value, json};
use tauri::ipc::{InvokeBody, Request};
use tauri::{AppHandle, Manager, State, WebviewWindow};

use super::SnapShot;
use super::pending::{Capture, Pending};
use super::settings::Settings;
use super::state::SnapShotState;

const FORBIDDEN: &str = "forbidden";
const NOT_FOUND: &str = "not_found";
const UNAVAILABLE: &str = "unavailable";
const INVALID_SETTINGS: &str = "invalid_settings";

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupAction {
    AllowScreenRecording,
    AllowAccessibility,
    TestCapture,
    RetryShortcut,
}

fn require_caller(window: &WebviewWindow) -> Result<(), &'static str> {
    crate::presence::require_caller(window).map_err(|_| FORBIDDEN)
}

fn take_answer(id: &str, capture: Capture) -> Value {
    let mut source = json!({
        "kind": "snap_shot",
        "captured_at": capture.captured_at(),
        "app_name": capture.app_name(),
        "window_title": capture.window_title(),
        "app_id": capture.app_id(),
    });
    if let Some(accessible_text) = capture.accessible_text() {
        source["accessible_text"] = json!(accessible_text);
    }
    json!({
        "id": id,
        "name": "Snap Shot.png",
        "mime_type": "image/png",
        "size_bytes": capture.png().len(),
        "source": source,
        "png_base64": STANDARD.encode(capture.png()),
    })
}

fn take_capture(pending: &mut Pending, id: &str) -> Result<Capture, &'static str> {
    pending.take(id).ok_or(NOT_FOUND)
}

fn settings_from_body(body: &InvokeBody) -> Result<Settings, &'static str> {
    match body {
        InvokeBody::Json(value) => {
            serde_json::from_value(value.clone()).map_err(|_| INVALID_SETTINGS)
        }
        InvokeBody::Raw(_) => Err(INVALID_SETTINGS),
    }
}

fn settings_from(request: Request<'_>) -> Result<Settings, &'static str> {
    settings_from_body(request.body())
}

#[tauri::command]
pub fn snapshot_state(
    window: WebviewWindow,
    snapshot: State<'_, SnapShot>,
) -> Result<SnapShotState, &'static str> {
    require_caller(&window)?;
    Ok(snapshot.state())
}

/// `snapshot_configure` takes the flat settings object the page sends, not a
/// `{ settings: ... }` wrapper.
#[tauri::command]
pub async fn snapshot_configure(
    app: AppHandle,
    window: WebviewWindow,
    request: Request<'_>,
) -> Result<SnapShotState, &'static str> {
    require_caller(&window)?;
    let settings = settings_from(request)?;
    tauri::async_runtime::spawn_blocking(move || {
        let snapshot = app.state::<SnapShot>();
        snapshot.configure(&app, settings)
    })
    .await
    .map_err(|_| UNAVAILABLE)?
}

#[tauri::command]
pub fn snapshot_capture(
    app: AppHandle,
    window: WebviewWindow,
    snapshot: State<'_, SnapShot>,
) -> Result<Value, &'static str> {
    require_caller(&window)?;
    let id = snapshot.start_capture(&app)?;
    Ok(json!({ "id": id }))
}

#[tauri::command]
pub async fn snapshot_take(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
) -> Result<Value, &'static str> {
    require_caller(&window)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<Value, &'static str> {
        let snapshot = app.state::<SnapShot>();
        let capture = {
            let mut pending = snapshot.pending.lock().map_err(|_| UNAVAILABLE)?;
            take_capture(&mut pending, &id)?
        };
        Ok(take_answer(&id, capture))
    })
    .await
    .map_err(|_| UNAVAILABLE)?
}

#[tauri::command]
pub fn snapshot_discard(
    window: WebviewWindow,
    snapshot: State<'_, SnapShot>,
    id: String,
) -> Result<Value, &'static str> {
    require_caller(&window)?;
    let _ = snapshot.gate.cancel(&id);
    if let Ok(mut pending) = snapshot.pending.lock() {
        pending.discard(&id);
    }
    Ok(Value::Null)
}

#[tauri::command]
pub async fn snapshot_setup(
    app: AppHandle,
    window: WebviewWindow,
    action: SetupAction,
) -> Result<SnapShotState, &'static str> {
    require_caller(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        let snapshot = app.state::<SnapShot>();
        let state = match action {
            // The person's click asks macOS for the grant and, when it is still
            // missing, opens the Screen Recording pane. Accessibility only reads
            // the grant back: accessible text is Planned and this build needs none.
            SetupAction::AllowScreenRecording => snapshot.allow_screen_recording()?,
            SetupAction::AllowAccessibility => snapshot.state(),
            SetupAction::TestCapture => {
                snapshot.start_capture(&app)?;
                snapshot.state()
            }
            SetupAction::RetryShortcut => {
                snapshot.apply_shortcut(&app);
                snapshot.state()
            }
        };
        Ok(state)
    })
    .await
    .map_err(|_| UNAVAILABLE)?
}

#[cfg(test)]
mod tests {
    use super::super::pending::{
        MAX_APP_ID_CHARS, MAX_APP_NAME_CHARS, MAX_WINDOW_TITLE_CHARS, PENDING_TTL,
    };
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    fn capture() -> Capture {
        Capture::new(
            b"png bytes".to_vec(),
            "Notes".to_owned(),
            "Weekly plan".to_owned(),
            "com.apple.Notes".to_owned(),
            "2026-11-22T10:00:00Z".to_owned(),
        )
    }

    fn clocked(start: Instant) -> (Arc<Mutex<Instant>>, Pending) {
        let clock = Arc::new(Mutex::new(start));
        let handle = Arc::clone(&clock);
        let pending = Pending::with_clock(Arc::new(move || *handle.lock().expect("test clock")));
        (clock, pending)
    }

    #[test]
    fn take_answers_once_then_not_found() {
        let mut pending = Pending::default();
        let id = pending.insert(capture());
        let taken = take_capture(&mut pending, &id).unwrap();
        let answer = take_answer(&id, taken);
        assert_eq!(answer["id"], id);
        assert_eq!(answer["name"], "Snap Shot.png");
        assert_eq!(answer["mime_type"], "image/png");
        assert_eq!(answer["size_bytes"], 9_u64);
        assert!(matches!(take_capture(&mut pending, &id), Err(NOT_FOUND)));
    }

    #[test]
    fn the_answer_has_the_source_and_the_png_round_trips() {
        let mut pending = Pending::default();
        let id = pending.insert(capture().with_accessible_text(None));
        let taken = take_capture(&mut pending, &id).unwrap();
        let answer = take_answer(&id, taken);
        assert_eq!(
            answer["source"],
            json!({
                "kind": "snap_shot",
                "captured_at": "2026-11-22T10:00:00Z",
                "app_name": "Notes",
                "window_title": "Weekly plan",
                "app_id": "com.apple.Notes"
            })
        );
        assert!(answer["source"].get("accessible_text").is_none());
        let encoded = answer["png_base64"].as_str().unwrap();
        assert_eq!(STANDARD.decode(encoded).unwrap(), b"png bytes");
    }

    #[test]
    fn the_answer_carries_accessible_text_when_present() {
        let text = "Read this from Notes.\nSecond line.";
        let mut pending = Pending::default();
        let id = pending.insert(capture().with_accessible_text(Some(text.to_owned())));
        let taken = take_capture(&mut pending, &id).unwrap();
        let answer = take_answer(&id, taken);
        assert_eq!(answer["source"]["accessible_text"].as_str(), Some(text));
    }

    #[test]
    fn snapshot_take_checks_the_caller_then_encodes_off_the_async_thread() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/snapshot/commands.rs");
        let source = std::fs::read_to_string(path).unwrap().replace("\r\n", "\n");
        let marker = format!("#[tauri::command]{}pub async fn snapshot_take(", '\n');
        let take = source
            .split(&marker)
            .nth(1)
            .expect("snapshot_take is an async command");
        let take = take.split("\n}\n").next().unwrap();
        let caller = take
            .find("require_caller(&window)?;")
            .expect("the caller check is first");
        let blocking = take
            .find("spawn_blocking")
            .expect("encoding runs on the blocking pool");
        assert!(
            caller < blocking,
            "the caller check must come before the blocking work"
        );
        assert!(
            take.contains("take_capture"),
            "the capture is consumed once"
        );
    }

    #[test]
    fn take_answers_cut_the_source_to_the_wire_limits() {
        let mut pending = Pending::default();
        let id = pending.insert(Capture::new(
            b"png".to_vec(),
            "a".repeat(500),
            "t".repeat(5_000),
            "i".repeat(500),
            "2026-11-22T10:00:00Z".to_owned(),
        ));
        let taken = take_capture(&mut pending, &id).unwrap();
        let answer = take_answer(&id, taken);
        let source = &answer["source"];
        assert_eq!(
            source["app_name"].as_str().unwrap().chars().count(),
            MAX_APP_NAME_CHARS
        );
        assert_eq!(
            source["window_title"].as_str().unwrap().chars().count(),
            MAX_WINDOW_TITLE_CHARS
        );
        assert_eq!(
            source["app_id"].as_str().unwrap().chars().count(),
            MAX_APP_ID_CHARS
        );
    }

    #[test]
    fn configure_reads_the_flat_settings_object() {
        let body = InvokeBody::Json(json!({
            "snapshot_enabled": true,
            "snapshot_shortcut": { "kind": "chord", "key": "9", "mod": true, "shift": true }
        }));
        let settings = settings_from_body(&body).unwrap();
        assert!(settings.snapshot_enabled);
        assert_eq!(settings.snapshot_shortcut.key, "9");
        assert!(!settings.snapshot_include_accessibility);
        assert_eq!(
            settings_from_body(&InvokeBody::Json(json!({ "unknown": true }))),
            Err(INVALID_SETTINGS)
        );
        assert_eq!(
            settings_from_body(&InvokeBody::Raw(Vec::new())),
            Err(INVALID_SETTINGS)
        );
    }

    #[test]
    fn a_capture_older_than_five_minutes_is_not_found() {
        let start = Instant::now();
        let (clock, mut pending) = clocked(start);
        let id = pending.insert(capture());
        *clock.lock().expect("test clock") = start + PENDING_TTL;
        assert!(matches!(take_capture(&mut pending, &id), Err(NOT_FOUND)));
    }
}
