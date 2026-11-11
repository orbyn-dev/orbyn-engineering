//! "Confirm it's you": the password typed again for one action, answered with a nonce that the
//! action must send back. A confirmation is for one action (and one device, request or choice),
//! is used up by its first use, lives in memory only and is cleared by sign-out and a restart.

use std::time::Instant;

use super::super::AccountCenter;
use super::super::api::Password;
use super::{CONFIRM_WINDOW, ConfirmFor, Confirmed};
use crate::error::{CoreError, Result};

/// A confirmation waiting to be used. In memory only, never stored.
pub(in crate::account) struct Grant {
    pub(super) action: ConfirmFor,
    pub(super) at: Instant,
    /// What `confirm` handed back to the window that typed the password. The action must send it.
    pub(super) nonce: String,
}

/// Compares two secrets without stopping at the first difference.
fn same_secret(left: &str, right: &str) -> bool {
    left.len() == right.len()
        && left
            .bytes()
            .zip(right.bytes())
            .fold(0u8, |diff, (a, b)| diff | (a ^ b))
            == 0
}

impl AccountCenter {
    // Confirm it's you

    fn grant(&self) -> std::sync::MutexGuard<'_, Option<Grant>> {
        self.confirmed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Whether a fresh confirmation for exactly this action is waiting, and the nonce sent (when one was) is the one
    /// that made it. It does not use it up: the route layer asks this before the handler runs,
    /// and the handler uses it.
    pub fn confirmed_for(&self, action: &ConfirmFor, nonce: Option<&str>) -> bool {
        self.grant().as_ref().is_some_and(|grant| {
            grant.action == *action
                && grant.at.elapsed() < CONFIRM_WINDOW
                && nonce.is_some_and(|sent| same_secret(&grant.nonce, sent))
        })
    }

    /// The only way to get a [`Confirmed`]. Needs a signed-in account and a fresh confirmation
    /// for this action, and the nonce that made it, which it uses up: the next action asks again,
    /// even if this one fails. Whoever holds only the service token never saw the nonce.
    pub async fn require_confirmed(
        &self,
        action: ConfirmFor,
        nonce: Option<&str>,
    ) -> Result<Confirmed> {
        self.backend()?;
        if self.inner.lock().await.session.is_none() {
            return Err(CoreError::ReauthRequired);
        }
        let mut waiting = self.grant();
        let fresh = waiting.as_ref().is_some_and(|grant| {
            grant.action == action
                && grant.at.elapsed() < CONFIRM_WINDOW
                && nonce.is_some_and(|sent| same_secret(&grant.nonce, sent))
        });
        if fresh {
            *waiting = None;
            Ok(Confirmed { action })
        } else {
            Err(CoreError::ReauthRequired)
        }
    }

    /// Checks the password with the Engine and, when it is right, keeps one confirmation for
    /// `action` for ten minutes and hands back the nonce that goes with it. A later confirmation
    /// replaces it. The Engine applies its own recent-password rule to the action too
    /// (orbyn-engine#120); this check is the re-login it counts.
    pub async fn confirm(&self, action: ConfirmFor, password: Password) -> Result<String> {
        let backend = self.backend()?;
        if self.inner.lock().await.session.is_none() {
            return Err(CoreError::ReauthRequired);
        }
        let checked = backend.account.verify_password(&password).await;
        self.guard(checked).await?;
        let nonce = crate::util::random_bytes::<16>()?
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        *self.grant() = Some(Grant {
            action,
            at: Instant::now(),
            nonce: nonce.clone(),
        });
        #[cfg(test)]
        {
            *self
                .last_nonce
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = nonce.clone();
        }
        Ok(nonce)
    }

    /// The last confirmation's nonce, as the window that typed the password would hold it.
    #[cfg(test)]
    fn last_nonce(&self) -> String {
        self.last_nonce
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    #[cfg(test)]
    pub(crate) async fn require_confirmed_last(&self, action: ConfirmFor) -> Result<Confirmed> {
        let nonce = self.last_nonce();
        self.require_confirmed(action, Some(&nonce)).await
    }

    #[cfg(test)]
    pub(crate) fn confirmed_for_last(&self, action: &ConfirmFor) -> bool {
        self.confirmed_for(action, Some(&self.last_nonce()))
    }

    /// The dialog was cancelled: the confirmation it made does not stay live. Only its own nonce
    /// can end it, so nothing else can cancel a confirmation it did not make.
    pub fn cancel_confirmation(&self, nonce: &str) {
        let mut waiting = self.grant();
        if waiting
            .as_ref()
            .is_some_and(|grant| same_secret(&grant.nonce, nonce))
        {
            *waiting = None;
        }
    }

    /// Drops any waiting confirmation, as a restart, a sign-out and a sign-out everywhere do.
    pub fn forget_confirmation(&self) {
        *self.grant() = None;
    }
}
