//! Crew Flow (Preview) over HTTP: the crews that have work, a crew's tasks with the ten cells of each, and one task
//! zoomed. Reads only: three GETs, each listed in `orbyn_core::crew::changes::READS`, so Autopilot's N1 marker does not
//! treat them as changes. Nothing here approves, merges or reverts. Transport only: `orbyn_core::crew` builds every view
//! and keeps the answer key, check text, repros, thread ids and paths out of it. The routes are merged into
//! `crew::routes` before its Preview layer, so with the flag off they answer as a route that is not there.

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::get;
use orbyn_core::CoreError;
use orbyn_core::crew::Page;

use crate::http::{ApiResult, AppState, QueryMap};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/crew/flow/crews", get(crews))
        .route("/api/crew/flow/tasks", get(tasks))
        .route("/api/crew/flow/tasks/{id}", get(task))
}

pub(super) fn number(query: &QueryMap, name: &str) -> Result<Option<u32>, CoreError> {
    query
        .0
        .get(name)
        .map(|text| {
            text.parse::<u32>()
                .map_err(|_| CoreError::Invalid(format!("{name} is a whole number")))
        })
        .transpose()
}

pub(super) fn page(query: &QueryMap) -> Result<Page, CoreError> {
    Page::new(number(query, "limit")?, number(query, "offset")?)
}

/// `limit` and `offset`.
async fn crews(State(state): State<AppState>, query: QueryMap) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_flow_crews(page(&query)?)?))
}

/// `crew` (required), `filter`, `engine`, `q`, `limit`, `offset`.
async fn tasks(State(state): State<AppState>, query: QueryMap) -> ApiResult<impl IntoResponse> {
    let get = |name: &str| query.0.get(name).map(String::as_str);
    let crew = get("crew").ok_or_else(|| CoreError::Invalid("crew is required".into()))?;
    Ok(Json(state.core.crew_flow_tasks(
        crew,
        get("filter"),
        get("engine"),
        get("q"),
        page(&query)?,
    )?))
}

async fn task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_flow_task(&id)?))
}
