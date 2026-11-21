//! Crew (Preview) over HTTP: the Preview flag, background crews' state (`/api/crew/background`: list, read, create, update, on, off,
//! delete, the runs and Run now, which starts one run by hand; nothing fires a trigger on its own), Autopilot's modes, trust, risk, the Review queue, the merge queue, the watch on
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
//! one. The typed `/tasks/{id}/merge` route requests Core-owned forge queue/process/watch operations with stored targets and
//! person-owned startup CI policy; it grants no agent direct merge authority. A local compatibility revert runs only the repository's own checks (`.orbyn/crew.json` at the commit before the
//! merge, a commit the crew did not write) in a scratch checkout under the data folder, which is always removed, and then moves the
//! local branch by compare-and-swap; it is refused when a later merge needs the merge, when the branch is checked out anywhere or
//! no longer holds the merge. It refuses forge records, which need a corrective PR. The repository and branch come from the stored merge, never from the
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
//! are refused for all three. Every Crew route that changes state also needs the person key (`routes/classes.rs`), so a worker that
//! read the service token gets `403 person_required`; the Preview flag is for readiness, not for worker-proofing. What #441 still
//! holds is the Windows sandbox and the store read.

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
        "POST",
        "/api/crew/approvals/second-engine/{id}/dismiss",
        "core autopilot_dismiss_second_engine_notice: Preview, durable idempotent dismissal; never approves or retries; local only",
    ),
    (
        "POST",
        "/api/crew/settings/engine-tiers/save",
        "core crew_set_engine_tiers: local only, person transport, revision compare-and-swap, exact configured ACP engine ids, unique entries, exactly one visible marked default, expected_default_model precondition independently checked by core, native and unavailable engines refused; clear restores builder-only; no agent tool",
    ),
    (
        "POST",
        "/api/crew/capacity/settings",
        "core crew_set_capacity_settings: Preview, limit of at least 1, sampling, pause_enabled (off by default); audit row in the same transaction; no agent tool; local only",
    ),
    (
        "POST",
        "/api/crew/capacity/tasks/{id}",
        "core crew_set_task_capacity: Preview, an existing task, priority and/or crew_started, nothing else; no agent tool; local only",
    ),
    (
        "POST",
        "/api/crew/review/{id}/consent",
        "core crew_review_consent: local only, person transport, exact pending request id with pinned head and round, once or wait, ten-minute expiry, atomic single consumption; no client actor or grant flag; no model tool",
    ),
    (
        "POST",
        "/api/crew/review/{id}/remove-lane",
        "core crew_remove_review_lane: local only, person transport, nonempty bounded reason, required lane from pinned base or default risk lanes, locked lane refused, durable waiver and audit, never clears findings; no model tool",
    ),
    (
        "POST",
        "/api/crew/tasks/{id}/merge",
        "core forge queue: stored task/project target, person-owned CI policy, pinned head/base, durable effect reconciliation; person release, local only",
    ),
    (
        "PUT",
        "/api/crew/autopilot/mode",
        "core set_crew_autopilot_mode: a repository must be a project, a crew must have a task, the person's choice only; local only",
    ),
    (
        "POST",
        "/api/crew/autopilot/digest/schedule/set",
        "core crew_set_digest_schedule: the person's daily digest send time as HH:MM and the UTC offset their local time was in when they saved it, shape-checked by the preference catalog; one person-level preference, no notification is sent by it; local only",
    ),
    (
        "POST",
        "/api/crew/autopilot/confirm",
        "core set_crew_autopilot_mode keeping the default as it is; local only",
    ),
    (
        "POST",
        "/api/crew/autopilot/merge-queue/{id}/resolve",
        "core crew_forge_resolve: a person's answer for a forge merge that needs one, reject or retry, the intent as seen (else stale); the forge is read first and a PR already merged by hand is recorded as merged instead; the audit row first, in one transaction with the intent and its queue entry; no write is ever sent to the forge; retry is refused after a merge request, and a reject then stays reconcilable; no agent tool can reject or retry (the lead's merge tool only records a merge the forge already shows); local only",
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
        "core crew_answer_review revert. Local merge: made in a scratch checkout that is removed after, the repository's own checks, later merges, branch held and not checked out, compare-and-swap, nothing pushed. Forge merge: queues the verified corrective PR for the person's merge route, which pushes that exact commit (never forced) and opens the PR; the correction checkout is kept until it completes and no review reads it; the audit row first; local only",
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
        "core background_create: a live project, bounded name, run budget, daily per-engine budget with the person's UTC offset, and trigger settings of the trigger's own kind, redacted, created off, audited in the same transaction; local only",
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
        "core background_turn_off: stops a run in progress with the reason disabled and cancels its worker's task, audited in the same transaction; local only",
    ),
    (
        "POST",
        "/api/crew/background/{id}/delete",
        "core background_delete: refused only while a run is in progress; its runs and waiting reviews are removed with the crew and counted in the audit row, which stays; local only",
    ),
    (
        "POST",
        "/api/crew/background/{id}/run-now",
        "core crew_background_run_now: the manual event. Preview, an enabled crew whose trigger kind can start, no run in progress, a trusted project and a ready engine; starts one run through crew_start as a Low Crew-started task (review capacity may pause it) and returns the run; nothing is stored when it is refused; local only",
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
        "/api/crew/tasks/{id}/retry-elsewhere",
        "core crew_retry_elsewhere: the person's retry of a task whose worker never ran (a start refusal); the task must derive as could-not-start (no report, no evidence) and another engine must be on and allowed to run workers (409 task_busy otherwise), then the person's cancel with its audit rows and a message to the lead to start it again; no body; no agent tool reaches it; local only",
    ),
    (
        "POST",
        "/api/crew/tasks/{id}/rerun-checks",
        "core crew_rerun_checks: the person's re-run of the key's checks only; the task must be passed, or stopped for a benign reason after its checks ran live (a safety stop is refused), with a key and a checkout, no live verification, no open worker run, no other re-run on the device and a 10 second gap per task (409 task_busy); the intent audit row before any command, every command screened by the always-denied list and the blast-radius guard (a refused check is not run), bounded in time, the outcome row after with counts only; no Evidence or Gate is stored; the answer has counts and the ids of refused checks, never a command, output or claim, and no state changes; optional reason 1 to 200 characters, redacted; no agent tool reaches it; local only",
    ),
    (
        "POST",
        "/api/crew/backtest/start",
        "core crew_backtest_start: a person's choice only; a request with a lane, an engine and a model each, a token budget for every engine (tokens per engine, never summed) and no other field, a review or check lane and never a builder, no money field; one run at a time (a second start while one runs is refused, check and insert in one transaction); refused while no replay engine is connected; no scheduler or tool reaches it; local only",
    ),
    (
        "POST",
        "/api/crew/backtest/{id}/stop",
        "core crew_backtest_stop: a person's request; only records the ask, the run ends as stopped before its next replay and keeps what it replayed; a run that already ended is returned unchanged; no agent tool reaches it; local only",
    ),
    (
        "POST",
        "/api/crew/projects/{id}/pinned-ref/confirm",
        "core crew_confirm_pinned_ref: the default branch must still be the whole commit the person saw, else stale; the pin and its audit row in one transaction; no agent tool reaches it; local only",
    ),
    (
        "POST",
        "/api/crew/projects/{id}/pinned-ref/reset",
        "core crew_reset_pinned_ref: clears only this project's remembered default branch, audited in the same transaction; a later task reads the ref the checkout has then; no agent tool reaches it; local only",
    ),
    (
        "POST",
        "/api/crew/escapes/mark",
        "core crew_escape_mark: a person's mark of a merge of the named project as an escape, confirmed at once (the mark is the confirm); an existing possible escape is confirmed, a confirmed one returned unchanged, a dismissed one refused; the escape, its events and the audit row (first) in one transaction; bounded at 5,000 escapes; no agent tool reaches it; local only",
    ),
    (
        "POST",
        "/api/crew/escapes/{id}/confirm",
        "core crew_escape_confirm: the only way an escape becomes confirmed; possible to confirmed only, the escape as seen (else stale), the same act twice writes nothing, the audit row first in the same transaction as the state and its event; confirmed escapes feed no breaker or router; no agent tool reaches it; local only",
    ),
    (
        "POST",
        "/api/crew/escapes/{id}/dismiss",
        "core crew_escape_dismiss: possible to dismissed only, the escape as seen (else stale), the same act twice writes nothing, the audit row first in the same transaction as the state and its event; a dismissed escape is not offered again; no agent tool reaches it; local only",
    ),
    (
        "POST",
        "/api/crew/escapes/{id}/gap",
        "core crew_escape_set_gap: a test gap or a tooling gap on a possible or confirmed escape, the escape as seen (else stale), the same gap twice writes nothing, a reclassified event and the audit row in one transaction; changes no count and starts nothing; no agent tool reaches it; local only",
    ),
    (
        "POST",
        "/api/crew/escapes/{id}/check",
        "core crew_escape_check_propose: a proposal of one new test file for a confirmed escape with a test gap (never a tooling gap), the escape and check as seen (else stale), the audit row first in the same transaction as the row and its check_proposed event; it writes no task and starts no worker; no agent tool reaches it; local only",
    ),
    (
        "POST",
        "/api/crew/escapes/{id}/check/accept",
        "core crew_escape_check_accept: the only route that starts a worker; the audit row and writing first, then the crew task through the router like crew_start; the worker may add test files only (its diff is judged against the test globs) and the proof must fail on the escaped merge and pass on the fix before the merge queue sees it; no lane is removed; no agent tool reaches it; local only",
    ),
    (
        "POST",
        "/api/crew/escapes/{id}/check/retry",
        "core crew_escape_check_retry: a failed check goes back to proposed, the audit row first in the same transaction; it starts nothing, only accept starts a worker; no agent tool reaches it; local only",
    ),
    (
        "POST",
        "/api/crew/escapes/{id}/check/withdraw",
        "core crew_escape_check_withdraw: withdraws a check that is not merged, the audit row first, then a worker still writing it is cancelled and a queued merge of it is turned down; no agent tool reaches it; local only",
    ),
    (
        "POST",
        "/api/crew/escapes/{id}/check/approve",
        "core crew_escape_check_approve: a person approves exactly the commit and the hash over the paths and bytes the core read from the worker's change, audit row first; only then does a proof that cannot be restricted run it; a different change needs a new approval; no agent tool reaches it; local only",
    ),
    (
        "POST",
        "/api/crew/projects/{id}/worker-provider/set",
        "core crew_set_worker_provider: a person's per-project choice of same_as_lead or any, a known project, audited in the same transaction; it changes where later work runs and never an engine a task already has; no agent tool reaches it; local only",
    ),
    (
        "POST",
        "/api/crew/profile/apply",
        "core apply_profile_to_crew: a person applies an execution profile to a crew, a copy and never a link; a profile that grants full access or Autopilot Full needs its grant codes acknowledged or answers confirm_required and applies nothing; the audit row first in the same transaction; no agent tool reaches it; local only",
    ),
];

