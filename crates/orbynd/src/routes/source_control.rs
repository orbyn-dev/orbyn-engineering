//! Source control for a thread: git actions, branches, commits, the diff and Jujutsu status.

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use orbyn_contracts::{CreateBranch, GitAction, SwitchBranch};
use orbyn_core::CoreError;
use serde::Deserialize;

use crate::http::{ApiJson, ApiResult, AppState, LOCAL_USER};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/threads/{id}/commits", get(commits))
        .route("/api/threads/{id}/git/action", post(git_action))
        .route(
            "/api/threads/{id}/branches",
            get(branches).post(create_branch),
        )
        .route("/api/threads/{id}/branches/switch", post(switch_branch))
        .route(
            "/api/source-control/repository-star",
            get(repository_star_status),
        )
        .route("/api/threads/{id}/jj", get(thread_jj_status))
        .route("/api/threads/{id}/diff", get(thread_diff))
}

async fn repository_star_status(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.core.repository_star_status().await)
}

async fn git_action(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<GitAction>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.git_action(&id, &body, LOCAL_USER).await?))
}

async fn branches(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.branches(&id).await?))
}

async fn create_branch(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<CreateBranch>,
) -> ApiResult<StatusCode> {
    state.core.create_branch(&id, &body, LOCAL_USER).await?;
    Ok(StatusCode::CREATED)
}

async fn switch_branch(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<SwitchBranch>,
) -> ApiResult<StatusCode> {
    state
        .core
        .switch_branch(&id, &body.name, LOCAL_USER)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The Jujutsu working-copy change of the thread's checkout, or `null`.
async fn thread_jj_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.thread_jj_status(&id).await?))
}

#[derive(Deserialize)]
struct DiffQuery {
    /// `worktree` by default, or `turn`, `thread`, `branch` or `commit`.
    scope: Option<String>,
    run: Option<String>,
    base: Option<String>,
    sha: Option<String>,
}

async fn thread_diff(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<DiffQuery>,
) -> ApiResult<impl IntoResponse> {
    let core = &state.core;
    let missing = |what: &str| CoreError::Invalid(format!("This diff needs {what}."));
    let files = match query.scope.as_deref() {
        None | Some("worktree") => core.thread_diff(&id).await?,
        Some("turn") => {
            let run = query.run.ok_or_else(|| missing("a run"))?;
            core.turn_diff(&id, &run).await?
        }
        Some("thread") => core.full_thread_diff(&id).await?,
        Some("branch") => {
            let base = query.base.ok_or_else(|| missing("a base branch"))?;
            core.branch_diff(&id, &base).await?
        }
        Some("commit") => {
            let sha = query.sha.ok_or_else(|| missing("a commit"))?;
            core.commit_diff(&id, &sha).await?
        }
        Some(_) => return Err(CoreError::Invalid("Unknown diff scope.".into()).into()),
    };
    Ok(Json(files))
}

async fn commits(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.thread_commits(&id).await?))
}
