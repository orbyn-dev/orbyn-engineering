//! The left explorer: a project's worktrees and a read-only look at their files. Core
//! decides what may be shown; these handlers only route. There is no write route.

use std::collections::{BTreeSet, HashMap};
use std::convert::Infallible;

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use futures_util::{Stream, stream};
use orbyn_contracts::ShellEvent;
use orbyn_core::CoreError;

use crate::http::{ApiResult, AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/explorer/events", get(events))
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

/// This stream owns the leases; actual changes use the existing shell event channel.
async fn events(
    State(state): State<AppState>,
    Query(query): Params,
) -> ApiResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let raw = param(&query, "worktrees");
    if raw.len() > 8192 {
        return Err(CoreError::Invalid("Too many worktrees to watch.".into()).into());
    }
    let pairs: Vec<(String, String)> = serde_json::from_str(raw)
        .map_err(|_| CoreError::Invalid("Expected project and worktree id pairs.".into()))?;
    let watches = state.core.explorer_watch(&pairs).await?;
    // A ready frame lets the client catch up after the watch became live, including
    // after focus or a reconnect. It is the same hint as the shell event stream.
    let projects = pairs
        .into_iter()
        .map(|(project, _)| project)
        .collect::<BTreeSet<_>>();
    let frames = projects
        .into_iter()
        .map(|project| {
            Ok(Event::default()
                .json_data(ShellEvent::DataChanged {
                    what: format!("explorer:{project}"),
                })
                .expect("a topic serializes"))
        })
        .collect::<Vec<Result<Event, Infallible>>>();
    let end = stream::unfold(watches, |watches| async move {
        futures_util::future::select_all(watches.iter().map(|watch| Box::pin(watch.stopped())))
            .await;
        None::<(Result<Event, Infallible>, _)>
    });
    use futures_util::StreamExt;
    Ok(Sse::new(stream::iter(frames).chain(end)).keep_alive(KeepAlive::default()))
}