pub fn routes(state: AppState) -> Router<AppState> {
    let writes = Router::new()
        .route(
            "/api/crew/settings/engine-tiers/save",
            post(set_engine_tiers),
        )
        .route("/api/crew/review/{task_id}/consent", post(review_consent))
        .route(
            "/api/crew/review/{task_id}/remove-lane",
            post(remove_review_lane),
        )
        .route("/api/crew/autopilot/mode", put(set_mode))
        .route(
            "/api/crew/autopilot/digest/schedule/set",
            post(set_digest_schedule),
        )
        .route("/api/crew/autopilot/confirm", post(confirm))
        .route(
            "/api/crew/autopilot/merge-queue/{task_id}/resolve",
            post(forge_resolve),
        )
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
        .route(
            "/api/crew/approvals/second-engine/{id}/dismiss",
            post(dismiss_second_engine_notice),
        )
        .route("/api/crew/background/create", post(background_create))
        .route("/api/crew/background/{id}/update", post(background_update))
        .route("/api/crew/background/{id}/enable", post(background_enable))
        .route(
            "/api/crew/background/{id}/disable",
            post(background_disable),
        )
        .route("/api/crew/background/{id}/delete", post(background_delete))
        .route(
            "/api/crew/background/{id}/run-now",
            post(background_run_now),
        )
        .route("/api/crew/tasks/{id}/cancel", post(cancel_task))
        .route(
            "/api/crew/projects/{id}/pinned-ref/confirm",
            post(pinned_ref_confirm),
        )
        .route(
            "/api/crew/projects/{id}/pinned-ref/reset",
            post(pinned_ref_reset),
        )
        .route(
            "/api/crew/projects/{id}/worker-provider/set",
            post(worker_provider_set),
        )
        .route("/api/crew/profile/apply", post(apply_profile))
        .route("/api/crew/guard/targets/add", post(guard_add))
        .route("/api/crew/guard/targets/{id}/remove", post(guard_remove))
        .merge(super::crew_areas::routes())
        .merge(super::crew_backtest::writes())
        .merge(super::crew_capacity::writes())
        .merge(super::crew_escapes::writes())
        .route("/api/crew/tasks/{id}/rerun-checks", post(rerun_checks))
        .route(
            "/api/crew/tasks/{id}/retry-elsewhere",
            post(retry_elsewhere),
        )
        .route("/api/crew/tasks/{id}/merge", post(merge_request))
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
        .route("/api/crew/autopilot/digest/schedule", get(digest_schedule))
        .route("/api/crew/autopilot/areas", get(areas))
        .route("/api/crew/autopilot/areas/history", get(area_history))
        .route("/api/crew/autopilot/thresholds", get(thresholds))
        .route("/api/crew/approvals", get(approvals))
        .route("/api/crew/approvals/held", get(held))
        .route(
            "/api/crew/approvals/second-engine",
            get(second_engine_notices),
        )
        .route("/api/crew/stops", get(stops))
        .route("/api/crew/trust", get(trust))
        .route("/api/crew/background", get(background_list))
        .route("/api/crew/background/{id}", get(background_get))
        .route("/api/crew/background/{id}/runs", get(background_runs))
        .route("/api/crew/guard/targets", get(guard_targets))
        .route("/api/crew/guard/test", get(guard_test))
        .route("/api/crew/projects/{id}/pinned-ref", get(pinned_ref))
        .route(
            "/api/crew/projects/{id}/worker-provider",
            get(worker_provider),
        )
        .route("/api/crew/classes", get(classes))
        .route("/api/crew/classes/test", get(classes_test))
        .route("/api/crew/route/{task_id}", get(route_view))
        .route("/api/crew/review/{task_id}/controls", get(review_controls))
        .route("/api/crew/settings/engine-tiers", get(engine_tiers))
        .merge(super::crew_capacity::reads())
        .merge(writes)
        .merge(super::crew_backtest::reads())
        .merge(super::crew_tasks::routes())
        .merge(super::crew_board::routes())
        .merge(super::crew_escapes::routes())
        .merge(super::crew_map::routes())
        .layer(from_fn_with_state(state, preview_gate))
}

