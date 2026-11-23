//! A new device signing in with a trusted device's help, the trusted side of that, and joining
//! this computer's settings to the account's the first time. Transport-free: the routes call
//! these, and the stand-in plays the account server in tests.

use std::time::{Duration, Instant};

use orbyn_contracts::{
    AccountApprovalRequest, AccountApprovalState, AccountApprovalView, AccountApproved,
};

use super::super::api::{ApprovalDigits, ApprovalStatus, SessionInfo, refusal};
use super::super::device_api::DeviceError;
use super::super::digits;
use super::super::status::SyncState;
use super::super::sync::SyncTiming;
use super::super::{AccountCenter, FirstSync, settings_record};
use super::{ConfirmFor, Confirmed};
use crate::error::{CoreError, Result};

/// How long a request to sign in waits for a trusted device, counted on this computer's clock.
pub const APPROVAL_LIFETIME: Duration = Duration::from_secs(10 * 60);

/// This computer's own request to sign in, while a trusted device decides.
///
/// It never holds the digits: a trusted device shows them after it allows the request, and the
/// person types them here. What it keeps is how many tries were used, so the app stays bounded
/// even if the Engine did not count.
pub(super) struct Approval {
    pub(super) request_id: String,
    /// Tells this request from a later one that reuses the id.
    pub(super) serial: u64,
    pub(super) asked: Instant,
    /// A trusted device allowed the request: the digits are due.
    pub(super) allowed: bool,
    /// Tries left after a wrong entry, as the Engine counted them (never above the app's own).
    pub(super) tries_left: Option<u8>,
    /// Entries sent to the Engine, right or wrong: the app's own bound on tries.
    pub(super) spent_tries: u8,
    /// Set once the request has an end; an open request has none.
    pub(super) answer: Option<AccountApprovalState>,
}

/// Whether this computer's settings are joined to the account's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Connect {
    #[default]
    Connected,
    Needed,
    Declined,
}

impl AccountCenter {
    // A new device waiting for a trusted one

