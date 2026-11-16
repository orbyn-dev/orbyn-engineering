//! The Crew Taskboard (Preview) over HTTP: one read, `GET /api/crew/flow/board`, a crew's work tasks grouped into the seven
//! columns of the run states. Listed in `orbyn_core::crew::changes::READS`, so Autopilot's N1 marker does not treat it as a
//! change. Nothing here moves a card: drag has no route, and cancel and re-run are the person's own writes elsewhere. Transport
//! only: `orbyn_core::crew` builds the view and keeps the answer key, check text and counts, thread ids and paths out of it. It is
//! merged into `crew::routes` before the Preview layer, so with the flag off it answers as a route that is not there.

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use orbyn_core::CoreError;

use super::crew_tasks::{number, page};
use crate::http::{ApiResult, AppState, QueryMap};

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/crew/flow/board", get(board))
}

/// `crew` (required), `engine`, `kind`, `q`, `per_column`, and with `column` the paging of that column: `limit`, `offset`.
async fn board(State(state): State<AppState>, query: QueryMap) -> ApiResult<impl IntoResponse> {
    let get = |name: &str| query.0.get(name).map(String::as_str);
    let crew = get("crew").ok_or_else(|| CoreError::Invalid("crew is required".into()))?;
    Ok(Json(state.core.crew_flow_board(
        crew,
        get("engine"),
        get("kind"),
        get("q"),
        get("column"),
        number(&query, "per_column")?,
        page(&query)?,
    )?))
}
