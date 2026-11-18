//! The escape log (Crew Preview, slice 4k, PR 1) over HTTP: three reads, `GET /api/crew/escapes?project=`, `/escapes/{id}` and
//! `/escapes/thresholds`. Listed in `orbyn_core::crew::changes::READS` (class Local), so Autopilot's N1 marker does not treat them
//! as changes. Display only: nothing here writes, confirms or enforces, and no value read here feeds a gate. Writes come in PR 2, as
//! Person routes. Merged into `crew::routes` before its Preview layer, so with the flag off they answer as a route that is not
//! there. A damaged log is `500 unreadable`, never a partial answer. Transport only: `orbyn_core::crew::escapes` builds every view.

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::get;
use orbyn_core::CoreError;

use super::crew_tasks::page;
use crate::http::{ApiResult, AppState, QueryMap};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/crew/escapes", get(log))
        .route("/api/crew/escapes/thresholds", get(thresholds))
        .route("/api/crew/escapes/{id}", get(detail))
}

/// `project` (required), `state` (`possible`, `confirmed` or `dismissed`), `limit`, `offset`.
async fn log(State(state): State<AppState>, query: QueryMap) -> ApiResult<impl IntoResponse> {
    let project = query
        .0
        .get("project")
        .ok_or_else(|| CoreError::Invalid("project is required".into()))?;
    Ok(Json(state.core.crew_escape_log(
        project,
        query.0.get("state").map(String::as_str),
        page(&query)?,
    )?))
}

async fn detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_escape_detail(&id)?))
}

async fn thresholds(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_escape_thresholds()?))
}
