//! Threads: create, read, change, move and delete, their messages and items, and the preparation of a thread's checkout.

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use orbyn_contracts::{
    AdoptScratch, BatchThreads, CreateThread, DelegateTask, ForkThread, MoveThread, ReorderThreads,
    RetryPreparation, SwitchEngine, UpdateThread,
};
use serde::Deserialize;

use crate::http::{ApiJson, ApiResult, AppState, LOCAL_USER};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/threads/{id}/project", post(move_thread))
        .route("/api/threads/{id}/adopt", post(adopt_scratch))
        .route("/api/threads", get(list_threads).post(create_thread))
        .route(
            "/api/threads/{id}",
            get(get_thread).patch(update_thread).delete(delete_thread),
        )
        .route("/api/threads/{id}/items", get(list_items))
        .route("/api/threads/{id}/messages", get(thread_messages))
        .route("/api/threads/{id}/items/{item}", get(get_item))
        .route("/api/threads/{id}/usage", get(thread_usage))
        .route("/api/threads/{id}/fork", post(fork_thread))
        .route("/api/threads/{id}/bring-back", post(bring_back))
        .route("/api/threads/{id}/restart-session", post(restart_session))
        .route("/api/threads/{id}/switch-engine", post(switch_engine))
        .route("/api/threads/order", post(reorder_threads))
        .route("/api/threads/{id}/title", post(regenerate_title))
        .route("/api/threads/{id}/delegate", post(delegate))
        .route("/api/threads/batch", post(batch_threads))
        .route("/api/threads/{id}/setup", get(thread_setup))
        .route("/api/threads/{id}/setup/cancel", post(cancel_setup))
        .route("/api/threads/{id}/preparation", get(preparation_retry))
        .route(
            "/api/threads/{id}/preparation/retry",
            post(retry_preparation),
        )
        .route("/api/search/messages", get(search_messages))
}

async fn move_thread(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<MoveThread>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state.core.move_thread(&id, body.project_id.as_deref())?,
    ))
}

async fn adopt_scratch(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<AdoptScratch>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.adopt_scratch(&id, body.name.as_deref())?))
}

#[derive(Deserialize)]
struct ThreadsQuery {
    project_id: Option<String>,
    archived: Option<bool>,
}

async fn list_threads(
    State(state): State<AppState>,
    Query(query): Query<ThreadsQuery>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.store.list_threads(
        query.project_id.as_deref(),
        query.archived.unwrap_or(false),
    )?))
}

async fn create_thread(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<CreateThread>,
) -> ApiResult<impl IntoResponse> {
    Ok((
        StatusCode::CREATED,
        Json(state.core.create_thread(body).await?),
    ))
}

async fn get_thread(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.store.thread(&id)?))
}

async fn delete_thread(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.core.delete_thread(&id, LOCAL_USER)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn update_thread(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(patch): ApiJson<UpdateThread>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.update_thread(&id, &patch, LOCAL_USER)?))
}

async fn regenerate_title(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.regenerate_title(&id, LOCAL_USER).await?))
}

async fn reorder_threads(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<ReorderThreads>,
) -> ApiResult<StatusCode> {
    state.core.store.reorder_threads(&body.ids)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_items(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    state.core.store.thread(&id)?;
    Ok(Json(state.core.store.items(&id, 500)?))
}

async fn get_item(
    State(state): State<AppState>,
    Path((id, item)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.store.item(&id, &item)?))
}

async fn thread_usage(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.store.usage(&id)?))
}

async fn delegate(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<DelegateTask>,
) -> ApiResult<impl IntoResponse> {
    Ok((
        StatusCode::CREATED,
        Json(state.core.delegate(&id, &body, LOCAL_USER)?),
    ))
}

async fn batch_threads(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<BatchThreads>,
) -> impl IntoResponse {
    Json(state.core.batch_threads(body.threads).await)
}

/// The thread's worktree setup action and where it stands, or `null`.
async fn thread_setup(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.thread_setup(&id)?))
}

/// Stops the thread's setup action. A setup that already ended is not an error;
/// a thread whose checkout ran no setup is `404`.
async fn cancel_setup(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.cancel_setup(&id, LOCAL_USER)?))
}

async fn preparation_retry(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state
            .core
            .store
            .preparation_retry_run(&id)?
            .map(|run_id| RetryPreparation { run_id }),
    ))
}

async fn retry_preparation(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<RetryPreparation>,
) -> ApiResult<impl IntoResponse> {
    let run = state
        .core
        .retry_preparation(&id, &body.run_id, LOCAL_USER)
        .await?;
    Ok((StatusCode::ACCEPTED, Json(run)))
}

#[derive(Deserialize)]
struct MessageSearchQuery {
    q: String,
    project_id: Option<String>,
    thread_id: Option<String>,
    limit: Option<usize>,
    archived: Option<bool>,
}

async fn search_messages(
    State(state): State<AppState>,
    Query(query): Query<MessageSearchQuery>,
) -> ApiResult<impl IntoResponse> {
    let scope = orbyn_core::conversation::search::SearchScope {
        project_id: query.project_id,
        thread_id: query.thread_id,
        archived: query.archived.unwrap_or(false),
        limit: query.limit,
    };
    Ok(Json(state.core.store.search_messages(&query.q, &scope)?))
}

#[derive(Deserialize)]
struct MessagesQuery {
    limit: Option<usize>,
}

async fn thread_messages(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<MessagesQuery>,
) -> ApiResult<impl IntoResponse> {
    let limit = query.limit.unwrap_or(200);
    Ok(Json(state.core.store.thread_messages(&id, limit)?))
}

async fn fork_thread(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<ForkThread>,
) -> ApiResult<impl IntoResponse> {
    Ok((
        StatusCode::CREATED,
        Json(state.core.fork_thread(&id, &body, LOCAL_USER)?),
    ))
}

async fn restart_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    state.core.restart_session(&id, LOCAL_USER)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn bring_back(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    state.core.bring_back(&id, LOCAL_USER)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn switch_engine(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<SwitchEngine>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.switch_engine(&id, &body, LOCAL_USER)?))
}
