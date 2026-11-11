//! The optional account: status, sign-up, sign-in, "Confirm it's you", devices, sync
//! control and sign-out. Transport only; `orbyn_core::account` owns every rule.
//!
//! **The re-entry guard.** Removing a device and signing out everywhere each need the
//! password typed again, in the last ten minutes, for that one action (and that one
//! device): a confirmation is used up by the action it was for. Two things keep a new
//! account route from skipping that: the confirmed routes sit in [`confirmed_routes`],
//! whose layers refuse with `reauth_required` before the handler runs, and the core's own
//! action takes a `Confirmed` token that only `require_confirmed` makes, for that action.
//! `routes/account_tests.rs` walks the whole route table: every route under
//! `/api/account` must be a read in [`READS`], open in [`OPEN`] (each with its reason) or
//! refused without the confirmation, and no route elsewhere may name an account, a sign-in,
//! a sign-out or a device.
//!
//! The password reaches this service once, in the body of a request on loopback, and
//! goes straight to the core's `Password`. It is never stored, logged or echoed, and a
//! remote viewer cannot reach any of these routes.

use axum::Json;
use axum::Router;
use axum::extract::{Path, Request, State};
use axum::http::StatusCode;
use axum::middleware::{Next, from_fn_with_state};
use axum::response::Response;
use axum::routing::{delete, get, post};
use orbyn_contracts as wire;
use orbyn_core::CoreError;
use orbyn_core::account::{ConfirmFor, FirstSync, Password};

use crate::http::{ApiFailure, ApiJson, ApiResult, AppState};

/// State-changing account routes that do not need the password again, each with why.
/// Anything else under `/api/account` that changes state must be confirmed.
#[cfg(test)]
pub const OPEN: &[(&str, &str)] = &[
    (
        "POST /api/account/sign-up/invite",
        "before there is an account",
    ),
    (
        "POST /api/account/sign-up/start",
        "before there is an account",
    ),
    (
        "POST /api/account/sign-up/resend",
        "before there is an account",
    ),
    (
        "POST /api/account/sign-up/code",
        "before there is an account",
    ),
    (
        "POST /api/account/sign-up/register",
        "creates the account; the password is the proof",
    ),
    ("POST /api/account/sign-in", "the password is the proof"),
    (
        "POST /api/account/sign-in/second-factor",
        "the second step of signing in",
    ),
    (
        "POST /api/account/sign-in/cancel",
        "gives up a sign-in that is not finished",
    ),
    ("POST /api/account/confirm", "is the confirmation itself"),
    ("POST /api/account/sync/now", "only wakes the worker"),
    (
        "POST /api/account/sync/pause",
        "a choice about this computer",
    ),
    (
        "POST /api/account/sync/resume",
        "a choice about this computer",
    ),
    (
        "POST /api/account/sign-out",
        "signs out of this computer only",
    ),
    (
        "POST /api/account/approvals/{id}/deny",
        "only ever closes a request; it lets nobody in",
    ),
    (
        "POST /api/account/connect",
        "chooses how this computer's own settings join the account's; the account is already signed in",
    ),
    (
        "POST /api/account/connect/later",
        "leaves this computer's settings where they are",
    ),
    ("POST /api/account/connect/seen", "closes a summary"),
];

/// Reads under `/api/account`: they change nothing.
#[cfg(test)]
pub const READS: &[(&str, &str)] = &[
    ("GET /api/account", "the status"),
    ("GET /api/account/devices", "the device list"),
    (
        "GET /api/account/approvals",
        "the new devices waiting for this one",
    ),
    (
        "GET /api/account/sign-in/approval",
        "asks how this computer's own request stands",
    ),
];

pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        .merge(open_routes())
        .merge(confirmed_routes(state.clone()))
        .route_layer(from_fn_with_state(state, local_only))
}

