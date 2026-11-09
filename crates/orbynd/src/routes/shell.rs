//! The desktop shell: its status and the actions it can be asked to take.

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use orbyn_core::CoreError;

use crate::http::{ApiJson, ApiResult, AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/shell", get(shell_status))
        .route("/api/shell/actions", post(shell_action))
}

async fn shell_status(State(state): State<AppState>) -> Json<orbyn_contracts::ShellStatus> {
    Json(state.core.shell.status())
}

async fn shell_action(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<orbyn_contracts::ShellRequest>,
) -> ApiResult<StatusCode> {
    // A remote viewer must not stop, restart or look into the desktop app and its runs.
    if state.remote
        && matches!(
            body.action,
            orbyn_contracts::ShellAction::Quit
                | orbyn_contracts::ShellAction::RestartService
                | orbyn_contracts::ShellAction::ShowLog
        )
    {
        return Err(CoreError::Invalid("Do this from the computer the app runs on.".into()).into());
    }
    state.core.shell.request(body.action)?;
    Ok(StatusCode::ACCEPTED)
}
