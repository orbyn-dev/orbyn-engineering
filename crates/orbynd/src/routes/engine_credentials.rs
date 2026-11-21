//! Settings › an engine's API key. The opt-in is a local setting; the key itself goes only into the
//! OS keychain through `orbyn_core::credentials` and is never read back by a route. Writes are the
//! person's (a `Person` route, desktop key only) and local to this computer; the status is `Local`
//! and carries booleans and the engine id, never the key, its length or its last characters.
//!
//! The key travels in the request body only, never in a path or query, and no error or answer
//! names it: `CredentialError` has no key text and this module maps it to fixed messages.

use axum::Json;
use axum::Router;
use axum::extract::{Path, Request, State};
use axum::http::StatusCode;
use axum::middleware::{Next, from_fn_with_state};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use orbyn_contracts::{
    EngineCredentialKeyInput, EngineCredentialsOptIn, EngineCredentialsRevoked,
    EngineCredentialsStatus,
};
use orbyn_core::Core;
use orbyn_core::credentials::CredentialError;
use orbyn_core::redaction::Redactor;

use crate::http::{ApiFailure, ApiJson, ApiResult, AppState};

pub fn routes(state: AppState) -> Router<AppState> {
    // Saving or removing a key is the person's answer at this computer, like the redaction writes.
    let writes = Router::new()
        .route("/api/settings/engine-credentials/{engine}", put(set_opt_in))
        .route(
            "/api/settings/engine-credentials/{engine}/key",
            put(save_key).delete(remove_key),
        )
        .route(
            "/api/settings/engine-credentials/{engine}/retry",
            post(retry),
        )
        .route_layer(from_fn_with_state(state.clone(), local_only));
    Router::new()
        .route("/api/settings/engine-credentials/{engine}", get(status))
        .merge(writes)
        .route_layer(from_fn_with_state(state, redact_errors))
}

/// The most of an error body read for redaction; ours are a few hundred bytes.
const ERROR_BODY_LIMIT: usize = 64 * 1024;

/// The words of an error with every recognised secret format hidden (the fixed set no setting can turn
/// off, then the person's own patterns), so a key-shaped string that reaches a message never leaves.
pub(super) fn redact_message(core: &Core, text: &str) -> String {
    let text = Redactor::locked().labelled(text);
    match core.display_redactor() {
        Ok(person) => person.labelled(&text),
        Err(_) => text,
    }
}

/// Every error these routes return goes through the redaction layer before it leaves.
pub(super) async fn redact_errors(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let response = next.run(request).await;
    if !(response.status().is_client_error() || response.status().is_server_error()) {
        return response;
    }
    let (mut parts, body) = response.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, ERROR_BODY_LIMIT).await else {
        return (parts.status, "").into_response();
    };
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return (parts.status, "").into_response();
    };
    if let Some(message) = value.get("message").and_then(|m| m.as_str()) {
        value["message"] = serde_json::Value::String(redact_message(&state.core, message));
    }
    parts.headers.remove(axum::http::header::CONTENT_LENGTH);
    (parts.status, parts.headers, Json(value)).into_response()
}

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

/// A refusal with a fixed message: no key text, whatever the cause.
fn refuse(error: CredentialError) -> ApiFailure {
    let (status, code, message) = match error {
        CredentialError::UnknownEngine => (
            StatusCode::NOT_FOUND,
            "not_found",
            "This engine does not keep a key.",
        ),
        CredentialError::Off => (
            StatusCode::CONFLICT,
            "opt_in_off",
            "Turn on keeping a key for this engine first.",
        ),
        CredentialError::InvalidKey => (
            StatusCode::BAD_REQUEST,
            "invalid_key",
            "The key is empty, too long or has a space or line break.",
        ),
        CredentialError::Unavailable => (
            StatusCode::SERVICE_UNAVAILABLE,
            "keychain_unavailable",
            "There is no usable keychain on this computer.",
        ),
        CredentialError::Blocked => (
            StatusCode::CONFLICT,
            "keychain_blocked",
            "The keychain refused for now. Unlock it and try again.",
        ),
        CredentialError::Failed => (
            StatusCode::SERVICE_UNAVAILABLE,
            "keychain_failed",
            "The keychain could not be used. Try again.",
        ),
    };
    ApiFailure::new(status, code, message)
}

/// The keychain may prompt or block, so it runs off the async threads.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, CredentialError> + Send + 'static,
) -> ApiResult<T> {
    match tokio::task::spawn_blocking(work).await {
        Ok(result) => result.map_err(refuse),
        Err(_) => Err(refuse(CredentialError::Failed)),
    }
}

async fn status(
    State(state): State<AppState>,
    Path(engine): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let core = state.core.clone();
    Ok(Json::<EngineCredentialsStatus>(
        blocking(move || core.engine_credentials_status(&engine)).await?,
    ))
}

async fn set_opt_in(
    State(state): State<AppState>,
    Path(engine): Path<String>,
    ApiJson(change): ApiJson<EngineCredentialsOptIn>,
) -> ApiResult<impl IntoResponse> {
    let core = state.core.clone();
    Ok(Json(
        blocking(move || core.set_engine_credentials_opt_in(&engine, change.opted_in)).await?,
    ))
}

async fn save_key(
    State(state): State<AppState>,
    Path(engine): Path<String>,
    ApiJson(input): ApiJson<EngineCredentialKeyInput>,
) -> ApiResult<impl IntoResponse> {
    let core = state.core.clone();
    Ok(Json(
        blocking(move || core.save_engine_credentials_key(&engine, &input.key)).await?,
    ))
}

async fn remove_key(
    State(state): State<AppState>,
    Path(engine): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let core = state.core.clone();
    let (status, stopped) = blocking(move || core.remove_engine_credentials_key(&engine)).await?;
    Ok(Json(EngineCredentialsRevoked {
        status,
        stopped: u32::try_from(stopped).unwrap_or(u32::MAX),
    }))
}

/// Asks the keychain again, for example after it was blocked, and answers the status. Changes nothing.
async fn retry(
    State(state): State<AppState>,
    Path(engine): Path<String>,
) -> ApiResult<impl IntoResponse> {
    status(State(state), Path(engine)).await
}
