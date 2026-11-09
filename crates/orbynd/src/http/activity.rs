//! Middleware that notes a person is here and keeps cached status fresh.

use axum::extract::{Request, State};
use axum::response::Response;

use crate::http::AppState;

/// A request that changes something means a person is here, which puts off an
/// automatic update. Reads don't count: a window left open polls.
pub async fn note_activity(
    State(state): State<AppState>,
    request: Request,
    next: axum::middleware::Next,
) -> Response {
    let route = request
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .map_or("operation", axum::extract::MatchedPath::as_str);
    let timing = state
        .core
        .timings
        .span(&format!("{} {route}", request.method()));
    if request.method() != axum::http::Method::GET
        && !matches!(
            request.uri().path(),
            "/api/settings/project-scopes" | "/api/settings/preferences/query"
        )
    {
        state.core.updates.note_activity();
    }
    // Anything that changes a project, thread, pull request or the workspace may move a
    // branch or a worktree, so the cached workspace status is dropped whether or not
    // the request succeeded. Reads and the settings queries never do.
    let changes = request.method() != axum::http::Method::GET
        && request.uri().path().starts_with("/api/")
        && !matches!(
            request.uri().path(),
            "/api/settings/project-scopes" | "/api/settings/preferences/query"
        );
    let response = next.run(request).await;
    if changes {
        state.core.workspace.invalidate_status();
    }
    timing.finish(response.status().as_u16() < 400);
    response
}
