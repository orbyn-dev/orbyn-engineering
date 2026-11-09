//! Approvals and questions an agent is waiting on.

use std::collections::HashMap;

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use orbyn_contracts::{AnswerQuestion, ApprovalState, DecideApproval, QuestionState};
use orbyn_core::CoreError;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::http::{ApiFailure, ApiJson, ApiResult, AppState, LOCAL_USER};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/approvals", get(list_approvals))
        .route("/api/approvals/{id}/decide", post(decide_approval))
        .route("/api/questions", get(list_questions))
        .route("/api/questions/{id}/answer", post(answer_question))
        .route("/api/questions/{id}/dismiss", post(dismiss_question))
}

async fn list_questions(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<impl IntoResponse> {
    let filter = query
        .get("state")
        .map(|text| serde_json::from_value::<QuestionState>(json!(text)))
        .transpose()
        .map_err(|_| ApiFailure::new(StatusCode::BAD_REQUEST, "invalid_input", "Unknown state."))?;
    let thread = query.get("thread_id").map(String::as_str);
    Ok(Json(state.core.store.list_questions(thread, filter)?))
}

async fn answer_question(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<AnswerQuestion>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.questions.answer(
        &id,
        body.answers,
        body.attachments.unwrap_or_default(),
        LOCAL_USER,
    )?))
}

async fn dismiss_question(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.questions.dismiss(&id, LOCAL_USER)?))
}

#[derive(Deserialize)]
struct ApprovalsQuery {
    state: Option<String>,
}

async fn list_approvals(
    State(state): State<AppState>,
    Query(query): Query<ApprovalsQuery>,
) -> ApiResult<impl IntoResponse> {
    let filter = query
        .state
        .map(|text| {
            serde_json::from_value::<ApprovalState>(Value::String(text))
                .map_err(|_| CoreError::Invalid("Unknown approval state.".into()))
        })
        .transpose()?;
    Ok(Json(state.core.store.list_approvals(filter)?))
}

async fn decide_approval(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<DecideApproval>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.gate.decide_with(
        &id,
        body.allow,
        body.remember,
        body.feedback,
        LOCAL_USER,
    )?))
}
