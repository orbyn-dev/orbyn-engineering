//! Code Mode (docs/code-mode.md). Behind `CoreConfig::code_mode`: with it off every route here answers 404.
//! Reads are `Local`. Switching an engine's Code Mode is a `Person` route: it changes what an engine may call.

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::get;
use orbyn_contracts::CodeModeSetting;

use crate::http::{ApiJson, ApiResult, AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/code-runs/{id}", get(run))
        .route("/api/threads/{id}/code-runs", get(thread_runs))
        .route("/api/engines/{id}/code-mode", get(setting).put(set_setting))
}

async fn run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.code_run_record(&id)?))
}

async fn thread_runs(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.code_runs_list(&id)?))
}

async fn setting(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.code_mode_setting(&id)?))
}

async fn set_setting(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<CodeModeSetting>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.set_code_mode(&id, body.enabled)?))
}
