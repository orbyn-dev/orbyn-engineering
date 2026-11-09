//! Usage and rate limits.

use std::collections::HashMap;

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};

use crate::http::{ApiFailure, ApiResult, AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/usage", get(usage).post(usage))
        .route("/api/usage/refresh", post(refresh_usage))
        .route("/api/limits", get(limits_overview))
        .route("/api/threads/{id}/limits", get(thread_limits))
        .route("/api/engines/{id}/limits", get(engine_limits))
}

fn period(query: &HashMap<String, String>) -> ApiResult<Option<u32>> {
    query
        .get("days")
        .map(|days| {
            days.parse().map_err(|_| {
                ApiFailure::new(
                    StatusCode::BAD_REQUEST,
                    "invalid_input",
                    "`days` must be a number.",
                )
            })
        })
        .transpose()
}

/// Set once Usage has been read, which is what makes the next launch warm the scan.
pub const USAGE_OPENED: &str = "usage_opened";

async fn usage(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<impl IntoResponse> {
    if state.core.store.setting(USAGE_OPENED)?.is_none() {
        state.core.store.set_setting(USAGE_OPENED, "1")?;
    }
    let settings = state.core.store.settings()?;
    let days = period(&query)?;
    Ok(Json(state.core.usage.report(&settings, days).await?))
}

/// Reads history and limits again from their sources, and rereads prices.
async fn refresh_usage(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<impl IntoResponse> {
    state.core.usage.forget();
    let settings = state.core.store.settings()?;
    let days = period(&query)?;
    Ok(Json(state.core.usage.report(&settings, days).await?))
}

fn forced(query: &HashMap<String, String>) -> bool {
    query.get("force").is_some_and(|value| value == "true")
}

async fn limits_overview(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.limits_overview(forced(&query)).await?))
}

async fn thread_limits(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<impl IntoResponse> {
    let engine = state.core.store.thread(&id)?.engine;
    Ok(Json(
        state.core.engine_limits(&engine, forced(&query)).await?,
    ))
}

async fn engine_limits(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.engine_limits(&id, forced(&query)).await?))
}
