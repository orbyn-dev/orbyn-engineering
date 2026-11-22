//! Devices: the local AVD and simulator list, one device and its actions, and the
//! typed action write (docs/devices.md). Reads are local; every write is Person-class.

use axum::body::to_bytes;
use axum::extract::{Path, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::routing::{get, post};
use axum::{Json, Router};
use orbyn_contracts::{DeviceActionInput, DeviceActionOutput, DeviceDetail, DeviceList};
use orbyn_core::devices::INVALID_ACTION_MESSAGE;

use crate::http::{ApiFailure, ApiResult, AppState};

/// The typed action body is at most 4 KiB, before any process can start.
const MAX_ACTION_BODY: usize = 4 * 1024;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/devices", get(list))
        .route("/api/devices/{id}", get(detail))
        .route("/api/devices/{id}/actions", post(act))
}

async fn list(State(state): State<AppState>) -> Json<DeviceList> {
    Json(state.core.devices.list().await)
}

async fn detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<DeviceDetail>> {
    Ok(Json(state.core.devices.detail(&id).await?))
}

async fn act(
    State(state): State<AppState>,
    Path(id): Path<String>,
    request: Request,
) -> ApiResult<Json<DeviceActionOutput>> {
    if !json_content_type(request.headers()) {
        return Err(invalid_action());
    }
    let bytes = to_bytes(request.into_body(), MAX_ACTION_BODY)
        .await
        .map_err(|_| invalid_action())?;
    let input: DeviceActionInput = serde_json::from_slice(&bytes).map_err(|_| invalid_action())?;
    if !input.is_valid() {
        return Err(invalid_action());
    }
    Ok(Json(state.core.devices.act(&id, input).await?))
}

fn invalid_action() -> ApiFailure {
    ApiFailure::new(
        StatusCode::BAD_REQUEST,
        "invalid_action",
        INVALID_ACTION_MESSAGE,
    )
}

/// `application/json`, or an `application/*+json` type.
fn json_content_type(headers: &HeaderMap) -> bool {
    let Some(value) = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let essence = value
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    essence == "application/json"
        || (essence.starts_with("application/") && essence.ends_with("+json"))
}
