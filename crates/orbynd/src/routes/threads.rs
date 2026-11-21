//! Threads: create, read, change, move and delete, their messages and items, and the preparation of a thread's checkout.

use std::collections::VecDeque;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use futures_util::{Stream, stream};
use orbyn_contracts::{
    AdoptScratch, BatchThreads, CreateThread, DelegateTask, ForkThread, MessageSearchEvent,
    MoveThread, ReorderThreads, RetryPreparation, SwitchEngine, UpdateThread,
};
use orbyn_core::Core;
use orbyn_core::conversation::search::{SearchCursor, SearchScope};
use serde::Deserialize;

use crate::http::{ApiJson, ApiResult, AppState, LOCAL_USER};
/// How many messages one page of a streamed search reads.
const SEARCH_PAGE: usize = 200;
const SEARCH_KEEP_ALIVE: Duration = Duration::from_secs(15);

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/threads/{id}/project", post(move_thread))
        .route("/api/threads/{id}/adopt", post(adopt_scratch))
        .route("/api/threads", get(list_threads).post(create_thread))
        .route(
            "/api/threads/{id}",
            get(get_thread).patch(update_thread).delete(delete_thread),
        )
        .route("/api/threads/{id}/family", get(thread_family))
        .route("/api/threads/{id}/items", get(list_items))
        .route("/api/threads/{id}/messages", get(thread_messages))
        .route("/api/threads/{id}/export", get(export_transcript))
        .route("/api/threads/import", post(import_transcript))
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
        .route("/api/search/messages/events", get(search_messages_stream))
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

/// A thread's whole family: the topmost thread and everything below it.
async fn thread_family(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.store.thread_family(&id)?))
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
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.batch_threads(body.threads).await?))
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

impl MessageSearchQuery {
    fn scope(&self) -> SearchScope {
        SearchScope {
            project_id: self.project_id.clone(),
            thread_id: self.thread_id.clone(),
            archived: self.archived.unwrap_or(false),
            limit: self.limit,
        }
    }
}

async fn search_messages(
    State(state): State<AppState>,
    Query(query): Query<MessageSearchQuery>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state.core.store.search_messages(&query.q, &query.scope())?,
    ))
}

/// The matches of one search, newest first, sent while the read runs: the same order
/// and the same flags as `GET /api/search/messages`, but a client can draw them as
/// they are found and stop reading early.
struct SearchFrames {
    core: Arc<Core>,
    query: String,
    scope: SearchScope,
    cursor: SearchCursor,
    pending: VecDeque<MessageSearchEvent>,
    words: Vec<String>,
    too_short: bool,
    finished: bool,
}

impl SearchFrames {
    /// The next frame: hits in read order, then one `done` (or a `failed` if the read
    /// broke after some hits), then `None`.
    fn next_frame(&mut self) -> Option<MessageSearchEvent> {
        loop {
            if let Some(frame) = self.pending.pop_front() {
                return Some(frame);
            }
            if self.finished {
                return None;
            }
            if self.cursor.done() {
                self.finished = true;
                return Some(MessageSearchEvent::Done {
                    words: std::mem::take(&mut self.words),
                    too_short: self.too_short,
                    truncated: self.cursor.truncated(),
                });
            }
            match self.core.store.search_messages_page(
                &self.query,
                &self.scope,
                &mut self.cursor,
                SEARCH_PAGE,
            ) {
                Ok(page) => {
                    self.words = page.words;
                    self.too_short = page.too_short;
                    self.pending.extend(
                        page.hits
                            .into_iter()
                            .map(|hit| MessageSearchEvent::Hit { hit }),
                    );
                }
                Err(error) => {
                    self.finished = true;
                    return Some(MessageSearchEvent::Failed {
                        code: error.code().to_owned(),
                        message: error.public_message(),
                    });
                }
            }
        }
    }
}

async fn search_messages_stream(
    State(state): State<AppState>,
    Query(query): Query<MessageSearchQuery>,
) -> ApiResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let scope = query.scope();
    let mut cursor = SearchCursor::default();
    // The first page is read here, so a query the whole read refuses is a `400` rather
    // than a stream that opens and fails.
    let first =
        state
            .core
            .store
            .search_messages_page(&query.q, &scope, &mut cursor, SEARCH_PAGE)?;
    let frames = SearchFrames {
        core: state.core.clone(),
        query: query.q,
        scope,
        cursor,
        pending: first
            .hits
            .into_iter()
            .map(|hit| MessageSearchEvent::Hit { hit })
            .collect(),
        words: first.words,
        too_short: first.too_short,
        finished: false,
    };
    let frames = stream::unfold(frames, |mut frames| async move {
        let event = frames.next_frame()?;
        let frame = Event::default().json_data(&event).ok()?;
        Some((Ok::<_, Infallible>(frame), frames))
    });
    Ok(Sse::new(frames).keep_alive(KeepAlive::new().interval(SEARCH_KEEP_ALIVE)))
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

#[derive(Deserialize)]
struct ImportQuery {
    project_id: String,
    engine: Option<String>,
    dry_run: Option<bool>,
}

/// The transcript document. The client saves it; the service writes no file and takes no path.
async fn export_transcript(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.export_transcript(&id)?))
}

/// The file as the client read it from a file the person picked, as raw bytes. Core refuses over 2 MiB and drops a bad entry on
/// its own; `?name=` is a label the service does not read.
async fn import_transcript(
    State(state): State<AppState>,
    Query(query): Query<ImportQuery>,
    body: axum::body::Bytes,
) -> ApiResult<impl IntoResponse> {
    let answer = state
        .core
        .import_transcript(
            &body,
            &query.project_id,
            query.engine.as_deref(),
            query.dry_run.unwrap_or(false),
        )
        .await?;
    let status = if answer.dry_run {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, Json(answer)))
}
