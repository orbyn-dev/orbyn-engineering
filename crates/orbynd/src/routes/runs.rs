//! Runs: start, cancel and force-stop a run, and a thread's workflows.

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use orbyn_contracts::ModelSelection;
use orbyn_contracts::{StartRun, UpdateThread, WorkflowStop};
use orbyn_core::agents::selection;

use crate::http::{ApiJson, ApiResult, AppState, LOCAL_USER};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/threads/{id}/runs", post(start_run))
        .route("/api/runs/{id}/cancel", post(cancel_run))
        .route("/api/threads/{id}/force-stop", post(force_stop))
        .route(
            "/api/threads/{id}/workflows/{item}/script",
            get(workflow_script),
        )
        .route(
            "/api/threads/{id}/workflows/{item}/stop",
            post(stop_workflow),
        )
}

async fn start_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<StartRun>,
) -> ApiResult<impl IntoResponse> {
    let wanted = ModelSelection {
        model: body.model.clone(),
        effort: body.effort.clone(),
        options: body.options.clone(),
    };
    state.core.check_turn(&id, &wanted).await?;
    if body.permission_mode.is_some() || !selection::is_empty(&wanted) {
        let patch = UpdateThread {
            permission_mode: body.permission_mode,
            model: wanted.model,
            effort: wanted.effort,
            options: Some(wanted.options),
            ..UpdateThread::default()
        };
        state.core.update_thread(&id, &patch, LOCAL_USER)?;
    }
    let run = state
        .core
        .runs
        .admit_with(&id, &body.prompt, &body.attachment_ids, LOCAL_USER)?;
    Ok((StatusCode::ACCEPTED, Json(run)))
}

async fn cancel_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.core.runs.cancel(&id)?;
    Ok(StatusCode::ACCEPTED)
}

async fn force_stop(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.runs.force_stop(&id, LOCAL_USER).await?))
}

async fn workflow_script(
    State(state): State<AppState>,
    Path((id, item)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.workflow_script(&id, &item)?))
}

async fn stop_workflow(
    State(state): State<AppState>,
    Path((id, item)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    state.core.runs.stop_workflow(&id, &item, LOCAL_USER)?;
    Ok((StatusCode::ACCEPTED, Json(WorkflowStop { requested: true })))
}
