//! A new device signing in with a trusted device's help, the trusted side of that, and joining
//! this computer's settings to the account's the first time. Transport-free: the routes call
//! these, and the stand-in plays the account server in tests.

use std::time::{Duration, Instant};

use orbyn_contracts::{AccountApprovalRequest, AccountApprovalState, AccountApprovalView};

use super::super::api::{ApprovalStatus, SessionInfo, refusal};
use super::super::status::SyncState;
use super::super::sync::SyncTiming;
use super::super::{AccountCenter, FirstSync, settings_record};
use super::{ConfirmFor, Confirmed};
use crate::error::{CoreError, Result};

/// How long a request to sign in waits for a trusted device, counted on this computer's clock.
pub const APPROVAL_LIFETIME: Duration = Duration::from_secs(10 * 60);

/// This computer's own request to sign in, while a trusted device decides.
pub(super) struct Approval {
    pub(super) request_id: String,
    pub(super) digits: String,
    pub(super) asked: Instant,
    /// Set once the request has an answer; a request that is waiting has none.
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
        let digits = if approval.digits.len() == 6 {
            format!("{} {}", &approval.digits[..3], &approval.digits[3..])
        } else {
            approval.digits.clone()
        };
        let left = self
            .approval_life()
            .saturating_sub(approval.asked.elapsed());
        AccountApprovalView {
            state: approval.answer.unwrap_or(AccountApprovalState::Waiting),
            digits,
            expires_in_secs: if approval.answer.is_none() {
                left.as_secs()
            } else {
                0
            },
        }
    }

    /// Asks how this computer's request stands. A trusted device's answer ends it; nobody
    /// answering for ten minutes, on this computer's clock, ends it too. Allowed signs in.
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
            Ok(ApprovalStatus::Denied) => {
                return Ok(self.close_approval(&id, AccountApprovalState::Denied).await);
            }
            Ok(ApprovalStatus::CodesDiffer) => {
                return Ok(self
                    .close_approval(&id, AccountApprovalState::CodesDiffer)
                    .await);
            }
            Ok(ApprovalStatus::Approved(session)) => {
                return self.finish_approval(&id, session).await;
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
            drop(inner);
            let _ = backend.account.sign_out().await;
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
                digits: if request.digits.len() == 6 {
                    format!("{} {}", &request.digits[..3], &request.digits[3..])
                } else {
                    request.digits
                },
                expires_in_secs: request.expires_in_secs,
            })
            .collect())
    }

    /// Lets the new device in. Needs a confirmation for exactly this request.
    pub async fn approve(&self, confirmed: Confirmed, id: &str) -> Result<()> {
        if confirmed.action != ConfirmFor::ApproveDevice(id.to_owned()) {
            return Err(CoreError::ReauthRequired);
        }
        let result = self.backend()?.account.approve(id).await;
        self.guard(result).await
    }

    /// Refuses the new device. This only ever closes a request, so it needs no confirmation.
    pub async fn deny(&self, id: &str, codes_differ: bool) -> Result<()> {
        let backend = self.backend()?;
        if self.inner.lock().await.session.is_none() {
            return Err(CoreError::ReauthRequired);
        }
        let result = backend.account.deny(id, codes_differ).await;
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