    fn approval_life(&self) -> Duration {
        *self
            .approval_life
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(super) fn approval_view(&self, approval: &Approval) -> AccountApprovalView {
        let left = self
            .approval_life()
            .saturating_sub(approval.asked.elapsed());
        let open = matches!(
            approval.answer,
            None | Some(AccountApprovalState::EnterDigits)
        );
        AccountApprovalView {
            state: approval.answer.unwrap_or(if approval.allowed {
                AccountApprovalState::EnterDigits
            } else {
                AccountApprovalState::Waiting
            }),
            expires_in_secs: if open { left.as_secs() } else { 0 },
            tries_left: if approval.answer.is_none() && approval.allowed {
                approval.tries_left
            } else {
                None
            },
        }
    }

    /// Asks how this computer's request stands. A trusted device's refusal ends it, and so does
    /// nobody finishing it for ten minutes on this computer's clock. An allowed request does not
    /// sign in: it asks for the digits ([`Self::submit_digits`]), and only the right ones finish.
    pub async fn poll_approval(&self) -> Result<Option<AccountApprovalView>> {
        let backend = self.backend()?.clone();
        let (id, serial, expired) = {
            let inner = self.inner.lock().await;
            let Some(approval) = &inner.approval else {
                return Ok(None);
            };
            if approval.answer.is_some() {
                return Ok(Some(self.approval_view(approval)));
            }
            (
                approval.request_id.clone(),
                approval.serial,
                approval.asked.elapsed() >= self.approval_life(),
            )
        };
        if expired {
            return Ok(self
                .end_request(&backend, &id, serial, AccountApprovalState::TimedOut)
                .await);
        }
        let status = backend.account.approval_status(&id).await;
        // The server ended the request on its own clock: it closes here as expired too, rather than
        // as one a trusted device refused.
        if let Err(DeviceError::ApprovalExpired) = &status {
            return Ok(self
                .end_request(&backend, &id, serial, AccountApprovalState::TimedOut)
                .await);
        }
        match status {
            Ok(ApprovalStatus::Waiting) => {}
            Ok(ApprovalStatus::Allowed) => {
                let mut inner = self.inner.lock().await;
                if let Some(approval) = inner
                    .approval
                    .as_mut()
                    .filter(|a| a.request_id == id && a.answer.is_none())
                {
                    approval.allowed = true;
                }
            }
            Ok(ApprovalStatus::Denied) => {
                return Ok(self
                    .close_approval(&id, serial, AccountApprovalState::Denied)
                    .await);
            }
            Ok(ApprovalStatus::Locked) => {
                return Ok(self
                    .close_approval(&id, serial, AccountApprovalState::Locked)
                    .await);
            }
            Err(DeviceError::RequestGone | DeviceError::ApprovalRefused) => {
                return Ok(self
                    .close_approval(&id, serial, AccountApprovalState::Denied)
                    .await);
            }
            Err(error) => return Err(refusal(&error)),
        }
        Ok(self
            .inner
            .lock()
            .await
            .approval
            .as_ref()
            .map(|approval| self.approval_view(approval)))
    }

    /// The six digits typed on this computer for its allowed request. Only the right digits sign
    /// in, and never automatically: an allowed request waits for them. A wrong entry keeps the
    /// request open and reports the tries left; the last wrong one closes it. The digits are
    /// checked once by the account server, bounded here too, and never stored, logged or returned.
    pub async fn submit_digits(
        &self,
        typed: ApprovalDigits,
    ) -> Result<Option<AccountApprovalView>> {
        let backend = self.backend()?.clone();
        let typed = ApprovalDigits::new(digits::clean(typed.expose()));
        if !digits::is_well_formed(typed.expose()) {
            return Err(CoreError::Account {
                code: "invalid_digits",
                message: "Type the six digits.".to_owned(),
                retry_after_secs: None,
                tries_left: None,
            });
        }
        let (id, serial) = {
            let mut inner = self.inner.lock().await;
            let life = self.approval_life();
            let Some(approval) = inner.approval.as_mut() else {
                return Ok(None);
            };
            if approval.answer.is_some() {
                return Ok(Some(self.approval_view(approval)));
            }
            if !approval.allowed {
                // The digits are only asked for once a trusted device allowed the request.
                return Err(CoreError::Account {
                    code: "request_not_allowed",
                    message: "A trusted device has not allowed this yet.".to_owned(),
                    retry_after_secs: None,
                    tries_left: None,
                });
            }
            let id = approval.request_id.clone();
            let serial = approval.serial;
            if approval.asked.elapsed() >= life {
                drop(inner);
                return Ok(self
                    .end_request(&backend, &id, serial, AccountApprovalState::TimedOut)
                    .await);
            }
            if approval.spent_tries >= digits::TRIES {
                drop(inner);
                return Ok(self
                    .end_request(&backend, &id, serial, AccountApprovalState::Locked)
                    .await);
            }
            // The try is taken before the call, so two entries at once cannot share one.
            approval.spent_tries += 1;
            (id, serial)
        };
        match backend.account.finish_approval(&id, &typed).await {
            Ok(session) => self.finish_approval(&id, serial, session).await,
            Err(DeviceError::WrongDigits { tries_left }) => {
                let mut inner = self.inner.lock().await;
                let Some(approval) = inner
                    .approval
                    .as_mut()
                    .filter(|a| a.request_id == id && a.answer.is_none())
                else {
                    return Ok(None);
                };
                let own = digits::TRIES.saturating_sub(approval.spent_tries);
                let left = tries_left.min(own);
                if left == 0 {
                    drop(inner);
                    return Ok(self
                        .end_request(&backend, &id, serial, AccountApprovalState::Locked)
                        .await);
                }
                approval.tries_left = Some(left);
                Ok(Some(self.approval_view(approval)))
            }
            Err(DeviceError::DigitsLocked) => Ok(self
                .close_approval(&id, serial, AccountApprovalState::Locked)
                .await),
            Err(DeviceError::RequestGone | DeviceError::ApprovalRefused) => Ok(self
                .close_approval(&id, serial, AccountApprovalState::Denied)
                .await),
            // The request's own ten minutes ran out on the server's clock: it ends as expired, not
            // as one a person or a trusted device ended, so the screen does not say "Denied or
            // cancelled".
            Err(DeviceError::ApprovalExpired) => Ok(self
                .end_request(&backend, &id, serial, AccountApprovalState::TimedOut)
                .await),
            Err(error) => {
                // A try is reserved under the lock that enforces the limit, and given back at most
                // once, by the request that reserved it and only while that request is still open.
                // An error the server answered keeps the try: a 5xx, a rate limit and a timeout it
                // may have taken all reached it, and its `tries_left` is the count that applies. Only
                // an error that never left this computer gives the try back, and a reservation that
                // was refused took nothing, so a failed reserve leaves no try to give back.
                if !error.reached_the_server() {
                    let mut inner = self.inner.lock().await;
                    if let Some(approval) = inner.approval.as_mut().filter(|approval| {
                        approval.request_id == id
                            && approval.serial == serial
                            && approval.answer.is_none()
                    }) {
                        approval.spent_tries = approval.spent_tries.saturating_sub(1);
                    }
                }
                Err(refusal(&error))
            }
        }
    }

    /// The tries this computer has sent to the server for its own request, so a test can check the
    /// count the app keeps itself.
    #[cfg(test)]
    pub(crate) async fn spent_digit_tries(&self) -> Option<u8> {
        self.inner
            .lock()
            .await
            .approval
            .as_ref()
            .map(|approval| approval.spent_tries)
    }

    /// Signs in on an allowed request, under one lock: if the person gave the request up
    /// meanwhile, nobody is signed in here and the Engine is told to forget the session. Only
    /// the request `serial` named is signed in or ended: a later sign-in that reuses the id is
    /// left alone, and its session is not ended either.
    pub(crate) async fn finish_approval(
        &self,
        id: &str,
        serial: u64,
        session: SessionInfo,
    ) -> Result<Option<AccountApprovalView>> {
        let backend = self.backend()?.clone();
        let mut inner = self.inner.lock().await;
        let current = inner.approval.as_ref().is_some_and(|approval| {
            approval.request_id == id && approval.serial == serial && approval.answer.is_none()
        });
        if !current {
            drop(inner);
            return self.discard_answer(&backend, serial, session).await;
        }
        let mut shown = match inner.approval.as_mut() {
            Some(approval) => {
                approval.answer = Some(AccountApprovalState::Approved);
                self.approval_view(approval)
            }
            None => return Ok(None),
        };
        self.established_in(&mut inner, session, true).await?;
        shown.state = AccountApprovalState::Approved;
        Ok(Some(shown))
    }

    /// The answer to a request that is no longer the live one: a session this computer already
    /// holds is kept, one it did not make is ended. The one-at-a-time gate keeps this away from a
    /// sign-in on its way, and a later request that reuses the id is not touched at all.
    async fn discard_answer(
        &self,
        backend: &super::Backend,
        serial: u64,
        session: SessionInfo,
    ) -> Result<Option<AccountApprovalView>> {
        let _one_at_a_time = self.cancelling.lock().await;
        let mut inner = self.inner.lock().await;
        if inner
            .approval
            .as_ref()
            .is_some_and(|approval| approval.serial != serial && approval.answer.is_none())
        {
            // A later sign-in reused the id and is still open: its answer is its own, and
            // ending this session now would wipe the Engine's state for it. The Engine may
            // already hold this session, so it is ended when that sign-in ends without one.
            inner.owed_sign_out = true;
            return Ok(None);
        }
        // Another look at the same allowed request may have signed in first: that session
        // is this one, and must stay. Only a session this request did not make is ended.
        let same = inner.session.as_ref().is_some_and(|current| {
            current.device_id == session.device_id
                && current
                    .email
                    .trim()
                    .eq_ignore_ascii_case(session.email.trim())
        });
        drop(inner);
        if !same {
            let _ = backend.account.sign_out().await;
        }
        Ok(None)
    }

    /// The serial of this computer's own request, so a test can answer the one before it.
    #[cfg(test)]
    pub(crate) async fn approval_serial(&self) -> Option<u64> {
        self.inner
            .lock()
            .await
            .approval
            .as_ref()
            .map(|approval| approval.serial)
    }

    /// Whether a sign-out for a late answer is still owed, so a test can see it settled.
    #[cfg(test)]
    pub(crate) async fn owed_sign_out(&self) -> bool {
        self.inner.lock().await.owed_sign_out
    }

    /// Marks a sign-out as owed, so a test can see the person's own sign-out settle it.
    #[cfg(test)]
    pub(crate) async fn owe_sign_out(&self) {
        self.inner.lock().await.owed_sign_out = true;
    }

    /// Ends this computer's own request on the service and closes it with `state`, one cancel
    /// at a time and only if it is still the request `serial` named: a sign-in that reused the
    /// id meanwhile is not cancelled or closed by an older request's end.
    async fn end_request(
        &self,
        backend: &super::Backend,
        id: &str,
        serial: u64,
        state: AccountApprovalState,
    ) -> Option<AccountApprovalView> {
        let _one_at_a_time = self.cancelling.lock().await;
        let current = {
            let inner = self.inner.lock().await;
            inner
                .approval
                .as_ref()
                .is_some_and(|a| a.serial == serial && a.answer.is_none())
        };
        if !current {
            return self
                .inner
                .lock()
                .await
                .approval
                .as_ref()
                .map(|a| self.approval_view(a));
        }
        let _ = backend.account.cancel_approval(id).await;
        self.close_approval_held(backend, id, serial, state).await
    }

    /// The request ended without signing in: this computer is signed out, and the screen
    /// keeps saying why until the person leaves it. Only the request `serial` named is closed,
    /// so an older end never closes a sign-in that reused the id.
    async fn close_approval(
        &self,
        id: &str,
        serial: u64,
        state: AccountApprovalState,
    ) -> Option<AccountApprovalView> {
        let backend = self.backend().ok()?.clone();
        let _one_at_a_time = self.cancelling.lock().await;
        self.close_approval_held(&backend, id, serial, state).await
    }

    /// [`Self::close_approval`] for a caller that already holds the one-at-a-time gate. When a
    /// late answer left a session in the Engine meanwhile, it is ended now: nothing needs the
    /// Engine's state any more.
    async fn close_approval_held(
        &self,
        backend: &super::Backend,
        id: &str,
        serial: u64,
        state: AccountApprovalState,
    ) -> Option<AccountApprovalView> {
        let (shown, owed) = {
            let mut inner = self.inner.lock().await;
            let approval = inner
                .approval
                .as_mut()
                .filter(|approval| approval.request_id == id && approval.serial == serial)?;
            approval.answer = Some(state);
            let shown = self.approval_view(approval);
            (shown, std::mem::take(&mut inner.owed_sign_out))
        };
        if owed {
            let _ = backend.account.sign_out().await;
        }
        self.status
            .update(|s| s.state = SyncState::SignedOut { reason: None });
        Some(shown)
    }

    // On a trusted device

    /// The new devices waiting for this one to let them in.
    pub async fn approval_requests(&self) -> Result<Vec<AccountApprovalRequest>> {
        let backend = self.backend()?;
        if self.inner.lock().await.session.is_none() {
            return Err(CoreError::ReauthRequired);
        }
        let list = backend.account.approval_requests().await;
        let list = self.guard(list).await?;
        Ok(list
            .into_iter()
            .map(|request| AccountApprovalRequest {
                id: request.id,
                device_name: request.device_name,
                app: request.app,
                near: request.near,
                asked_at: request.asked_at,
                allowed: request.allowed,
                expires_in_secs: request.expires_in_secs,
            })
            .collect())
    }

    /// Allows the new device and answers with the six digits to show for it. Needs a
    /// confirmation for exactly this request. Allowing does not sign the new device in: it does
    /// that by typing the digits, which this device never sees again.
    pub async fn approve(&self, confirmed: Confirmed, id: &str) -> Result<AccountApproved> {
        if confirmed.action != ConfirmFor::ApproveDevice(id.to_owned()) {
            return Err(CoreError::ReauthRequired);
        }
        let backend = self.backend()?;
        let result = backend.account.approve(id).await;
        let left = backend.account.time_left(id).map(|left| left.as_secs());
        let shown = self.guard(result).await?;
        let digits = shown.expose();
        Ok(AccountApproved {
            expires_in_secs: left,
            digits: if digits.len() == digits::LEN {
                format!("{} {}", &digits[..3], &digits[3..])
            } else {
                digits.to_owned()
            },
        })
    }

    /// Refuses the new device, or closes a request that was allowed. This only ever closes a
    /// request, so it needs no confirmation.
    pub async fn deny(&self, id: &str) -> Result<()> {
        let backend = self.backend()?;
        if self.inner.lock().await.session.is_none() {
            return Err(CoreError::ReauthRequired);
        }
        let result = backend.account.deny(id).await;
        self.guard(result).await
    }

    // Joining this computer's settings to the account's

    /// Joins this computer's settings to the account's. `Combine` needs nothing more; the two
    /// choices that overwrite values somewhere (`UseDevice` on every other device, `UseAccount`
    /// here) need a confirmation for exactly that choice.
    pub async fn connect(&self, first: FirstSync, confirmed: Option<Confirmed>) -> Result<()> {
        let backend = self.backend()?.clone();
        if first != FirstSync::Combine
            && confirmed.is_none_or(|token| token.action != ConfirmFor::ConnectSettings(first))
        {
            return Err(CoreError::ReauthRequired);
        }
        let mut inner = self.inner.lock().await;
        let Some(session) = inner.session.clone() else {
            return Err(CoreError::ReauthRequired);
        };
        if inner.connect == Connect::Connected {
            return Ok(());
        }
        inner.sync = Some(self.start_first_sync(backend.device, SyncTiming::default(), first)?);
        self.store
            .account_claim(settings_record::ZONE, &session.email)?;
        inner.connect = Connect::Connected;
        inner.summary_pending = true;
        Ok(())
    }

    /// "Not now": the settings stay on this computer until the person connects them.
    pub async fn connect_later(&self) -> Result<()> {
        self.backend()?;
        let mut inner = self.inner.lock().await;
        if inner.session.is_some() && inner.connect == Connect::Needed {
            inner.connect = Connect::Declined;
        }
        Ok(())
    }

    /// The person closed the summary of the first sync.
    pub async fn connect_summary_seen(&self) -> Result<()> {
        self.backend()?;
        self.inner.lock().await.summary_pending = false;
        Ok(())
    }
}
