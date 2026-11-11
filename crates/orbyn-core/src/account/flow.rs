//! The account's actions: sign-up, sign-in, "Confirm it's you", devices, sync control
//! and sign-out. Every call goes to the [`Backend`]; under the `Off` gate, or with no
//! backend, each answers `account_disabled` before anything else happens. A password
//! passes through here once and is never stored.

use std::time::{Duration, Instant};

use orbyn_contracts::{
    AccountConnectState, AccountConnectSummary, AccountDevice, AccountReason, AccountSessionView,
    AccountSignInOutcome, AccountSyncKind, AccountSyncView, AccountView,
};

use super::api::{Backend, Password, SessionInfo, SignInOutcome, refusal};
use super::codes::{normalize_email_code, normalize_invite};
use super::device_api::DeviceError;
use super::status::{PauseReason, SignOutReason, SyncState};
use super::sync::{SyncHandle, SyncTiming};
use super::{AccountCenter, AccountGate, FirstSync, settings_record};

mod approval;
use crate::error::{CoreError, Result};
pub use approval::APPROVAL_LIFETIME;
use approval::{Approval, Connect};

/// How long a typed password counts for the sensitive actions.
pub const CONFIRM_WINDOW: Duration = Duration::from_secs(10 * 60);
/// Wrong two-factor codes in a row that close the sign-in.
pub const SECOND_FACTOR_TRIES: u8 = 5;

/// The action a "Confirm it's you" confirmation is for. One confirmation is for exactly one
/// action, and for one device when it removes a device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfirmFor {
    RemoveDevice(String),
    SignOutEverywhere,
    /// Let the new device with this request id sign in.
    ApproveDevice(String),
    /// Join this computer's settings to the account's by this choice (never `Combine`, which
    /// needs no password).
    ConnectSettings(FirstSync),
}

/// Proof that the password was typed again, in the last ten minutes, for this one action.
/// Only [`AccountCenter::require_confirmed`] makes one, and it uses the confirmation up. Every
/// account action that removes a device or ends other sessions takes one, so no route can
/// skip it.
#[derive(Debug)]
pub struct Confirmed {
    action: ConfirmFor,
}

/// A confirmation waiting to be used. In memory only, never stored.
pub(super) struct Grant {
    action: ConfirmFor,
    at: Instant,
}

/// What the person knows about their session having ended.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Ended {
    /// They were told, or dismissed the notice.
    seen: bool,
    /// They ended every session themselves, so there is nothing to tell them.
    own: bool,
}

#[derive(Default)]
pub(super) struct Inner {
    session: Option<SessionInfo>,
    approval: Option<Approval>,
    connect: Connect,
    /// The first sync of a joined computer ended, and its summary has not been closed yet.
    summary_pending: bool,
    /// Wrong codes so far, while a sign-in waits for its second factor.
    second_factor: Option<u8>,
    sync: Option<SyncHandle>,
    paused: bool,
    deletion_scheduled: bool,
}

fn invalid(code: &'static str, message: &str) -> CoreError {
    CoreError::Account {
        code,
        message: message.to_owned(),
        retry_after_secs: None,
        tries_left: None,
    }
}

impl AccountCenter {
    fn backend(&self) -> Result<&Backend> {
        match (self.gate, &self.backend) {
            (AccountGate::Off, _) | (_, None) => Err(CoreError::AccountDisabled),
            (_, Some(backend)) => Ok(backend),
        }
    }

    /// The gate is on and a backend answers. Nothing of the account shows otherwise.
    pub fn available(&self) -> bool {
        self.backend().is_ok()
    }

