//! Skills and skill profiles.

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use orbyn_contracts::SkillProfile;
use orbyn_core::CoreError;
use orbyn_core::tools::skills::DEFAULT_PROFILE;
use orbyn_core::util::new_id;

use crate::http::{ApiJson, ApiResult, AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/skills", get(skills))
        .route("/api/skills/rebuild", post(skills))
        .route(
            "/api/skills/profiles",
            get(list_profiles).post(create_profile),
        )
        .route("/api/skills/profiles/{id}", delete(delete_profile))
}

async fn skills(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.skills.status().await?))
}

async fn list_profiles(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.skills.profiles()?))
}

async fn create_profile(
    State(state): State<AppState>,
    ApiJson(mut profile): ApiJson<SkillProfile>,
) -> ApiResult<impl IntoResponse> {
    if profile.id == DEFAULT_PROFILE {
        return Err(CoreError::Invalid("The default profile is built in.".into()).into());
    }
    if profile.name.trim().is_empty() {
        return Err(CoreError::Invalid("Name this profile.".into()).into());
    }
    if profile.id.is_empty() {
        profile.id = new_id();
    }
    state.core.store.save_profile(&profile)?;
    Ok((StatusCode::CREATED, Json(profile)))
}

async fn delete_profile(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    if id == DEFAULT_PROFILE {
        return Err(CoreError::Invalid("The default profile is built in.".into()).into());
    }
    state.core.store.delete_profile(&id)?;
    Ok(StatusCode::NO_CONTENT)
}
