//! Opening a session with an access key. The one route that needs no credential and is not a hook.

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde::Deserialize;

use crate::http::access::Access;
use crate::http::{ApiFailure, ApiJson, ApiResult, AppState};

pub fn routes() -> Router<AppState> {
    Router::new().route("/auth/session", post(open_session))
}

#[derive(Deserialize)]
struct SessionRequest {
    key: String,
}

async fn open_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<SessionRequest>,
) -> ApiResult<Response> {
    let Some(session) = state.access.open_session(&body.key) else {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        return Err(ApiFailure::new(
            StatusCode::UNAUTHORIZED,
            "invalid_key",
            "That access key is not valid.",
        ));
    };
    let secure = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        == Some("https");
    Ok((
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, Access::session_cookie(&session, secure))],
    )
        .into_response())
}
