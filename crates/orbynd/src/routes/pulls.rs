//! Pull request routes: a project's forge read and written through its own CLI.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use orbyn_contracts::{
    AddPullComment, AskAboutStop, BulkClosePulls, CheckoutPull, EditPullComment, GeneratePullGuide,
    MergePull, PullAction, PullDetailsRequest, ReactPullComment, ResolvePullThread, SetFilesViewed,
    SetPullPeople, StackAction, SubmitReview, UpdatePull, WorkOnPull,
};
use orbyn_core::source_control::forge::PullFilter;

use crate::http::{ApiJson, ApiResult, AppState, LOCAL_USER};
use axum::Router;
use axum::routing::{get, patch, post};
use orbyn_contracts::LinkPull;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/threads/{id}/pulls", get(linked_pulls).post(link_pull))
        .route("/api/threads/{id}/pulls/refresh", post(refresh_pulls))
        .route(
            "/api/threads/{id}/pulls/{number}",
            get(pull_detail).delete(unlink_pull),
        )
        .route("/api/threads/{id}/pulls/{number}/review", post(review_pull))
        .route("/api/threads/{id}/pulls/{number}/merge", post(merge_pull))
        .route("/api/forges", get(forges))
        .route("/api/pulls", get(all))
        .route("/api/pulls/details", post(details))
        .route("/api/projects/{id}/pulls", get(list))
        .route("/api/projects/{id}/pulls/close", post(close_many))
        .route(
            "/api/projects/{id}/pulls/reviewers",
            get(reviewer_candidates),
        )
        .route("/api/projects/{id}/pulls/labels", get(label_candidates))
        .route(
            "/api/projects/{id}/workflows/{run}/approve",
            post(approve_workflow),
        )
        .route(
            "/api/projects/{id}/pulls/{number}",
            get(detail).patch(update),
        )
        .route("/api/projects/{id}/pulls/{number}/checks", get(checks))
        .route(
            "/api/projects/{id}/pulls/{number}/comments",
            get(conversation).post(comment),
        )
        .route(
            "/api/projects/{id}/pulls/{number}/comments/{comment}",
            patch(edit_comment),
        )
        .route(
            "/api/projects/{id}/pulls/{number}/comments/{comment}/react",
            post(react),
        )
        .route(
            "/api/projects/{id}/pulls/{number}/threads/{thread}/resolve",
            post(resolve_thread),
        )
        .route("/api/projects/{id}/pulls/{number}/timeline", get(timeline))
        .route("/api/projects/{id}/pulls/{number}/files", get(files))
        .route(
            "/api/projects/{id}/pulls/{number}/revisions",
            get(revisions),
        )
        .route(
            "/api/projects/{id}/commits/{sha}/files",
            get(revision_files),
        )
        .route("/api/projects/{id}/pulls/{number}/viewed", post(set_viewed))
        .route("/api/projects/{id}/pulls/{number}/review", post(review))
        .route(
            "/api/projects/{id}/pulls/{number}/reviewers",
            post(set_reviewers),
        )
        .route("/api/projects/{id}/pulls/{number}/labels", post(set_labels))
        .route("/api/projects/{id}/pulls/{number}/merge", post(merge))
        .route("/api/projects/{id}/pulls/{number}/action", post(action))
        .route(
            "/api/projects/{id}/pulls/{number}/stack",
            get(stack).post(stack_action),
        )
        .route("/api/projects/{id}/pulls/{number}/checkout", post(checkout))
        .route("/api/projects/{id}/pulls/{number}/work", post(work_on))
        .route(
            "/api/projects/{id}/pulls/{number}/guide",
            get(guide).post(generate_guide),
        )
        .route(
            "/api/projects/{id}/pulls/{number}/guide/ask",
            post(ask_about_stop),
        )
        .route("/api/threads/{id}/watches", get(watches))
        .route("/api/watches/check", post(check_watches))
        .route(
            "/api/threads/{id}/pulls/{number}/watch",
            post(watch).delete(unwatch),
        )
}

fn filter_of(query: HashMap<String, String>) -> PullFilter {
    let mut query = query;
    PullFilter {
        state: query.remove("state").unwrap_or_else(|| "open".to_owned()),
        search: query.remove("q").filter(|text| !text.is_empty()),
        author: query.remove("author").filter(|text| !text.is_empty()),
        label: query.remove("label").filter(|text| !text.is_empty()),
        head: None,
    }
}

pub async fn all(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.all_pulls(&filter_of(query)).await?))
}

/// Details the base list leaves out, asked for after the list has been shown.
pub async fn details(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<PullDetailsRequest>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.pull_details(&body).await?))
}

pub async fn list(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state.core.project_pulls(&id, &filter_of(query)).await?,
    ))
}

pub async fn detail(
    State(state): State<AppState>,
    Path((id, number)): Path<(String, u64)>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.project_pull(&id, number).await?))
}

