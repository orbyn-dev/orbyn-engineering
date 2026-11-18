//! The class of every protected route: `Local` is opened by the service token alone, `Person` is a route where acting as
//! the person matters (design: `docs/worker-token-isolation.md`, #441). A `Person` route listed in [`ENFORCED`] also needs
//! this launch's person key (`x-orbyn-person`), which only the desktop shell holds. Browser mode and the CLI cannot use an
//! enforced route (owner ruling): the answer is `403 desktop_required`. Since step 3 every `Person` route is enforced, and a
//! test fails for a `Person` route that is not.
//!
//! The rule for `Person`: the route decides or grants something only the person may (an approval, a trust or policy
//! grant, redaction), acts on the person's files or shell, replaces the app, or writes account or Crew state behind
//! `local_only`. Reads, an engine's own thread work and settings that grant nothing stay `Local`.
//!
//! Left `Local` on purpose: `POST /api/threads/{id}/git/action` (push). Two routes are `Local` as a whole but check the key
//! in their handler for part of the request, listed in [`FIELD_LEVEL`]: `POST /api/threads/{id}/runs` when its body raises
//! `permission_mode` above the thread's current mode, and `POST /agui/threads/{id}` when its `resume[]` is non-empty (it
//! answers an interrupt, as `decide` does). `/agui/threads/{id}` reads only `threadId`, `runId` and `resume`; `queue` and
//! `steer` take model, effort and options but no `permission_mode`. `PATCH /api/threads/{id}` also takes it and is `Person`
//! as a whole.
//!
//! `Person` routes are listed here with a reason each. Every other protected route is `"local"` in `tests/routes.json`,
//! one line each, so a new route must be classified in a diff: the route-table test fails for a route with no class.
//!
//! The terminal sockets are `GET` routes a browser opens with `new WebSocket`, which cannot set a header, and the key must
//! not be in the URL. The page offers it as the subprotocol `orbyn.person.<key>` beside `orbyn.v1` (the only one the upgrade
//! answers); [`require_person`] reads it on an upgrade request only (`http/access.rs`).
//!
//! To add a `Person` route: one row in [`PERSON`], one line in [`ENFORCED`], one in the web client's list
//! (`web/src/api/personKey.ts`), and a row in `tests/routes.json`. Tests fail for an entry that is not a `Person` route or not
//! a registered route, and for a `Person` route that is not enforced.

