//! Code Mode (docs/code-mode.md). Behind `CoreConfig::code_mode`: with it off every route here answers 404.
//! Reads are `Local`. Switching an engine's Code Mode and cancelling a run are `Person` routes: only the
//! person may change what an engine can call, or end a run that is already doing something.

use axum::Json;
use axum::Router;
use axum::body::to_bytes;
use axum::extract::{Path, Request, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use orbyn_contracts::CodeModeSetting;

use crate::http::{ApiFailure, ApiJson, ApiResult, AppState};

/// A cancel takes no body; anything over this is refused before the run is read.
const MAX_CANCEL_BODY: usize = 1024;
const CANCEL_BODY_MESSAGE: &str = "This route takes no body.";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/code-runs/{id}", get(run))
        .route("/api/code-runs/{id}/cancel", post(cancel))
        .route("/api/threads/{id}/code-runs", get(thread_runs))
        .route("/api/engines/{id}/code-mode", get(setting).put(set_setting))
}

async fn run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.code_run_record(&id)?))
}

async fn cancel(
    State(state): State<AppState>,
    Path(id): Path<String>,
    request: Request,
) -> ApiResult<impl IntoResponse> {
    // The flag first: with Code Mode off this route does not exist, whatever the body says.
    state.core.code_mode_required()?;
    let _ = to_bytes(request.into_body(), MAX_CANCEL_BODY)
        .await
        .map_err(|_| invalid_cancel_body())?;
    Ok(Json(state.core.code_run_cancel(&id).await?))
}

fn invalid_cancel_body() -> ApiFailure {
    ApiFailure::new(
        StatusCode::BAD_REQUEST,
        "invalid_input",
        CANCEL_BODY_MESSAGE,
    )
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
