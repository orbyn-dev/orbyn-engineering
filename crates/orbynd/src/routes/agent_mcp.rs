//! The MCP endpoint an agent reaches by thread token, outside the credential layer.

use axum::Json;
use axum::Router;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde_json::{Value, json};

use crate::http::{ApiFailure, ApiJson, AppState};

pub fn routes() -> Router<AppState> {
    Router::new().route(
        "/mcp/{thread_token}",
        post(mcp).get(mcp_get).layer(DefaultBodyLimit::max(
            orbyn_core::tools::mcp::MAX_REQUEST_BYTES,
        )),
    )
}

/// `POST /mcp/:thread_token`. The token is the only credential on this route.
async fn mcp(
    State(state): State<AppState>,
    Path(token): Path<String>,
    ApiJson(message): ApiJson<Value>,
) -> Response {
    // Drop-safe in core: a client that disconnects mid-approval expires it.
    match state.core.mcp.handle(&token, &message).await {
        Ok(Some(reply)) => Json(reply).into_response(),
        Ok(None) => StatusCode::ACCEPTED.into_response(),
        Err(_) => ApiFailure::new(
            StatusCode::NOT_FOUND,
            "unknown_token",
            "This MCP session has ended.",
        )
        .into_response(),
    }
}

async fn mcp_get() -> Response {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        [(header::ALLOW, "POST")],
        Json(
            json!({ "code": "method_not_allowed", "message": "This server has no event stream." }),
        ),
    )
        .into_response()
}
