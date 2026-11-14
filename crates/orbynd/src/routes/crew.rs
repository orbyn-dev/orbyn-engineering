//! Crew (Preview) over HTTP: the Preview flag, background crews' state (`/api/crew/background`: list, read, create, update, on, off,
//! delete and the runs; nothing fires a trigger or starts an engine), Autopilot's modes, trust, risk, the Review queue, the merge queue, the watch on
//! main, the digest, the approval ledger, the trip wires and the guards, and three reads of the repository's data classes (`/api/crew/classes`,
//! `/api/crew/classes/test`, `/api/crew/route/{task}`). Transport only: `orbyn_core::crew` builds every view and
//! owns every rule.
//!
//! **Flag off, nothing here exists.** Every route sits behind [`preview_gate`], which answers exactly what the fallback answers
//! for a route that is not there, and the core's own functions check the flag again. `routes/crew_tests.rs` walks the whole
//! route table, so a `/api/crew` route registered without the gate fails there.
//!
//! **Writes only through the core's guards.** The only routes that change anything are in `orbyn_core::crew::changes::STATE_CHANGING`
//! (the marker Autopilot's N1 refuses by path), each with its guard in [`GUARDS`]. No route takes a command from the client or runs
//! one, and no route pushes or merges. A revert runs only the repository's own checks (`.orbyn/crew.json` at the commit before the
//! merge, a commit the crew did not write) in a scratch checkout under the data folder, which is always removed, and then moves the
//! local branch by compare-and-swap; it is refused when a later merge needs the merge, when the branch is checked out anywhere or
//! no longer holds the merge, and nothing reaches a remote. The repository and branch come from the stored merge, never from the
//! request, and the request must name the whole merge commit it was shown. Writes are refused for a remote viewer, as the account's
//! are. The Preview flag is never settable from here.
//!
//! **The blast-radius guard's lists** (`/api/crew/guard/targets`, `.../add`, `.../{id}/remove`, and the read-only `/api/crew/guard/test`)
//! live on this device. Adding a `safe` target or removing a `prod` one loosens the guard and needs the desktop presence grant
//! approving once needs; until the shell's dialog for it exists every caller gets `desktop_required` or `presence_required` for
//! those two. Tightening needs no grant. A remote viewer reads and is refused the writes.
//!
//! The same marker makes Autopilot refuse an agent's request to any of these paths (N1), so a worker cannot answer for the person.
//!
//! **No agent approves; a person may approve once.** A command the chain denied is held (`GET /api/crew/approvals/held`). The person
//! may approve that exact command once (`POST /api/crew/approvals/{id}/approve-once`), deny it or dismiss it. Approving needs the
//! person's click in the desktop app's own native dialog: the shell records a one-time grant with the service on the channel only
//! the shell holds, and the page sends the grant's nonce in `x-orbyn-confirm`. A service without a desktop shell, and a remote one,
//! answers `403 desktop_required`; a missing, wrong, used, expired or mismatched nonce is `403 presence_required` and does not use
//! the grant up. No caller that only speaks HTTP to the service can approve once, with or without the service token. This is not
//! OS-level presence (docs/crew.md, "Approving once: local presence"). Deny and dismiss need no grant (they only refuse), so a
//! token holder can deny or dismiss a hold, never approve one; their ledger rows say `local_caller`, not `person`. Remote viewers
//! are refused for all three. Until #441 (workers can read the service token and the data folder) is fixed, the Preview flag stays
//! for development only.

use axum::Json;
use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::{Next, from_fn_with_state};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use orbyn_contracts as wire;
use orbyn_core::CoreError;
use orbyn_core::crew::{Answering, Page};
use orbyn_core::util::unix_now;

use crate::http::{ApiFailure, ApiJson, ApiResult, AppState, QueryMap};

