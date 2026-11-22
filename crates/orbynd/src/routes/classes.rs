//! The class of every protected route: `Local` is opened by the service token alone, `Person` is a route where acting as
//! the person matters (design: `docs/worker-token-isolation.md`, #441). A `Person` route listed in [`ENFORCED`] also needs
//! this launch's person key (`x-orbyn-person`), which only the desktop shell holds. Browser mode and the CLI cannot use an
//! enforced route (owner ruling): the answer is `403 desktop_required`. Since step 3 every `Person` route is enforced, and a
//! test fails for a `Person` route that is not.
//!
//! The rule for `Person`: the route decides or grants something only the person may (an approval, a trust or policy
//! grant, redaction), acts on the person's files or shell, moves a checkout (initialises git, creates or switches a branch or
//! ref: ruling F3, so a service-token agent cannot move the checkout under the person), replaces the app, or writes account or Crew state behind
//! `local_only`. Reads, an engine's own thread work and settings that grant nothing stay `Local`. The exception is a read that returns a secret itself: `GET /api/settings/secrets/patterns/{id}` returns a
//! pattern's text, which can be the secret spelled out, so a service-token agent must not read it.
//!
//! Left `Local` on purpose: `POST /api/threads/{id}/git/action` (push). Two routes are `Local` as a whole but check the key
//! in their handler for part of the request, listed in [`FIELD_LEVEL`]: `POST /api/projects/{id}/pulls/{number}/checkout` when its
//! body has `worktree: false` (a switch in the project's checkout), `POST /api/threads/{id}/runs` when its body raises
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
    ("POST", "/api/crew/backtest/start"),
    ("POST", "/api/crew/backtest/{id}/stop"),
    ("POST", "/api/crew/capacity/settings"),
    ("POST", "/api/crew/capacity/tasks/{id}"),
    ("POST", "/api/crew/approvals/second-engine/{id}/dismiss"),
    ("POST", "/api/crew/approvals/{id}/approve-once"),
    ("POST", "/api/crew/approvals/{id}/deny"),
    ("POST", "/api/crew/approvals/{id}/dismiss"),
    ("POST", "/api/crew/autopilot/areas/follow"),
    ("POST", "/api/crew/autopilot/areas/override"),
    ("POST", "/api/crew/autopilot/confirm"),
    ("PUT", "/api/crew/autopilot/mode"),
    ("POST", "/api/crew/autopilot/digest/schedule/set"),
    ("POST", "/api/crew/autopilot/merge-queue/{task_id}/resolve"),
    ("POST", "/api/crew/autopilot/review/{task_id}/comment"),
    ("POST", "/api/crew/autopilot/review/{task_id}/looks-good"),
    ("POST", "/api/crew/autopilot/review/{task_id}/revert"),
    ("POST", "/api/crew/background/create"),
    ("POST", "/api/crew/background/{id}/delete"),
    ("POST", "/api/crew/background/{id}/disable"),
    ("POST", "/api/crew/background/{id}/enable"),
    ("POST", "/api/crew/background/{id}/run-now"),
    ("POST", "/api/crew/background/{id}/update"),
    ("POST", "/api/crew/guard/targets/add"),
    ("POST", "/api/crew/guard/targets/{id}/remove"),
    ("POST", "/api/crew/escapes/mark"),
    ("POST", "/api/crew/escapes/{id}/confirm"),
    ("POST", "/api/crew/escapes/{id}/dismiss"),
    ("POST", "/api/crew/escapes/{id}/gap"),
    ("POST", "/api/crew/escapes/{id}/check"),
    ("POST", "/api/crew/escapes/{id}/check/accept"),
    ("POST", "/api/crew/escapes/{id}/check/retry"),
    ("POST", "/api/crew/escapes/{id}/check/withdraw"),
    ("POST", "/api/crew/escapes/{id}/check/approve"),
    ("POST", "/api/crew/projects/{id}/pinned-ref/confirm"),
    ("POST", "/api/crew/projects/{id}/pinned-ref/reset"),
    ("POST", "/api/crew/projects/{id}/worker-provider/set"),
    ("POST", "/api/crew/review/{task_id}/consent"),
    ("POST", "/api/crew/review/{task_id}/remove-lane"),
    ("POST", "/api/crew/settings/engine-tiers/save"),
    ("POST", "/api/crew/tasks/{id}/cancel"),
    ("POST", "/api/crew/tasks/{id}/merge"),
    ("POST", "/api/crew/tasks/{id}/rerun-checks"),
    ("POST", "/api/crew/tasks/{id}/checks/{check}/decide"),
    ("POST", "/api/crew/tasks/{id}/retry-elsewhere"),
    // Engines, updates, workspace and the shell.
    ("POST", "/api/acp/registry/{id}"),
    ("POST", "/api/engine-host/{family}/restart"),
    ("POST", "/api/engine-host/{family}/stop"),
    ("POST", "/api/engines/custom"),
    ("POST", "/api/engines/probe-binary"),
    ("POST", "/api/engines/update-all"),
    ("POST", "/api/engines/{id}/install-recommended"),
    ("POST", "/api/engines/{id}/update"),
    (
        "POST",
        "/api/projects/{project_id}/engines/{engine}/notice/acknowledge",
    ),
    ("POST", "/api/threads/{id}/send-without-approvals"),
    ("POST", "/api/shell/actions"),
    ("POST", "/api/updates/app/apply"),
    ("POST", "/api/updates/app/download"),
    ("POST", "/api/updates/apply-all"),
    ("POST", "/api/workspace/rollback"),
    ("POST", "/api/workspace/setup"),
    ("POST", "/api/workspace/update"),
    // Preferences, policy and redaction.
    ("POST", "/api/policy/rules"),
    ("DELETE", "/api/policy/rules/{id}"),
    ("PUT", "/api/preferences"),
    ("PUT", "/api/projects/{id}/preferences"),
    ("PUT", "/api/settings/engine-credentials/{engine}"),
    ("PUT", "/api/settings/engine-credentials/{engine}/key"),
    ("POST", "/api/settings/engine-credentials/{engine}/retry"),
    ("DELETE", "/api/settings/engine-credentials/{engine}/key"),
    ("PUT", "/api/settings/preferences"),
    ("PUT", "/api/settings/secrets/formats/{id}"),
    ("POST", "/api/settings/secrets/patterns"),
    ("GET", "/api/settings/secrets/patterns/{id}"),
    ("DELETE", "/api/settings/secrets/patterns/{id}"),
    ("PUT", "/api/settings/secrets/patterns/{id}"),
    ("PUT", "/api/settings/secrets/projects/{id}"),
    ("PUT", "/api/settings/secrets/surfaces"),
    // Projects, approvals and questions.
    ("POST", "/api/approvals/{id}/decide"),
    ("POST", "/api/projects/{id}/git/init"),
    ("POST", "/api/projects/{id}/publish"),
    ("POST", "/api/projects/{id}/pulls/close"),
    ("PATCH", "/api/projects/{id}/pulls/{number}"),
    ("POST", "/api/projects/{id}/pulls/{number}/action"),
    ("POST", "/api/projects/{id}/pulls/{number}/comments"),
    (
        "PATCH",
        "/api/projects/{id}/pulls/{number}/comments/{comment}",
    ),
    (
        "POST",
        "/api/projects/{id}/pulls/{number}/comments/{comment}/react",
    ),
    ("POST", "/api/projects/{id}/pulls/{number}/labels"),
    ("POST", "/api/projects/{id}/pulls/{number}/reviewers"),
    ("POST", "/api/projects/{id}/pulls/{number}/stack"),
    (
        "POST",
        "/api/projects/{id}/pulls/{number}/threads/{thread}/resolve",
    ),
    ("POST", "/api/projects/{id}/pulls/{number}/merge"),
    ("POST", "/api/projects/{id}/pulls/{number}/review"),
    ("POST", "/api/projects/{id}/trust"),
    ("POST", "/api/projects/{id}/workflows/{run}/approve"),
    ("DELETE", "/api/projects/{id}/worktrees/{worktree}"),
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
    ("POST", "/api/threads/{id}/branches"),
    ("POST", "/api/threads/{id}/branches/switch"),
    ("POST", "/api/threads/{id}/computer"),
    ("POST", "/api/threads/{id}/console"),
    ("PUT", "/api/threads/{id}/files/content"),
    ("POST", "/api/threads/{id}/files/mutate"),
    ("POST", "/api/threads/{id}/pulls/{number}/merge"),
    ("POST", "/api/threads/{id}/pulls/{number}/review"),
    ("POST", "/api/threads/{id}/scripts"),
    ("POST", "/api/threads/{id}/terminals"),
    ("POST", "/api/threads/{id}/vcs/refs"),
    ("POST", "/api/threads/{id}/vcs/refs/switch"),
    ("POST", "/api/threads/import"),
    ("POST", "/api/threads/{id}/transcripts"),
    ("PUT", "/api/attachments/{id}/source"),
    ("POST", "/api/issues/trackers/{tracker}/connect"),
    ("POST", "/api/issues/trackers/{tracker}/disconnect"),
    ("POST", "/api/issues/start-thread"),
    ("POST", "/api/threads/{id}/issue-links"),
    ("POST", "/api/threads/{id}/issue-links/remove"),
    // Execution profiles (docs/execution-profiles-and-library.md, Part 1).
    ("POST", "/api/profiles"),
    ("PATCH", "/api/profiles/{id}"),
    ("DELETE", "/api/profiles/{id}"),
    ("PUT", "/api/threads/{id}/profile"),
    ("POST", "/api/crew/profile/apply"),
    ("PUT", "/api/engines/{id}/code-mode"),
    ("POST", "/api/projects/{id}/repos/add-folder"),
    ("POST", "/api/projects/{id}/repos/create"),
    ("POST", "/api/projects/{id}/repos/clone"),
    ("POST", "/api/projects/{id}/repos/scan"),
    ("POST", "/api/projects/{id}/repos/{repo_id}/publish"),
    ("POST", "/api/projects/{id}/repos/{repo_id}/trust"),
    ("POST", "/api/projects/{id}/repos/{repo_id}/setup-git"),
    ("POST", "/api/projects/{id}/repos/{repo_id}/remove"),
    ("POST", "/api/projects/{id}/repos/{repo_id}/clone/cancel"),
    // The personal library (docs/execution-profiles-and-library.md, Part 2).
    ("POST", "/api/library/workflows"),
    ("PATCH", "/api/library/workflows/{id}"),
    ("DELETE", "/api/library/workflows/{id}"),
    ("POST", "/api/library/workflows/{id}/run"),
    ("POST", "/api/library/prompts"),
    ("PATCH", "/api/library/prompts/{id}"),
    ("DELETE", "/api/library/prompts/{id}"),
    ("POST", "/api/library/env-sets"),
    ("PATCH", "/api/library/env-sets/{id}"),
    ("DELETE", "/api/library/env-sets/{id}"),
    ("PUT", "/api/library/env-sets/{id}/keys/{key}"),
    ("DELETE", "/api/library/env-sets/{id}/keys/{key}"),
    ("POST", "/api/library/import"),
    ("POST", "/api/projects/{id}/library/suggestions/copy"),
    ("POST", "/api/projects/{id}/repo-threads"),
    ("POST", "/api/threads/{id}/repos"),
    ("DELETE", "/api/threads/{id}/repos/{repo_id}"),
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
    (
        "POST",
        "/api/projects/{id}/pulls/{number}/checkout",
        "worktree false, which switches the project's own checkout (routes/pulls.rs)",
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
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|path| path.as_str().to_owned())
        .filter(|path| needs_key(request.method().as_str(), path));
    if let Some(route) = route
        && let Err(refusal) = access.person_check(request.headers())
    {
        // The row names the method and the matched pattern, never the URL, a header or a body.
        access.audit_person_refusal(
            request.method().as_str(),
            &route,
            None,
            access.caller_class(request.headers()),
        );
        return Err(refusal);
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
        "/api/crew/capacity/settings",
        "Crew write behind local_only: a project's review limit, sampling and whether new low-priority starts may pause",
    ),
    (
        "POST",
        "/api/crew/capacity/tasks/{id}",
        "Crew write behind local_only: a task's priority and whether Crew began it, which decides if a start can pause",
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
        "/api/crew/autopilot/digest/schedule/set",
        "Crew write behind local_only: the person's daily digest send time",
    ),
    (
        "POST",
        "/api/crew/autopilot/merge-queue/{task_id}/resolve",
        "Crew write behind local_only: the person rejects or retries a forge merge that needs one",
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
        "/api/crew/background/{id}/run-now",
        "Crew write behind local_only: the person starts one run of a background worker",
    ),
    (
        "POST",
        "/api/crew/background/{id}/update",
        "Crew write behind local_only: edits a background worker",
    ),
    (
        "POST",
        "/api/crew/backtest/start",
        "Crew write behind local_only: the person starts a reviewer backtest with a token budget per engine; one at a time",
    ),
    (
        "POST",
        "/api/crew/backtest/{id}/stop",
        "Crew write behind local_only: the person asks a running reviewer backtest to stop",
    ),
    (
        "POST",
        "/api/crew/escapes/mark",
        "Crew write behind local_only: the person marks a merge as an escape, which confirms it at once",
    ),
    (
        "POST",
        "/api/crew/escapes/{id}/confirm",
        "Crew write behind local_only: the person confirms a possible escape; only a person can",
    ),
    (
        "POST",
        "/api/crew/escapes/{id}/dismiss",
        "Crew write behind local_only: the person dismisses a possible escape",
    ),
    (
        "POST",
        "/api/crew/escapes/{id}/gap",
        "Crew write behind local_only: the person sets an escape's test gap or tooling gap",
    ),
    (
        "POST",
        "/api/crew/escapes/{id}/check",
        "Crew write behind local_only: the person proposes a test-only check for a confirmed escape; nothing runs",
    ),
    (
        "POST",
        "/api/crew/escapes/{id}/check/accept",
        "Crew write behind local_only: the person accepts a proposed check; the only escape route that starts a worker",
    ),
    (
        "POST",
        "/api/crew/escapes/{id}/check/retry",
        "Crew write behind local_only: the person sends a failed check back to a proposal; starts nothing",
    ),
    (
        "POST",
        "/api/crew/escapes/{id}/check/withdraw",
        "Crew write behind local_only: the person withdraws a check that is not merged",
    ),
    (
        "POST",
        "/api/crew/escapes/{id}/check/approve",
        "Crew write behind local_only: the person approves the exact change a proof may run on a machine that cannot restrict it",
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
        "/api/crew/projects/{id}/worker-provider/set",
        "Crew write behind local_only: the person chooses whether a project's workers stay on the lead's provider or may go to any allowed one",
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
        "/api/crew/tasks/{id}/checks/{check}/decide",
        "Crew write behind local_only: the person passes or denies a manual check that holds the task",
    ),
    (
        "POST",
        "/api/crew/tasks/{id}/retry-elsewhere",
        "Crew write behind local_only: the person's retry of a task that could not start, on another engine",
    ),
    (
        "POST",
        "/api/engine-host/{family}/restart",
        "restarts an engine host, which ends the runs on it and resets its crash history",
    ),
    (
        "POST",
        "/api/engine-host/{family}/stop",
        "stops an engine host, which ends the runs on it",
    ),
    (
        "POST",
        "/api/projects/{project_id}/engines/{engine}/notice/acknowledge",
        "the person's acknowledgement of an engine's one-time notice, which lets runs without approvals start in the project",
    ),
    (
        "POST",
        "/api/threads/{id}/send-without-approvals",
        "runs one turn on an engine that never asks, without the mode the thread saved",
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
        "/api/projects/{id}/git/init",
        "turns the person's folder into a repository (writes .git in it)",
    ),
    (
        "POST",
        "/api/projects/{id}/publish",
        "publishes the project to the forge",
    ),
    (
        "POST",
        "/api/projects/{id}/pulls/close",
        "closes pull requests on the forge as the person",
    ),
    (
        "PATCH",
        "/api/projects/{id}/pulls/{number}",
        "edits a pull request on the forge as the person",
    ),
    (
        "POST",
        "/api/projects/{id}/pulls/{number}/action",
        "closes, reopens, reverts or deletes the branch of a pull request as the person",
    ),
    (
        "POST",
        "/api/projects/{id}/pulls/{number}/comments",
        "posts a comment on the forge as the person",
    ),
    (
        "PATCH",
        "/api/projects/{id}/pulls/{number}/comments/{comment}",
        "edits the person's comment on the forge",
    ),
    (
        "POST",
        "/api/projects/{id}/pulls/{number}/comments/{comment}/react",
        "reacts on the forge as the person",
    ),
    (
        "POST",
        "/api/projects/{id}/pulls/{number}/labels",
        "changes a pull request's labels on the forge",
    ),
    (
        "POST",
        "/api/projects/{id}/pulls/{number}/reviewers",
        "requests or removes reviewers on the forge as the person",
    ),
    (
        "POST",
        "/api/projects/{id}/pulls/{number}/stack",
        "merges or updates a stack of pull requests as the person",
    ),
    (
        "POST",
        "/api/projects/{id}/pulls/{number}/threads/{thread}/resolve",
        "resolves a review thread on the forge as the person",
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
        "DELETE",
        "/api/projects/{id}/worktrees/{worktree}",
        "removes a worktree and every file in it",
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
        "/api/settings/engine-credentials/{engine}",
        "turns keeping an engine's API key in the keychain off or on",
    ),
    (
        "PUT",
        "/api/settings/engine-credentials/{engine}/key",
        "saves or replaces an engine's API key in the keychain",
    ),
    (
        "POST",
        "/api/settings/engine-credentials/{engine}/retry",
        "asks the keychain again for an engine's API key state",
    ),
    (
        "DELETE",
        "/api/settings/engine-credentials/{engine}/key",
        "removes an engine's API key from the keychain",
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
        "GET",
        "/api/settings/secrets/patterns/{id}",
        "reads a redaction pattern's text, which can be a secret spelled out",
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
        "/api/threads/import",
        "creates a thread from a file the person picked: untrusted content that becomes history in their project",
    ),
    (
        "POST",
        "/api/threads/{id}/transcripts",
        "attaches a file the person picked as untrusted context for the thread's next message",
    ),
    (
        "PUT",
        "/api/attachments/{id}/source",
        "sets the app name, title and window text that go into the next prompt as untrusted text, so a service-token agent cannot rewrite a Snap Shot the person has already reviewed",
    ),
    (
        "POST",
        "/api/threads/{id}/branches",
        "creates a branch and switches the person's checkout to it",
    ),
    (
        "POST",
        "/api/threads/{id}/branches/switch",
        "moves the person's checkout to another branch",
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
        "POST",
        "/api/threads/{id}/files/mutate",
        "creates, renames or deletes files in the person's checkout",
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
        "/api/threads/{id}/vcs/refs",
        "creates a ref in the person's repository",
    ),
    (
        "POST",
        "/api/threads/{id}/vcs/refs/switch",
        "moves the person's checkout to another ref",
    ),
    (
        "POST",
        "/api/updates/app/apply",
        "replaces the installed app",
    ),
    (
        "POST",
        "/api/updates/app/download",
        "consents to an app download and reserves run admission",
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
    (
        "POST",
        "/api/issues/trackers/{tracker}/connect",
        "the person connects an issue tracker: a token goes to the keychain and a host is stored",
    ),
    (
        "POST",
        "/api/issues/trackers/{tracker}/disconnect",
        "the person disconnects an issue tracker: its token and host are removed",
    ),
    (
        "POST",
        "/api/issues/start-thread",
        "the person starts a thread from an issue, which creates and links it and queues a message",
    ),
    (
        "POST",
        "/api/threads/{id}/issue-links",
        "the person links a thread to an issue",
    ),
    (
        "POST",
        "/api/threads/{id}/issue-links/remove",
        "the person removes a thread's link to an issue",
    ),
    (
        "POST",
        "/api/profiles",
        "the person saves an execution profile, which can name full access or Autopilot Full",
    ),
    (
        "PATCH",
        "/api/profiles/{id}",
        "the person changes an execution profile, which can name full access or Autopilot Full",
    ),
    (
        "DELETE",
        "/api/profiles/{id}",
        "the person deletes an execution profile",
    ),
    (
        "PUT",
        "/api/threads/{id}/profile",
        "applies a profile to a thread, which can raise its permission mode, or forgets the applied profile",
    ),
    (
        "POST",
        "/api/crew/profile/apply",
        "applies a profile to a crew, which can raise its permission mode and Autopilot mode",
    ),
    (
        "PUT",
        "/api/engines/{id}/code-mode",
        "the person turns Code Mode on or off for an engine, which changes what the engine may call",
    ),
    (
        "POST",
        "/api/projects/{id}/repos/add-folder",
        "the person adds a folder to a project in place; nothing is copied and it starts untrusted",
    ),
    (
        "POST",
        "/api/projects/{id}/repos/create",
        "the person creates a new repository under Orbyn's data",
    ),
    (
        "POST",
        "/api/projects/{id}/repos/clone",
        "the person clones a repository from a remote into Orbyn's data",
    ),
    (
        "POST",
        "/api/projects/{id}/repos/scan",
        "reads a folder or workspace file the person picked, which may be anywhere on disk",
    ),
    (
        "POST",
        "/api/projects/{id}/repos/{repo_id}/publish",
        "the person publishes a repository to a host",
    ),
    (
        "POST",
        "/api/projects/{id}/repos/{repo_id}/trust",
        "the person trusts a folder, which lets Orbyn run things in it",
    ),
    (
        "POST",
        "/api/projects/{id}/repos/{repo_id}/setup-git",
        "the person starts Git in a folder that has none",
    ),
    (
        "POST",
        "/api/projects/{id}/repos/{repo_id}/remove",
        "the person stops tracking a repository",
    ),
    (
        "POST",
        "/api/projects/{id}/repos/{repo_id}/clone/cancel",
        "the person stops a running clone",
    ),
    (
        "POST",
        "/api/library/workflows",
        "the person saves a library workflow, a command that can later run in their shell",
    ),
    (
        "PATCH",
        "/api/library/workflows/{id}",
        "the person changes a library workflow, a command that can later run in their shell",
    ),
    (
        "DELETE",
        "/api/library/workflows/{id}",
        "the person deletes a library workflow",
    ),
    (
        "POST",
        "/api/library/workflows/{id}/run",
        "starts a library workflow in the person's shell, with the values of its environment sets",
    ),
    (
        "POST",
        "/api/library/prompts",
        "the person saves a library prompt",
    ),
    (
        "PATCH",
        "/api/library/prompts/{id}",
        "the person changes a library prompt",
    ),
    (
        "DELETE",
        "/api/library/prompts/{id}",
        "the person deletes a library prompt",
    ),
    (
        "POST",
        "/api/library/env-sets",
        "the person creates an environment set",
    ),
    (
        "PATCH",
        "/api/library/env-sets/{id}",
        "the person renames an environment set or changes its key names, which deletes the keychain item of a removed key",
    ),
    (
        "DELETE",
        "/api/library/env-sets/{id}",
        "the person deletes an environment set and every keychain value in it",
    ),
    (
        "PUT",
        "/api/library/env-sets/{id}/keys/{key}",
        "the person stores a secret value in the OS keychain",
    ),
    (
        "DELETE",
        "/api/library/env-sets/{id}/keys/{key}",
        "the person removes a secret value from the OS keychain",
    ),
    (
        "POST",
        "/api/library/import",
        "imports an untrusted document into the personal library",
    ),
    (
        "POST",
        "/api/projects/{id}/library/suggestions/copy",
        "the person copies an entry from a repository's committed .orbyn/ file into the device library",
    ),
    (
        "POST",
        "/api/projects/{id}/repo-threads",
        "the person starts a thread in several repositories, which creates a worktree and a branch in each",
    ),
    (
        "POST",
        "/api/threads/{id}/repos",
        "the person adds a repository to a thread, which creates a worktree and a branch in it",
    ),
    (
        "DELETE",
        "/api/threads/{id}/repos/{repo_id}",
        "the person removes a repository from a thread, which removes its worktree",
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
        // One route per path: a path with several enforced methods would merge two `any` fallbacks.
        let paths: std::collections::BTreeSet<&str> =
            ENFORCED.iter().map(|(_, path)| *path).collect();
        for path in paths {
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

    use crate::http::access::{CallerClass, PersonRefusal, REFUSAL_WINDOW};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    type Rows = Arc<Mutex<Vec<PersonRefusal>>>;

    /// A stub router over every enforced route, with the refusal audit going to `rows` (or failing) and a clock the test moves.
    async fn audited(key: Option<&str>, fail: bool) -> (String, Rows, Arc<AtomicU64>, Arc<Access>) {
        let rows: Rows = Arc::default();
        let offset = Arc::new(AtomicU64::new(0));
        let base = Instant::now();
        let sink_rows = rows.clone();
        let clock_offset = offset.clone();
        let access = Arc::new(
            Access::new("secret", Vec::new(), None)
                .with_person_key(key)
                .with_refusal_audit_clock(
                    Arc::new(move |refusal| {
                        if fail {
                            return Err("disk full".to_owned());
                        }
                        sink_rows.lock().unwrap().push(refusal.clone());
                        Ok(())
                    }),
                    Arc::new(move || {
                        base + Duration::from_secs(clock_offset.load(Ordering::SeqCst))
                    }),
                ),
        );
        let mut router = Router::new();
        let paths: std::collections::BTreeSet<&str> =
            ENFORCED.iter().map(|(_, path)| *path).collect();
        for path in paths {
            router = router.route(path, any(|| async { StatusCode::NO_CONTENT }));
        }
        let router = router.route_layer(from_fn_with_state(access.clone(), require_person));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, router).await });
        (url, rows, offset, access)
    }

    const ANSWER: &str = "/api/questions/{id}/answer";

    async fn post(base: &str, path: &str, origin: bool) -> reqwest::Response {
        let mut request = reqwest::Client::new()
            .post(format!("{base}{path}"))
            .header("authorization", "Bearer secret-service-token")
            .header("x-secret-header", "header-secret")
            .body("body-secret");
        if origin {
            request = request.header("origin", "http://127.0.0.1:5391");
        }
        request.send().await.unwrap()
    }

    #[tokio::test]
    async fn a_refused_person_call_writes_one_row_with_the_pattern_and_nothing_secret() {
        let (base, rows, _, _) = audited(Some("the-key"), false).await;
        let response = post(&base, "/api/questions/q-123/answer?x=1", false).await;
        assert_eq!(response.status().as_u16(), 403);
        let rows = rows.lock().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (rows[0].method.as_str(), rows[0].route.as_str()),
            ("POST", ANSWER)
        );
        assert_eq!(rows[0].caller, CallerClass::ServiceToken);
        assert_eq!(rows[0].code, "person_required");
        let text = format!("{:?}", rows[0]);
        for secret in [
            "q-123",
            "x=1",
            "secret-service-token",
            "header-secret",
            "body-secret",
        ] {
            assert!(!text.contains(secret), "{secret} leaked into {text}");
        }
    }

    #[tokio::test]
    async fn a_service_with_no_shell_records_desktop_required() {
        let (base, rows, _, _) = audited(None, false).await;
        assert_eq!(
            post(&base, "/api/questions/q/answer", false).await.status(),
            403
        );
        assert_eq!(rows.lock().unwrap()[0].code, "desktop_required");
    }

    #[tokio::test]
    async fn the_right_key_and_a_local_route_write_no_row() {
        let (base, rows, _, _) = audited(Some("the-key"), false).await;
        let response = reqwest::Client::new()
            .post(format!("{base}/api/questions/q/answer"))
            .header("x-orbyn-person", "the-key")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 204);
        assert!(rows.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn repeats_inside_a_minute_coalesce_and_a_new_minute_or_route_or_class_writes_again() {
        let (base, rows, offset, _) = audited(Some("the-key"), false).await;
        for _ in 0..5 {
            assert_eq!(
                post(&base, "/api/questions/a/answer", false).await.status(),
                403
            );
        }
        assert_eq!(rows.lock().unwrap().len(), 1, "a loop writes one row");
        offset.store(REFUSAL_WINDOW.as_secs() - 1, Ordering::SeqCst);
        post(&base, "/api/questions/b/answer", false).await;
        assert_eq!(rows.lock().unwrap().len(), 1, "still inside the minute");
        // Another route, and the same route from a browser, each get a row of their own.
        post(&base, "/api/questions/a/dismiss", false).await;
        post(&base, "/api/questions/a/answer", true).await;
        assert_eq!(rows.lock().unwrap().len(), 3);
        offset.store(REFUSAL_WINDOW.as_secs() + 1, Ordering::SeqCst);
        post(&base, "/api/questions/a/answer", false).await;
        let rows = rows.lock().unwrap();
        assert_eq!(rows.len(), 4, "a new minute writes again");
        assert_eq!(rows[2].caller, CallerClass::Browser);
    }

    #[tokio::test]
    async fn the_memory_of_refusals_stays_bounded() {
        let (_, rows, _, access) = audited(Some("the-key"), false).await;
        for n in 0..5000 {
            access.audit_person_refusal(
                "POST",
                &format!("/r/{n}"),
                None,
                CallerClass::ServiceToken,
            );
        }
        assert!(rows.lock().unwrap().len() <= 1024);
    }

    #[tokio::test]
    async fn a_handler_level_refusal_is_audited_by_field_and_a_failed_write_still_refuses() {
        let headers = axum::http::HeaderMap::new();
        let (_, rows, offset, access) = audited(Some("the-key"), false).await;
        for _ in 0..3 {
            let refused = access.person_check_field(&headers, "POST", "/r/{id}", "resume");
            assert!(refused.is_err());
        }
        let _ = access.person_check_field(&headers, "POST", "/r/{id}", "worktree");
        access.audit_person_refusal("POST", "/r/{id}", None, CallerClass::ServiceToken);
        {
            let rows = rows.lock().unwrap();
            let fields: Vec<_> = rows.iter().map(|row| row.field).collect();
            assert_eq!(fields, [Some("resume"), Some("worktree"), None]);
        }
        offset.store(REFUSAL_WINDOW.as_secs() + 1, Ordering::SeqCst);
        let _ = access.person_check_field(&headers, "POST", "/r/{id}", "resume");
        assert_eq!(rows.lock().unwrap().len(), 4);

        let (_, rows, _, failing) = audited(Some("the-key"), true).await;
        let refused = failing.person_check_field(&headers, "POST", "/r/{id}", "resume");
        assert!(refused.is_err(), "a failed audit write still refuses");
        assert!(rows.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_failed_audit_write_still_answers_403() {
        let (base, rows, _, _) = audited(Some("the-key"), true).await;
        let response = post(&base, "/api/questions/q/answer", false).await;
        assert_eq!(response.status().as_u16(), 403);
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["code"], "person_required");
        assert!(rows.lock().unwrap().is_empty());
    }
}
