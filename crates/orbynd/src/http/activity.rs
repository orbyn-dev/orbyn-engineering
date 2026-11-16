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
    // A write to a project, thread, task, queue, clone or the workspace may move a branch or
    // a worktree, so the cached workspace status is dropped, whether or not the request
    // succeeded and without telling the windows: the one that wrote reads again, and a
    // change to the library itself announces itself. Settings, engines, usage and the like
    // cannot move a checkout.
    let changes =
        request.method() != axum::http::Method::GET && moves_checkouts(request.uri().path());
    let response = next.run(request).await;
    if changes {
        state.core.workspace.invalidate_status();
        state.core.explorer.invalidate();
    }
    timing.finish(response.status().as_u16() < 400);
    response
}

/// Whether a write to this path can move a branch or a worktree.
fn moves_checkouts(path: &str) -> bool {
    const PREFIXES: [&str; 11] = [
        "/api/projects",
        "/api/threads",
        "/api/workspace",
        "/api/watches",
        "/api/tasks",
        "/api/queue",
        "/api/clones",
        "/api/runs",
        "/api/approvals",
        "/api/history",
        "/api/storage",
    ];
    PREFIXES.iter().any(|prefix| {
        path.strip_prefix(prefix)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    })
}

#[cfg(test)]
mod tests {
    use super::moves_checkouts;

    #[test]
    fn only_writes_that_can_move_a_checkout_drop_the_workspace_status() {
        assert!(moves_checkouts("/api/threads/t1/git/action"));
        assert!(moves_checkouts("/api/projects/p1/pulls/close"));
        assert!(moves_checkouts("/api/workspace/setup"));
        assert!(!moves_checkouts("/api/settings/preferences"));
        assert!(!moves_checkouts("/api/engines/claude/update"));
        assert!(!moves_checkouts("/api/usage/refresh"));
        assert!(!moves_checkouts("/api/threadsx"));
    }
}