/// The guard of each state-changing route, as `(method, path, guard)`. The paths are `orbyn_core::crew::changes::STATE_CHANGING`
/// with `{id}` for the task; the route-table test checks the two agree and that every route is listed there or in `READS`.
#[cfg(test)]
pub const GUARDS: &[(&str, &str, &str)] = &[
    (
        "PUT",
        "/api/crew/autopilot/mode",
        "core set_crew_autopilot_mode: a repository must be a project, a crew must have a task, the person's choice only; local only",
    ),
    (
        "POST",
        "/api/crew/autopilot/confirm",
        "core set_crew_autopilot_mode keeping the default as it is; local only",
    ),
    (
        "POST",
        "/api/crew/autopilot/review/{id}/looks-good",
        "core crew_answer_review: the stored merge, the whole commit the person saw, one answer at a time; local only",
    ),
    (
        "POST",
        "/api/crew/autopilot/review/{id}/comment",
        "core crew_answer_review: the stored merge, the whole commit the person saw, 1 to 2000 characters, redacted; local only",
    ),
    (
        "POST",
        "/api/crew/autopilot/review/{id}/revert",
        "core crew_answer_review revert: scratch checkout removed after, the repository's own checks, later merges, branch held and not checked out, compare-and-swap, nothing pushed; local only",
    ),
    (
        "POST",
        "/api/crew/autopilot/areas/override",
        "core autopilot_area_override: keep_ceiling or off only, never above the ceiling, acknowledged, the record as seen (else stale), audit row first in the same transaction as the area event and mode row, the same choice twice records nothing; local only",
    ),
    (
        "POST",
        "/api/crew/autopilot/areas/follow",
        "core autopilot_area_follow: ends a person's override and nothing else, the record as seen when given (else stale), audit row first in the same transaction as the area event and mode row, idempotent; local only",
    ),
    (
        "POST",
        "/api/crew/approvals/{id}/approve-once",
        "core autopilot_approve_once: a desktop shell, the person's grant for this hold as it stands (x-orbyn-confirm), used up; the hold open, its task running, the command read whole, the always-denied list again, one decision; local only",
    ),
    (
        "POST",
        "/api/crew/approvals/{id}/deny",
        "core autopilot_deny_held: one decision per hold, a ledger row; only ever refuses; local only",
    ),
    (
        "POST",
        "/api/crew/approvals/{id}/dismiss",
        "core autopilot_dismiss_held: one decision per hold, a ledger row; only ever hides; local only",
    ),
    (
        "POST",
        "/api/crew/background/create",
        "core background_create: a live project, bounded name, budget and trigger settings of the trigger's own kind, redacted, created off, audited in the same transaction; local only",
    ),
    (
        "POST",
        "/api/crew/background/{id}/update",
        "core background_update: the name and trigger only (project, template and budget are fixed, also by the table), redacted, audited in the same transaction; local only",
    ),
    (
        "POST",
        "/api/crew/background/{id}/enable",
        "core background_set_enabled: the person's switch, audited in the same transaction; starts nothing, no trigger fires; local only",
    ),
    (
        "POST",
        "/api/crew/background/{id}/disable",
        "core background_set_enabled: stops a run in progress with the reason disabled, audited in the same transaction; local only",
    ),
    (
        "POST",
        "/api/crew/background/{id}/delete",
        "core background_delete: refused only while a run is in progress; its runs and waiting reviews are removed with the crew and counted in the audit row, which stays; local only",
    ),
    (
        "POST",
        "/api/crew/tasks/{id}/cancel",
        "core crew_cancel_task: the person's stop button; the intent audit row before any effect, the worker's engine stopped and confirmed (else the state is kept and a failed-attempt row is written), then the state and its row in one transaction; already cancelled answers already and writes nothing; passed or stopped is a conflict; optional reason 1 to 200 characters, redacted; no agent tool reaches it; local only",
    ),
    (
        "POST",
        "/api/crew/guard/targets/add",
        "core guard_target_add: a bare name or *.suffix, no secret, at most 200 entries, audited in the same transaction before the entry; adding a safe target loosens the guard and needs the person's presence grant (x-orbyn-confirm); local only",
    ),
    (
        "POST",
        "/api/crew/guard/targets/{id}/remove",
        "core guard_target_remove: audited in the same transaction before the removal; removing a prod target loosens the guard and needs the person's presence grant (x-orbyn-confirm); local only",
    ),
    (
        "POST",
        "/api/crew/tasks/{id}/rerun-checks",
        "core crew_rerun_checks: the person's re-run of the key's checks only; the task must be passed, or stopped for a benign reason after its checks ran live (a safety stop is refused), with a key and a checkout, no live verification, no open worker run, no other re-run on the device and a 10 second gap per task (409 task_busy); the intent audit row before any command, every command screened by the always-denied list and the blast-radius guard (a refused check is not run), bounded in time, the outcome row after with counts only; no Evidence or Gate is stored; the answer has counts and the ids of refused checks, never a command, output or claim, and no state changes; optional reason 1 to 200 characters, redacted; no agent tool reaches it; local only",
    ),
];