async fn merge_request(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<orbyn_core::crew::MergeRequest>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_merge_request(&id, &body).await?))
}

/// With the flag off the route is not there: the same answer as any other path nothing serves, for every method (a wrong one
/// included, which is why this is a `layer` and not a `route_layer`).
pub(super) async fn preview_gate(
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

/// The repository's data classes at the commit a task started in this thread would start from, or at the project's own checkout
/// with `project=`. One of the two, never both. Read-only.
async fn classes(State(state): State<AppState>, pairs: ClassQuery) -> ApiResult<impl IntoResponse> {
    state.core.crew_preview_state()?;
    let pairs = class_pairs(pairs)?;
    let (thread, project) = class_ids(&pairs)?;
    Ok(Json(state.core.crew_classes_view(thread, project).await?))
}

/// Up to 10 paths against those rules. `path` may be repeated, and each value may hold several paths, one per line.
async fn classes_test(
    State(state): State<AppState>,
    pairs: ClassQuery,
) -> ApiResult<impl IntoResponse> {
    // The flag comes before the query is read, so with it off every request is the absent route.
    state.core.crew_preview_state()?;
    let pairs = class_pairs(pairs)?;
    let (thread, project) = class_ids(&pairs)?;
    let paths: Vec<String> = pairs
        .iter()
        .filter(|(name, _)| name == "path")
        .flat_map(|(_, value)| value.lines())
        .map(|line| line.trim_end_matches('\r'))
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    Ok(Json(
        state.core.crew_class_test(thread, project, &paths).await?,
    ))
}

type ClassQuery =
    Result<axum::extract::Query<Vec<(String, String)>>, axum::extract::rejection::QueryRejection>;

fn class_pairs(pairs: ClassQuery) -> Result<Vec<(String, String)>, CoreError> {
    pairs
        .map(|axum::extract::Query(pairs)| pairs)
        .map_err(|_| CoreError::Invalid("the query is not valid".into()))
}

/// The `thread` and `project` of a class query. Either key given twice is refused, so both routes read one id the same way.
fn class_ids(pairs: &[(String, String)]) -> Result<(Option<&str>, Option<&str>), CoreError> {
    let one = |key: &str| {
        let mut found = pairs.iter().filter(|(name, _)| name == key);
        let first = found.next().map(|(_, value)| value.as_str());
        match found.next() {
            Some(_) => Err(CoreError::Invalid(format!("give {key} once"))),
            None => Ok(first),
        }
    };
    Ok((one("thread")?, one("project")?))
}

/// How one task was routed: the decision or the refusal, and the ledger rows of the routing.
async fn route_view(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_route_view(&task_id)?))
}

