//! The personal library (docs/execution-profiles-and-library.md, Part 2). Reads, the two renders and the export are `Local`: an
//! environment set answers key names and `present` only, so a service token can see that a key exists and never what it is. Every
//! write is a `Person` route (`classes.rs`), a workflow run and an environment value included.

use std::collections::HashMap;

use axum::Json;
use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post, put};
use orbyn_contracts::{
    CopySuggestion, EnvSetDraft, PromptDraft, RenderValues, RunWorkflow, SetEnvValue, UpdateEnvSet,
    UpdatePrompt, UpdateWorkflow, WorkflowDraft,
};

use crate::http::{ApiJson, ApiResult, AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/library", get(summary))
        .route(
            "/api/library/workflows",
            get(workflows).post(create_workflow),
        )
        .route(
            "/api/library/workflows/{id}",
            get(workflow).patch(update_workflow).delete(delete_workflow),
        )
        .route("/api/library/workflows/{id}/render", post(render_workflow))
        .route("/api/library/workflows/{id}/run", post(run_workflow))
        .route("/api/library/prompts", get(prompts).post(create_prompt))
        .route(
            "/api/library/prompts/{id}",
            get(prompt).patch(update_prompt).delete(delete_prompt),
        )
        .route("/api/library/prompts/{id}/render", post(render_prompt))
        .route("/api/library/env-sets", get(env_sets).post(create_env_set))
        .route(
            "/api/library/env-sets/{id}",
            get(env_set).patch(update_env_set).delete(delete_env_set),
        )
        .route(
            "/api/library/env-sets/{id}/keys/{key}",
            put(set_key).delete(remove_key),
        )
        .route("/api/library/export", get(export))
        .route("/api/library/import", post(import))
        .route("/api/projects/{id}/library/suggestions", get(suggestions))
        .route(
            "/api/projects/{id}/library/suggestions/copy",
            post(copy_suggestion),
        )
}

async fn summary(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.library_summary()?))
}

async fn workflows(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.list_workflows()?))
}

async fn workflow(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.get_workflow(&id)?))
}

async fn create_workflow(
    State(state): State<AppState>,
    ApiJson(draft): ApiJson<WorkflowDraft>,
) -> ApiResult<impl IntoResponse> {
    Ok((
        StatusCode::CREATED,
        Json(state.core.create_workflow(&draft)?),
    ))
}

async fn update_workflow(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(patch): ApiJson<UpdateWorkflow>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.update_workflow(&id, &patch)?))
}

async fn delete_workflow(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.core.delete_workflow(&id)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn render_workflow(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<RenderValues>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.render_workflow(&id, &body)?))
}

async fn run_workflow(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<RunWorkflow>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.run_workflow(&id, &body).await?))
}

async fn prompts(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.list_prompts()?))
}

async fn prompt(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.get_prompt(&id)?))
}

async fn create_prompt(
    State(state): State<AppState>,
    ApiJson(draft): ApiJson<PromptDraft>,
) -> ApiResult<impl IntoResponse> {
    Ok((StatusCode::CREATED, Json(state.core.create_prompt(&draft)?)))
}

async fn update_prompt(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(patch): ApiJson<UpdatePrompt>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.update_prompt(&id, &patch)?))
}

async fn delete_prompt(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.core.delete_prompt(&id)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn render_prompt(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<RenderValues>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.render_prompt(&id, &body)?))
}

async fn env_sets(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.list_env_sets()?))
}

async fn env_set(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.get_env_set(&id)?))
}

async fn create_env_set(
    State(state): State<AppState>,
    ApiJson(draft): ApiJson<EnvSetDraft>,
) -> ApiResult<impl IntoResponse> {
    Ok((
        StatusCode::CREATED,
        Json(state.core.create_env_set(&draft)?),
    ))
}

async fn update_env_set(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(patch): ApiJson<UpdateEnvSet>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.update_env_set(&id, &patch)?))
}

async fn delete_env_set(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.core.delete_env_set(&id)?;
    Ok(StatusCode::NO_CONTENT)
}

/// The body is the value and nothing else answers it: the answer is the key name and `present`.
async fn set_key(
    State(state): State<AppState>,
    Path((id, key)): Path<(String, String)>,
    ApiJson(body): ApiJson<SetEnvValue>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.set_env_key(&id, &key, &body.value)?))
}

async fn remove_key(
    State(state): State<AppState>,
    Path((id, key)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.remove_env_key(&id, &key)?))
}

/// The document the client saves: core writes no file and takes no path.
async fn export(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state
            .core
            .export_library(query.get("kinds").map(String::as_str))
            .await?,
    ))
}

/// The document as the client read it from a file the person picked. Read as bytes: core drops a bad entry on its own and refuses
/// a document over 2 MiB.
async fn import(State(state): State<AppState>, body: Bytes) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.import_library(&body).await?))
}

async fn suggestions(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.library_suggestions(&id).await?))
}

async fn copy_suggestion(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<CopySuggestion>,
) -> ApiResult<impl IntoResponse> {
    Ok((
        StatusCode::CREATED,
        Json(state.core.copy_suggestion(&id, &body).await?),
    ))
}