pub fn routes(state: AppState) -> Router<AppState> {
    let writes = Router::new()
        .route("/api/crew/autopilot/mode", put(set_mode))
        .route("/api/crew/autopilot/confirm", post(confirm))
        .route(
            "/api/crew/autopilot/review/{task_id}/looks-good",
            post(looks_good),
        )
        .route(
            "/api/crew/autopilot/review/{task_id}/comment",
            post(comment),
        )
        .route("/api/crew/autopilot/review/{task_id}/revert", post(revert))
        .route("/api/crew/approvals/{id}/approve-once", post(approve_once))
        .route("/api/crew/approvals/{id}/deny", post(deny_held))
        .route("/api/crew/approvals/{id}/dismiss", post(dismiss_held))
        .route("/api/crew/background/create", post(background_create))
        .route("/api/crew/background/{id}/update", post(background_update))
        .route("/api/crew/background/{id}/enable", post(background_enable))
        .route(
            "/api/crew/background/{id}/disable",
            post(background_disable),
        )
        .route("/api/crew/background/{id}/delete", post(background_delete))
        .route("/api/crew/tasks/{id}/cancel", post(cancel_task))
        .route("/api/crew/guard/targets/add", post(guard_add))
        .route("/api/crew/guard/targets/{id}/remove", post(guard_remove))
        .merge(super::crew_areas::routes())
        .route("/api/crew/tasks/{id}/rerun-checks", post(rerun_checks))
        .route_layer(from_fn_with_state(state.clone(), local_only));
    Router::new()
        .route("/api/crew/preview", get(preview))
        .route("/api/crew/autopilot", get(modes))
        .route("/api/crew/autopilot/guards", get(guards))
        .route("/api/crew/autopilot/risk/{task_id}", get(risk))
        .route("/api/crew/autopilot/review", get(review_queue))
        .route(
            "/api/crew/autopilot/review/{task_id}/revert-check",
            get(revert_check),
        )
        .route("/api/crew/autopilot/merge-queue", get(merge_queue))
        .route("/api/crew/autopilot/watch", get(watch))
        .route("/api/crew/autopilot/digest", get(digest))
        .route("/api/crew/autopilot/areas", get(areas))
        .route("/api/crew/autopilot/areas/history", get(area_history))
        .route("/api/crew/autopilot/thresholds", get(thresholds))
        .route("/api/crew/approvals", get(approvals))
        .route("/api/crew/approvals/held", get(held))
        .route("/api/crew/stops", get(stops))
        .route("/api/crew/trust", get(trust))
        .route("/api/crew/background", get(background_list))
        .route("/api/crew/background/{id}", get(background_get))
        .route("/api/crew/background/{id}/runs", get(background_runs))
        .route("/api/crew/guard/targets", get(guard_targets))
        .route("/api/crew/guard/test", get(guard_test))
        .route("/api/crew/classes", get(classes))
        .route("/api/crew/classes/test", get(classes_test))
        .route("/api/crew/route/{task_id}", get(route_view))
        .merge(writes)
        .merge(super::crew_tasks::routes())
        .merge(super::crew_board::routes())
        .layer(from_fn_with_state(state, preview_gate))
}

