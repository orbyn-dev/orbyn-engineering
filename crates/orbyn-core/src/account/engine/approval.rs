//! The approval flow over the real client, mapped to the app's request and digits screens.
//!
//! **Trusted device.** The Engine has no "approve this request": the trusted device starts an
//! approval itself (`start_approval`), waits until the new device joined and the digits exist
//! (`approval_state`: `Waiting`, then `Digits`), and confirms (`confirm_approval`) or denies
//! (`deny_approval`). The request in the list is an untrusted entry of `devices()`; its id is that
//! device's id. Allow does all of that and answers with the digits; Cancel on the digits removes
//! the device, which is the only way left to close an approval that was confirmed.
//!
//! **New device.** `pending_approval` shows an approval once a trusted device started one,
//! `join_approval` answers it, `device_digits` yields the digits, and `finish_approval` takes the
//! person's typed digits: the two-way check. It is never called without typed digits. The typed
//! digits are compared here first, with the app's three tries, because the Engine cancels the
//! approval on any one mismatch.
//!
//! No digit is formatted, logged or put in an error. The trusted side's digits reach the screen
//! only through the approve answer; the new device's are kept in a redacting holder.

use orbyn_device_client as dc;

use super::account::EngineAccount;
use super::errors::map;
use crate::account::api::{ApprovalDigits, ApprovalStatus, IncomingApproval, SessionInfo};
use crate::account::device_api::DeviceError;
use crate::account::digits::{self, Check, Issued};

/// A request older than this is not listed: the new device gave up on its own clock.
const LISTED_FOR: std::time::Duration = crate::account::APPROVAL_LIFETIME;

/// This computer's own request to sign in, as the new device.
pub(super) struct Joining {
    request_id: String,
    joined: Option<dc::JoinedApproval>,
    /// What the trusted device shows, with the tries left for the typed check.
    digits: Option<Issued>,
    /// The text the Engine gave, passed back to its own check once ours has passed.
    engine_text: Option<ApprovalDigits>,
}

impl Joining {
    pub(super) fn new(request_id: String) -> Self {
        Self {
            request_id,
            joined: None,
            digits: None,
            engine_text: None,
        }
    }
}

fn only_digits(text: &str) -> String {
    text.chars().filter(char::is_ascii_digit).collect()
}

