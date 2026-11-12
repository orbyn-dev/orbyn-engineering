//! Settings › Secrets: which values are hidden, the person's own patterns and the projects whose
//! agents read values as they are. Local to this computer; the account's settings sync never
//! reads or writes it.

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, post, put};
use orbyn_contracts::{
    RedactionFormatChange, RedactionPatternInput, RedactionPatternTestInput,
    RedactionProjectChange, RedactionSurfaceChange,
};

use crate::http::{ApiJson, ApiResult, AppState, LOCAL_USER};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/settings/secrets", get(read))
        .route("/api/settings/secrets/surfaces", put(surfaces))
        .route("/api/settings/secrets/formats/{id}", put(format))
        .route("/api/settings/secrets/projects/{id}", put(project))
        .route("/api/settings/secrets/patterns", post(add_pattern))
        .route("/api/settings/secrets/patterns/test", post(test_pattern))
        .route(
            "/api/settings/secrets/patterns/{id}",
            put(edit_pattern).delete(delete_pattern),
        )
}

async fn read(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.redaction_settings()?))
}

async fn surfaces(
    State(state): State<AppState>,
    ApiJson(change): ApiJson<RedactionSurfaceChange>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state.core.change_redaction_surfaces(LOCAL_USER, &change)?,
    ))
}

async fn format(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(change): ApiJson<RedactionFormatChange>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.change_redaction_format(
        LOCAL_USER,
        &id,
        change.enabled,
    )?))
}

async fn project(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(change): ApiJson<RedactionProjectChange>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.change_redaction_project(
        LOCAL_USER,
        &id,
        change.redaction,
    )?))
}

async fn add_pattern(
    State(state): State<AppState>,
    ApiJson(input): ApiJson<RedactionPatternInput>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state
            .core
            .save_redaction_pattern(LOCAL_USER, None, &input)?,
    ))
}

async fn edit_pattern(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(input): ApiJson<RedactionPatternInput>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.save_redaction_pattern(
        LOCAL_USER,
        Some(&id),
        &input,
    )?))
}

async fn delete_pattern(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.delete_redaction_pattern(LOCAL_USER, &id)?))
}

async fn test_pattern(
    State(state): State<AppState>,
    ApiJson(input): ApiJson<RedactionPatternTestInput>,
) -> impl IntoResponse {
    Json(state.core.test_redaction_pattern(&input))
}