    /// An error from an authenticated call; a session the server ended signs out here.
    async fn guard<T>(&self, result: std::result::Result<T, DeviceError>) -> Result<T> {
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                match error {
                    DeviceError::Unauthorized => {
                        self.end_session(SignOutReason::SessionEnded).await
                    }
                    DeviceError::AccountGone => self.end_session(SignOutReason::AccountGone).await,
                    DeviceError::DeletionScheduled => {
                        self.inner.lock().await.deletion_scheduled = true;
                    }
                    _ => {}
                }
                Err(refusal(&error))
            }
        }
    }

    fn ended_lock(&self) -> std::sync::MutexGuard<'_, Ended> {
        self.ended
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The person was told, or dismissed it: it is not said again for this ended session.
    pub async fn ended_seen(&self) -> Result<()> {
        self.backend()?;
        // Only an ended session can be dismissed: a call while signed in, from a stale tab or an
        // engine, must not hide the next real notice.
        let ended = matches!(
            self.status.current().state,
            SyncState::SignedOut {
                reason: Some(SignOutReason::SessionEnded)
            }
        );
        if ended {
            self.ended_lock().seen = true;
        }
        Ok(())
    }

    async fn end_session(&self, reason: SignOutReason) {
        let mut inner = self.inner.lock().await;
        self.forget(&mut inner).await;
        self.status.update(|status| {
            status.state = SyncState::SignedOut {
                reason: Some(reason),
            }
        });
    }

    /// Stops sync and forgets the session. Settings and the outbox stay.
    async fn forget(&self, inner: &mut Inner) {
        if let Some(handle) = inner.sync.take() {
            handle.stop().await;
        }
        *inner = Inner::default();
        self.forget_confirmation();
    }

    pub async fn view(&self, platform: &str) -> AccountView {
        let backend = self.backend().ok();
        let inner = self.inner.lock().await;
        let status = self.status.current();
        let (kind, reason, offline_since, retry) = match &status.state {
            SyncState::SignedOut { reason } => (
                AccountSyncKind::SignedOut,
                reason.map(|reason| match reason {
                    SignOutReason::SessionEnded => AccountReason::SessionEnded,
                    SignOutReason::AccountGone => AccountReason::AccountGone,
                }),
                None,
                None,
            ),
            SyncState::SigningIn => (AccountSyncKind::SigningIn, None, None, None),
            SyncState::NotConnected => (AccountSyncKind::NotConnected, None, None, None),
            SyncState::Online => (AccountSyncKind::Online, None, None, None),
            SyncState::Syncing => (AccountSyncKind::Syncing, None, None, None),
            SyncState::Offline { since_unix } => {
                (AccountSyncKind::Offline, None, Some(*since_unix), None)
            }
            SyncState::Throttled { retry_after_secs } => (
                AccountSyncKind::Throttled,
                None,
                None,
                Some(*retry_after_secs),
            ),
            SyncState::Paused { reason } => (
                AccountSyncKind::Paused,
                Some(match reason {
                    PauseReason::ByPerson => AccountReason::ByPerson,
                    PauseReason::ProtectionAdvanced => AccountReason::ProtectionAdvanced,
                    PauseReason::DeletionScheduled => AccountReason::DeletionScheduled,
                    PauseReason::InvalidKeyboardFile => AccountReason::InvalidKeyboardFile,
                    PauseReason::ZoneFailed => AccountReason::ZoneFailed,
                    PauseReason::TooLarge => AccountReason::TooLarge,
                }),
                None,
                None,
            ),
        };
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        AccountView {
            available: backend.is_some(),
            invite_required: backend.is_some_and(|b| b.account.invite_required()),
            platform: platform.to_owned(),
            session: inner.session.as_ref().map(|session| AccountSessionView {
                email: session.email.clone(),
                device_name: session.device_name.clone(),
            }),
            needs_second_factor: inner.second_factor.is_some(),
            approval: inner
                .approval
                .as_ref()
                .map(|approval| self.approval_view(approval)),
            ended_notice: matches!(
                status.state,
                SyncState::SignedOut {
                    reason: Some(SignOutReason::SessionEnded)
                }
            ) && {
                let ended = self.ended_lock();
                !ended.seen && !ended.own
            },
            connect: match inner.connect {
                Connect::Connected => AccountConnectState::Connected,
                Connect::Needed => AccountConnectState::Needed,
                Connect::Declined => AccountConnectState::Declined,
            },
            connect_summary: (inner.summary_pending
                && matches!(status.state, SyncState::Online)
                && status.waiting == 0)
                .then(|| AccountConnectSummary {
                    synced: u32::try_from(
                        self.store
                            .account_field_count(settings_record::ZONE)
                            .unwrap_or(0),
                    )
                    .unwrap_or(u32::MAX),
                }),
            deletion_scheduled: inner.deletion_scheduled
                || reason == Some(AccountReason::DeletionScheduled),
            sync: AccountSyncView {
                kind,
                reason,
                offline_since,
                retry_after_secs: retry,
                waiting: count(status.waiting),
                not_synced: count(status.not_synced),
                conflicts: count(status.conflicts),
                last_synced: status.last_synced,
            },
        }
    }

    // Sign-up

    pub async fn check_invite(&self, invite: &str) -> Result<()> {
        let backend = self.backend()?;
        let invite = normalize_invite(invite)
            .ok_or_else(|| invalid("invalid_invite", "That invite code isn't right."))?;
        backend
            .account
            .check_invite(&invite)
            .await
            .map_err(|e| refusal(&e))
    }

    /// `invite` may be empty only while sign-up is open; the server decides.
    pub async fn start_sign_up(&self, invite: &str, email: &str) -> Result<()> {
        let backend = self.backend()?;
        let invite = if invite.trim().is_empty() && !backend.account.invite_required() {
            String::new()
        } else {
            normalize_invite(invite)
                .ok_or_else(|| invalid("invalid_invite", "That invite code isn't right."))?
        };
        let email = clean_email(email)?;
        backend
            .account
            .start_sign_up(&invite, &email)
            .await
            .map_err(|e| refusal(&e))
    }

    pub async fn resend_code(&self) -> Result<()> {
        self.backend()?
            .account
            .resend_code()
            .await
            .map_err(|e| refusal(&e))
    }

    pub async fn verify_email_code(&self, code: &str) -> Result<()> {
        let backend = self.backend()?;
        let code = normalize_email_code(code)
            .ok_or_else(|| invalid("invalid_code", "That code isn't right."))?;
        backend
            .account
            .verify_email_code(&code)
            .await
            .map_err(|e| refusal(&e))
    }

    pub async fn register(&self, password: Password) -> Result<()> {
        let backend = self.backend()?;
        let session = backend
            .account
            .register(&password)
            .await
            .map_err(|e| refusal(&e))?;
        self.established(session, false).await
    }

    // Sign-in

    pub async fn sign_in(&self, email: &str, password: Password) -> Result<AccountSignInOutcome> {
        let backend = self.backend()?;
        let email = clean_email(email)?;
        self.status.update(|s| s.state = SyncState::SigningIn);
        let outcome = backend.account.sign_in(&email, &password).await;
        match outcome {
            Ok(SignInOutcome::Signed(session)) => {
                self.established(session, true).await?;
                Ok(AccountSignInOutcome::Signed)
            }
            Ok(SignInOutcome::SecondFactor) => {
                self.inner.lock().await.second_factor = Some(0);
                Ok(AccountSignInOutcome::SecondFactor)
            }
            Ok(SignInOutcome::AwaitingApproval(pending)) => {
                self.inner.lock().await.approval = Some(Approval {
                    request_id: pending.request_id,
                    digits: pending.digits,
                    asked: Instant::now(),
                    answer: None,
                });
                Ok(AccountSignInOutcome::AwaitingApproval)
            }
            Err(error) => {
                self.status
                    .update(|s| s.state = SyncState::SignedOut { reason: None });
                Err(refusal(&error))
            }
        }
    }

    /// A six-digit authenticator code or a recovery code. The fifth wrong one in a row
    /// closes the sign-in, which starts again from the password.
    pub async fn second_factor(&self, code: &str) -> Result<()> {
        let backend = self.backend()?;
        if self.inner.lock().await.second_factor.is_none() {
            return Err(invalid("sign_in_closed", "Start signing in again."));
        }
        match backend.account.second_factor(code).await {
            Ok(session) => self.established(session, true).await,
            Err(DeviceError::InvalidCode) => {
                let mut inner = self.inner.lock().await;
                let wrong = inner.second_factor.map_or(1, |n| n + 1);
                if wrong >= SECOND_FACTOR_TRIES {
                    inner.second_factor = None;
                    drop(inner);
                    self.status
                        .update(|s| s.state = SyncState::SignedOut { reason: None });
                    return Err(invalid("sign_in_closed", "Too many wrong codes."));
                }
                inner.second_factor = Some(wrong);
                Err(CoreError::Account {
                    code: "invalid_code",
                    message: format!(
                        "That code isn't right. {} tries left.",
                        SECOND_FACTOR_TRIES - wrong
                    ),
                    retry_after_secs: None,
                    tries_left: Some(SECOND_FACTOR_TRIES - wrong),
                })
            }
            Err(error) => Err(refusal(&error)),
        }
    }

    /// Gives up the sign-in that waits for its code or for a trusted device: the person is
    /// back at the password, or at the Account page.
    pub async fn cancel_second_factor(&self) -> Result<()> {
        let backend = self.backend()?.clone();
        let mut inner = self.inner.lock().await;
        let asked = inner.approval.take();
        if inner.second_factor.take().is_some() || asked.is_some() {
            drop(inner);
            if let Some(Approval {
                request_id,
                answer: None,
                ..
            }) = asked
            {
                let _ = backend.account.cancel_approval(&request_id).await;
            }
            self.status
                .update(|s| s.state = SyncState::SignedOut { reason: None });
        }
        Ok(())
    }

    /// Signed in: keep what the session knows and start the settings worker. Signing in is
    /// not a confirmation: each sensitive action asks for the password on its own.
    ///
    /// `ask` is true for a sign-in: on a computer that has settings and never joined this account
    /// before, sync waits until the person says how to join them. A new account has nothing to
    /// join, and a computer that joined before simply carries on.
    async fn established(&self, session: SessionInfo, ask: bool) -> Result<()> {
        let mut inner = self.inner.lock().await;
        self.established_in(&mut inner, session, ask).await
    }

    async fn established_in(
        &self,
        inner: &mut Inner,
        session: SessionInfo,
        ask: bool,
    ) -> Result<()> {
        let backend = self.backend()?.clone();
        let zone = settings_record::ZONE;
        // This data folder joined another account before: that account's record and unsent
        // changes must not follow the person into this one.
        // The key is the email, compared without case or edge spaces, until the real client
        // gives an account id (`SessionInfo` has none yet). A row from before the owner was
        // recorded belongs to whoever signs in now.
        if self.store.account_known(zone)? {
            match self.store.account_owner(zone)? {
                Some(owner) if owner.trim().eq_ignore_ascii_case(session.email.trim()) => {}
                Some(_) => self.store.account_reset(zone)?,
                None => self.store.account_claim(zone, &session.email)?,
            }
        }
        *self.ended_lock() = Ended::default();
        let ask = ask && !self.store.account_known(zone)? && self.has_settings()?;
        if let Some(old) = inner.sync.take() {
            old.stop().await;
        }
        let email = session.email.clone();
        inner.session = Some(session);
        inner.second_factor = None;
        inner.approval = None;
        inner.paused = false;
        inner.deletion_scheduled = false;
        if ask {
            inner.connect = Connect::Needed;
            self.status.update(|s| s.state = SyncState::NotConnected);
        } else {
            inner.connect = Connect::Connected;
            inner.sync = Some(self.start_settings_sync(backend.device, SyncTiming::default())?);
            self.store.account_claim(zone, &email)?;
        }
        Ok(())
    }

    // Confirm it's you

    fn grant(&self) -> std::sync::MutexGuard<'_, Option<Grant>> {
        self.confirmed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Whether a fresh confirmation for exactly this action is waiting. It does not use it up:
    /// the route layer asks this before the handler runs, and the handler uses it.
    pub fn confirmed_for(&self, action: &ConfirmFor) -> bool {
        self.grant()
            .as_ref()
            .is_some_and(|grant| grant.action == *action && grant.at.elapsed() < CONFIRM_WINDOW)
    }

    /// The only way to get a [`Confirmed`]. Needs a signed-in account and a fresh confirmation
    /// for this action, which it uses up: the next action asks again, even if this one fails.
    pub async fn require_confirmed(&self, action: ConfirmFor) -> Result<Confirmed> {
        self.backend()?;
        if self.inner.lock().await.session.is_none() {
            return Err(CoreError::ReauthRequired);
        }
        let mut waiting = self.grant();
        let fresh = waiting
            .as_ref()
            .is_some_and(|grant| grant.action == action && grant.at.elapsed() < CONFIRM_WINDOW);
        if fresh {
            *waiting = None;
            Ok(Confirmed { action })
        } else {
            Err(CoreError::ReauthRequired)
        }
    }

    /// Checks the password with the Engine and, when it is right, keeps one confirmation for
    /// `action` for ten minutes. A later confirmation replaces it. The Engine applies its own
    /// recent-password rule to the action too (orbyn-engine#120); this check is the re-login it
    /// counts.
    pub async fn confirm(&self, action: ConfirmFor, password: Password) -> Result<()> {
        let backend = self.backend()?;
        if self.inner.lock().await.session.is_none() {
            return Err(CoreError::ReauthRequired);
        }
        let checked = backend.account.verify_password(&password).await;
        self.guard(checked).await?;
        *self.grant() = Some(Grant {
            action,
            at: Instant::now(),
        });
        Ok(())
    }

    /// Drops any waiting confirmation, as a restart, a sign-out and a sign-out everywhere do.
    pub fn forget_confirmation(&self) {
        *self.grant() = None;
    }

    // Devices

    pub async fn devices(&self) -> Result<Vec<AccountDevice>> {
        let backend = self.backend()?;
        if self.inner.lock().await.session.is_none() {
            return Err(CoreError::ReauthRequired);
        }
        let list = backend.account.devices().await;
        self.guard(list).await
    }

    pub async fn remove_device(&self, confirmed: Confirmed, id: &str) -> Result<()> {
        if confirmed.action != ConfirmFor::RemoveDevice(id.to_owned()) {
            return Err(CoreError::ReauthRequired);
        }
        let result = self.backend()?.account.remove_device(id).await;
        self.guard(result).await
    }

    /// Ends every session of the account, this one too. The Engine refuses while
    /// deletion is scheduled.
    pub async fn sign_out_everywhere(&self, confirmed: Confirmed) -> Result<()> {
        if confirmed.action != ConfirmFor::SignOutEverywhere {
            return Err(CoreError::ReauthRequired);
        }
        let result = self.backend()?.account.sign_out_everywhere().await;
        self.guard(result).await?;
        // The person did this: no "you were signed out" for it.
        self.ended_lock().own = true;
        self.end_session(SignOutReason::SessionEnded).await;
        Ok(())
    }

    /// Signs out of this computer only. It never waits for the network: the server is
    /// told once, and a failure changes nothing here.
    pub async fn sign_out(&self, discard_unsynced: bool) -> Result<()> {
        let backend = self.backend()?.clone();
        let mut inner = self.inner.lock().await;
        if inner.session.is_none() {
            return Ok(());
        }
        self.forget(&mut inner).await;
        drop(inner);
        if discard_unsynced {
            self.store.account_discard_outbox(settings_record::ZONE)?;
        }
        let _ = backend.account.sign_out().await;
        self.status.publish(super::AccountStatus::default());
        Ok(())
    }

    // Sync control

    /// Sends and fetches at once: wakes the worker.
    pub async fn sync_now(&self) -> Result<()> {
        self.backend()?;
        if self.inner.lock().await.sync.is_some() {
            self.store.account_poke();
        }
        Ok(())
    }

    pub async fn pause_sync(&self) -> Result<()> {
        self.backend()?;
        let mut inner = self.inner.lock().await;
        if let Some(handle) = inner.sync.take() {
            handle.stop().await;
            inner.paused = true;
            self.status.update(|s| {
                s.state = SyncState::Paused {
                    reason: PauseReason::ByPerson,
                }
            });
        }
        Ok(())
    }

    pub async fn resume_sync(&self) -> Result<()> {
        let backend = self.backend()?.clone();
        let mut inner = self.inner.lock().await;
        if inner.paused && inner.session.is_some() {
            inner.paused = false;
            inner.sync = Some(self.start_settings_sync(backend.device, SyncTiming::default())?);
        }
        Ok(())
    }
}

fn clean_email(email: &str) -> Result<String> {
    let email = email.trim();
    let sane = email.len() <= 254
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.contains('.') && !email.contains(char::is_whitespace)
        });
    if sane {
        Ok(email.to_owned())
    } else {
        Err(CoreError::Invalid("Enter a valid email address.".into()))
    }
}
