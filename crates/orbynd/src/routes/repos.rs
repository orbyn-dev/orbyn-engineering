//! Repositories of a project (docs/multi-repo.md). Beta, behind `CoreConfig::multi_repo`: with it off every route
//! here answers 404. The list is `Local`; every write is a `Person` route, the scan included, because it reads a
//! folder the person picked.

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use orbyn_contracts::{
    AddRepoFolder, AddThreadRepo, CloneRepo, CreateRepo, ScanRepos, StartThreadInput,
};

use crate::http::{ApiJson, ApiResult, AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/projects/{id}/repos", get(list))
        .route(
            "/api/threads/{id}/repos",
            get(thread_repos).post(add_thread_repo),
        )
        .route(
            "/api/threads/{id}/repos/{repo_id}",
            axum::routing::delete(remove_thread_repo),
        )
        .route("/api/threads/{id}/task-folder", get(task_folder))
        .route("/api/projects/{id}/repo-threads", post(start_thread))
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

async fn thread_repos(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.thread_repos(&id).await?))
}

async fn add_thread_repo(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<AddThreadRepo>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.add_thread_repo(&id, body).await?))
}

async fn remove_thread_repo(
    State(state): State<AppState>,
    Path((id, repo_id)): Path<(String, String)>,
) -> ApiResult<axum::http::StatusCode> {
    state.core.remove_thread_repo(&id, &repo_id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

async fn task_folder(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.task_folder(&id).await?))
}

async fn start_thread(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<StartThreadInput>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.start_repo_thread(&id, body).await?))
}