/// With the flag off the route is not there: the same answer as any other path nothing serves, for every method (a wrong one
/// included, which is why this is a `layer` and not a `route_layer`).
async fn preview_gate(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiFailure> {
    if !state.core.config.crew_preview {
        return Err(ApiFailure::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "No such route.",
        ));
    }
    Ok(next.run(request).await)
}

/// A remote viewer reads; the person at this computer answers.
async fn local_only(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiFailure> {
    if state.remote && request.uri().path().ends_with("/approve-once") {
        return Err(ApiFailure::forbidden(
            "desktop_required",
            "Approving a command needs the Orbyn desktop app on this computer.",
        ));
    }
    if state.remote {
        return Err(ApiFailure::forbidden(
            "local_only",
            "Do this from the computer the app runs on.",
        ));
    }
    Ok(next.run(request).await)
}

fn number(query: &QueryMap, name: &str) -> Result<Option<u32>, CoreError> {
    query
        .0
        .get(name)
        .map(|text| {
            text.parse::<u32>()
                .map_err(|_| CoreError::Invalid(format!("{name} is a whole number")))
        })
        .transpose()
}

fn page(query: &QueryMap) -> Result<Page, CoreError> {
    Page::new(number(query, "limit")?, number(query, "offset")?)
}

/// The repository's data classes at the commit a task started in this thread would start from. Read-only.
async fn classes(State(state): State<AppState>, query: QueryMap) -> ApiResult<impl IntoResponse> {
    let thread = thread_of(&query)?;
    Ok(Json(state.core.crew_classes_view(&thread).await?))
}

/// Up to 10 paths against those rules. `path` may be repeated, and each value may hold several paths, one per line.
async fn classes_test(
    State(state): State<AppState>,
    pairs: Result<
        axum::extract::Query<Vec<(String, String)>>,
        axum::extract::rejection::QueryRejection,
    >,
) -> ApiResult<impl IntoResponse> {
    let axum::extract::Query(pairs) =
        pairs.map_err(|_| CoreError::Invalid("the query is not valid".into()))?;
    let thread = pairs
        .iter()
        .find(|(name, _)| name == "thread")
        .map(|(_, value)| value.clone())
        .ok_or_else(|| CoreError::Invalid("thread is required".into()))?;
    let paths: Vec<String> = pairs
        .iter()
        .filter(|(name, _)| name == "path")
        .flat_map(|(_, value)| value.lines())
        .map(|line| line.trim_end_matches('\r'))
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    Ok(Json(state.core.crew_class_test(&thread, &paths).await?))
}

/// How one task was routed: the decision or the refusal, and the ledger rows of the routing.
async fn route_view(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_route_view(&task_id)?))
}

fn required(query: &QueryMap, name: &str) -> Result<String, CoreError> {
    query
        .0
        .get(name)
        .cloned()
        .ok_or_else(|| CoreError::Invalid(format!("{name} is required")))
}

fn thread_of(query: &QueryMap) -> Result<String, CoreError> {
    query
        .0
        .get("thread")
        .cloned()
        .ok_or_else(|| CoreError::Invalid("thread is required".into()))
}

async fn preview(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_preview_state()?))
}

async fn modes(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.autopilot_modes()?))
}

async fn set_mode(
    State(state): State<AppState>,
    ApiJson(change): ApiJson<wire::AutopilotModeChange>,
) -> ApiResult<impl IntoResponse> {
    state.core.autopilot_set_mode(&change)?;
    Ok(Json(state.core.autopilot_modes()?))
}