async fn review_controls(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let mut view = state.core.crew_review_control_view(&task_id).await?;
    if state.remote {
        for lane in &mut view.lanes {
            lane.can_remove = false;
        }
    }
    Ok(Json(view))
}

async fn engine_tiers(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    let mut view = state.core.crew_engine_tier_view().await?;
    view.writable = !state.remote;
    Ok(Json(view))
}

async fn set_engine_tiers(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<serde_json::Value>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_set_engine_tiers(&body).await?))
}

async fn review_consent(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    ApiJson(body): ApiJson<serde_json::Value>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_review_consent(&task_id, &body).await?))
}

async fn remove_review_lane(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    ApiJson(body): ApiJson<serde_json::Value>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state.core.crew_remove_review_lane(&task_id, &body).await?,
    ))
}

/// The default branch this project remembers, the one the checkout reads now, and whether a start would accept it. Read-only.
async fn pinned_ref(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_pinned_ref_view(&project_id).await?))
}

/// A person accepts the default branch's new commit, naming the whole commit the view showed.
async fn pinned_ref_confirm(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    ApiJson(body): ApiJson<wire::CrewPinnedRefConfirm>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state
            .core
            .crew_confirm_pinned_ref(&project_id, &body.commit)
            .await?,
    ))
}

/// A person forgets the remembered default branch; the next start reads the ref the checkout has then.
async fn pinned_ref_reset(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_reset_pinned_ref(&project_id).await?))
}