pub async fn update(
    State(state): State<AppState>,
    Path((id, number)): Path<(String, u64)>,
    ApiJson(body): ApiJson<UpdatePull>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state
            .core
            .update_pull(&id, number, &body, LOCAL_USER)
            .await?,
    ))
}

pub async fn checks(
    State(state): State<AppState>,
    Path((id, number)): Path<(String, u64)>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.pull_checks(&id, number).await?))
}

pub async fn approve_workflow(
    State(state): State<AppState>,
    Path((id, run)): Path<(String, u64)>,
) -> ApiResult<StatusCode> {
    state.core.approve_workflow(&id, run, LOCAL_USER).await?;
    Ok(StatusCode::ACCEPTED)
}

pub async fn conversation(
    State(state): State<AppState>,
    Path((id, number)): Path<(String, u64)>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.pull_conversation(&id, number).await?))
}

pub async fn timeline(
    State(state): State<AppState>,
    Path((id, number)): Path<(String, u64)>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.pull_timeline(&id, number).await?))
}

pub async fn files(
    State(state): State<AppState>,
    Path((id, number)): Path<(String, u64)>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.pull_files(&id, number).await?))
}

pub async fn revisions(
    State(state): State<AppState>,
    Path((id, number)): Path<(String, u64)>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.pull_revisions(&id, number).await?))
}

pub async fn revision_files(
    State(state): State<AppState>,
    Path((id, sha)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.revision_files(&id, &sha).await?))
}

pub async fn set_viewed(
    State(state): State<AppState>,
    Path((id, number)): Path<(String, u64)>,
    ApiJson(body): ApiJson<SetFilesViewed>,
) -> ApiResult<StatusCode> {
    state
        .core
        .set_files_viewed(&id, number, &body.paths, body.viewed)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn comment(
    State(state): State<AppState>,
    Path((id, number)): Path<(String, u64)>,
    ApiJson(body): ApiJson<AddPullComment>,
) -> ApiResult<StatusCode> {
    state
        .core
        .comment_pull(&id, number, &body, LOCAL_USER)
        .await?;
    Ok(StatusCode::ACCEPTED)
}

pub async fn edit_comment(
    State(state): State<AppState>,
    Path((id, number, comment)): Path<(String, u64, String)>,
    ApiJson(body): ApiJson<EditPullComment>,
) -> ApiResult<StatusCode> {
    state
        .core
        .edit_pull_comment(&id, number, &comment, &body, LOCAL_USER)
        .await?;
    Ok(StatusCode::ACCEPTED)
}

pub async fn react(
    State(state): State<AppState>,
    Path((id, number, comment)): Path<(String, u64, String)>,
    ApiJson(body): ApiJson<ReactPullComment>,
) -> ApiResult<StatusCode> {
    state
        .core
        .react_pull_comment(&id, number, &comment, &body)
        .await?;
    Ok(StatusCode::ACCEPTED)
}

pub async fn resolve_thread(
    State(state): State<AppState>,
    Path((id, number, thread)): Path<(String, u64, String)>,
    ApiJson(body): ApiJson<ResolvePullThread>,
) -> ApiResult<StatusCode> {
    state
        .core
        .resolve_pull_thread(&id, number, &thread, body.resolved, LOCAL_USER)
        .await?;
    Ok(StatusCode::ACCEPTED)
}

pub async fn review(
    State(state): State<AppState>,
    Path((id, number)): Path<(String, u64)>,
    ApiJson(body): ApiJson<SubmitReview>,
) -> ApiResult<StatusCode> {
    state
        .core
        .submit_pull_review(&id, number, &body, LOCAL_USER)
        .await?;
    Ok(StatusCode::ACCEPTED)
}

pub async fn reviewer_candidates(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.pull_reviewers(&id).await?))
}

pub async fn label_candidates(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.pull_labels(&id).await?))
}

pub async fn set_reviewers(
    State(state): State<AppState>,
    Path((id, number)): Path<(String, u64)>,
    ApiJson(body): ApiJson<SetPullPeople>,
) -> ApiResult<StatusCode> {
    state
        .core
        .set_pull_reviewers(&id, number, &body, LOCAL_USER)
        .await?;
    Ok(StatusCode::ACCEPTED)
}

pub async fn set_labels(
    State(state): State<AppState>,
    Path((id, number)): Path<(String, u64)>,
    ApiJson(body): ApiJson<SetPullPeople>,
) -> ApiResult<StatusCode> {
    state
        .core
        .set_pull_labels(&id, number, &body, LOCAL_USER)
        .await?;
    Ok(StatusCode::ACCEPTED)
}

pub async fn merge(
    State(state): State<AppState>,
    Path((id, number)): Path<(String, u64)>,
    ApiJson(body): ApiJson<MergePull>,
) -> ApiResult<StatusCode> {
    state
        .core
        .merge_project_pull(&id, number, &body, LOCAL_USER)
        .await?;
    Ok(StatusCode::ACCEPTED)
}

