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
    /// The service ended this computer's session (not the person: a removal or an ended session)
    /// and the person has not been told yet.
    pub ended_notice: bool,
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
    /// A trusted device has to allow this one; `approval` in the view says how the request stands.
    AwaitingApproval,
}
}

wire_enum! {
/// Where a new device's request to sign in stands.
pub enum AccountApprovalState {
    /// Asked; waiting for a trusted device to allow it.
    Waiting,
    /// A trusted device allowed it and shows six digits; this device types them.
    /// `tries_left` is set after wrong digits.
    EnterDigits,
    /// The right digits were typed; finishing the sign-in.
    Approved,
    /// A trusted device said no, or the request was cancelled.
    Denied,
    /// The tries for the digits ran out.
    Locked,
    /// Nobody finished it in ten minutes, counted on this computer's clock.
    TimedOut,
}
}

wire! {
/// This computer's own request to sign in, while a trusted device decides. It never carries
/// the digits: the trusted device shows them and this one types them.
pub struct AccountApprovalView {
    pub state: AccountApprovalState,
    /// Seconds left of the ten minutes, while the request is open.
    #[ts(type = "number")]
    pub expires_in_secs: u64,
    /// Tries left for the digits, after a wrong entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub tries_left: Option<u8>,
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
    /// This device allowed it and showed the digits; the new device has not typed them yet.
    pub allowed: bool,
    #[ts(type = "number")]
    pub expires_in_secs: u64,
}
}

/// What letting a new device in answers with: the six digits to type on the new device, in two
/// groups ("418 207"). They are the trusted device's to show and are never logged.
#[derive(Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../web/src/generated/")]
pub struct AccountApproved {
    pub digits: String,
}

impl fmt::Debug for AccountApproved {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AccountApproved")
            .field("digits", &"[redacted]")
            .finish_non_exhaustive()
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
/// dropped and is never serialised. The web UI builds the request bodies below by hand,
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

/// The password step of sign-up: it creates the account and emails the code. The invite and the
/// email come with it because the earlier steps send nothing.
#[derive(Debug, Deserialize)]
pub struct AccountSignUp {
    pub invite: String,
    pub email: String,
    pub password: SecretText,
}

/// The six digits typed on the new device. They are checked once and never read back or logged.
#[derive(Debug, Deserialize)]
pub struct AccountDigits {
    pub digits: SecretText,
}

wire_enum! {
/// What a "Confirm it's you" confirmation is for. One confirmation is for one action.
pub enum AccountConfirmAction {
    RemoveDevice,
    SignOutEverywhere,
    /// Let a new device sign in; `device_id` is the request's id.
    ApproveDevice,
    /// Join the settings by `use_device` or `use_account`; `policy` says which.
    ConnectSettings,
}
}

wire! {
/// What "Confirm it's you" answers with. The nonce goes back with the action, in a header, so only
/// the window that typed the password can use the confirmation. It is single-use and in memory only.
pub struct AccountConfirmed {
    pub nonce: String,
}
}

/// Gives a confirmation up (the dialog was cancelled): only the nonce that made it can.
#[derive(Debug, Deserialize)]
pub struct AccountConfirmCancel {
    pub nonce: SecretText,
}

/// "Confirm it's you": the password again, for one sensitive action. `device_id` names the
/// device for `remove_device`, and the request for `approve_device`.
#[derive(Debug, Deserialize)]
pub struct AccountConfirm {
    pub password: SecretText,
    pub action: AccountConfirmAction,
    #[serde(default)]
    pub device_id: Option<String>,
    /// For `connect_settings`: the choice this confirmation is for.
    #[serde(default)]
    pub policy: Option<AccountConnectPolicy>,
}