/// The project's stored worker-provider choice; `same_as_lead` when none. Read-only.
async fn worker_provider(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_worker_provider_view(&project_id)?))
}

/// A person chooses where this project's workers are routed when a brief names no engine.
async fn worker_provider_set(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    ApiJson(body): ApiJson<wire::CrewWorkerProviderSet>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        state
            .core
            .crew_set_worker_provider(&project_id, &body)
            .await?,
    ))
}

fn required(query: &QueryMap, name: &str) -> Result<String, CoreError> {
    query
        .0
        .get(name)
        .cloned()
        .ok_or_else(|| CoreError::Invalid(format!("{name} is required")))
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

/// Applies an execution profile to a crew (docs/execution-profiles-and-library.md): `Person`, a copy, never a link.
async fn apply_profile(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<orbyn_contracts::ApplyCrewProfile>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.apply_profile_to_crew(&body).await?))
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

/// A person's answer for a forge merge that needs one: `{ "action": "reject" | "retry", "seen_revision": n }`.
async fn forge_resolve(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    ApiJson(body): ApiJson<wire::CrewForgeResolve>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_forge_resolve(&task_id, &body).await?))
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
    Ok(Json(state.core.autopilot_merge_queue(page(&query)?).await?))
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

async fn digest_schedule(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_digest_schedule()?))
}

async fn set_digest_schedule(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<wire::AutopilotDigestScheduleInput>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_set_digest_schedule(&body)?))
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

async fn second_engine_notices(
    State(state): State<AppState>,
    query: QueryMap,
) -> ApiResult<impl IntoResponse> {
    let before = query
        .0
        .get("before")
        .map(|value| {
            value
                .parse::<i64>()
                .map_err(|_| CoreError::Invalid("that notice cursor is not valid".into()))
        })
        .transpose()?;
    Ok(Json(
        state
            .core
            .autopilot_second_engine_notices(before, number(&query, "limit")?)
            .await?,
    ))
}

async fn dismiss_second_engine_notice(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<impl IntoResponse> {
    state.core.autopilot_dismiss_second_engine_notice(id)?;
    Ok(Json(serde_json::json!({ "dismissed": true })))
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
    Ok(Json(state.core.background_turn_off(&id).await?))
}

async fn background_delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    state.core.background_delete(&id)?;
    Ok(Json(serde_json::json!({ "deleted": true })))
}

/// The person's Run now: starts one run of an enabled background crew by hand. The core refuses what cannot start, with its
/// plain reason, and returns the run it opened.
async fn background_run_now(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_background_run_now(&id, "user").await?))
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

/// The person's retry of a task that could not start: cancels it and asks its lead to start it again. No body.
async fn retry_elsewhere(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    // The flag first: with it off nothing here exists.
    if !state.core.config.crew_preview {
        return Err(CoreError::NotFound("route".into()).into());
    }
    Ok(Json(state.core.crew_retry_elsewhere(&id, "user").await?))
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
