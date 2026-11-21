//! App and engine updates: status, check and apply.

use axum::Json;
use axum::Router;
use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use serde::Deserialize;

use crate::http::{ApiResult, AppState, LOCAL_USER, to_the_end};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/updates", get(updates))
        .route("/api/updates/check", post(check_updates))
        .route("/api/updates/app/apply", post(apply_app_update))
        .route("/api/updates/app/download", post(download_app_update))
        .route("/api/updates/apply-all", post(apply_all_updates))
}

/// The last known status, so the poll behind it never waits on Git or the engines.
/// `?refresh=1` reads them now.
async fn updates(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
) -> Json<orbyn_contracts::UpdateStatus> {
    let refresh = matches!(query.get("refresh").map(String::as_str), Some("1" | "true"));
    Json(state.core.updates.cached_status(refresh).await)
}

async fn check_updates(State(state): State<AppState>) -> Json<orbyn_contracts::UpdateStatus> {
    Json(state.core.updates.check().await)
}

#[derive(Deserialize)]
struct AppApplyQuery {
    #[serde(default)]
    complete: bool,
}

async fn apply_app_update(
    State(state): State<AppState>,
    Query(query): Query<AppApplyQuery>,
    Json(request): Json<orbyn_contracts::AppUpdateRequest>,
) -> ApiResult<StatusCode> {
    if query.complete {
        state.core.updates.complete_app(LOCAL_USER, request).await?;
    } else {
        state.core.updates.install_app(LOCAL_USER, request).await?;
    }
    Ok(StatusCode::ACCEPTED)
}

async fn apply_all_updates(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    let updates = state.core.updates.clone();
    Ok(Json(
        to_the_end(async move { updates.apply_all(LOCAL_USER).await }).await?,
    ))
}

async fn download_app_update(
    State(state): State<AppState>,
    Json(request): Json<orbyn_contracts::AppUpdateRequest>,
) -> ApiResult<StatusCode> {
    state.core.updates.download_app(LOCAL_USER, request).await?;
    Ok(StatusCode::ACCEPTED)
}
