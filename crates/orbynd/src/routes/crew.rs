//! Crew (Preview) over HTTP: the Preview flag, Autopilot's modes, trust, risk, the Review queue, the merge queue, the watch on
//! main, the digest, the approval ledger, the trip wires and the guards. Transport only: `orbyn_core::crew` builds every view and
//! owns every rule.
//!
//! **Flag off, nothing here exists.** Every route sits behind [`preview_gate`], which answers exactly what the fallback answers
//! for a route that is not there, and the core's own functions check the flag again. `routes/crew_tests.rs` walks the whole
//! route table, so a `/api/crew` route registered without the gate fails there.
//!
//! **Writes only through the core's guards.** The only routes that change anything are in [`CHANGES`], each with the guard it
//! relies on. None merges, pushes, approves a request or runs a command: a revert is `crew_answer_review`'s (made and checked in a
//! scratch checkout, refused when a later merge needs it, never on a checked-out branch, nothing pushed). The repository and branch
//! come from the stored merge, never from the request, and the request must name the merge commit it was shown. Writes are
//! refused for a remote viewer, as the account's are. The Preview flag is never settable from here.
//!
//! Autopilot's own always-denied rule for a state-changing route (N1) covers the account only, so an agent holding the service
//! token could call these routes. The core has no confirmation nonce for Crew to bind them to (the account's is its own); see
//! `docs/crew.md`.

use axum::Json;
use axum::Router;
use axum::extract::{Path, Request, State};
use axum::http::StatusCode;
use axum::middleware::{Next, from_fn_with_state};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use orbyn_contracts as wire;
use orbyn_core::CoreError;
use orbyn_core::crew::{Answering, Page};
use orbyn_core::util::unix_now;

use crate::http::{ApiFailure, ApiJson, ApiResult, AppState, QueryMap};

/// Reads under `/api/crew`: they change nothing.
#[cfg(test)]
pub const READS: &[(&str, &str)] = &[
    ("GET", "/api/crew/preview"),
    ("GET", "/api/crew/autopilot"),
    ("GET", "/api/crew/autopilot/guards"),
    ("GET", "/api/crew/autopilot/risk/{task_id}"),
    ("GET", "/api/crew/autopilot/review"),
    ("GET", "/api/crew/autopilot/review/{task_id}/revert-check"),
    ("GET", "/api/crew/autopilot/merge-queue"),
    ("GET", "/api/crew/autopilot/watch"),
    ("GET", "/api/crew/autopilot/digest"),
    ("GET", "/api/crew/approvals"),
    ("GET", "/api/crew/stops"),
    ("GET", "/api/crew/trust"),
];

/// Routes that change state, as `(method, path, guard)`: the guard is what stands between the request and the change. A route
/// is listed here or in [`READS`], and the route-table test refuses any other under `/api/crew`.
#[cfg(test)]
pub const CHANGES: &[(&str, &str, &str)] = &[
    (
        "PUT",
        "/api/crew/autopilot/mode",
        "core set_crew_autopilot_mode: a repository must be a project, a crew must have a task, the person's choice only; local only",
    ),
    (
        "POST",
        "/api/crew/autopilot/confirm",
        "core set_crew_autopilot_mode keeping the default as it is; local only",
    ),
    (
        "POST",
        "/api/crew/autopilot/review/{task_id}/looks-good",
        "core crew_answer_review: the stored merge, the commit the person saw; local only",
    ),
    (
        "POST",
        "/api/crew/autopilot/review/{task_id}/comment",
        "core crew_answer_review: the stored merge, the commit the person saw, 1 to 2000 characters, redacted; local only",
    ),
    (
        "POST",
        "/api/crew/autopilot/review/{task_id}/revert",
        "core crew_answer_review revert: scratch checkout, repository checks, later merges, branch not checked out, compare-and-swap, nothing pushed; local only",
    ),
];

