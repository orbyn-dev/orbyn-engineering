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
    json!({
        "id": id,
        "name": "Snap Shot.png",
        "mime_type": "image/png",
        "size_bytes": capture.png().len(),
        "source": {
            "kind": "snap_shot",
            "captured_at": capture.captured_at(),
            "app_name": capture.app_name(),
            "window_title": capture.window_title(),
            "app_id": capture.app_id(),
        },
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
pub fn snapshot_take(
    window: WebviewWindow,
    snapshot: State<'_, SnapShot>,
    id: String,
) -> Result<Value, &'static str> {
    require_caller(&window)?;
    let capture = {
        let mut pending = snapshot.pending.lock().map_err(|_| UNAVAILABLE)?;
        take_capture(&mut pending, &id)?
    };
    Ok(take_answer(&id, capture))
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
        match action {
            // Setup is Planned. Reading the grants back is safe; prompting or
            // opening System Settings is not this command's job.
            SetupAction::AllowScreenRecording | SetupAction::AllowAccessibility => {}
            SetupAction::TestCapture => {
                snapshot.start_capture(&app)?;
            }
            SetupAction::RetryShortcut => snapshot.apply_shortcut(&app),
        }
        Ok(snapshot.state())
    })
    .await
    .map_err(|_| UNAVAILABLE)?
}

#[cfg(test)]
mod tests {
    use super::super::pending::PENDING_TTL;
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
        let id = pending.insert(capture());
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
