//! App and engine updates: status, check and apply.

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};

use crate::http::{ApiResult, AppState, LOCAL_USER, to_the_end};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/updates", get(updates))
        .route("/api/updates/check", post(check_updates))
        .route("/api/updates/app/apply", post(apply_app_update))
        .route("/api/updates/apply-all", post(apply_all_updates))
}

async fn updates(State(state): State<AppState>) -> Json<orbyn_contracts::UpdateStatus> {
    Json(state.core.updates.status().await)
}

async fn check_updates(State(state): State<AppState>) -> Json<orbyn_contracts::UpdateStatus> {
    Json(state.core.updates.check().await)
}

async fn apply_app_update(State(state): State<AppState>) -> ApiResult<StatusCode> {
    state.core.updates.apply_app(LOCAL_USER).await?;
    Ok(StatusCode::ACCEPTED)
}

async fn apply_all_updates(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    let updates = state.core.updates.clone();
    Ok(Json(
        to_the_end(async move { updates.apply_all(LOCAL_USER).await }).await?,
    ))
}