async fn confirm(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    state.core.autopilot_confirm()?;
    Ok(Json(state.core.autopilot_modes()?))
}

async fn guards(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.autopilot_guards()?))
}

/// Revert rates; `project` limits them to one repository's merges.
async fn trust(State(state): State<AppState>, query: QueryMap) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.autopilot_trust(
        unix_now(),
        query.0.get("project").map(String::as_str),
    )?))
}

/// The areas of a project under the circuit breaker (shadow): `project`, `limit`, `offset`. Read-only.
async fn areas(State(state): State<AppState>, query: QueryMap) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.autopilot_areas(
        &required(&query, "project")?,
        page(&query)?,
        unix_now(),
    )?))
}

/// The trail of one area: `project` and `area` (in the query, as it contains `/`). Read-only.
async fn area_history(
    State(state): State<AppState>,
    query: QueryMap,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.autopilot_area_history(
        &required(&query, "project")?,
        &required(&query, "area")?,
    )?))
}

/// The breaker's constants, proposed.
async fn thresholds(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.autopilot_thresholds()?))
}

async fn risk(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.autopilot_risk(&task_id)?))
}

async fn review_queue(
    State(state): State<AppState>,
    query: QueryMap,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state
            .core
            .autopilot_review_queue(unix_now(), page(&query)?)?,
    ))
}

async fn revert_check(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.autopilot_revert_check(&task_id)?))
}

async fn answer(
    state: AppState,
    task_id: String,
    body: wire::AutopilotAnswerBody,
    answering: Answering,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state
            .core
            .autopilot_answer(&task_id, &body, answering, unix_now())
            .await?,
    ))
}

async fn looks_good(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    ApiJson(body): ApiJson<wire::AutopilotAnswerBody>,
) -> ApiResult<impl IntoResponse> {
    answer(state, task_id, body, Answering::LooksGood).await
}

async fn comment(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    ApiJson(body): ApiJson<wire::AutopilotAnswerBody>,
) -> ApiResult<impl IntoResponse> {
    answer(state, task_id, body, Answering::Comment).await
}

async fn revert(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    ApiJson(body): ApiJson<wire::AutopilotAnswerBody>,
) -> ApiResult<impl IntoResponse> {
    answer(state, task_id, body, Answering::Revert).await
}

async fn merge_queue(
    State(state): State<AppState>,
    query: QueryMap,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.autopilot_merge_queue(page(&query)?)?))
}

async fn watch(State(state): State<AppState>, query: QueryMap) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.autopilot_watch(page(&query)?)?))
}

async fn digest(State(state): State<AppState>, query: QueryMap) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state
            .core
            .autopilot_digest(unix_now(), number(&query, "hours")?)?,
    ))
}

/// The approval ledger: `task`, `kind`, `limit`, `offset`.
async fn approvals(State(state): State<AppState>, query: QueryMap) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.autopilot_log(
        query.0.get("task").map(String::as_str),
        query.0.get("kind").map(String::as_str),
        page(&query)?,
    )?))
}

async fn stops(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.autopilot_tripwires()?))
}

/// The commands Autopilot holds for the person, newest first.
async fn held(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.autopilot_held()?))
}

/// The header the person's grant nonce comes in: what the desktop app handed the page after the person clicked Approve.
const CONFIRM_HEADER: &str = "x-orbyn-confirm";

async fn approve_once(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> ApiResult<impl IntoResponse> {
    let nonce = headers
        .get(CONFIRM_HEADER)
        .and_then(|value| value.to_str().ok());
    Ok(Json(state.core.autopilot_approve_once(&id, nonce).await?))
}

async fn deny_held(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.autopilot_deny_held(&id)?))
}

async fn dismiss_held(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.autopilot_dismiss_held(&id)?))
}

