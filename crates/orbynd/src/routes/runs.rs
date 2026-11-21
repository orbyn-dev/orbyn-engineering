//! Runs: start, cancel and force-stop a run, and a thread's workflows.

use axum::Json;
use axum::Router;
use axum::extract::{MatchedPath, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use orbyn_contracts::ModelSelection;
use orbyn_contracts::{PermissionMode, StartRun, UpdateThread, WorkflowStop};
use orbyn_core::agents::selection;

use crate::http::{ApiJson, ApiResult, AppState, LOCAL_USER};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/threads/{id}/runs", post(start_run))
        .route("/api/runs/{id}/cancel", post(cancel_run))
        .route("/api/threads/{id}/force-stop", post(force_stop))
        .route(
            "/api/threads/{id}/workflows/{item}/script",
            get(workflow_script),
        )
        .route(
            "/api/threads/{id}/workflows/{item}/stop",
            post(stop_workflow),
        )
}

async fn start_run(
    State(state): State<AppState>,
    matched: MatchedPath,
    Path(id): Path<String>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<StartRun>,
) -> ApiResult<impl IntoResponse> {
    // A run may ask for a looser permission mode. Raising it grants an engine more, so it needs the person key (#441,
    // docs/worker-token-isolation.md); keeping or lowering the mode stays `Local`.
    let needs_key = |wanted: PermissionMode| -> ApiResult<()> {
        let current = state
            .core
            .store
            .thread(&id)?
            .lifecycle
            .map_or(PermissionMode::Supervised, |lifecycle| {
                lifecycle.permission_mode
            });
        if wanted.looser_than(current) {
            state.access.person_check_field(
                &headers,
                "POST",
                matched.as_str(),
                "permission_mode",
            )?;
        }
        Ok(())
    };
    if let Some(wanted) = body.permission_mode {
        needs_key(wanted)?;
    }
    let wanted = ModelSelection {
        model: body.model.clone(),
        effort: body.effort.clone(),
        options: body.options.clone(),
    };
    state.core.check_turn(&id, &wanted).await?;
    if body.permission_mode.is_some() || !selection::is_empty(&wanted) {
        // `check_turn` awaited, so a person may have lowered the mode meanwhile: check again with no await before the write,
        // or a token-only request that sent the old, equal mode would restore the looser one.
        if let Some(mode) = body.permission_mode {
            needs_key(mode)?;
        }
        let patch = UpdateThread {
            permission_mode: body.permission_mode,
            model: wanted.model,
            effort: wanted.effort,
            options: Some(wanted.options),
            ..UpdateThread::default()
        };
        state.core.update_thread(&id, &patch, LOCAL_USER)?;
    }
    let run = state
        .core
        .runs
        .admit_with(&id, &body.prompt, &body.attachment_ids, LOCAL_USER)?;
    Ok((StatusCode::ACCEPTED, Json(run)))
}

async fn cancel_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.core.runs.cancel(&id)?;
    Ok(StatusCode::ACCEPTED)
}

async fn force_stop(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.runs.force_stop(&id, LOCAL_USER).await?))
}

async fn workflow_script(
    State(state): State<AppState>,
    Path((id, item)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.workflow_script(&id, &item)?))
}

async fn stop_workflow(
    State(state): State<AppState>,
    Path((id, item)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    state.core.runs.stop_workflow(&id, &item, LOCAL_USER)?;
    Ok((StatusCode::ACCEPTED, Json(WorkflowStop { requested: true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_looser_mode_counts_as_raising_it() {
        use PermissionMode::*;
        assert!(AutoEdits.looser_than(Supervised));
        assert!(FullAccess.looser_than(Auto));
        assert!(FullAccess.looser_than(Supervised));
        assert!(!Auto.looser_than(Auto));
        assert!(!Supervised.looser_than(FullAccess));
        assert!(!AutoEdits.looser_than(Auto));
    }
}
