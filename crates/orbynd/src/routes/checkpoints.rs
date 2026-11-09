//! Checkpoints: what a run changed, and restoring or rewinding to an earlier state.

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use orbyn_contracts::{RestoreChanges, RestoreWorkspace, RewindThread};
use serde::Deserialize;

use crate::http::{ApiJson, ApiResult, AppState, LOCAL_USER};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/threads/{id}/runs/{run}/changes", get(run_changes))
        .route(
            "/api/threads/{id}/runs/{run}/changes/file",
            get(changed_file),
        )
        .route(
            "/api/threads/{id}/runs/{run}/changes/restore",
            post(restore_changes),
        )
        .route("/api/threads/{id}/checkpoints", get(checkpoints))
        .route("/api/threads/{id}/restore", post(restore_workspace))
        .route("/api/threads/{id}/rewind", post(rewind_thread))
}

async fn run_changes(
    State(state): State<AppState>,
    Path((id, run)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.run_changes(&id, &run)?))
}

#[derive(Deserialize)]
struct ChangedFileQuery {
    path: String,
}

async fn changed_file(
    State(state): State<AppState>,
    Path((id, run)): Path<(String, String)>,
    Query(query): Query<ChangedFileQuery>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.changed_file(&id, &run, &query.path)?))
}

async fn restore_changes(
    State(state): State<AppState>,
    Path((id, run)): Path<(String, String)>,
    ApiJson(body): ApiJson<RestoreChanges>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state.core.restore_changes(&id, &run, &body, LOCAL_USER)?,
    ))
}

async fn checkpoints(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    state.core.store.thread(&id)?;
    Ok(Json(state.core.store.checkpoints(&id)?))
}

async fn restore_workspace(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<RestoreWorkspace>,
) -> ApiResult<StatusCode> {
    state.core.restore_workspace(&id, &body, LOCAL_USER).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn rewind_thread(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<RewindThread>,
) -> ApiResult<StatusCode> {
    state.core.rewind_thread(&id, &body, LOCAL_USER).await?;
    Ok(StatusCode::NO_CONTENT)
}
