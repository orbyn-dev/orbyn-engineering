//! Execution profiles (docs/execution-profiles-and-library.md, Part 1). Reads and the draft check are `Local`. Creating, changing,
//! deleting and applying a profile are `Person` routes: they can raise a permission mode or an Autopilot mode, so a service token
//! alone is refused (`classes.rs`). Applying to a crew is `POST /api/crew/profile/apply`, in `crew.rs` with the other Crew writes.

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post, put};
use orbyn_contracts::{ApplyProfile, ProfileDraft, UpdateProfile};

use crate::http::{ApiJson, ApiResult, AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/profiles", get(list).post(create))
        .route("/api/profiles/check", post(check))
        .route("/api/profiles/{id}", get(one).patch(update).delete(remove))
        .route("/api/threads/{id}/profile", put(apply_thread))
}

/// A bare array: built-ins first.
async fn list(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.list_profiles().await?))
}

async fn one(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.get_profile(&id).await?))
}

async fn check(
    State(state): State<AppState>,
    ApiJson(draft): ApiJson<ProfileDraft>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.check_profile(&draft).await?))
}

async fn create(
    State(state): State<AppState>,
    ApiJson(draft): ApiJson<ProfileDraft>,
) -> ApiResult<impl IntoResponse> {
    Ok((
        StatusCode::CREATED,
        Json(state.core.create_profile(&draft).await?),
    ))
}

async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(patch): ApiJson<UpdateProfile>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.update_profile(&id, &patch).await?))
}

async fn remove(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    state.core.delete_profile(&id)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn apply_thread(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<ApplyProfile>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.apply_profile_to_thread(&id, &body).await?))
}
