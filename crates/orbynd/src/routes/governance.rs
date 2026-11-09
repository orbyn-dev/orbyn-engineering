//! Policy rules, the audit log and computer-use access.

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use orbyn_contracts::{ComputerAction, CreatePolicyRule};
use orbyn_core::CoreError;
use orbyn_core::governance::computer::Caller;
use serde::Deserialize;

use crate::http::{ApiJson, ApiResult, AppState, LOCAL_USER};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/policy/rules", get(list_rules).post(create_rule))
        .route("/api/policy/rules/{id}", delete(delete_rule))
        .route("/api/audit", get(audit))
        .route("/api/computer", get(computer_status))
        .route("/api/threads/{id}/computer", post(computer_action))
}

async fn list_rules(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.store.list_rules()?))
}

async fn create_rule(
    State(state): State<AppState>,
    ApiJson(rule): ApiJson<CreatePolicyRule>,
) -> ApiResult<impl IntoResponse> {
    if rule.pattern.is_empty() {
        return Err(CoreError::Invalid("Enter a pattern.".into()).into());
    }
    Ok((
        StatusCode::CREATED,
        Json(state.core.store.add_rule(&rule, LOCAL_USER)?),
    ))
}

async fn delete_rule(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.core.store.delete_rule(&id, LOCAL_USER)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct AuditQuery {
    thread_id: Option<String>,
    limit: Option<u32>,
}

async fn audit(
    State(state): State<AppState>,
    Query(query): Query<AuditQuery>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.store.list_audit(
        query.thread_id.as_deref(),
        query.limit.unwrap_or(200),
    )?))
}

async fn computer_status(State(state): State<AppState>) -> Json<orbyn_contracts::ComputerStatus> {
    Json(state.core.computer.status())
}

/// Runs against the thread's active run when there is one, so `ask` can reach a
/// person. With no run, only an explicit allow rule passes.
async fn computer_action(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(action): ApiJson<ComputerAction>,
) -> ApiResult<impl IntoResponse> {
    let thread = state.core.store.thread(&id)?;
    let result = match &thread.active_run {
        Some(run) => {
            let scope = orbyn_core::governance::approvals::RunScope {
                run_id: run.id.clone(),
                thread_id: thread.id.clone(),
                engine: thread.engine.clone(),
                family: state.core.engines.driver(&thread.engine)?.family(),
                write_scope: None,
                mode: orbyn_contracts::PermissionMode::Supervised,
                plan: false,
            };
            state.core.computer.act(Caller::Run(&scope), action).await?
        }
        None => {
            let caller = Caller::Direct {
                thread_id: &thread.id,
                actor: LOCAL_USER,
            };
            state.core.computer.act(caller, action).await?
        }
    };
    Ok(Json(result))
}
