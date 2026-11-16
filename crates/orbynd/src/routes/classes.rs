//! The class of every protected route: `Local` is opened by the service token alone, `Person` is a route where acting as
//! the person matters (design: `docs/worker-token-isolation.md`, #441). Step 1 only: nothing is enforced, every route
//! behaves as before. A later step makes a `Person` route need a key a worker cannot read.
//!
//! The rule for `Person`: the route decides or grants something only the person may (an approval, a trust or policy
//! grant, redaction), acts on the person's files or shell, replaces the app, or writes account or Crew state behind
//! `local_only`. Reads, an engine's own thread work and settings that grant nothing stay `Local`. Browser mode and the
//! CLI will not be able to use `Person` routes (owner ruling).
//!
//! Left `Local` on purpose: `POST /api/threads/{id}/git/action` (push) and `POST /api/threads/{id}/runs`. `runs` takes
//! `permission_mode` (and model, effort, options) in its body, so raising the mode there needs a field-level person
//! check in step 2. Checked for the same field: `/agui/threads/{id}` reads only `threadId`, `runId` and `resume`;
//! `queue` and `steer` take model, effort and options but no `permission_mode`. `PATCH /api/threads/{id}` also takes it
//! and is `Person` as a whole.
//!
//! `Person` routes are listed here with a reason each. Every other protected route is `"local"` in `tests/routes.json`,
//! one line each, so a new route must be classified in a diff: the route-table test fails for a route with no class.

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
