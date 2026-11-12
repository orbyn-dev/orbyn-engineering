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
        let (id, expired) = {
            let inner = self.inner.lock().await;
            let Some(approval) = &inner.approval else {
                return Ok(None);
            };
            if approval.answer.is_some() {
                return Ok(Some(self.approval_view(approval)));
            }
            (
                approval.request_id.clone(),
                approval.asked.elapsed() >= self.approval_life(),
            )
        };
        if expired {
            let _ = backend.account.cancel_approval(&id).await;
            return Ok(self
                .close_approval(&id, AccountApprovalState::TimedOut)
                .await);
        }
        match backend.account.approval_status(&id).await {
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
                return Ok(self.close_approval(&id, AccountApprovalState::Denied).await);
            }
            Ok(ApprovalStatus::Locked) => {
                return Ok(self.close_approval(&id, AccountApprovalState::Locked).await);
            }
            Err(DeviceError::RequestGone) => {
                return Ok(self.close_approval(&id, AccountApprovalState::Denied).await);
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
        let id = {
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
            if approval.asked.elapsed() >= life {
                drop(inner);
                let _ = backend.account.cancel_approval(&id).await;
                return Ok(self
                    .close_approval(&id, AccountApprovalState::TimedOut)
                    .await);
            }
            if approval.spent_tries >= digits::TRIES {
                drop(inner);
                let _ = backend.account.cancel_approval(&id).await;
                return Ok(self.close_approval(&id, AccountApprovalState::Locked).await);
            }
            // The try is taken before the call, so two entries at once cannot share one.
            approval.spent_tries += 1;
            id
        };
        match backend.account.finish_approval(&id, &typed).await {
            Ok(session) => self.finish_approval(&id, session).await,
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
                    let _ = backend.account.cancel_approval(&id).await;
                    return Ok(self.close_approval(&id, AccountApprovalState::Locked).await);
                }
                approval.tries_left = Some(left);
                Ok(Some(self.approval_view(approval)))
            }
            Err(DeviceError::DigitsLocked) => {
                Ok(self.close_approval(&id, AccountApprovalState::Locked).await)
            }
            Err(DeviceError::RequestGone) => {
                Ok(self.close_approval(&id, AccountApprovalState::Denied).await)
            }
            Err(error) => {
                // Nothing reached the server, so the try was not used.
                let mut inner = self.inner.lock().await;
                if let Some(approval) = inner.approval.as_mut().filter(|a| a.request_id == id) {
                    approval.spent_tries = approval.spent_tries.saturating_sub(1);
                }
                Err(refusal(&error))
            }
        }
    }

    /// Signs in on an allowed request, under one lock: if the person gave the request up
    /// meanwhile, nobody is signed in here and the Engine is told to forget the session.
    pub(crate) async fn finish_approval(
        &self,
        id: &str,
        session: SessionInfo,
    ) -> Result<Option<AccountApprovalView>> {
        let backend = self.backend()?.clone();
        let mut inner = self.inner.lock().await;
        let pending = inner
            .approval
            .as_ref()
            .is_some_and(|approval| approval.request_id == id && approval.answer.is_none());
        if !pending {
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
            return Ok(None);
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

    /// The request ended without signing in: this computer is signed out, and the screen
    /// keeps saying why until the person leaves it.
    async fn close_approval(
        &self,
        id: &str,
        state: AccountApprovalState,
    ) -> Option<AccountApprovalView> {
        let shown = {
            let mut inner = self.inner.lock().await;
            let approval = inner.approval.as_mut().filter(|a| a.request_id == id)?;
            approval.answer = Some(state);
            self.approval_view(approval)
        };
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
        let result = self.backend()?.account.approve(id).await;
        let shown = self.guard(result).await?;
        let digits = shown.expose();
        Ok(AccountApproved {
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
