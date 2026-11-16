//! The Crew Map, message and fact reads (Preview) over HTTP: `GET /api/crew/flow/map`, `/messages` and `/facts`. All three are
//! listed in `orbyn_core::crew::changes::READS`, so Autopilot's N1 marker does not treat them as changes. Nothing here writes, opens a
//! thread or moves a worker. Transport only: `orbyn_core::crew` builds each view by labels (a message's author, kind, time and
//! delivery; a fact's key and class) and never returns a body, a value, a thread id, a path or the answer key. They are merged into
//! `crew::routes` before the Preview layer, so with the flag off they answer as a route that is not there.

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use orbyn_core::CoreError;

use super::crew_tasks::page;
use crate::http::{ApiResult, AppState, QueryMap};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/crew/flow/map", get(map))
        .route("/api/crew/flow/messages", get(messages))
        .route("/api/crew/flow/facts", get(facts))
}

fn crew(query: &QueryMap) -> Result<&str, CoreError> {
    query
        .0
        .get("crew")
        .map(String::as_str)
        .ok_or_else(|| CoreError::Invalid("crew is required".into()))
}

/// `crew` (required).
async fn map(State(state): State<AppState>, query: QueryMap) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_flow_map(crew(&query)?).await?))
}

/// `crew` (required), `member`, `limit`, `offset`.
async fn messages(State(state): State<AppState>, query: QueryMap) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_flow_messages(
        crew(&query)?,
        query.0.get("member").map(String::as_str),
        page(&query)?,
    )?))
}

/// `crew` (required).
async fn facts(State(state): State<AppState>, query: QueryMap) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_flow_facts(crew(&query)?)?))
}