pub async fn action(
    State(state): State<AppState>,
    Path((id, number)): Path<(String, u64)>,
    ApiJson(body): ApiJson<PullAction>,
) -> ApiResult<StatusCode> {
    state
        .core
        .pull_action(&id, number, &body.action, LOCAL_USER)
        .await?;
    Ok(StatusCode::ACCEPTED)
}

pub async fn close_many(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<BulkClosePulls>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state
            .core
            .close_pulls(&id, &body.numbers, LOCAL_USER)
            .await?,
    ))
}

pub async fn stack(
    State(state): State<AppState>,
    Path((id, number)): Path<(String, u64)>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.pull_stack(&id, number).await?))
}

pub async fn stack_action(
    State(state): State<AppState>,
    Path((id, number)): Path<(String, u64)>,
    ApiJson(body): ApiJson<StackAction>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state
            .core
            .stack_action(&id, number, &body, LOCAL_USER)
            .await?,
    ))
}

pub async fn checkout(
    State(state): State<AppState>,
    Path((id, number)): Path<(String, u64)>,
    ApiJson(body): ApiJson<CheckoutPull>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state
            .core
            .checkout_pull(&id, number, &body, LOCAL_USER)
            .await?,
    ))
}

pub async fn work_on(
    State(state): State<AppState>,
    Path((id, number)): Path<(String, u64)>,
    ApiJson(body): ApiJson<WorkOnPull>,
) -> ApiResult<impl IntoResponse> {
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .core
                .work_on_pull(&id, number, &body, LOCAL_USER)
                .await?,
        ),
    ))
}

pub async fn guide(
    State(state): State<AppState>,
    Path((id, number)): Path<(String, u64)>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.pull_guide_state(&id, number)?))
}

/// Starts the thread that writes the guide; the guide arrives when its agent
/// calls the write tool.
pub async fn generate_guide(
    State(state): State<AppState>,
    Path((id, number)): Path<(String, u64)>,
    ApiJson(body): ApiJson<GeneratePullGuide>,
) -> ApiResult<impl IntoResponse> {
    Ok((
        StatusCode::CREATED,
        Json(state.core.generate_pull_guide(&id, number, &body).await?),
    ))
}

/// A new thread about one guide stop; the stop comes back as its draft.
pub async fn ask_about_stop(
    State(state): State<AppState>,
    Path((id, number)): Path<(String, u64)>,
    ApiJson(body): ApiJson<AskAboutStop>,
) -> ApiResult<impl IntoResponse> {
    Ok((
        StatusCode::CREATED,
        Json(state.core.ask_about_stop(&id, number, &body).await?),
    ))
}

pub async fn watches(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.thread_watches(&id)?))
}

pub async fn watch(
    State(state): State<AppState>,
    Path((id, number)): Path<(String, u64)>,
) -> ApiResult<impl IntoResponse> {
    Ok((
        StatusCode::CREATED,
        Json(state.core.watch_pull(&id, number)?),
    ))
}

pub async fn unwatch(
    State(state): State<AppState>,
    Path((id, number)): Path<(String, u64)>,
) -> ApiResult<StatusCode> {
    state.core.store.unwatch_pull(&id, number)?;
    Ok(StatusCode::NO_CONTENT)
}

/// One pass of the watch now, rather than at the next two-minute tick.
pub async fn check_watches(State(state): State<AppState>) -> StatusCode {
    state.core.watch_pulls().await;
    StatusCode::NO_CONTENT
}

pub async fn forges(
    State(state): State<AppState>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let rescan = query.get("rescan").is_some_and(|value| value == "true");
    Json(state.core.forges(rescan).await)
}

async fn linked_pulls(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    state.core.store.thread(&id)?;
    Ok(Json(state.core.store.linked_pulls(&id)?))
}

async fn link_pull(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<LinkPull>,
) -> ApiResult<impl IntoResponse> {
    Ok((
        StatusCode::CREATED,
        Json(state.core.link_pull(&id, body.number).await?),
    ))
}

async fn unlink_pull(
    State(state): State<AppState>,
    Path((id, number)): Path<(String, u64)>,
) -> ApiResult<StatusCode> {
    state.core.store.unlink_pull(&id, number)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn refresh_pulls(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.refresh_pulls(&id).await?))
}

async fn pull_detail(
    State(state): State<AppState>,
    Path((id, number)): Path<(String, u64)>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.pull_detail(&id, number).await?))
}

async fn review_pull(
    State(state): State<AppState>,
    Path((id, number)): Path<(String, u64)>,
    ApiJson(body): ApiJson<SubmitReview>,
) -> ApiResult<StatusCode> {
    state
        .core
        .review_pull(&id, number, &body, LOCAL_USER)
        .await?;
    Ok(StatusCode::ACCEPTED)
}

async fn merge_pull(
    State(state): State<AppState>,
    Path((id, number)): Path<(String, u64)>,
    ApiJson(body): ApiJson<MergePull>,
) -> ApiResult<StatusCode> {
    state
        .core
        .merge_pull(&id, number, &body, LOCAL_USER)
        .await?;
    Ok(StatusCode::ACCEPTED)
}