fn open_routes() -> Router<AppState> {
    Router::new()
        .route("/api/account", get(view))
        .route("/api/account/devices", get(devices))
        .route("/api/account/approvals", get(approvals))
        .route("/api/account/approvals/{id}/deny", post(deny_approval))
        .route("/api/account/sign-in/approval", get(sign_in_approval))
        .route("/api/account/connect", post(connect))
        .route("/api/account/connect/later", post(connect_later))
        .route("/api/account/connect/seen", post(connect_seen))
        .route("/api/account/sign-up/invite", post(check_invite))
        .route("/api/account/sign-up/start", post(start_sign_up))
        .route("/api/account/sign-up/resend", post(resend_code))
        .route("/api/account/sign-up/code", post(verify_code))
        .route("/api/account/sign-up/register", post(register))
        .route("/api/account/sign-in", post(sign_in))
        .route("/api/account/sign-in/second-factor", post(second_factor))
        .route("/api/account/sign-in/cancel", post(cancel_sign_in))
        .route("/api/account/confirm", post(confirm))
        .route("/api/account/sync/now", post(sync_now))
        .route("/api/account/sync/pause", post(pause_sync))
        .route("/api/account/sync/resume", post(resume_sync))
        .route("/api/account/sign-out", post(sign_out))
}

/// Every route here needs the password typed again, in the last ten minutes, for that one
/// action (and device), and uses the confirmation up.
fn confirmed_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/api/account/devices/{id}",
            delete(remove_device).route_layer(from_fn_with_state(state.clone(), confirmed_device)),
        )
        .route(
            "/api/account/approvals/{id}/approve",
            post(approve).route_layer(from_fn_with_state(state.clone(), confirmed_approval)),
        )
        .route(
            "/api/account/sign-out-everywhere",
            post(sign_out_everywhere).route_layer(from_fn_with_state(state, confirmed_sign_out)),
        )
}

