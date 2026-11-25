//! Issue trackers (docs/issues.md). Beta, behind `CoreConfig::issues`: with it off every route here answers 404.
//! Reads are `Local`; connecting, disconnecting, binding a project's tracker, creating, commenting, reacting, linking
//! and starting a thread are `Person` routes (docs/issues.md).

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use orbyn_contracts::{
    CommentIssue, ConnectTracker, CreateIssue, IssueKey, ReactIssue, StartIssueThread, TrackerKind,
};
use orbyn_core::CoreError;
use orbyn_core::issues::IssueQuery;
use serde::Deserialize;

use crate::http::{ApiJson, ApiResult, AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/issues/trackers", get(trackers))
        .route("/api/issues/trackers/{tracker}/connect", post(connect))
        .route(
            "/api/issues/trackers/{tracker}/disconnect",
            post(disconnect),
        )
        .route("/api/issues", get(list))
        .route("/api/issues/item", get(item))
        .route("/api/issues/templates", get(templates))
        .route("/api/issues/links", get(links))
        .route("/api/issues/find", get(find))
        .route("/api/issues/draft", get(draft))
        .route("/api/issues/item/create", post(create))
        .route("/api/issues/item/comment", post(comment))
        .route("/api/issues/item/react", post(react))
        .route("/api/issues/start-thread", post(start_thread))
        .route("/api/threads/{id}/issue-links", post(link))
        .route("/api/threads/{id}/issue-links/remove", post(unlink))
}

pub(crate) fn on(state: &AppState) -> Result<(), CoreError> {
    if state.core.config.issues {
        Ok(())
    } else {
        Err(CoreError::NotFound("issues".into()))
    }
}

async fn trackers(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    on(&state)?;
    Ok(Json(state.core.issue_trackers().await?))
}

async fn connect(
    State(state): State<AppState>,
    Path(tracker): Path<TrackerKind>,
    ApiJson(body): ApiJson<ConnectTracker>,
) -> ApiResult<impl IntoResponse> {
    on(&state)?;
    Ok(Json(state.core.connect_issue_tracker(tracker, body).await?))
}

async fn disconnect(
    State(state): State<AppState>,
    Path(tracker): Path<TrackerKind>,
) -> ApiResult<impl IntoResponse> {
    on(&state)?;
    Ok(Json(state.core.disconnect_issue_tracker(tracker).await?))
}

type Pairs = Result<Query<Vec<(String, String)>>, axum::extract::rejection::QueryRejection>;

fn pairs(query: Pairs) -> Result<Vec<(String, String)>, CoreError> {
    query
        .map(|Query(p)| p)
        .map_err(|_| CoreError::Invalid("the query is not valid".into()))
}

async fn list(State(state): State<AppState>, query: Pairs) -> ApiResult<impl IntoResponse> {
    on(&state)?;
    let query = IssueQuery::from_pairs(&pairs(query)?)?;
    Ok(Json(state.core.list_issues(&query).await?))
}

#[derive(Deserialize)]
struct KeyQuery {
    tracker: TrackerKind,
    scope: String,
    key: String,
}

impl From<KeyQuery> for IssueKey {
    fn from(q: KeyQuery) -> Self {
        Self {
            tracker: q.tracker,
            scope: q.scope,
            key: q.key,
        }
    }
}

async fn item(
    State(state): State<AppState>,
    Query(q): Query<KeyQuery>,
) -> ApiResult<impl IntoResponse> {
    on(&state)?;
    Ok(Json(state.core.issue(&q.into()).await?))
}

#[derive(Deserialize)]
struct TemplatesQuery {
    tracker: TrackerKind,
    scope: String,
}

async fn templates(
    State(state): State<AppState>,
    Query(q): Query<TemplatesQuery>,
) -> ApiResult<impl IntoResponse> {
    on(&state)?;
    Ok(Json(state.core.issue_templates(q.tracker, &q.scope).await?))
}

async fn links(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    on(&state)?;
    Ok(Json(state.core.issue_links()?))
}

#[derive(Deserialize)]
struct FindQuery {
    q: String,
}

async fn find(
    State(state): State<AppState>,
    Query(q): Query<FindQuery>,
) -> ApiResult<impl IntoResponse> {
    on(&state)?;
    Ok(Json(state.core.find_issues(&q.q).await?))
}

async fn draft(
    State(state): State<AppState>,
    Query(q): Query<KeyQuery>,
) -> ApiResult<impl IntoResponse> {
    on(&state)?;
    Ok(Json(state.core.issue_draft(&q.into()).await?))
}

async fn start_thread(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<StartIssueThread>,
) -> ApiResult<impl IntoResponse> {
    on(&state)?;
    Ok(Json(state.core.start_issue_thread(body).await?))
}

async fn create(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<CreateIssue>,
) -> ApiResult<impl IntoResponse> {
    on(&state)?;
    Ok(Json(state.core.create_issue(body).await?))
}

async fn comment(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<CommentIssue>,
) -> ApiResult<impl IntoResponse> {
    on(&state)?;
    Ok(Json(state.core.comment_issue(body).await?))
}

async fn react(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<ReactIssue>,
) -> ApiResult<impl IntoResponse> {
    on(&state)?;
    Ok(Json(state.core.react_issue(body).await?))
}

async fn link(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<IssueKey>,
) -> ApiResult<impl IntoResponse> {
    on(&state)?;
    state.core.link_issue(&id, &body).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn unlink(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<IssueKey>,
) -> ApiResult<impl IntoResponse> {
    on(&state)?;
    state.core.unlink_issue(&id, &body)?;
    Ok(StatusCode::NO_CONTENT)
}
