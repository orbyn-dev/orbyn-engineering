//! A JSON body extractor whose rejection is an `ApiError`.

use axum::Json;
use axum::extract::{FromRequest, Request};
use axum::http::StatusCode;

use crate::http::ApiFailure;

/// A JSON body whose rejection is an `ApiError`, so every failure has a code.
pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    Json<T>: FromRequest<S, Rejection = axum::extract::rejection::JsonRejection>,
{
    type Rejection = ApiFailure;

    async fn from_request(request: Request, state: &S) -> Result<Self, ApiFailure> {
        match Json::<T>::from_request(request, state).await {
            Ok(Json(value)) => Ok(Self(value)),
            Err(rejection) => Err(ApiFailure::new(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                &rejection.body_text(),
            )),
        }
    }
}
