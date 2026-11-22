//! Source control for a thread: git actions, branches, commits, the diff and Jujutsu
//! status, and the local repository reads the app answers from its own records.

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
            "/api/threads/{id}/vcs/refresh-status",
            post(refresh_vcs_status),
        )
        .route("/api/threads/{id}/vcs/refs", post(create_ref))
        .route("/api/threads/{id}/vcs/refs/switch", post(switch_ref))
        .route(
            "/api/source-control/repository-star",
            get(repository_star_status),
        )
        .route("/api/source-control/repository", get(repository_lookup))
        .route("/api/source-control/starred", get(starred_repositories))
        .route("/api/threads/{id}/jj", get(thread_jj_status))
        .route("/api/threads/{id}/diff", get(thread_diff))
}

async fn repository_star_status(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.core.repository_star_status().await)
}

/// The repository's `owner/name` as its remotes name it; the lookup is local.
#[derive(Deserialize)]
struct RepositoryQuery {
    #[serde(default)]
    repository: String,
}

async fn repository_lookup(
    State(state): State<AppState>,
    Query(query): Query<RepositoryQuery>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.repository_lookup(&query.repository).await?))
}

async fn starred_repositories(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.core.starred_repositories().await)
}

async fn git_action(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<GitAction>,
) -> ApiResult<impl IntoResponse> {
    let repo = body.repo_id.clone();
    Ok(Json(
        orbyn_core::in_repo(repo, state.core.git_action(&id, &body, LOCAL_USER)).await?,
    ))
}

/// A multi-repository thread's repository for a read; absent is the primary one.
#[derive(Deserialize)]
struct RepoQuery {
    #[serde(default)]
    repo_id: Option<String>,
}

async fn branches(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<RepoQuery>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        orbyn_core::in_repo(query.repo_id, state.core.branches(&id)).await?,
    ))
}

async fn create_branch(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<CreateBranch>,
) -> ApiResult<StatusCode> {
    let repo = body.repo_id.clone();
    orbyn_core::in_repo(repo, state.core.create_branch(&id, &body, LOCAL_USER)).await?;
    Ok(StatusCode::CREATED)
}

async fn switch_branch(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<SwitchBranch>,
) -> ApiResult<StatusCode> {
    let repo = body.repo_id.clone();
    orbyn_core::in_repo(repo, state.core.switch_branch(&id, &body.name, LOCAL_USER)).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The explicit VCS `refreshStatus`: the checkout's branch, HEAD, tracking ref and
/// working-tree counts, read now rather than from a kept answer.
async fn refresh_vcs_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.refresh_vcs_status(&id).await?))
}

/// The explicit VCS `createRef`: a branch at an optional start, without switching.
async fn create_ref(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<CreateBranch>,
) -> ApiResult<StatusCode> {
    state.core.create_ref(&id, &body, LOCAL_USER).await?;
    Ok(StatusCode::CREATED)
}

/// The explicit VCS `switchRef`: move the checkout to an existing ref.
async fn switch_ref(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<SwitchBranch>,
) -> ApiResult<StatusCode> {
    state.core.switch_ref(&id, &body.name, LOCAL_USER).await?;
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
    repo_id: Option<String>,
}

async fn thread_diff(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<DiffQuery>,
) -> ApiResult<impl IntoResponse> {
    let repo = query.repo_id.clone();
    let which = state.core.diff_repo(&id, repo.as_deref())?;
    let Json(mut files) = orbyn_core::in_repo(repo, thread_diff_in_repo(state, id, query)).await?;
    for file in &mut files {
        file.repo_id.clone_from(&which);
    }
    Ok(Json(files))
}

async fn thread_diff_in_repo(
    state: AppState,
    id: String,
    query: DiffQuery,
) -> ApiResult<Json<Vec<orbyn_contracts::FileDiff>>> {
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
    Query(query): Query<RepoQuery>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        orbyn_core::in_repo(query.repo_id, state.core.thread_commits(&id)).await?,
    ))
}
