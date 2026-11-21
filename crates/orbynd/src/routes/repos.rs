//! Repositories of a project (docs/multi-repo.md). Beta, behind `CoreConfig::multi_repo`: with it off every route
//! here answers 404. The list is `Local`; every write is a `Person` route, the scan included, because it reads a
//! folder the person picked.

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use orbyn_contracts::{AddRepoFolder, CloneRepo, CreateRepo, ScanRepos};

use crate::http::{ApiJson, ApiResult, AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/projects/{id}/repos", get(list))
        .route("/api/projects/{id}/repos/add-folder", post(add_folder))
        .route("/api/projects/{id}/repos/create", post(create))
        .route("/api/projects/{id}/repos/clone", post(clone))
        .route("/api/projects/{id}/repos/scan", post(scan))
        .route("/api/projects/{id}/repos/{repo_id}/publish", post(publish))
        .route("/api/projects/{id}/repos/{repo_id}/trust", post(trust))
        .route(
            "/api/projects/{id}/repos/{repo_id}/setup-git",
            post(setup_git),
        )
        .route("/api/projects/{id}/repos/{repo_id}/remove", post(remove))
        .route(
            "/api/projects/{id}/repos/{repo_id}/clone/cancel",
            post(cancel_clone),
        )
}

async fn list(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.repos_view(&id).await?))
}

async fn add_folder(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<AddRepoFolder>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.add_repo_folder(&id, body).await?))
}

async fn create(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<CreateRepo>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.create_repo(&id, body).await?))
}

async fn clone(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<CloneRepo>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.clone_repo(&id, body).await?))
}

async fn scan(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<ScanRepos>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.scan_repos(&id, body).await?))
}

async fn publish(
    State(state): State<AppState>,
    Path((id, repo_id)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.publish_repo(&id, &repo_id).await?))
}

async fn trust(
    State(state): State<AppState>,
    Path((id, repo_id)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.trust_repo(&id, &repo_id).await?))
}

async fn setup_git(
    State(state): State<AppState>,
    Path((id, repo_id)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.setup_repo_git(&id, &repo_id).await?))
}

async fn remove(
    State(state): State<AppState>,
    Path((id, repo_id)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    state.core.remove_repo(&id, &repo_id)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn cancel_clone(
    State(state): State<AppState>,
    Path((id, repo_id)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.cancel_repo_clone(&id, &repo_id)?))
}
