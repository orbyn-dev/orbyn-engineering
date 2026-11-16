//! A person's two writes about one area (slice 4j, enforcement): keep the ceiling over the circuit breaker's lowering or set the area to
//! `off`, and go back to following the record. Both are in `orbyn_core::crew::changes::STATE_CHANGING`, so Autopilot's N1 refuses an
//! agent's request to them by path, and both sit behind the Preview gate and `local_only` of [`super::crew::routes`], which merges
//! this router before it applies them. Plain local writes: no presence grant. Any local process that holds the service token can post
//! the same body, as it can `PUT /api/crew/autopilot/mode`, which already sets a whole repository to Full. `keep_ceiling` loosens: it
//! returns an area to the ceiling the crew has, so it is no stronger than that, but it is a loosening.

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::post;
use orbyn_contracts as wire;
use orbyn_core::util::unix_now;

use crate::http::{ApiJson, ApiResult, AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/crew/autopilot/areas/override", post(area_override))
        .route("/api/crew/autopilot/areas/follow", post(area_follow))
}

async fn area_override(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<wire::AutopilotAreaOverrideBody>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state
            .core
            .autopilot_area_override(&body, unix_now())
            .await?,
    ))
}

async fn area_follow(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<wire::AutopilotAreaFollowBody>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state.core.autopilot_area_follow(&body, unix_now()).await?,
    ))
}
