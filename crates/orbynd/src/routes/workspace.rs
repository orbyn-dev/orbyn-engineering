//! The managed skills workspace: status, setup, update and rollback.

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use orbyn_contracts::{LibraryUpdateRequest, WorkspaceSetup};

use crate::http::{ApiJson, ApiResult, AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/workspace", get(workspace_status))
        .route("/api/workspace/setup", post(workspace_setup))
        .route("/api/workspace/update", post(workspace_update))
        .route("/api/workspace/rollback", post(workspace_rollback))
}

async fn workspace_status(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    let projects = state.core.store.list_projects()?;
    Ok(Json(state.core.workspace.status(&projects).await?))
}

async fn workspace_setup(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<WorkspaceSetup>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.setup_workspace(body).await?))
}

async fn workspace_update(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<LibraryUpdateRequest>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.workspace.update(body.apply).await?))
}

async fn workspace_rollback(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.workspace.rollback().await?))
}
