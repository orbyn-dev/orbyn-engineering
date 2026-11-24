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
    /// Fields sent beside `code` and `message` (a run-start refusal's `engine`, `thread_id` and so on).
    extra: Option<serde_json::Map<String, serde_json::Value>>,
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
            extra: None,
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
            CoreError::PatternRefused { detail, .. } => Some(detail.clone()),
            CoreError::Refused { detail, .. } if !detail.is_null() => Some(detail.clone()),
            CoreError::FileConflict { revision } => Some(json!({ "revision": revision })),
            CoreError::Account {
                retry_after_secs: Some(secs),
                ..
            } => Some(json!({ "retry_after_secs": secs })),
            CoreError::Account {
                tries_left: Some(left),
                ..
            } => Some(json!({ "tries_left": left })),
            _ => None,
        };
        let extra = match &error {
            CoreError::EngineRefused { fields, .. } => fields.as_object().cloned(),
            _ => None,
        };
        Self {
            status,
            body: ApiError {
                code: error.code().to_owned(),
                message: error.public_message(),
                detail,
            },
            extra,
        }
    }
}

impl IntoResponse for ApiFailure {
    fn into_response(self) -> Response {
        let Some(extra) = self.extra else {
            return (self.status, Json(self.body)).into_response();
        };
        let mut body = serde_json::to_value(&self.body).unwrap_or_default();
        if let Some(object) = body.as_object_mut() {
            // `code` and `message` stay the error's own.
            for (name, value) in extra {
                object.entry(name).or_insert(value);
            }
        }
        (self.status, Json(body)).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiFailure>;
