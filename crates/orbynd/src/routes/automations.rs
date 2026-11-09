//! Scheduled tasks and their webhooks.

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post, put};
use orbyn_contracts::{TaskEnabled, UpsertTask};
use serde_json::json;

use crate::http::{ApiFailure, ApiJson, ApiResult, AppState, LOCAL_USER};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/tasks", get(tasks).post(save_task))
        .route("/api/tasks/{id}", put(update_task).delete(delete_task))
        .route("/api/tasks/{id}/enabled", post(set_task_enabled))
        .route("/api/tasks/{id}/run", post(run_task))
        .route(
            "/api/tasks/{id}/webhook",
            get(task_webhook).post(rotate_webhook),
        )
        .route("/api/tasks/{id}/runs", get(task_runs))
        .route("/api/tasks/{id}/deliveries", get(task_deliveries))
}

pub fn public_routes() -> Router<AppState> {
    Router::new().route("/hooks/{id}/{token}", post(webhook))
}

async fn tasks(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.tasks()?))
}

async fn save_task(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<UpsertTask>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.save_task(body, LOCAL_USER)?))
}

async fn update_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<UpsertTask>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.update_task(&id, body, LOCAL_USER)?))
}

async fn set_task_enabled(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<TaskEnabled>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.set_task_enabled(&id, body.enabled)?))
}

async fn delete_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.core.delete_task(&id, LOCAL_USER)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn run_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let thread = state.core.run_task(&id).await?;
    Ok((StatusCode::ACCEPTED, Json(json!({ "thread_id": thread }))))
}

async fn rotate_webhook(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.rotate_webhook(&id, LOCAL_USER)?))
}

async fn task_webhook(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.webhook_address(&id)?))
}

async fn task_runs(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.task_runs(&id)?))
}

async fn task_deliveries(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.deliveries(&id)?))
}

/// Public, because a sender cannot hold a session. The token in the path is the
/// credential, and a wrong one is the same `404` as an unknown task.
async fn webhook(
    State(state): State<AppState>,
    Path((id, token)): Path<(String, String)>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<impl IntoResponse> {
    let headers = headers
        .iter()
        .filter_map(|(name, value)| {
            Some((name.as_str().to_owned(), value.to_str().ok()?.to_owned()))
        })
        .collect();
    match state.core.webhook(&id, &token, &headers, &body).await {
        Ok(delivery) => {
            let status = if delivery.status == "accepted" {
                StatusCode::ACCEPTED
            } else {
                StatusCode::OK
            };
            Ok((
                status,
                Json(json!({ "status": delivery.status, "id": delivery.id })),
            ))
        }
        Err(_) => Err(ApiFailure::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "Not found.",
        )),
    }
}