use axum::body::Body;
use axum::extract::{MatchedPath, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use std::sync::Arc;

use crate::http::ApiFailure;
use crate::http::access::Access;

/// The `Person` routes that need the person key. Each must also be in [`PERSON`], and every `PERSON` route is here.
pub const ENFORCED: &[(&str, &str)] = &[
    // Account (behind its own local_only layer).
    ("POST", "/api/account/approvals/{id}/approve"),
    ("POST", "/api/account/approvals/{id}/deny"),
    ("POST", "/api/account/confirm"),
    ("POST", "/api/account/confirm/cancel"),
    ("POST", "/api/account/connect"),
    ("POST", "/api/account/connect/later"),
    ("POST", "/api/account/connect/seen"),
    ("POST", "/api/account/connect/use-account"),
    ("POST", "/api/account/connect/use-device"),
    ("DELETE", "/api/account/devices/{id}"),
    ("POST", "/api/account/ended/seen"),
    ("POST", "/api/account/sign-in"),
    ("POST", "/api/account/sign-in/approval"),
    ("POST", "/api/account/sign-in/cancel"),
    ("POST", "/api/account/sign-in/digits"),
    ("POST", "/api/account/sign-in/second-factor"),
    ("POST", "/api/account/sign-out"),
    ("POST", "/api/account/sign-out-everywhere"),
    ("POST", "/api/account/sign-up/code"),
    ("POST", "/api/account/sign-up/invite"),
    ("POST", "/api/account/sign-up/resend"),
    ("POST", "/api/account/sign-up/start"),
    ("POST", "/api/account/sync/now"),
    ("POST", "/api/account/sync/pause"),
    ("POST", "/api/account/sync/resume"),
    // Crew person writes (behind local_only).
    ("POST", "/api/crew/approvals/second-engine/{id}/dismiss"),
    ("POST", "/api/crew/approvals/{id}/approve-once"),
    ("POST", "/api/crew/approvals/{id}/deny"),
    ("POST", "/api/crew/approvals/{id}/dismiss"),
    ("POST", "/api/crew/autopilot/areas/follow"),
    ("POST", "/api/crew/autopilot/areas/override"),
    ("POST", "/api/crew/autopilot/confirm"),
    ("PUT", "/api/crew/autopilot/mode"),
    ("POST", "/api/crew/autopilot/review/{task_id}/comment"),
    ("POST", "/api/crew/autopilot/review/{task_id}/looks-good"),
    ("POST", "/api/crew/autopilot/review/{task_id}/revert"),
    ("POST", "/api/crew/background/create"),
    ("POST", "/api/crew/background/{id}/delete"),
    ("POST", "/api/crew/background/{id}/disable"),
    ("POST", "/api/crew/background/{id}/enable"),
    ("POST", "/api/crew/background/{id}/update"),
    ("POST", "/api/crew/guard/targets/add"),
    ("POST", "/api/crew/guard/targets/{id}/remove"),
    ("POST", "/api/crew/projects/{id}/pinned-ref/confirm"),
    ("POST", "/api/crew/projects/{id}/pinned-ref/reset"),
    ("POST", "/api/crew/review/{task_id}/consent"),
    ("POST", "/api/crew/review/{task_id}/remove-lane"),
    ("POST", "/api/crew/settings/engine-tiers/save"),
    ("POST", "/api/crew/tasks/{id}/cancel"),
    ("POST", "/api/crew/tasks/{id}/merge"),
    ("POST", "/api/crew/tasks/{id}/rerun-checks"),
    // Engines, updates, workspace and the shell.
    ("POST", "/api/acp/registry/{id}"),
    ("POST", "/api/engines/custom"),
    ("POST", "/api/engines/probe-binary"),
    ("POST", "/api/engines/update-all"),
    ("POST", "/api/engines/{id}/install-recommended"),
    ("POST", "/api/engines/{id}/update"),
    ("POST", "/api/shell/actions"),
    ("POST", "/api/updates/app/apply"),
    ("POST", "/api/updates/apply-all"),
    ("POST", "/api/workspace/rollback"),
    ("POST", "/api/workspace/setup"),
    ("POST", "/api/workspace/update"),
    // Preferences, policy and redaction.
    ("POST", "/api/policy/rules"),
    ("DELETE", "/api/policy/rules/{id}"),
    ("PUT", "/api/preferences"),
    ("PUT", "/api/projects/{id}/preferences"),
    ("PUT", "/api/settings/preferences"),
    ("PUT", "/api/settings/secrets/formats/{id}"),
    ("POST", "/api/settings/secrets/patterns"),
    ("DELETE", "/api/settings/secrets/patterns/{id}"),
    ("PUT", "/api/settings/secrets/patterns/{id}"),
    ("PUT", "/api/settings/secrets/projects/{id}"),
    ("PUT", "/api/settings/secrets/surfaces"),
    // Projects, approvals and questions.
    ("POST", "/api/approvals/{id}/decide"),
    ("POST", "/api/projects/{id}/publish"),
    ("POST", "/api/projects/{id}/pulls/{number}/merge"),
    ("POST", "/api/projects/{id}/pulls/{number}/review"),
    ("POST", "/api/projects/{id}/trust"),
    ("POST", "/api/projects/{id}/workflows/{run}/approve"),
    (
        "POST",
        "/api/projects/{id}/worktrees/{worktree}/file-actions",
    ),
    ("POST", "/api/questions/{id}/answer"),
    ("POST", "/api/questions/{id}/dismiss"),
    // Scheduled tasks.
    ("POST", "/api/tasks"),
    ("PUT", "/api/tasks/{id}"),
    ("POST", "/api/tasks/{id}/enabled"),
    // Terminal sockets: the key travels as a subprotocol, never in the URL.
    ("GET", "/ws/terminal/{thread_id}"),
    ("GET", "/ws/terminal/{thread_id}/{terminal}"),
    // Threads: console, terminals, scripts, files, computer use, pull requests, permission mode.
    ("PATCH", "/api/threads/{id}"),
    ("POST", "/api/threads/{id}/computer"),
    ("POST", "/api/threads/{id}/console"),
    ("PUT", "/api/threads/{id}/files/content"),
    ("POST", "/api/threads/{id}/pulls/{number}/merge"),
    ("POST", "/api/threads/{id}/pulls/{number}/review"),
    ("POST", "/api/threads/{id}/scripts"),
    ("POST", "/api/threads/{id}/terminals"),
];

/// Routes that stay `Local` as a whole but check the key in their handler for part of the request body (docs/worker-token-isolation.md,
/// "field-level"): `(method, path, field)`. A test fails for an entry that is not a `local` row of `tests/routes.json`, and
/// the web client sends the key on these routes too (`web/src/api/personKey.ts`).
#[cfg(test)]
pub const FIELD_LEVEL: &[(&str, &str, &str)] = &[
    (
        "POST",
        "/api/threads/{id}/runs",
        "permission_mode above the thread's current mode (routes/runs.rs)",
    ),
    (
        "POST",
        "/agui/threads/{id}",
        "a non-empty resume[], which answers an interrupt (routes/stream.rs)",
    ),
];

/// Whether a request for this method and matched route pattern needs the person key.
fn needs_key(method: &str, pattern: &str) -> bool {
    ENFORCED.iter().any(|(m, p)| (*m, *p) == (method, pattern))
        && PERSON.iter().any(|(m, p, _)| (*m, *p) == (method, pattern))
}

/// Refuses an enforced `Person` route without this launch's person key, with 403. Runs after `require_auth`, so a
/// request with no credential still answers 401, and inside the route, so the matched pattern is known.
pub async fn require_person(
    State(access): State<Arc<Access>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, ApiFailure> {
    let enforced = request
        .extensions()
        .get::<MatchedPath>()
        .is_some_and(|path| needs_key(request.method().as_str(), path.as_str()));
    if enforced {
        access.person_check(request.headers())?;
    }
    Ok(next.run(request).await)
}

/// `(method, path, why)` for each `Person` route.
pub const PERSON: &[(&str, &str, &str)] = &[
    (
        "POST",
        "/api/account/approvals/{id}/approve",
        "the person's account identity, sign-in, devices or sync, behind the account's own local_only layer",
    ),
    (
        "POST",
        "/api/account/approvals/{id}/deny",
        "the person's account identity, sign-in, devices or sync, behind the account's own local_only layer",
    ),
    (
        "POST",
        "/api/account/confirm",
        "the person's account identity, sign-in, devices or sync, behind the account's own local_only layer",
    ),
    (
        "POST",
        "/api/account/confirm/cancel",
        "the person's account identity, sign-in, devices or sync, behind the account's own local_only layer",
    ),
    (
        "POST",
        "/api/account/connect",
        "the person's account identity, sign-in, devices or sync, behind the account's own local_only layer",
    ),
    (
        "POST",
        "/api/account/connect/later",
        "the person's account identity, sign-in, devices or sync, behind the account's own local_only layer",
    ),
    (
        "POST",
        "/api/account/connect/seen",
        "the person's account identity, sign-in, devices or sync, behind the account's own local_only layer",
    ),
    (
        "POST",
        "/api/account/connect/use-account",
        "the person's account identity, sign-in, devices or sync, behind the account's own local_only layer",
    ),
    (
        "POST",
        "/api/account/connect/use-device",
        "the person's account identity, sign-in, devices or sync, behind the account's own local_only layer",
    ),
    (
        "DELETE",
        "/api/account/devices/{id}",
        "the person's account identity, sign-in, devices or sync, behind the account's own local_only layer",
    ),
    (
        "POST",
        "/api/account/ended/seen",
        "the person's account identity, sign-in, devices or sync, behind the account's own local_only layer",
    ),
    (
        "POST",
        "/api/account/sign-in",
        "the person's account identity, sign-in, devices or sync, behind the account's own local_only layer",
    ),
    (
        "POST",
        "/api/account/sign-in/approval",
        "the person's account identity, sign-in, devices or sync, behind the account's own local_only layer",
    ),
    (
        "POST",
        "/api/account/sign-in/cancel",
        "the person's account identity, sign-in, devices or sync, behind the account's own local_only layer",
    ),
    (
        "POST",
        "/api/account/sign-in/digits",
        "the person's account identity, sign-in, devices or sync, behind the account's own local_only layer",
    ),
    (
        "POST",
        "/api/account/sign-in/second-factor",
        "the person's account identity, sign-in, devices or sync, behind the account's own local_only layer",
    ),
    (
        "POST",
        "/api/account/sign-out",
        "the person's account identity, sign-in, devices or sync, behind the account's own local_only layer",
    ),
    (
        "POST",
        "/api/account/sign-out-everywhere",
        "the person's account identity, sign-in, devices or sync, behind the account's own local_only layer",
    ),
    (
        "POST",
        "/api/account/sign-up/code",
        "the person's account identity, sign-in, devices or sync, behind the account's own local_only layer",
    ),
    (
        "POST",
        "/api/account/sign-up/invite",
        "the person's account identity, sign-in, devices or sync, behind the account's own local_only layer",
    ),
    (
        "POST",
        "/api/account/sign-up/resend",
        "the person's account identity, sign-in, devices or sync, behind the account's own local_only layer",
    ),
    (
        "POST",
        "/api/account/sign-up/start",
        "the person's account identity, sign-in, devices or sync, behind the account's own local_only layer",
    ),
    (
        "POST",
        "/api/account/sync/now",
        "the person's account identity, sign-in, devices or sync, behind the account's own local_only layer",
    ),
    (
        "POST",
        "/api/account/sync/pause",
        "the person's account identity, sign-in, devices or sync, behind the account's own local_only layer",
    ),
    (
        "POST",
        "/api/account/sync/resume",
        "the person's account identity, sign-in, devices or sync, behind the account's own local_only layer",
    ),
    (
        "POST",
        "/api/acp/registry/{id}",
        "installs an agent from the registry",
    ),
    (
        "POST",
        "/api/approvals/{id}/decide",
        "the person's yes or no on an engine's request; a worker must not approve itself (owner ruling)",
    ),
    (
        "POST",
        "/api/crew/approvals/second-engine/{id}/dismiss",
        "Crew write behind local_only: dismisses a second-engine notice",
    ),
    (
        "POST",
        "/api/crew/approvals/{id}/approve-once",
        "Crew write behind local_only: approve-once for a held command; also needs the desktop presence nonce",
    ),
    (
        "POST",
        "/api/crew/approvals/{id}/deny",
        "Crew write behind local_only: the person denies a held command",
    ),
    (
        "POST",
        "/api/crew/approvals/{id}/dismiss",
        "Crew write behind local_only: the person dismisses a held command",
    ),
    (
        "POST",
        "/api/crew/autopilot/areas/follow",
        "Crew write behind local_only: returns an area to the mode's default",
    ),
    (
        "POST",
        "/api/crew/autopilot/areas/override",
        "Crew write behind local_only: saves a per-area override of Autopilot's autonomy",
    ),
    (
        "POST",
        "/api/crew/autopilot/confirm",
        "Crew write behind local_only: the person's consent for an Autopilot mode",
    ),
    (
        "PUT",
        "/api/crew/autopilot/mode",
        "Crew write behind local_only: sets how far Autopilot may act",
    ),
    (
        "POST",
        "/api/crew/autopilot/review/{task_id}/comment",
        "Crew write behind local_only: the person's review comment on a finished task",
    ),
    (
        "POST",
        "/api/crew/autopilot/review/{task_id}/looks-good",
        "Crew write behind local_only: the person's sign-off on a reviewed task",
    ),
    (
        "POST",
        "/api/crew/autopilot/review/{task_id}/revert",
        "Crew write behind local_only: the person reverts a task's change",
    ),
    (
        "POST",
        "/api/crew/background/create",
        "Crew write behind local_only: creates a background worker",
    ),
    (
        "POST",
        "/api/crew/background/{id}/delete",
        "Crew write behind local_only: deletes a background worker",
    ),
    (
        "POST",
        "/api/crew/background/{id}/disable",
        "Crew write behind local_only: disables a background worker",
    ),
    (
        "POST",
        "/api/crew/background/{id}/enable",
        "Crew write behind local_only: enables a background worker",
    ),
    (
        "POST",
        "/api/crew/background/{id}/update",
        "Crew write behind local_only: edits a background worker",
    ),
    (
        "POST",
        "/api/crew/guard/targets/add",
        "Crew write behind local_only: adds a protected target to the guard",
    ),
    (
        "POST",
        "/api/crew/guard/targets/{id}/remove",
        "Crew write behind local_only: removes a protected target (lane) from the guard",
    ),
    (
        "POST",
        "/api/crew/projects/{id}/pinned-ref/confirm",
        "Crew write behind local_only: the person confirms the project's default-branch commit that class rules are read from",
    ),
    (
        "POST",
        "/api/crew/projects/{id}/pinned-ref/reset",
        "Crew write behind local_only: the person clears the project's remembered default-branch commit",
    ),
    (
        "POST",
        "/api/crew/review/{task_id}/consent",
        "Crew write behind local_only: the person consents once or waits for a same-engine reviewer",
    ),
    (
        "POST",
        "/api/crew/review/{task_id}/remove-lane",
        "Crew write behind local_only: the person removes an unlocked review lane with a reason",
    ),
    (
        "POST",
        "/api/crew/settings/engine-tiers/save",
        "Crew write behind local_only: the person assigns ACP engines to tiers",
    ),
    (
        "POST",
        "/api/crew/tasks/{id}/cancel",
        "Crew write behind local_only: the person stops a task",
    ),
    (
        "POST",
        "/api/crew/tasks/{id}/merge",
        "Crew write behind local_only: merge release: the person's release of a task's change",
    ),
    (
        "POST",
        "/api/crew/tasks/{id}/rerun-checks",
        "Crew write behind local_only: the person's re-run of a task's checks",
    ),
    (
        "POST",
        "/api/engines/custom",
        "registers a program to run as an engine",
    ),
    (
        "POST",
        "/api/engines/probe-binary",
        "runs a program the caller names",
    ),
    (
        "POST",
        "/api/engines/update-all",
        "updates all engine software",
    ),
    (
        "POST",
        "/api/engines/{id}/install-recommended",
        "installs engine software",
    ),
    (
        "POST",
        "/api/engines/{id}/update",
        "updates engine software",
    ),
    (
        "POST",
        "/api/policy/rules",
        "adds a rule that can auto-allow an engine's tool calls",
    ),
    (
        "DELETE",
        "/api/policy/rules/{id}",
        "removes a rule the person set; changes what is allowed or asked",
    ),
    (
        "PUT",
        "/api/preferences",
        "changes preferences that can set permission defaults",
    ),
    (
        "PUT",
        "/api/projects/{id}/preferences",
        "changes a project's preferences",
    ),
    (
        "POST",
        "/api/projects/{id}/publish",
        "publishes the project to the forge",
    ),
    (
        "POST",
        "/api/projects/{id}/pulls/{number}/merge",
        "merges a pull request as the person",
    ),
    (
        "POST",
        "/api/projects/{id}/pulls/{number}/review",
        "submits a review, which can approve, as the person",
    ),
    (
        "POST",
        "/api/projects/{id}/trust",
        "grants a project's own settings and scripts authority to run",
    ),
    (
        "POST",
        "/api/projects/{id}/workflows/{run}/approve",
        "approves a workflow run on the forge",
    ),
    (
        "POST",
        "/api/projects/{id}/worktrees/{worktree}/file-actions",
        "explorer writes to the person's files",
    ),
    (
        "POST",
        "/api/questions/{id}/answer",
        "the person's answer to an agent's question",
    ),
    (
        "POST",
        "/api/questions/{id}/dismiss",
        "the person's dismissal of an agent's question",
    ),
    (
        "PUT",
        "/api/settings/preferences",
        "changes scoped preferences",
    ),
    (
        "PUT",
        "/api/settings/secrets/formats/{id}",
        "turns a redaction format off or on",
    ),
    (
        "POST",
        "/api/settings/secrets/patterns",
        "adds a redaction pattern",
    ),
    (
        "DELETE",
        "/api/settings/secrets/patterns/{id}",
        "removes a redaction pattern",
    ),
    (
        "PUT",
        "/api/settings/secrets/patterns/{id}",
        "edits a redaction pattern",
    ),
    (
        "PUT",
        "/api/settings/secrets/projects/{id}",
        "lets a project's agents read values unredacted",
    ),
    (
        "PUT",
        "/api/settings/secrets/surfaces",
        "turns redaction off or on for a surface",
    ),
    (
        "POST",
        "/api/shell/actions",
        "runs a native shell action as the person",
    ),
    (
        "POST",
        "/api/tasks",
        "creates a scheduled task that runs unattended",
    ),
    (
        "PUT",
        "/api/tasks/{id}",
        "edits a scheduled task that runs unattended",
    ),
    (
        "POST",
        "/api/tasks/{id}/enabled",
        "turns a scheduled task on or off",
    ),
    (
        "PATCH",
        "/api/threads/{id}",
        "can set the thread's permission_mode",
    ),
    (
        "POST",
        "/api/threads/{id}/computer",
        "computer use on the person's machine",
    ),
    (
        "POST",
        "/api/threads/{id}/console",
        "runs a command as the person in the thread's checkout",
    ),
    (
        "PUT",
        "/api/threads/{id}/files/content",
        "writes a file in the person's checkout",
    ),
    (
        "POST",
        "/api/threads/{id}/pulls/{number}/merge",
        "merges a pull request as the person",
    ),
    (
        "POST",
        "/api/threads/{id}/pulls/{number}/review",
        "submits a review, which can approve, as the person",
    ),
    (
        "POST",
        "/api/threads/{id}/scripts",
        "runs a project script as the person",
    ),
    (
        "POST",
        "/api/threads/{id}/terminals",
        "opens a shell as the person",
    ),
    (
        "POST",
        "/api/updates/app/apply",
        "replaces the installed app",
    ),
    (
        "POST",
        "/api/updates/apply-all",
        "installs app and engine updates",
    ),
    (
        "POST",
        "/api/workspace/rollback",
        "rolls back the skills workspace",
    ),
    (
        "POST",
        "/api/workspace/setup",
        "sets up the skills workspace",
    ),
    (
        "POST",
        "/api/workspace/update",
        "updates the skills workspace the engines read",
    ),
    (
        "GET",
        "/ws/terminal/{thread_id}",
        "opens a shell as the person over the socket",
    ),
    (
        "GET",
        "/ws/terminal/{thread_id}/{terminal}",
        "attaches to a person's shell, which takes keystrokes",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::http::StatusCode;
    use axum::middleware::from_fn_with_state;
    use axum::routing::any;

    /// Every enforced route behind a stub handler that does nothing: the layer refuses a request with no key or a wrong one and
    /// lets the right key through, for routes that must never be called for real in a test (updates, workspace, engines).
    #[tokio::test]
    async fn the_layer_guards_every_enforced_route() {
        let access =
            Arc::new(Access::new("secret", Vec::new(), None).with_person_key(Some("the-key")));
        let mut router = Router::new();
        for (_, path) in ENFORCED {
            router = router.route(path, any(|| async { StatusCode::NO_CONTENT }));
        }
        let router = router.route_layer(from_fn_with_state(access, require_person));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, router).await });
        let client = reqwest::Client::new();
        for (method, path) in ENFORCED {
            let uri = format!("{base}{}", path.replace(['{', '}'], ""));
            for (key, want) in [(None, 403), (Some("wrong"), 403), (Some("the-key"), 204)] {
                let mut request = client.request(
                    reqwest::Method::from_bytes(method.as_bytes()).unwrap(),
                    &uri,
                );
                if let Some(key) = key {
                    request = request.header("x-orbyn-person", key);
                }
                let status = request.send().await.unwrap().status().as_u16();
                assert_eq!(status, want, "{method} {path} with {key:?}");
            }
        }
    }
}
