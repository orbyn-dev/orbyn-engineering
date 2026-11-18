//! Review capacity and review coverage (Crew Preview). Two reads, Local: the capacity state and the coverage map. Two
//! writes, Person, behind `local_only` and in `orbyn_core::crew::changes::STATE_CHANGING` so Autopilot's N1 refuses an
//! agent's request to them by path: a project's settings (limit, sampling, whether pausing is on) and a task's priority
//! and `crew_started`. Transport only: `orbyn_core::crew::capacity` owns every rule. The Preview gate of
//! [`super::crew::routes`] wraps both routers.

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use orbyn_contracts as wire;
use orbyn_core::CoreError;

use crate::http::{ApiJson, ApiResult, AppState, QueryMap};

pub fn reads() -> Router<AppState> {
    Router::new()
        .route("/api/crew/capacity", get(capacity))
        .route("/api/crew/capacity/coverage", get(coverage))
}

pub fn writes() -> Router<AppState> {
    Router::new()
        .route("/api/crew/capacity/settings", post(set_settings))
        .route("/api/crew/capacity/tasks/{id}", post(set_task))
}

fn project(query: &QueryMap) -> Result<String, CoreError> {
    query
        .0
        .get("project")
        .cloned()
        .ok_or_else(|| CoreError::Invalid("project is required".into()))
}

async fn capacity(State(state): State<AppState>, query: QueryMap) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_capacity_state(&project(&query)?)?))
}

/// `since` is unix seconds; left out it is 0, every merge.
async fn coverage(State(state): State<AppState>, query: QueryMap) -> ApiResult<impl IntoResponse> {
    let since = query
        .0
        .get("since")
        .map(|text| {
            text.parse::<i64>()
                .ok()
                .filter(|n| *n >= 0)
                .ok_or_else(|| CoreError::Invalid("since is unix seconds".into()))
        })
        .transpose()?
        .unwrap_or(0);
    Ok(Json(
        state.core.crew_review_coverage(&project(&query)?, since)?,
    ))
}

async fn set_settings(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<wire::CapacitySettingsInput>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_set_capacity_settings(&body)?))
}

async fn set_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<wire::CapacityTaskInput>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_set_task_capacity(&id, &body)?))
}
