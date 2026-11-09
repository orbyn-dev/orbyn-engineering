//! Engines: the providers, their models, tools, limits and updates, custom engines, the ACP registry and native sessions.

use std::collections::HashMap;

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use orbyn_contracts::{AcpNativeSessionsQuery, DeleteAcpNativeSession, ImportAcpNativeSession};
use orbyn_contracts::{CustomEngine, EngineFeedbackRequest, InstallAcpAgent};
use orbyn_core::CoreError;
use serde::Deserialize;

use crate::http::{ApiJson, ApiResult, AppState, LOCAL_USER, to_the_end};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/threads/{id}/tools", get(thread_tools))
        .route("/api/engines/{id}/tools", get(engine_tools))
        .route("/api/engines/{id}/models", get(engine_models))
        .route("/api/engines/{id}/feedback", post(engine_feedback))
        .route(
            "/api/engines/{id}/native-sessions/capabilities",
            get(native_session_capabilities),
        )
        .route(
            "/api/engines/{id}/native-sessions/count",
            get(native_session_count),
        )
        .route(
            "/api/engines/{id}/native-sessions",
            get(list_native_sessions),
        )
        .route(
            "/api/engines/{id}/native-sessions/import",
            post(import_native_session),
        )
        .route(
            "/api/engines/{id}/native-sessions/delete",
            post(delete_native_session),
        )
        .route("/api/history/sessions", get(history_sessions))
        .route("/api/triage/playbook", get(triage_playbook))
        .route("/api/history/import", post(import_history_session))
        .route("/api/engines", get(list_engines))
        .route("/api/engines/refresh", post(refresh_engines))
        .route("/api/engines/probe-binary", post(probe_engine_binary))
        .route("/api/engines/custom", post(add_custom_engine))
        .route("/api/engines/custom/{id}", delete(delete_custom_engine))
        .route("/api/engines/update-all", post(update_all_engines))
        .route("/api/engines/{id}/update", post(update_engine))
        .route(
            "/api/engines/{id}/install-recommended",
            post(install_recommended_engine),
        )
        .route("/api/acp/registry", get(search_acp_registry))
        .route(
            "/api/acp/registry/{id}",
            post(install_acp_agent).delete(remove_acp_agent),
        )
}

/// What the thread's engine has available: servers, connectors, plugins and tools.
async fn thread_tools(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state
            .core
            .thread_tools(&id, query.contains_key("refresh"))
            .await?,
    ))
}

/// The same for Settings: an engine, without a thread.
async fn engine_tools(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state
            .core
            .engine_tools(&id, query.contains_key("refresh"))
            .await?,
    ))
}

async fn engine_models(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<impl IntoResponse> {
    let refresh = query.get("refresh").is_some_and(|value| value == "true");
    Ok(Json(state.core.engine_models(&id, refresh).await?))
}

async fn engine_feedback(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<EngineFeedbackRequest>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.engines.feedback(&id, &body).await?))
}

async fn native_session_capabilities(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.native_session_capabilities(&id).await?))
}

async fn native_session_count(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.native_session_count(&id)?))
}

async fn probe_engine_binary(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<orbyn_contracts::ProbeBinary>,
) -> ApiResult<impl IntoResponse> {
    // The check runs the program, so only the computer the app runs on may ask for it.
    if state.remote {
        return Err(
            CoreError::Invalid("Check this from the computer the app runs on.".into()).into(),
        );
    }
    Ok(Json(
        orbyn_core::agents::engines::probe::probe_binary(&body.path).await,
    ))
}

async fn list_native_sessions(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<AcpNativeSessionsQuery>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.list_native_sessions(&id, &query).await?))
}

async fn import_native_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<ImportAcpNativeSession>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.import_native_session(&id, &body).await?))
}

async fn triage_playbook() -> Json<orbyn_contracts::TriagePlaybook> {
    let channel = match orbyn_core::settings::environment::release_channel() {
        orbyn_contracts::ReleaseChannel::Stable => "stable",
        orbyn_contracts::ReleaseChannel::Nightly => "nightly",
        orbyn_contracts::ReleaseChannel::Dev => "dev",
    };
    Json(orbyn_core::agents::triage::service_playbook(
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        channel,
    ))
}

/// Reads the engines' history folders off the async runtime.
async fn history_sessions(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    let core = state.core.clone();
    let scan = tokio::task::spawn_blocking(move || core.scan_history())
        .await
        .map_err(|error| CoreError::Internal(format!("The history scan stopped: {error}.")))??;
    Ok(Json(scan))
}

async fn import_history_session(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<orbyn_contracts::ImportHistorySession>,
) -> ApiResult<impl IntoResponse> {
    let core = state.core.clone();
    let imported = tokio::task::spawn_blocking(move || core.import_history(&body))
        .await
        .map_err(|error| CoreError::Internal(format!("The import stopped: {error}.")))??;
    Ok(Json(imported))
}

async fn delete_native_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<DeleteAcpNativeSession>,
) -> ApiResult<impl IntoResponse> {
    state.core.delete_native_session(&id, &body).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_engines(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.engines.statuses(false).await?))
}

async fn refresh_engines(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.engines.statuses(true).await?))
}

/// Updates every engine with a newer version, through the installer that owns each.
async fn update_all_engines(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    let engines = state.core.engines.clone();
    Ok(Json(
        to_the_end(async move { engines.update_all(LOCAL_USER).await }).await?,
    ))
}

async fn update_engine(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let engines = state.core.engines.clone();
    Ok(Json(
        to_the_end(async move { engines.update(&id, LOCAL_USER).await }).await?,
    ))
}

/// Installs the version the signed manifest recommends, through the installer
/// that owns the binary.
async fn install_recommended_engine(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let engines = state.core.engines.clone();
    Ok(Json(
        to_the_end(async move { engines.install_recommended(&id, LOCAL_USER).await }).await?,
    ))
}

async fn add_custom_engine(
    State(state): State<AppState>,
    ApiJson(mut engine): ApiJson<CustomEngine>,
) -> ApiResult<impl IntoResponse> {
    orbyn_core::agents::engines::known::prepare_custom_engine(&mut engine)?;
    state.core.store.save_custom_engine(&engine, LOCAL_USER)?;
    state.core.engines.invalidate().await;
    Ok((
        StatusCode::CREATED,
        Json(state.core.engines.status(&engine.id).await?),
    ))
}

#[derive(Deserialize)]
struct RegistryQuery {
    #[serde(default)]
    q: String,
}

async fn search_acp_registry(
    State(state): State<AppState>,
    Query(query): Query<RegistryQuery>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.search_acp_registry(&query.q).await?))
}

async fn install_acp_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<InstallAcpAgent>,
) -> ApiResult<impl IntoResponse> {
    let engine = state
        .core
        .install_acp_agent(&id, body.distribution.as_deref(), LOCAL_USER)
        .await?;
    Ok((StatusCode::CREATED, Json(engine)))
}

async fn remove_acp_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.core.remove_acp_agent(&id, LOCAL_USER).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_custom_engine(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.core.store.delete_custom_engine(&id, LOCAL_USER)?;
    state.core.engines.invalidate().await;
    Ok(StatusCode::NO_CONTENT)
}