/// The background crews, by name: `project`, `limit`, `offset`.
async fn background_list(
    State(state): State<AppState>,
    query: QueryMap,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.background_list(
        query.0.get("project").map(String::as_str),
        page(&query)?,
    )?))
}

async fn background_get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.background_get(&id)?))
}

/// A crew's runs with what each spent against its budget, newest first: `limit`, `offset`.
async fn background_runs(
    State(state): State<AppState>,
    Path(id): Path<String>,
    query: QueryMap,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.background_runs(&id, page(&query)?)?))
}

async fn background_create(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<wire::BackgroundCrewCreate>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.background_create(&body)?))
}

async fn background_update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<wire::BackgroundCrewUpdate>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.background_update(&id, &body)?))
}

async fn background_enable(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.background_set_enabled(&id, true)?))
}

async fn background_disable(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.background_set_enabled(&id, false)?))
}

async fn background_delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    state.core.background_delete(&id)?;
    Ok(Json(serde_json::json!({ "deleted": true })))
}

/// The person's stop button: cancels a task. The body may be empty, or `{ "reason": "..." }`.
async fn cancel_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Bytes,
) -> ApiResult<impl IntoResponse> {
    // The flag first: with it off, a malformed body is still "no such route".
    state.core.crew_preview_state()?;
    let body: wire::CrewTaskCancel = if body.iter().all(u8::is_ascii_whitespace) {
        wire::CrewTaskCancel { reason: None }
    } else {
        serde_json::from_slice(&body)
            .map_err(|_| CoreError::Invalid("the body is { \"reason\": text } or empty".into()))?
    };
    Ok(Json(
        state
            .core
            .crew_cancel_task(&id, body.reason.as_deref(), "user")
            .await?,
    ))
}

/// The blast-radius guard's two lists, on this device.
async fn guard_targets(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.guard_targets()?))
}

/// What the guard says about a command (`command`, and `thread` for a worker's worktree): read-only, nothing runs.
async fn guard_test(
    State(state): State<AppState>,
    query: QueryMap,
) -> ApiResult<impl IntoResponse> {
    let command = query
        .0
        .get("command")
        .ok_or_else(|| CoreError::Invalid("command is required".into()))?;
    Ok(Json(
        state
            .core
            .guard_test(command, query.0.get("thread").map(String::as_str))
            .await?,
    ))
}

/// Adding a `safe` target loosens the guard: it needs the same grant approving once does, in `x-orbyn-confirm`.
async fn guard_add(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<wire::GuardTargetAdd>,
) -> ApiResult<impl IntoResponse> {
    let nonce = headers
        .get(CONFIRM_HEADER)
        .and_then(|value| value.to_str().ok());
    Ok(Json(state.core.guard_target_add(&body, nonce)?))
}

/// Removing a `prod` target loosens the guard: it needs the same grant, in `x-orbyn-confirm`.
async fn guard_remove(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> ApiResult<impl IntoResponse> {
    let nonce = headers
        .get(CONFIRM_HEADER)
        .and_then(|value| value.to_str().ok());
    Ok(Json(state.core.guard_target_remove(&id, nonce)?))
}

/// The person's re-run of a task's key checks. The body may be empty, or `{ "reason": "..." }`. The answer is counts and outcomes.
async fn rerun_checks(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Bytes,
) -> ApiResult<impl IntoResponse> {
    // The flag first: with it off nothing here exists, and the body is not even read.
    if !state.core.config.crew_preview {
        return Err(CoreError::NotFound("route".into()).into());
    }
    let body: wire::CrewTaskRerun = if body.iter().all(u8::is_ascii_whitespace) {
        wire::CrewTaskRerun { reason: None }
    } else {
        serde_json::from_slice(&body)
            .map_err(|_| CoreError::Invalid("the body is { \"reason\": text } or empty".into()))?
    };
    Ok(Json(
        state
            .core
            .crew_rerun_checks(&id, body.reason.as_deref(), "user")
            .await?,
    ))
}
