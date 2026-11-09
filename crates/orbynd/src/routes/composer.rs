//! The composer: the queue of messages, steering, attachments, mentions and slash commands.

use std::collections::HashMap;

use axum::Json;
use axum::Router;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use orbyn_contracts::ModelSelection;
use orbyn_contracts::{EditQueuedMessage, EnqueueMessage, SteerResult, SteerRun, UpdateThread};
use orbyn_core::agents::selection;
use serde::Deserialize;

use crate::http::{ApiFailure, ApiJson, ApiResult, AppState, LOCAL_USER};

/// A little over the largest allowed file; core enforces the exact limits.
const UPLOAD_LIMIT: usize = 51 * 1024 * 1024;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/threads/{id}/queue", get(list_queue).post(enqueue))
        .route("/api/threads/{id}/queue/resume", post(resume_queue))
        .route("/api/queue/{id}", patch(edit_queued).delete(remove_queued))
        .route("/api/queue/{id}/send-now", post(send_queued_now))
        .route("/api/threads/{id}/steer", post(steer))
        .route(
            "/api/threads/{id}/attachments",
            get(list_attachments)
                .post(upload_attachment)
                .layer(DefaultBodyLimit::max(UPLOAD_LIMIT)),
        )
        .route("/api/attachments/{id}", delete(remove_attachment))
        .route("/api/attachments/{id}/content", get(attachment_content))
        .route("/api/threads/{id}/mentions", get(mentions))
        .route("/api/threads/{id}/commands", get(slash_commands))
}

async fn list_queue(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    state.core.store.thread(&id)?;
    Ok(Json(state.core.store.list_queue(&id)?))
}

async fn enqueue(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<EnqueueMessage>,
) -> ApiResult<impl IntoResponse> {
    let wanted = ModelSelection {
        model: body.model,
        effort: body.effort,
        options: body.options,
    };
    state.core.check_turn(&id, &wanted).await?;
    let message = state
        .core
        .runs
        .enqueue_with(&id, &body.prompt, &body.attachment_ids, &wanted)?;
    Ok((StatusCode::CREATED, Json(message)))
}

async fn edit_queued(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<EditQueuedMessage>,
) -> ApiResult<impl IntoResponse> {
    orbyn_core::conversation::composer::check_prompt(&body.prompt)?;
    Ok(Json(state.core.store.edit_queued(&id, &body.prompt)?))
}

async fn remove_queued(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.core.store.remove_queued(&id)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn send_queued_now(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.core.runs.send_now(&id)?;
    Ok(StatusCode::ACCEPTED)
}

async fn resume_queue(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.core.runs.resume_queue(&id)?;
    Ok(StatusCode::ACCEPTED)
}

async fn steer(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<SteerRun>,
) -> ApiResult<impl IntoResponse> {
    let wanted = ModelSelection {
        model: body.model,
        effort: body.effort,
        options: body.options,
    };
    state.core.check_turn(&id, &wanted).await?;
    let delivered = state.core.runs.steer(&id, &body.prompt)?;
    if !selection::is_empty(&wanted) {
        let patch = UpdateThread {
            model: wanted.model,
            effort: wanted.effort,
            options: Some(wanted.options),
            ..UpdateThread::default()
        };
        state.core.update_thread(&id, &patch, LOCAL_USER)?;
    }
    Ok((StatusCode::ACCEPTED, Json(SteerResult { state: delivered })))
}

#[derive(Deserialize)]
struct UploadQuery {
    name: String,
}

async fn upload_attachment(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<UploadQuery>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<impl IntoResponse> {
    let mime = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/octet-stream");
    let attachment = state.core.add_attachment(&id, &query.name, mime, &body)?;
    Ok((StatusCode::CREATED, Json(attachment)))
}

async fn list_attachments(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    state.core.store.thread(&id)?;
    let attachments: Vec<_> = state
        .core
        .store
        .list_attachments(&id)?
        .into_iter()
        .map(|(attachment, _)| attachment)
        .collect();
    Ok(Json(attachments))
}

async fn remove_attachment(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.core.remove_attachment(&id)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/attachments/{id}/content`. The bytes a sent message carried.
async fn attachment_content(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    let (attachment, path) = state.core.store.attachment(&id)?;
    let bytes = tokio::fs::read(&path).await.map_err(|_| {
        ApiFailure::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "That attachment is gone.",
        )
    })?;
    let content_type = HeaderValue::from_str(&attachment.mime)
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type),
            (
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            ),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("private, no-cache"),
            ),
        ],
        bytes,
    )
        .into_response())
}

#[derive(Deserialize)]
struct MentionQuery {
    #[serde(default)]
    q: String,
}

async fn mentions(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<MentionQuery>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.mentions(&id, &query.q).await?))
}

async fn slash_commands(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<impl IntoResponse> {
    let skills = query.get("skills").is_some_and(|value| value == "true");
    Ok(Json(state.core.slash_commands(&id, skills).await?))
}
