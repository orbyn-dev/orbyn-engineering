//! The optional account: what the windows read and send. A password is only ever a
//! field of a request on loopback; none of these types is read back or logged, and the
//! request types print as `[redacted]`.

use std::fmt;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use zeroize::Zeroize;

wire_enum! {
/// What the sync line shows.
pub enum AccountSyncKind {
    SignedOut,
    SigningIn,
    /// Signed in; this computer's settings are not joined to the account's.
    NotConnected,
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
    /// This computer's own request to sign in, while a trusted device decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub approval: Option<AccountApprovalView>,
    pub connect: AccountConnectState,
    /// Set after the first sync of a joined computer, until the person closes the summary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub connect_summary: Option<AccountConnectSummary>,
    pub deletion_scheduled: bool,
    pub sync: AccountSyncView,
}
}

wire_enum! {
pub enum AccountSignInOutcome {
    Signed,
    SecondFactor,
    /// A trusted device has to allow this one: `approval` in the view has the code to compare.
    AwaitingApproval,
}
}

wire_enum! {
/// Where a new device's request to sign in stands.
pub enum AccountApprovalState {
    /// Asked; waiting for a trusted device.
    Waiting,
    /// Allowed; finishing the sign-in.
    Approved,
    /// A trusted device said no.
    Denied,
    /// A trusted device said the codes were not the same.
    CodesDiffer,
    /// Nobody answered in ten minutes, counted on this computer's clock.
    TimedOut,
}
}

wire! {
/// This computer's own request to sign in, while a trusted device decides.
pub struct AccountApprovalView {
    pub state: AccountApprovalState,
    /// Six digits, shown in two groups: "418 207". The other device shows the same ones.
    pub digits: String,
    /// Seconds left of the ten minutes, while waiting.
    pub expires_in_secs: u64,
}
}

wire! {
/// A new device asking a trusted one to let it in.
pub struct AccountApprovalRequest {
    pub id: String,
    pub device_name: String,
    /// `planner` or `engineering`.
    pub app: String,
    /// Roughly where, such as "Near Melbourne", when the Engine says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub near: Option<String>,
    /// RFC 3339.
    pub asked_at: String,
    pub digits: String,
    pub expires_in_secs: u64,
}
}

wire! {
/// "Codes don't match" closes the request with that reason; "Don't allow" without it.
pub struct AccountDeny {
    #[serde(default)]
    pub codes_differ: bool,
}
}

wire_enum! {
/// How to join this computer's settings with the account's, the first time.
pub enum AccountConnectPolicy {
    /// One setting at a time; the account's newer value wins, and settings only this computer has go up.
    Combine,
    /// This computer's settings win everywhere.
    UseDevice,
    /// The account's settings win; this computer's are not uploaded.
    UseAccount,
}
}

wire_enum! {
/// Whether this computer's settings are joined to the account's.
pub enum AccountConnectState {
    /// Nothing to ask: this computer has no settings of its own, or it is already joined.
    Connected,
    /// Signed in on a computer with settings: Orbyn asks once how to join them.
    Needed,
    /// The person chose Not now: settings stay on this computer until they connect.
    Declined,
}
}

wire! {
/// What the first sync did.
pub struct AccountConnectSummary {
    /// Settings in the joined record.
    pub synced: u32,
}
}

wire! {
pub struct AccountConnectRequest {
    pub policy: AccountConnectPolicy,
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

/// A password in a request body. It prints as `[redacted]`, is cleared from memory when
/// dropped and is never serialised. The web UI builds the three request bodies below by hand,
/// so none of them is exported to TypeScript.
#[derive(Deserialize)]
#[serde(transparent)]
pub struct SecretText(String);

impl SecretText {
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl Drop for SecretText {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for SecretText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

/// Sign in. The password goes to the service once, on loopback.
#[derive(Debug, Deserialize)]
pub struct AccountSignIn {
    pub email: String,
    pub password: SecretText,
}

/// The last step of sign-up: the password for the new account.
#[derive(Debug, Deserialize)]
pub struct AccountRegister {
    pub password: SecretText,
}

wire_enum! {
/// What a "Confirm it's you" confirmation is for. One confirmation is for one action.
pub enum AccountConfirmAction {
    RemoveDevice,
    SignOutEverywhere,
    /// Let a new device sign in; `device_id` is the request's id.
    ApproveDevice,
}
}

/// "Confirm it's you": the password again, for one sensitive action. `device_id` names the
/// device for `remove_device`, and the request for `approve_device`.
#[derive(Debug, Deserialize)]
pub struct AccountConfirm {
    pub password: SecretText,
    pub action: AccountConfirmAction,
    #[serde(default)]
    pub device_id: Option<String>,
}