pub fn routes(state: AppState) -> Router<AppState> {
    let writes = Router::new()
        .route("/api/crew/autopilot/mode", put(set_mode))
        .route("/api/crew/autopilot/confirm", post(confirm))
        .route(
            "/api/crew/autopilot/review/{task_id}/looks-good",
            post(looks_good),
        )
        .route(
            "/api/crew/autopilot/review/{task_id}/comment",
            post(comment),
        )
        .route("/api/crew/autopilot/review/{task_id}/revert", post(revert))
        .route_layer(from_fn_with_state(state.clone(), local_only));
    Router::new()
        .route("/api/crew/preview", get(preview))
        .route("/api/crew/autopilot", get(modes))
        .route("/api/crew/autopilot/guards", get(guards))
        .route("/api/crew/autopilot/risk/{task_id}", get(risk))
        .route("/api/crew/autopilot/review", get(review_queue))
        .route(
            "/api/crew/autopilot/review/{task_id}/revert-check",
            get(revert_check),
        )
        .route("/api/crew/autopilot/merge-queue", get(merge_queue))
        .route("/api/crew/autopilot/watch", get(watch))
        .route("/api/crew/autopilot/digest", get(digest))
        .route("/api/crew/approvals", get(approvals))
        .route("/api/crew/stops", get(stops))
        .route("/api/crew/trust", get(trust))
        .merge(writes)
        .route_layer(from_fn_with_state(state, preview_gate))
}

/// With the flag off the route is not there: the same answer as any other path nothing serves.
async fn preview_gate(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiFailure> {
    if !state.core.config.crew_preview {
        return Err(ApiFailure::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "No such route.",
        ));
    }
    Ok(next.run(request).await)
}

/// A remote viewer reads; the person at this computer answers.
async fn local_only(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiFailure> {
    if state.remote {
        return Err(ApiFailure::forbidden(
            "local_only",
            "Do this from the computer the app runs on.",
        ));
    }
    Ok(next.run(request).await)
}

fn number(query: &QueryMap, name: &str) -> Result<Option<u32>, CoreError> {
    query
        .0
        .get(name)
        .map(|text| {
            text.parse::<u32>()
                .map_err(|_| CoreError::Invalid(format!("{name} is a whole number")))
        })
        .transpose()
}

fn page(query: &QueryMap) -> Result<Page, CoreError> {
    Page::new(number(query, "limit")?, number(query, "offset")?)
}

async fn preview(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_preview_state()?))
}

async fn modes(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.autopilot_modes()?))
}

async fn set_mode(
    State(state): State<AppState>,
    ApiJson(change): ApiJson<wire::AutopilotModeChange>,
) -> ApiResult<impl IntoResponse> {
    state.core.autopilot_set_mode(&change)?;
    Ok(Json(state.core.autopilot_modes()?))
}

async fn confirm(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    state.core.autopilot_confirm()?;
    Ok(Json(state.core.autopilot_modes()?))
}

async fn guards(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.autopilot_guards()?))
}

async fn trust(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.autopilot_trust(unix_now())?))
}

async fn risk(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.autopilot_risk(&task_id)?))
}

async fn review_queue(
    State(state): State<AppState>,
    query: QueryMap,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state
            .core
            .autopilot_review_queue(unix_now(), page(&query)?)?,
    ))
}

async fn revert_check(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.autopilot_revert_check(&task_id)?))
}

async fn answer(
    state: AppState,
    task_id: String,
    body: wire::AutopilotAnswerBody,
    answering: Answering,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state
            .core
            .autopilot_answer(&task_id, &body, answering, unix_now())
            .await?,
    ))
}

async fn looks_good(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    ApiJson(body): ApiJson<wire::AutopilotAnswerBody>,
) -> ApiResult<impl IntoResponse> {
    answer(state, task_id, body, Answering::LooksGood).await
}

async fn comment(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    ApiJson(body): ApiJson<wire::AutopilotAnswerBody>,
) -> ApiResult<impl IntoResponse> {
    answer(state, task_id, body, Answering::Comment).await
}

async fn revert(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    ApiJson(body): ApiJson<wire::AutopilotAnswerBody>,
) -> ApiResult<impl IntoResponse> {
    answer(state, task_id, body, Answering::Revert).await
}

async fn merge_queue(
    State(state): State<AppState>,
    query: QueryMap,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.autopilot_merge_queue(page(&query)?)?))
}

async fn watch(State(state): State<AppState>, query: QueryMap) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.autopilot_watch(page(&query)?)?))
}

async fn digest(State(state): State<AppState>, query: QueryMap) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state
            .core
            .autopilot_digest(unix_now(), number(&query, "hours")?)?,
    ))
}

/// The approval ledger: `task`, `kind`, `limit`, `offset`.
async fn approvals(State(state): State<AppState>, query: QueryMap) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.autopilot_log(
        query.0.get("task").map(String::as_str),
        query.0.get("kind").map(String::as_str),
        page(&query)?,
    )?))
}

async fn stops(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.autopilot_tripwires()?))
}