async fn local_only(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiFailure> {
    if state.remote {
        return Err(ApiFailure::forbidden(
            "local_only",
            "Do this from the computer the app runs on.",
        ));
    }
    Ok(next.run(request).await)
}

/// Looks for a confirmation for this device without using it up; the handler uses it.
async fn confirmed_device(
    State(state): State<AppState>,
    Path(id): Path<String>,
    request: Request,
    next: Next,
) -> Result<Response, ApiFailure> {
    if !state.core.account.available() {
        return Err(CoreError::AccountDisabled.into());
    }
    if !state
        .core
        .account
        .confirmed_for(&ConfirmFor::RemoveDevice(id))
    {
        return Err(CoreError::ReauthRequired.into());
    }
    Ok(next.run(request).await)
}

async fn confirmed_approval(
    State(state): State<AppState>,
    Path(id): Path<String>,
    request: Request,
    next: Next,
) -> Result<Response, ApiFailure> {
    if !state.core.account.available() {
        return Err(CoreError::AccountDisabled.into());
    }
    if !state
        .core
        .account
        .confirmed_for(&ConfirmFor::ApproveDevice(id))
    {
        return Err(CoreError::ReauthRequired.into());
    }
    Ok(next.run(request).await)
}

async fn confirmed_sign_out(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiFailure> {
    if !state.core.account.available() {
        return Err(CoreError::AccountDisabled.into());
    }
    if !state
        .core
        .account
        .confirmed_for(&ConfirmFor::SignOutEverywhere)
    {
        return Err(CoreError::ReauthRequired.into());
    }
    Ok(next.run(request).await)
}

async fn view(State(state): State<AppState>) -> Json<wire::AccountView> {
    Json(state.core.account.view(std::env::consts::OS).await)
}

async fn devices(State(state): State<AppState>) -> ApiResult<Json<Vec<wire::AccountDevice>>> {
    Ok(Json(state.core.account.devices().await?))
}

async fn check_invite(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<wire::AccountInvite>,
) -> ApiResult<StatusCode> {
    state.core.account.check_invite(&body.invite).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn start_sign_up(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<wire::AccountSignUpStart>,
) -> ApiResult<StatusCode> {
    state
        .core
        .account
        .start_sign_up(&body.invite, &body.email)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn resend_code(State(state): State<AppState>) -> ApiResult<StatusCode> {
    state.core.account.resend_code().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn verify_code(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<wire::AccountCode>,
) -> ApiResult<StatusCode> {
    state.core.account.verify_email_code(&body.code).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn register(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<wire::AccountRegister>,
) -> ApiResult<StatusCode> {
    state
        .core
        .account
        .register(Password::new(body.password.expose().to_owned()))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn sign_in(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<wire::AccountSignIn>,
) -> ApiResult<Json<wire::AccountSignInResult>> {
    let outcome = state
        .core
        .account
        .sign_in(
            &body.email,
            Password::new(body.password.expose().to_owned()),
        )
        .await?;
    Ok(Json(wire::AccountSignInResult { outcome }))
}

async fn second_factor(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<wire::AccountCode>,
) -> ApiResult<StatusCode> {
    state.core.account.second_factor(&body.code).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn cancel_sign_in(State(state): State<AppState>) -> ApiResult<StatusCode> {
    state.core.account.cancel_second_factor().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn confirm(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<wire::AccountConfirm>,
) -> ApiResult<StatusCode> {
    let action = match (body.action, body.device_id) {
        (wire::AccountConfirmAction::RemoveDevice, Some(id)) if !id.is_empty() => {
            ConfirmFor::RemoveDevice(id)
        }
        (wire::AccountConfirmAction::RemoveDevice, _) => {
            return Err(CoreError::Invalid("Say which device.".into()).into());
        }
        (wire::AccountConfirmAction::ApproveDevice, Some(id)) if !id.is_empty() => {
            ConfirmFor::ApproveDevice(id)
        }
        (wire::AccountConfirmAction::ApproveDevice, _) => {
            return Err(CoreError::Invalid("Say which request.".into()).into());
        }
        (wire::AccountConfirmAction::SignOutEverywhere, _) => ConfirmFor::SignOutEverywhere,
    };
    state
        .core
        .account
        .confirm(action, Password::new(body.password.expose().to_owned()))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn sync_now(State(state): State<AppState>) -> ApiResult<StatusCode> {
    state.core.account.sync_now().await?;
    Ok(StatusCode::ACCEPTED)
}

async fn pause_sync(State(state): State<AppState>) -> ApiResult<StatusCode> {
    state.core.account.pause_sync().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn resume_sync(State(state): State<AppState>) -> ApiResult<StatusCode> {
    state.core.account.resume_sync().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn sign_out(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<wire::AccountSignOut>,
) -> ApiResult<StatusCode> {
    state.core.account.sign_out(body.discard_unsynced).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn approvals(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<wire::AccountApprovalRequest>>> {
    Ok(Json(state.core.account.approval_requests().await?))
}

async fn sign_in_approval(
    State(state): State<AppState>,
) -> ApiResult<Json<Option<wire::AccountApprovalView>>> {
    Ok(Json(state.core.account.poll_approval().await?))
}

async fn approve(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    let confirmed = state
        .core
        .account
        .require_confirmed(ConfirmFor::ApproveDevice(id.clone()))
        .await?;
    state.core.account.approve(confirmed, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn deny_approval(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<wire::AccountDeny>,
) -> ApiResult<StatusCode> {
    state.core.account.deny(&id, body.codes_differ).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn connect(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<wire::AccountConnectRequest>,
) -> ApiResult<StatusCode> {
    let first = match body.policy {
        wire::AccountConnectPolicy::Combine => FirstSync::Combine,
        wire::AccountConnectPolicy::UseDevice => FirstSync::UseDevice,
        wire::AccountConnectPolicy::UseAccount => FirstSync::UseAccount,
    };
    state.core.account.connect(first).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn connect_later(State(state): State<AppState>) -> ApiResult<StatusCode> {
    state.core.account.connect_later().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn connect_seen(State(state): State<AppState>) -> ApiResult<StatusCode> {
    state.core.account.connect_summary_seen().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_device(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let confirmed = state
        .core
        .account
        .require_confirmed(ConfirmFor::RemoveDevice(id.clone()))
        .await?;
    state.core.account.remove_device(confirmed, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn sign_out_everywhere(State(state): State<AppState>) -> ApiResult<StatusCode> {
    let confirmed = state
        .core
        .account
        .require_confirmed(ConfirmFor::SignOutEverywhere)
        .await?;
    state.core.account.sign_out_everywhere(confirmed).await?;
    Ok(StatusCode::NO_CONTENT)
}
