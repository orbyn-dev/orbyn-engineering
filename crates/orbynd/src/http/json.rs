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
///
/// Only the first path segment is named. `serde_path_to_error` renders a struct field and a map key
/// alike (`Segment::Map`), so a deeper segment cannot be shown: inside a map field it is the key the
/// caller chose. Every body this extractor feeds is a struct, or `serde_json::Value` (which has no
/// data errors), so the first segment is a field name this crate declares.
fn refusal(error: &serde_path_to_error::Error<serde_json::Error>) -> String {
    let mut segments = error.path().iter();
    let field = segments.next().map(|segment| segment.to_string());
    let nested = segments.next().is_some();
    match error.inner().classify() {
        Category::Data => match field {
            Some(field) if nested => format!(
                "The request body is not valid: a value under a key in `{field}` has a type this route does not accept."
            ),
            Some(field) => format!(
                "The request body is not valid: the field `{field}` has a value this route does not accept."
            ),
            None => "The request body is not valid: it does not match what this route expects."
                .to_owned(),
        },
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde::Deserialize;

    use super::refusal;

    /// The refusal text a body of the wrong shape produces for `T`.
    fn refusal_of<T: serde::de::DeserializeOwned + std::fmt::Debug>(body: &str) -> String {
        let mut deserializer = serde_json::Deserializer::from_str(body);
        let error = serde_path_to_error::deserialize::<_, T>(&mut deserializer).unwrap_err();
        refusal(&error)
    }

    #[derive(Debug, Deserialize)]
    struct Plain {
        #[allow(dead_code)]
        action: String,
    }

    #[derive(Debug, Deserialize)]
    struct Mapped {
        #[allow(dead_code)]
        options: BTreeMap<String, String>,
    }

    /// A top-level field keeps its name in the refusal, and the value it was sent stays out.
    #[test]
    fn a_refused_field_is_named_and_its_value_is_not() {
        let text = refusal_of::<Plain>(r#"{"action": 48190273}"#);
        assert!(text.contains("`action`"), "{text}");
        assert!(
            !text.contains("48190273"),
            "the answer echoes a value: {text}"
        );
    }

    /// A wrong-typed value inside a map names the map's field, never the caller's key or the value:
    /// `serde_path_to_error` renders a map key like a struct field, so the segment itself is withheld.
    #[test]
    fn a_refused_map_entry_names_the_map_field_not_the_key() {
        let canary = "caller-chosen-key";
        let text = refusal_of::<Mapped>(&format!(r#"{{"options": {{"{canary}": 48190273}}}}"#));
        assert!(text.contains("`options`"), "{text}");
        assert!(
            !text.contains(canary),
            "the answer echoes the caller's key: {text}"
        );
        assert!(
            !text.contains("48190273"),
            "the answer echoes a value: {text}"
        );
    }
}
