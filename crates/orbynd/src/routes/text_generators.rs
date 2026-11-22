//! The one-shot text generator models, per known engine, for Settings. A read: it
//! starts no run and spends no model usage.

use std::collections::HashMap;

use axum::Json;
use axum::Router;
use axum::extract::{Query, State};
use axum::routing::get;
use orbyn_contracts::TextGenerators;

use crate::http::{ApiResult, AppState};

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/text-generators", get(list))
}

/// `?project=<id>` adds that project's effective choice to every engine.
async fn list(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<Json<TextGenerators>> {
    Ok(Json(
        state
            .core
            .text_generators(query.get("project").map(String::as_str))
            .await?,
    ))
}
