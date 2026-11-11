//! The optional account: status, sign-up, sign-in, "Confirm it's you", devices, sync
//! control and sign-out. Transport only; `orbyn_core::account` owns every rule.
//!
//! **The re-entry guard.** Removing a device and signing out everywhere need the
//! password typed again in the last ten minutes. Two things keep a new account route
//! from skipping that: the confirmed routes sit in [`confirmed_routes`], whose layer
//! refuses with `reauth_required` before the handler runs, and the core's own action
//! takes a `Confirmed` token that only `require_confirmed` makes. The route table test
//! in `routes/tests.rs` lists every state-changing account route and fails when one is
//! neither in [`OPEN`] (with its reason) nor refused without the confirmation.
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
use orbyn_core::account::Password;

use crate::http::{ApiFailure, ApiJson, ApiResult, AppState};

/// State-changing account routes that do not need the password again, each with why.
/// Anything else that changes state must be confirmed (see `routes/tests.rs`).
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

/// Every route here needs the password typed again in the last ten minutes.
fn confirmed_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/account/devices/{id}", delete(remove_device))
        .route(
            "/api/account/sign-out-everywhere",
            post(sign_out_everywhere),
        )
        .route_layer(from_fn_with_state(state, require_confirmed))
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

async fn require_confirmed(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiFailure> {
    state.core.account.require_confirmed().await?;
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
        .register(Password::new(body.password))
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
        .sign_in(&body.email, Password::new(body.password))
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
    state
        .core
        .account
        .confirm(Password::new(body.password))
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

async fn remove_device(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let confirmed = state.core.account.require_confirmed().await?;
    state.core.account.remove_device(&confirmed, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn sign_out_everywhere(State(state): State<AppState>) -> ApiResult<StatusCode> {
    let confirmed = state.core.account.require_confirmed().await?;
    state.core.account.sign_out_everywhere(&confirmed).await?;
    Ok(StatusCode::NO_CONTENT)
}
