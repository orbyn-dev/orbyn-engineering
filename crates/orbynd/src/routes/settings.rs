//! Settings: preferences, keybindings, server information and the environment (diagnostics, storage and dependency health).

use std::collections::HashMap;

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post, put};
use orbyn_contracts::{KeybindingRule, OtlpCheck, ServerInfo, ServerSettings, SetPreferences};

use crate::http::{ApiJson, ApiResult, AppState, LOCAL_USER};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/info", get(info))
        .route("/api/network", get(network_requests))
        .route("/api/settings", get(get_settings).put(put_settings))
        .route("/api/settings/preferences/query", post(scoped_preferences))
        .route("/api/settings/preferences", put(change_scoped_preferences))
        .route("/api/preferences", get(preferences).put(set_preferences))
        .route(
            "/api/projects/{id}/preferences",
            put(set_project_preferences),
        )
        .route("/api/keybindings", get(keybindings).post(save_keybinding))
        .route("/api/diagnostics", get(diagnostics))
        .route("/api/diagnostics/repair", post(repair_database))
        .route("/api/diagnostics/processes/{pid}/stop", post(stop_process))
        .route("/api/diagnostics/otlp", post(check_otlp))
        .route(
            "/api/storage/cleanup",
            get(cleanup_report).post(storage_cleanup),
        )
        .route("/api/storage/cleanup/preview", get(storage_preview))
        .route("/api/health/dependencies", get(dependency_health))
}

async fn info(State(state): State<AppState>) -> Json<ServerInfo> {
    Json(ServerInfo {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        remote: state.remote,
        platform: std::env::consts::OS.to_owned(),
        environment_label: state.core.environment_label(),
        // A storage error leaves the theme unpublished rather than failing /api/info.
        environment_theme: state.core.environment_theme().ok().flatten(),
        channel: Some(orbyn_core::settings::environment::release_channel()),
        environment_icon: state.core.environment_icon().ok().flatten(),
        database_upgrade: state.core.store.upgrade(),
    })
}

async fn network_requests(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.network_requests()?))
}

async fn scoped_preferences(
    State(state): State<AppState>,
    ApiJson(query): ApiJson<orbyn_contracts::ScopedPreferencesQuery>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.scoped_preferences(&query).await?))
}

async fn change_scoped_preferences(
    State(state): State<AppState>,
    ApiJson(change): ApiJson<orbyn_contracts::ScopedPreferencesChange>,
) -> ApiResult<impl IntoResponse> {
    let result = state.core.change_scoped_preferences(&change).await?;
    if change
        .edits
        .contains_key(orbyn_core::agents::engines::instances::PREFERENCE)
    {
        state.core.engines.instances_changed().await;
    }
    Ok(Json(result))
}

async fn preferences(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<impl IntoResponse> {
    let project = query.get("project_id").map(String::as_str);
    Ok(Json(state.core.preferences(project)?))
}

async fn set_preferences(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<SetPreferences>,
) -> ApiResult<impl IntoResponse> {
    state.core.set_preferences(None, &body.values)?;
    if body
        .values
        .contains_key(orbyn_core::agents::engines::instances::PREFERENCE)
    {
        state.core.engines.instances_changed().await;
    }
    Ok(Json(state.core.preferences(None)?))
}

async fn set_project_preferences(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<SetPreferences>,
) -> ApiResult<impl IntoResponse> {
    state.core.store.project(&id)?;
    state.core.set_preferences(Some(&id), &body.values)?;
    Ok(Json(state.core.preferences(Some(&id))?))
}

async fn keybindings(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.core.keybindings())
}

async fn save_keybinding(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<KeybindingRule>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.save_keybinding(&body)?))
}

async fn diagnostics(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.diagnostics()?))
}

async fn stop_process(Path(pid): Path<u32>) -> ApiResult<StatusCode> {
    orbyn_core::operations::diagnostics::stop_process(pid)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn repair_database(State(state): State<AppState>) -> ApiResult<StatusCode> {
    state.core.repair_database()?;
    Ok(StatusCode::NO_CONTENT)
}

async fn check_otlp(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<OtlpCheck>,
) -> impl IntoResponse {
    Json(state.core.check_otlp(&body.url).await)
}

async fn storage_cleanup(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<impl IntoResponse> {
    let dry_run = query.get("dry_run").is_some_and(|value| value == "true");
    Ok(Json(state.core.storage_cleanup(dry_run).await?))
}

async fn dependency_health(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.dependency_health().await?))
}

async fn storage_preview(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.storage_preview().await?))
}

async fn cleanup_report(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.cleanup_report()?))
}

async fn get_settings(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.store.settings()?))
}

async fn put_settings(
    State(state): State<AppState>,
    ApiJson(settings): ApiJson<ServerSettings>,
) -> ApiResult<impl IntoResponse> {
    state.core.store.save_settings(&settings, LOCAL_USER)?;
    Ok(Json(settings))
}
