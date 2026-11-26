//! Crew Flow (Preview) over HTTP: the crews that have work, a crew's tasks with the ten cells of each, one task zoomed,
//! one task's checks, artifacts and diff, one task's Run page, one of its reviewer lanes in full, and one task's
//! recorded EvalHistory samples. Reads only: seven GETs, each listed in `orbyn_core::crew::changes::READS`, so
//! Autopilot's N1 marker does not treat them as changes. The samples read is KC5 slice 4 read back: ids and enums
//! only, never lesson or brief text. Nothing here approves,
//! merges or reverts. Transport only: `orbyn_core::crew` builds every view and keeps the answer key, check text, repros,
//! thread ids and paths out of them; the Run page is the one read that names a check's command, and it never names the
//! key's id or its expectations (`docs/crew-run-page.md`). The artifacts read runs read-only git for its diff. The routes
//! are merged into `crew::routes` before its Preview layer, so with the flag off they answer as a route that is not there.

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
        .route("/api/crew/flow/tasks/{id}/artifacts", get(task_artifacts))
        .route("/api/crew/flow/tasks/{id}/lanes", get(lanes))
        .route("/api/crew/flow/tasks/{id}/run", get(run))
        .route("/api/crew/tasks/{id}/samples", get(task_samples))
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

/// The task's checks, the artifacts they named and the change they read. Read-only, and the only Flow route that reads git.
async fn task_artifacts(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_flow_task_artifacts(&id).await?))
}

/// `lane` (required): the lane's own name, as the task zoom shows it.
async fn lanes(
    State(state): State<AppState>,
    Path(id): Path<String>,
    query: QueryMap,
) -> ApiResult<impl IntoResponse> {
    let lane = query
        .0
        .get("lane")
        .map(String::as_str)
        .ok_or_else(|| CoreError::Invalid("lane is required".into()))?;
    Ok(Json(state.core.crew_flow_lane(&id, lane).await?))
}

/// One task's Run page: its prompt, the checks the verifier ran, what was not verified and the pull request it made.
async fn run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_flow_run(&id)?))
}

/// One task's EvalHistory samples (KC5 slice 4 read): the sample its start recorded and the outcome it ended with.
/// Ids and enums only, never lesson or brief text; an unknown task is `404 not_found`.
async fn task_samples(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_task_samples(&id)?))
}
