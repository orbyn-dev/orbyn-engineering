//! The debug-only dev seed route (KC9): `POST /api/crew/dev/seed-proposal` writes generic escape-source lessons and
//! lets the core's proposal maker offer the resulting skill. It exists so the web real-service specs can reach a
//! proposal without running three real confirmed escapes.
//!
//! The module is compiled only with `#[cfg(debug_assertions)]`, and `crew::routes` merges it only when
//! `ORBYN_DEV_SEED_PROPOSALS` was `1` or `true` at service start, so a release build and an ordinary debug run have no
//! such route (404). It adds nothing to `classes.rs` or `tests/routes.json`; the route-table test runs without the flag
//! and proves the route is absent. The handler is transport only: `Core::dev_seed_proposal` holds the logic and audits
//! every use. Crew Preview gates it like every Crew route.

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::post;
use orbyn_core::CoreError;
use serde::Deserialize;
use serde_json::json;

use crate::http::{ApiFailure, ApiJson, ApiResult, AppState};

/// The one debug route. Merged into the `local_only` block of `crew::routes`.
pub fn routes() -> Router<AppState> {
    Router::new().route("/api/crew/dev/seed-proposal", post(seed_proposal))
}

/// `{ repo, lessons?, area? }`: `lessons` is 1 to 6 (3 when absent) and `area` is `src/dev-seed` when absent.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedProposalBody {
    repo: String,
    #[serde(default = "default_lessons")]
    lessons: u32,
    #[serde(default = "default_area")]
    area: String,
}

fn default_lessons() -> u32 {
    3
}

fn default_area() -> String {
    "src/dev-seed".to_owned()
}

fn invalid(message: &str) -> ApiFailure {
    ApiFailure::new(StatusCode::BAD_REQUEST, "invalid", message)
}

/// The contract's `invalid` for a bad count; every other error keeps its status and code (`not_found`).
fn map_core<T>(result: orbyn_core::Result<T>) -> ApiResult<T> {
    result.map_err(|error| match error {
        CoreError::Invalid(message) => invalid(&message),
        other => other.into(),
    })
}

async fn seed_proposal(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<SeedProposalBody>,
) -> ApiResult<impl IntoResponse> {
    let seeded = map_core(
        state
            .core
            .dev_seed_proposal(&body.repo, body.lessons, &body.area),
    )?;
    Ok(Json(json!({
        "lessons": seeded.lessons,
        "proposal_id": seeded.proposal_id,
    })))
}
