//! A JSON body extractor whose rejection is an `ApiError` that names the field, never its value.

use axum::body::Bytes;
use axum::extract::{FromRequest, Request};
use axum::http::{HeaderMap, StatusCode, header};
use serde::de::DeserializeOwned;
use serde_json::error::Category;

use crate::http::ApiFailure;

/// A JSON body whose rejection is an `ApiError`, so every failure has a code.
///
/// The message names the field that refused, never the value that was sent: serde's own
/// rejection text quotes the value ("invalid type: integer `48190273`", "unknown variant
/// `...`"), and an account body can carry a password, an emailed code or a second-factor code.
/// This extractor is the owning boundary, so a new secret field cannot reintroduce the echo.
pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiFailure;

    async fn from_request(request: Request, state: &S) -> Result<Self, ApiFailure> {
        if !json_content_type(request.headers()) {
            return Err(ApiFailure::new(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                "Expected request with `Content-Type: application/json`",
            ));
        }
        let bytes = Bytes::from_request(request, state).await.map_err(|_| {
            ApiFailure::new(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                "The request body could not be read.",
            )
        })?;
        let mut deserializer = serde_json::Deserializer::from_slice(&bytes);
        let value = serde_path_to_error::deserialize(&mut deserializer).map_err(|error| {
            ApiFailure::new(StatusCode::BAD_REQUEST, "invalid_input", &refusal(&error))
        })?;
        deserializer.end().map_err(|_| {
            ApiFailure::new(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                "The request body is not valid JSON.",
            )
        })?;
        Ok(Self(value))
    }
}

/// Why a body of the wrong shape was refused, naming the field: the value and serde's rendering
/// of it stay out of the reply.
fn refusal(error: &serde_path_to_error::Error<serde_json::Error>) -> String {
    let at_field = error.path().iter().next().is_some();
    match error.inner().classify() {
        Category::Data if at_field => format!(
            "The request body is not valid: the field `{}` has a value this route does not accept.",
            error.path()
        ),
        Category::Data => {
            "The request body is not valid: it does not match what this route expects.".to_owned()
        }
        Category::Syntax | Category::Eof => "The request body is not valid JSON.".to_owned(),
        Category::Io => "The request body could not be read.".to_owned(),
    }
}

/// `application/json`, or an `application/*+json` type: what axum's `Json` accepts.
fn json_content_type(headers: &HeaderMap) -> bool {
    let Some(value) = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let essence = value
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    essence == "application/json"
        || (essence.starts_with("application/") && essence.ends_with("+json"))
}
