//! The escape log (Crew Preview, slice 4k) over HTTP. PR 1: three reads, `GET /api/crew/escapes?project=`, `/escapes/{id}` and
//! `/escapes/thresholds`, listed in `orbyn_core::crew::changes::READS` (class Local), so Autopilot's N1 marker does not treat them
//! as changes; nothing read here feeds a gate. PR 2: four writes, [`writes`], a person's alone: `POST /escapes/mark`,
//! `/escapes/{id}/confirm`, `/dismiss` and `/gap`. They are `Person` and enforced (the desktop shell's key; browser mode and the
//! CLI get `403 desktop_required`), behind `local_only`, in `STATE_CHANGING` (an agent's request is refused by path), and no tool,
//! CLI command or scheduler reaches them. Merged into `crew::routes` before its Preview layer, so with the flag off they answer as
//! a route that is not there. A damaged log is `500 unreadable`, never a partial answer; a choice on an escape that changed is
//! `409 stale`. Transport only: `orbyn_core::crew::escapes` builds every view and holds the rules.

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use orbyn_core::CoreError;

use super::crew_tasks::page;
use crate::http::{ApiJson, ApiResult, AppState, QueryMap};
use orbyn_contracts as wire;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/crew/escapes", get(log))
        .route("/api/crew/escapes/thresholds", get(thresholds))
        .route("/api/crew/escapes/possible", get(possible))
        .route("/api/crew/escapes/counts", get(counts))
        .route("/api/crew/escapes/{id}", get(detail))
}

/// The person's writes. Merged into the `local_only` block of `crew::routes`.
pub fn writes() -> Router<AppState> {
    Router::new()
        .route("/api/crew/escapes/mark", post(mark))
        .route("/api/crew/escapes/{id}/confirm", post(confirm))
        .route("/api/crew/escapes/{id}/dismiss", post(dismiss))
        .route("/api/crew/escapes/{id}/gap", post(gap))
        .route("/api/crew/escapes/{id}/check", post(check_propose))
        .route("/api/crew/escapes/{id}/check/accept", post(check_accept))
        .route("/api/crew/escapes/{id}/check/retry", post(check_retry))
        .route("/api/crew/escapes/{id}/check/approve", post(check_approve))
        .route(
            "/api/crew/escapes/{id}/check/withdraw",
            post(check_withdraw),
        )
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

/// `project` (required), `limit`, `offset`. Read-only; the scan that fills it runs on the service's own tick.
async fn possible(State(state): State<AppState>, query: QueryMap) -> ApiResult<impl IntoResponse> {
    let project = query
        .0
        .get("project")
        .ok_or_else(|| CoreError::Invalid("project is required".into()))?;
    Ok(Json(
        state.core.crew_escape_possible(project, page(&query)?)?,
    ))
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

/// `project` and `area` (both required): one area's per-engine counts. Display only, and a read: the Router reads no escape count in
/// v1. An unknown project or an empty area is refused, never served as an empty list.
async fn counts(State(state): State<AppState>, query: QueryMap) -> ApiResult<impl IntoResponse> {
    let project = query
        .0
        .get("project")
        .ok_or_else(|| CoreError::Invalid("project is required".into()))?;
    let area = query
        .0
        .get("area")
        .ok_or_else(|| CoreError::Invalid("area is required".into()))?;
    Ok(Json(state.core.crew_escape_counts(project, area)?))
}

async fn mark(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<wire::EscapeMark>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_escape_mark(&body)?))
}

async fn confirm(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<wire::EscapeDecide>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_escape_confirm(&id, &body)?))
}

async fn dismiss(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<wire::EscapeDecide>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_escape_dismiss(&id, &body)?))
}

async fn gap(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<wire::EscapeGapSet>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_escape_set_gap(&id, &body)?))
}

async fn check_propose(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<wire::EscapeCheckPropose>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state.core.crew_escape_check_propose(&id, &body).await?,
    ))
}

async fn check_accept(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<wire::EscapeCheckAct>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_escape_check_accept(&id, &body).await?))
}

async fn check_retry(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<wire::EscapeCheckAct>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_escape_check_retry(&id, &body)?))
}

async fn check_withdraw(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<wire::EscapeCheckAct>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state.core.crew_escape_check_withdraw(&id, &body).await?,
    ))
}

async fn check_approve(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<wire::EscapeCheckApprove>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state.core.crew_escape_check_approve(&id, &body).await?,
    ))
}