impl<T, S, C> EngineAccount<T, S, C>
where
    T: dc::DeviceApi + 'static,
    S: dc::KeyStore + 'static,
    C: dc::Clock + 'static,
{
    // ---- the new device ----

    /// A test plays the sign-in that left this device pending: the request is its own id.
    #[cfg(test)]
    pub(crate) async fn expect_approval(&self, request_id: &str) {
        *self.joining.lock().await = Some(Joining::new(request_id.to_owned()));
    }

    pub(super) async fn poll_joined(
        &self,
        request_id: &str,
    ) -> Result<ApprovalStatus, DeviceError> {
        let mut slot = self.joining.lock().await;
        let Some(joining) = slot.as_mut().filter(|j| j.request_id == request_id) else {
            return Err(DeviceError::RequestGone);
        };
        let client = &self.shared.client;
        let view = match client.pending_approval().await {
            Ok(view) => view,
            // A removed device, or a session the Engine ended, is a request that is over.
            Err(dc::DeviceError::SessionExpired | dc::DeviceError::NotSignedIn) => {
                return Err(DeviceError::RequestGone);
            }
            Err(error) => return Err(map(error)),
        };
        let Some(view) = view else {
            // Nothing is started yet, or it was cancelled, answered or timed out after we joined.
            return if joining.joined.is_some() {
                Err(DeviceError::RequestGone)
            } else {
                Ok(ApprovalStatus::Waiting)
            };
        };
        if joining.joined.is_none() {
            let guard = self.shared.engine().await?;
            let Some(engine) = guard.as_ref() else {
                return Err(DeviceError::Unauthorized);
            };
            match client.join_approval(engine).await.map_err(map)? {
                Some(joined) => joining.joined = Some(joined),
                None => return Ok(ApprovalStatus::Waiting),
            }
        }
        // The digits are asked for only once the approver revealed and signed: typing them before
        // that would have the Engine cancel the approval.
        if view.signature.is_none() || view.approver_nonce.is_none() {
            return Ok(ApprovalStatus::Waiting);
        }
        if joining.digits.is_none() {
            let Some(joined) = joining.joined.as_ref() else {
                return Ok(ApprovalStatus::Waiting);
            };
            match client.device_digits(joined).await.map_err(map)? {
                Some(text) => {
                    joining.digits = Some(Issued::new(only_digits(&text), digits::LIFETIME));
                    joining.engine_text = Some(ApprovalDigits::new(text));
                }
                None => return Ok(ApprovalStatus::Waiting),
            }
        }
        let locked = joining.digits.as_ref().is_some_and(|d| d.tries_left() == 0);
        Ok(if locked {
            ApprovalStatus::Locked
        } else {
            ApprovalStatus::Allowed
        })
    }

    pub(super) async fn cancel_joined(&self, request_id: &str) -> Result<(), DeviceError> {
        let mut slot = self.joining.lock().await;
        if !slot.as_ref().is_some_and(|j| j.request_id == request_id) {
            return Ok(());
        }
        let taken = slot.take();
        drop(slot);
        // Before the Engine's approval was joined there is nothing on the server to cancel.
        if let Some(joined) = taken.and_then(|j| j.joined) {
            let _ = self.shared.client.deny_joined(joined).await;
        }
        Ok(())
    }

    pub(super) async fn finish_joined(
        &self,
        request_id: &str,
        typed: &ApprovalDigits,
    ) -> Result<SessionInfo, DeviceError> {
        let mut slot = self.joining.lock().await;
        let Some(joining) = slot.as_mut().filter(|j| j.request_id == request_id) else {
            return Err(DeviceError::RequestGone);
        };
        let Some(issued) = joining.digits.as_mut() else {
            return Err(DeviceError::RequestGone);
        };
        // Our own check comes first: a wrong entry here costs one of three tries and leaves the
        // approval open, where the Engine would cancel it.
        match issued.check(&only_digits(typed.expose())) {
            Check::Accepted => {}
            Check::Wrong { tries_left } => return Err(DeviceError::WrongDigits { tries_left }),
            Check::Locked | Check::Expired => {
                let joined = slot.take().and_then(|j| j.joined);
                drop(slot);
                if let Some(joined) = joined {
                    let _ = self.shared.client.deny_joined(joined).await;
                }
                return Err(DeviceError::DigitsLocked);
            }
        }
        let Some(taken) = slot.take() else {
            return Err(DeviceError::RequestGone);
        };
        drop(slot);
        let (Some(joined), Some(text)) = (taken.joined, taken.engine_text) else {
            return Err(DeviceError::RequestGone);
        };
        let mut guard = self.shared.engine().await?;
        let Some(engine) = guard.as_mut() else {
            return Err(DeviceError::Unauthorized);
        };
        // The Engine checks the typed digits against its own, the age, the approver's signature
        // and its endorsement, and only then sends the digits, once. Any failure closes it: an
        // approver that cannot sign what this device accepts ends here too, as a refusal.
        self.shared
            .client
            .finish_approval(engine, joined, Some(text.expose()))
            .await
            .map_err(map)?;
        self.shared.save(engine);
        drop(guard);
        self.shared.session(&self.device_name)
    }

    // ---- the trusted device ----

    pub(super) async fn pending_requests(&self) -> Result<Vec<IncomingApproval>, DeviceError> {
        let list = self.shared.client.devices().await.map_err(map)?;
        let now = crate::util::unix_now();
        let mut trusted = self.trusted();
        // An approval that ended leaves nothing behind: what is kept for a device is dropped once
        // the Engine no longer lists it as waiting (trusted, removed or gone).
        let waiting: std::collections::HashSet<&str> = list
            .devices
            .iter()
            .filter(|device| device.state == dc::DeviceState::Pending)
            .map(|device| device.id.as_str())
            .collect();
        let in_flight = trusted.approving.clone();
        trusted
            .dismissed
            .retain(|id| waiting.contains(id.as_str()) || in_flight.contains(id));
        trusted.confirmed.retain(|id| waiting.contains(id.as_str()));
        trusted.left.retain(|id, _| waiting.contains(id.as_str()));
        Ok(list
            .devices
            .iter()
            .filter(|device| device.state == dc::DeviceState::Pending && !device.current)
            .filter(|device| !trusted.dismissed.contains(&device.id))
            .filter(|device| recent(&device.created_at, now))
            .map(|device| IncomingApproval {
                id: device.id.clone(),
                device_name: device.name.clone(),
                // The device's own claim, a hint and never proof.
                app: device.app.map_or("", dc::DeviceApp::as_str).to_owned(),
                near: None,
                asked_at: device.created_at.clone(),
                allowed: trusted.confirmed.contains(&device.id),
                // No time left is shown until the Engine says it.
                expires_in_secs: 0,
            })
            .collect())
    }

    /// Allow: start, wait for the digits, confirm, hand the zone keys over, answer with the digits.
    /// A Don't allow that arrives at any point wins: it is checked before the signature is made and
    /// again after it, and either undoes what was done.
    pub(super) async fn allow(&self, id: &str) -> Result<ApprovalDigits, DeviceError> {
        {
            let mut trusted = self.trusted();
            if trusted.dismissed.contains(id) || !trusted.approving.insert(id.to_owned()) {
                return Err(DeviceError::RequestGone);
            }
        }
        let result = self.allow_inner(id).await;
        let mut trusted = self.trusted();
        trusted.approving.remove(id);
        if result.is_err() {
            trusted.left.remove(id);
            trusted.confirmed.remove(id);
        }
        result
    }

    fn refused(&self, id: &str) -> bool {
        self.trusted().dismissed.contains(id)
    }

    async fn allow_inner(&self, id: &str) -> Result<ApprovalDigits, DeviceError> {
        let client = &self.shared.client;
        let mut approval = {
            let guard = self.shared.engine().await?;
            let Some(engine) = guard.as_ref() else {
                return Err(DeviceError::Unauthorized);
            };
            client
                .start_approval(engine, id)
                .await
                .map_err(|error| match error {
                    // Not in the account any more, or not one this device may approve.
                    dc::DeviceError::InvalidInput(_) => DeviceError::RequestGone,
                    other => map(other),
                })?
        };
        let deadline = tokio::time::Instant::now() + self.wait;
        let digits = loop {
            match client.approval_state(&mut approval).await {
                Ok(dc::ApproverState::Digits { digits, .. }) => break digits,
                Ok(dc::ApproverState::Waiting) => {
                    if self.refused(id) || tokio::time::Instant::now() >= deadline {
                        let _ = client.deny_approval(approval).await;
                        return Err(DeviceError::RequestGone);
                    }
                    tokio::time::sleep(self.step).await;
                }
                Err(error) => return Err(map(error)),
            }
        };
        #[cfg(test)]
        self.run_hook(0);
        // Right before signing: a Don't allow that came while the digits were being fetched wins.
        if self.refused(id) {
            let _ = client.deny_approval(approval).await;
            return Err(DeviceError::RequestGone);
        }
        let mut guard = self.shared.engine().await?;
        let Some(engine) = guard.as_mut() else {
            return Err(DeviceError::Unauthorized);
        };
        // Allow, behind the password, is the person's confirmation. The new device types these
        // digits, and its Engine refuses anything else; it needs this signature to do that.
        // Display only: the Engine's count, kept for the answer, never used to decide.
        let left = client.time_left(&approval);
        client
            .confirm_approval(engine, approval)
            .await
            .map_err(map)?;
        #[cfg(test)]
        self.run_hook(1);
        // And right after: one that came while the signature was going out removes the device,
        // the only way left to close a confirmed approval.
        if self.refused(id) {
            let _ = client.remove_device(&*engine, id).await;
            return Err(DeviceError::RequestGone);
        }
        {
            let mut trusted = self.trusted();
            trusted.confirmed.insert(id.to_owned());
            match left {
                Some(left) => trusted.left.insert(id.to_owned(), left),
                None => trusted.left.remove(id),
            };
        }
        // The new device can read nothing until it holds the zone keys, and the Engine seals them
        // only to a device it lists as trusted, which is after the digits are typed: they go on
        // the next pass.
        self.shared.owe_keys(id);
        self.shared.save(engine);
        Ok(ApprovalDigits::new(digits))
    }

    /// Don't allow. An approval being started is given up by the Allow that runs it. Otherwise the
    /// approver starts one and denies it at once, which closes it on the new device immediately
    /// and needs no recent sign-in. If one cannot be started the request stays listed and the
    /// error is returned, so the screen shows it and the person can try again. After an Allow was
    /// confirmed, Cancel removes the device, so the digits finish nothing.
    pub(super) async fn refuse(&self, id: &str) -> Result<(), DeviceError> {
        let (running, confirmed) = {
            let mut trusted = self.trusted();
            trusted.dismissed.insert(id.to_owned());
            (
                trusted.approving.contains(id),
                trusted.confirmed.contains(id),
            )
        };
        if running {
            return Ok(());
        }
        let result = self.close(id, confirmed).await;
        let mut trusted = self.trusted();
        if result.is_err() {
            // Not closed: it stays on the list so Don't allow can be tried again.
            trusted.dismissed.remove(id);
        } else {
            trusted.confirmed.remove(id);
            trusted.left.remove(id);
        }
        result
    }

    async fn close(&self, id: &str, confirmed: bool) -> Result<(), DeviceError> {
        let client = &self.shared.client;
        let guard = self.shared.engine().await?;
        let Some(engine) = guard.as_ref() else {
            return Err(DeviceError::Unauthorized);
        };
        let gone = |error| match error {
            dc::DeviceError::InvalidInput(_) | dc::DeviceError::ApprovalExpired => {
                DeviceError::RequestGone
            }
            other => map(other),
        };
        if confirmed {
            return client.remove_device(engine, id).await.map_err(gone);
        }
        match client.start_approval(engine, id).await {
            Ok(approval) => client.deny_approval(approval).await.map_err(gone),
            // Nothing to close: the device is not in the account any more.
            Err(dc::DeviceError::InvalidInput(_)) => Ok(()),
            Err(error) => Err(map(error)),
        }
    }

    /// A test plays a Don't allow arriving at a chosen point of an Allow.
    #[cfg(test)]
    fn run_hook(&self, at: usize) {
        let hook = self
            .hooks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)[at]
            .take();
        if let Some(hook) = hook {
            hook(&mut self.trusted());
        }
    }
}

/// Whether a device asked less than ten minutes ago. A time that cannot be read is kept.
fn recent(created_at: &str, now: i64) -> bool {
    time::OffsetDateTime::parse(created_at, &time::format_description::well_known::Rfc3339).map_or(
        true,
        |at| {
            now.saturating_sub(at.unix_timestamp())
                <= i64::try_from(LISTED_FOR.as_secs()).unwrap_or(600)
        },
    )
}
