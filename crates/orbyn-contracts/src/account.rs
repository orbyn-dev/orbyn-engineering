//! The optional account: what the windows read and send. A password is only ever a
//! field of a request on loopback; none of these types is read back or logged, and the
//! request types print as `[redacted]`.

use std::fmt;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire_enum! {
/// What the sync line shows.
pub enum AccountSyncKind {
    SignedOut,
    SigningIn,
    Online,
    Syncing,
    Offline,
    /// Refused for now; `retry_after_secs` says how long.
    Throttled,
    Paused,
}
}

wire_enum! {
/// Why sync is paused or the person was signed out.
pub enum AccountReason {
    /// The person paused sync.
    ByPerson,
    ProtectionAdvanced,
    DeletionScheduled,
    InvalidKeyboardFile,
    ZoneFailed,
    TooLarge,
    SessionEnded,
    AccountGone,
}
}

wire! {
pub struct AccountSyncView {
    pub kind: AccountSyncKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reason: Option<AccountReason>,
    /// Seconds since the Unix epoch, when offline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub offline_since: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub retry_after_secs: Option<u64>,
    /// Changes waiting to upload.
    pub waiting: u32,
    /// Settings kept on this computer because they are over a limit.
    pub not_synced: u32,
    pub conflicts: u32,
    /// Seconds since the Unix epoch of the last completed sync, when there was one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub last_synced: Option<i64>,
}
}

wire! {
pub struct AccountSessionView {
    pub email: String,
    pub device_name: String,
}
}

wire! {
pub struct AccountView {
    /// The sign-in gate is on and a backend answers. When false, the window shows
    /// nothing of the account at all.
    pub available: bool,
    /// Creating an account needs an invite while sign-up is invite-only.
    pub invite_required: bool,
    /// The platform the service runs on, for the Windows-only note.
    pub platform: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub session: Option<AccountSessionView>,
    /// A sign-in waits for its two-factor code.
    pub needs_second_factor: bool,
    /// The password was confirmed in the last 10 minutes.
    pub confirmed: bool,
    pub deletion_scheduled: bool,
    pub sync: AccountSyncView,
}
}

wire_enum! {
pub enum AccountSignInOutcome {
    Signed,
    SecondFactor,
}
}

wire! {
pub struct AccountSignInResult {
    pub outcome: AccountSignInOutcome,
}
}

wire! {
pub struct AccountInvite {
    pub invite: String,
}
}

wire! {
pub struct AccountSignUpStart {
    pub invite: String,
    pub email: String,
}
}

wire! {
pub struct AccountCode {
    pub code: String,
}
}

wire! {
pub struct AccountDevice {
    pub id: String,
    pub name: String,
    pub platform: String,
    /// `planner` or `engineering`.
    pub app: String,
    pub trusted_since: String,
    pub last_seen: String,
    pub current: bool,
}
}

wire! {
pub struct AccountSignOut {
    /// Delete the settings changes that have not synced, instead of keeping them.
    #[serde(default)]
    pub discard_unsynced: bool,
}
}

macro_rules! secret_request {
    ($(#[$meta:meta])* pub struct $name:ident { $(pub $field:ident: $ty:ty,)* }) => {
        $(#[$meta])*
        #[derive(Clone, PartialEq, Serialize, Deserialize, TS)]
        #[ts(export, export_to = "../../../web/src/generated/")]
        pub struct $name { $(pub $field: $ty,)* }
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($name), " { [redacted] }"))
            }
        }
    };
}

secret_request! {
/// Sign in. The password goes to the service once, on loopback.
pub struct AccountSignIn {
    pub email: String,
    pub password: String,
}
}

secret_request! {
/// The last step of sign-up: the password for the new account.
pub struct AccountRegister {
    pub password: String,
}
}

secret_request! {
/// "Confirm it's you": the password again, for the sensitive actions.
pub struct AccountConfirm {
    pub password: String,
}
}
