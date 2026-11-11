//! The left explorer: a project's worktrees and a read-only look at their files. Core
//! decides what may be shown; these handlers only route. There is no write route.

use std::collections::HashMap;

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::get;

use crate::http::{ApiResult, AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/projects/{id}/worktrees", get(worktrees))
        .route("/api/projects/{id}/worktrees/{worktree}/tree", get(tree))
        .route("/api/projects/{id}/worktrees/{worktree}/file", get(file))
}

type Params = Query<HashMap<String, String>>;

fn param<'a>(query: &'a HashMap<String, String>, name: &str) -> &'a str {
    query.get(name).map_or("", String::as_str)
}

/// `?refresh=1` or `?refresh=true` reads now instead of answering from what is kept.
fn refresh_of(query: &HashMap<String, String>) -> bool {
    matches!(query.get("refresh").map(String::as_str), Some("1" | "true"))
}

/// `?thread=<id>` names the thread asking, so a worktree another thread is running in
/// comes back read-only for it.
async fn worktrees(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Params,
) -> ApiResult<impl IntoResponse> {
    let thread = Some(param(&query, "thread")).filter(|thread| !thread.is_empty());
    Ok(Json(
        state
            .core
            .explorer_worktrees(&id, thread, refresh_of(&query))
            .await?,
    ))
}

/// `?path=` is a folder relative to the worktree, empty for its root; `?filter=` keeps
/// names containing it.
async fn tree(
    State(state): State<AppState>,
    Path((id, worktree)): Path<(String, String)>,
    Query(query): Params,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state
            .core
            .explorer_tree(
                &id,
                &worktree,
                param(&query, "path"),
                param(&query, "filter"),
                refresh_of(&query),
            )
            .await?,
    ))
}

async fn file(
    State(state): State<AppState>,
    Path((id, worktree)): Path<(String, String)>,
    Query(query): Params,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state
            .core
            .explorer_file(&id, &worktree, param(&query, "path"))
            .await?,
    ))
}
