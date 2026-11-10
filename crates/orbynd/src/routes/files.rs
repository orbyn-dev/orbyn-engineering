//! A thread's files: the tree, search, reading and writing, and opening in an editor.

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use orbyn_contracts::{OpenInEditor, ResolveThreadPath, WriteFile};
use serde::Deserialize;

use crate::http::{ApiJson, ApiResult, AppState, LOCAL_USER};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/threads/{id}/files", get(file_tree))
        .route("/api/threads/{id}/files/ignored", get(ignored_files))
        .route("/api/threads/{id}/paths/resolve", post(resolve_thread_path))
        .route("/api/threads/{id}/files/search", get(search_files))
        .route(
            "/api/threads/{id}/files/content",
            get(read_file).put(write_file),
        )
        .route("/api/threads/{id}/files/outside", get(read_outside_file))
        .route("/api/threads/{id}/files/open", post(open_in_editor))
        .route("/api/editors", get(editors))
}

#[derive(Deserialize)]
struct PathQuery {
    #[serde(default)]
    path: String,
}

async fn file_tree(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<PathQuery>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.list_files(&id, &query.path).await?))
}

async fn resolve_thread_path(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<ResolveThreadPath>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.resolve_thread_path(&id, &body).await?))
}

async fn ignored_files(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.ignored_files(&id).await?))
}

async fn read_file(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<PathQuery>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.read_file(&id, &query.path).await?))
}

/// A file outside the checkout, read-only; Core decides what may be shown.
async fn read_outside_file(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<PathQuery>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.read_outside_file(&id, &query.path).await?))
}

async fn write_file(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<WriteFile>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.write_file(&id, &body, LOCAL_USER).await?))
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    #[serde(default)]
    content: bool,
}

async fn search_files(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<SearchQuery>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state
            .core
            .search_files(&id, &query.q, query.content)
            .await?,
    ))
}

async fn editors() -> Json<Vec<orbyn_contracts::EditorInfo>> {
    Json(orbyn_core::project::files::installed_editors())
}

async fn open_in_editor(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<OpenInEditor>,
) -> ApiResult<StatusCode> {
    state.core.open_in_editor(&id, &body, LOCAL_USER).await?;
    Ok(StatusCode::ACCEPTED)
}
