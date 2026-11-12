//! The app's side of the Engine's account calls: sign-up, sign-in, devices and
//! sign-out. It is shaped like the device client's contract
//! ([docs/account.md](../../../../docs/account.md#dependency-on-the-engine)) so the real
//! client can replace the development stand-in. A password crosses this trait as a
//! [`Password`], which prints as `[redacted]` and is never stored.

use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use orbyn_contracts::AccountDevice;

use super::device_api::{DeviceApi, DeviceError};
use crate::error::CoreError;

/// A password on its way to the Engine. It is never stored, logged or formatted, and it is
/// cleared from memory when dropped.
pub struct Password(String);

impl Password {
    pub fn new(text: String) -> Self {
        Self(text)
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl Drop for Password {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.0);
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Password([redacted])")
    }
}

/// The six approval digits, typed on the new device or shown on the trusted one. Like a
/// password it prints as `[redacted]`, is never stored or logged, and is cleared when dropped.
pub struct ApprovalDigits(String);

impl ApprovalDigits {
    pub fn new(text: String) -> Self {
        Self(text)
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl Drop for ApprovalDigits {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.0);
    }
}

impl fmt::Debug for ApprovalDigits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApprovalDigits([redacted])")
    }
}

/// What a signed-in session knows. No key, token or password.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionInfo {
    pub email: String,
    pub device_id: String,
    pub device_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignInOutcome {
    Signed(SessionInfo),
    /// The account has two-factor on: an authenticator or recovery code comes next.
    SecondFactor,
    /// A trusted device has to allow this one, and then shows six digits to type here.
    AwaitingApproval(PendingApproval),
}

/// This device's request to sign in, waiting for a trusted device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingApproval {
    pub request_id: String,
}

/// Where this device's request stands. Ten minutes without an answer is the app's own count.
/// A request that was allowed never signs in by itself: only the right digits do
/// ([`AccountApi::finish_approval`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApprovalStatus {
    Waiting,
    /// A trusted device allowed it and shows six digits; this device has to type them.
    Allowed,
    Denied,
    /// The tries for the digits ran out.
    Locked,
}

/// A new device asking this trusted one to let it in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IncomingApproval {
    pub id: String,
    pub device_name: String,
    /// `planner` or `engineering`.
    pub app: String,
    pub near: Option<String>,
    /// RFC 3339.
    pub asked_at: String,
    /// This device allowed it and showed the digits; the new device has not typed them yet.
    pub allowed: bool,
    /// Seconds the request has left.
    pub expires_in_secs: u64,
}

#[async_trait]
pub trait AccountApi: Send + Sync {
    /// Whether creating an account needs an invite; the server decides.
    fn invite_required(&self) -> bool;
    async fn check_invite(&self, invite: &str) -> Result<(), DeviceError>;
    /// The password step: creates the account and emails the code. Answers the same whether or
    /// not the email has an account.
    async fn start_sign_up(
        &self,
        invite: &str,
        email: &str,
        password: &Password,
    ) -> Result<(), DeviceError>;
    async fn resend_code(&self) -> Result<(), DeviceError>;
    /// The emailed code: the last step, which signs the new account in.
    async fn verify_email_code(&self, code: &str) -> Result<SessionInfo, DeviceError>;
    async fn sign_in(&self, email: &str, password: &Password)
    -> Result<SignInOutcome, DeviceError>;
    /// An authenticator code (six digits) or a recovery code.
    async fn second_factor(&self, code: &str) -> Result<SessionInfo, DeviceError>;
    /// The new device asks again how its request stands.
    async fn approval_status(&self, request_id: &str) -> Result<ApprovalStatus, DeviceError>;
    /// The new device gives its request up.
    async fn cancel_approval(&self, request_id: &str) -> Result<(), DeviceError>;
    /// On a trusted device: the requests waiting for an answer.
    async fn approval_requests(&self) -> Result<Vec<IncomingApproval>, DeviceError>;
    /// On a trusted device: allow the request. It answers with the six digits to show, which the
    /// new device types; the sign-in is not finished until it does.
    async fn approve(&self, request_id: &str) -> Result<ApprovalDigits, DeviceError>;
    /// On a trusted device: refuse, or close a request that was allowed.
    async fn deny(&self, request_id: &str) -> Result<(), DeviceError>;
    /// The new device types the six digits. Right ones sign it in, once. Wrong ones are
    /// [`DeviceError::WrongDigits`] with the tries left, and the last wrong try closes the
    /// request ([`DeviceError::DigitsLocked`]).
    async fn finish_approval(
        &self,
        request_id: &str,
        digits: &ApprovalDigits,
    ) -> Result<SessionInfo, DeviceError>;
    /// Checks the password again for "Confirm it's you".
    async fn verify_password(&self, password: &Password) -> Result<(), DeviceError>;
    async fn devices(&self) -> Result<Vec<AccountDevice>, DeviceError>;
    async fn remove_device(&self, id: &str) -> Result<(), DeviceError>;
    async fn sign_out_everywhere(&self) -> Result<(), DeviceError>;
    /// Ends this device's session on the server. Local sign-out never waits for it.
    async fn sign_out(&self) -> Result<(), DeviceError>;
}

/// What the center talks to: the account calls and the sync adapter.
#[derive(Clone)]
pub struct Backend {
    pub account: Arc<dyn AccountApi>,
    pub device: Arc<dyn DeviceApi>,
}

/// The wire code, message and wait for a client error. None carries a request or a key.
pub fn refusal(error: &DeviceError) -> CoreError {
    let (code, message, retry): (&'static str, &str, Option<u64>) = match error {
        DeviceError::Offline => ("account_offline", "Couldn't reach Orbyn.", None),
        DeviceError::Unauthorized => ("session_ended", "The session ended.", None),
        DeviceError::InviteRequired => ("invite_required", "An invite is needed.", None),
        DeviceError::WrongPassword => ("wrong_password", "Wrong email or password.", None),
        DeviceError::RateLimited { retry_after } => (
            "rate_limited",
            "Too many attempts.",
            Some(retry_after.as_secs()),
        ),
        DeviceError::InvalidCode => ("invalid_code", "That code isn't right.", None),
        DeviceError::DeletionScheduled => ("deletion_scheduled", "Deletion is scheduled.", None),
        DeviceError::ProtectionAdvanced => ("protection_advanced", "Protection is on.", None),
        DeviceError::AccountGone => ("account_gone", "The account is gone.", None),
        DeviceError::RequestGone => ("request_gone", "That request has ended.", None),
        DeviceError::WrongDigits { tries_left } => {
            return CoreError::Account {
                code: "wrong_digits",
                message: "Those digits don't match.".to_owned(),
                retry_after_secs: None,
                tries_left: Some(*tries_left),
            };
        }
        DeviceError::DigitsLocked => (
            "digits_locked",
            "Too many tries. This request is closed.",
            None,
        ),
        DeviceError::ProofFailed => (
            "check_failed",
            "Couldn't finish checking this device.",
            None,
        ),
        DeviceError::Conflict(_)
        | DeviceError::TooLarge
        | DeviceError::InvalidKey
        | DeviceError::Server { .. } => ("account_unavailable", "Orbyn is not answering.", None),
    };
    CoreError::Account {
        code,
        message: message.to_owned(),
        retry_after_secs: retry,
        tries_left: None,
    }
}
