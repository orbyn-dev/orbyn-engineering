//! One error shape for every route: an `ApiError` body with a status.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use orbyn_contracts::ApiError;
use orbyn_core::CoreError;
use serde_json::json;

pub struct ApiFailure {
    status: StatusCode,
    body: ApiError,
}

impl ApiFailure {
    pub fn forbidden(code: &str, message: &str) -> Self {
        Self::new(StatusCode::FORBIDDEN, code, message)
    }

    pub fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "Sign in to use this service.",
        )
    }

    pub fn new(status: StatusCode, code: &str, message: &str) -> Self {
        Self {
            status,
            body: ApiError {
                code: code.to_owned(),
                message: message.to_owned(),
                detail: None,
            },
        }
    }
}

impl From<CoreError> for ApiFailure {
    fn from(error: CoreError) -> Self {
        let status =
            StatusCode::from_u16(error.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        if status.is_server_error() {
            tracing::error!(%error, "request failed");
        }
        let detail = match &error {
            CoreError::AlreadyDecided(existing) => serde_json::to_value(existing).ok(),
            CoreError::QuestionClosed(existing) => serde_json::to_value(existing).ok(),
            CoreError::ProjectExists(existing) => serde_json::to_value(existing).ok(),
            CoreError::ProjectHasWork { detail, .. } => Some(detail.clone()),
            CoreError::FileConflict { revision } => Some(json!({ "revision": revision })),
            _ => None,
        };
        Self {
            status,
            body: ApiError {
                code: error.code().to_owned(),
                message: error.public_message(),
                detail,
            },
        }
    }
}

impl IntoResponse for ApiFailure {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiFailure>;
