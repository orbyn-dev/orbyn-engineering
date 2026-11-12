//! The account calls over the real client: sign-up, sign-in, the second factor, devices and
//! sign-out. The approval calls are in `approval.rs`.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use orbyn_contracts::AccountDevice;
use orbyn_device_client as dc;

use super::errors::map;
use super::shared::Shared;
use crate::account::api::{
    AccountApi, ApprovalDigits, ApprovalStatus, IncomingApproval, Password, PendingApproval,
    SessionInfo, SignInOutcome,
};
use crate::account::device_api::DeviceError;

/// A sign-up that waits for its emailed code.
#[derive(Default)]
pub(super) struct SignUp {
    pub(super) pending: Option<dc::PendingSignUp>,
    /// The code was spent and the keys were not saved: asking again saves them, with no new code.
    pub(super) unsaved: bool,
}

/// What the trusted side remembers between calls. Ids are device ids.
#[derive(Default)]
pub(super) struct Trusted {
    /// Requests the person refused before they were allowed: hidden here, because the Engine has
    /// nothing to refuse until an approval starts.
    pub(super) dismissed: HashSet<String>,
    /// Approvals this device confirmed: the new device has the digits to type.
    pub(super) confirmed: HashSet<String>,
    /// Approvals being started right now.
    pub(super) approving: HashSet<String>,
    /// The Engine's own count of what an allowed approval had left when it was confirmed.
    pub(super) left: std::collections::HashMap<String, Duration>,
}

pub struct EngineAccount<T, S, C> {
    pub(super) shared: Arc<Shared<T, S, C>>,
    pub(super) device_name: String,
    pub(super) sign_up: tokio::sync::Mutex<SignUp>,
    pub(super) factor: tokio::sync::Mutex<Option<Box<dc::SecondFactorChallenge>>>,
    pub(super) joining: tokio::sync::Mutex<Option<super::approval::Joining>>,
    pub(super) trusted: Mutex<Trusted>,
    pub(super) wait: Duration,
    pub(super) step: Duration,
}

impl<T, S, C> EngineAccount<T, S, C>
where
    T: dc::DeviceApi + 'static,
    S: dc::KeyStore + 'static,
    C: dc::Clock + 'static,
{
    pub(super) fn new(shared: Arc<Shared<T, S, C>>, wait: Duration, step: Duration) -> Self {
        Self {
            device_name: "This computer".to_owned(),
            shared,
            sign_up: tokio::sync::Mutex::default(),
            factor: tokio::sync::Mutex::default(),
            joining: tokio::sync::Mutex::default(),
            trusted: Mutex::default(),
            wait,
            step,
        }
    }

    pub(super) fn trusted(&self) -> std::sync::MutexGuard<'_, Trusted> {
        self.trusted
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Signed in: the Engine's state is opened now, so the stamps are named before sync starts.
    async fn signed(&self) -> Result<SessionInfo, DeviceError> {
        drop(self.shared.engine().await?);
        self.shared.session(&self.device_name)
    }

    /// A sign-in's outcome as the app's.
    async fn outcome(&self, outcome: dc::SignInOutcome) -> Result<SignInOutcome, DeviceError> {
        match outcome {
            dc::SignInOutcome::SignedIn(_) => Ok(SignInOutcome::Signed(self.signed().await?)),
            // Pending until a trusted device lets it in. The request is this device's own id.
            dc::SignInOutcome::NeedsApproval(info) => {
                drop(self.shared.engine().await?);
                *self.joining.lock().await =
                    Some(super::approval::Joining::new(info.device_id.clone()));
                Ok(SignInOutcome::AwaitingApproval(PendingApproval {
                    request_id: info.device_id,
                }))
            }
            dc::SignInOutcome::SecondFactor(challenge) => {
                *self.factor.lock().await = Some(challenge);
                Ok(SignInOutcome::SecondFactor)
            }
        }
    }
}

fn engine_password(password: &Password) -> dc::Password {
    // The client wipes its copy when it is dropped; the app's `Password` wipes its own.
    dc::Password::new(password.expose().to_owned())
}

#[async_trait]
impl<T, S, C> AccountApi for EngineAccount<T, S, C>
where
    T: dc::DeviceApi + 'static,
    S: dc::KeyStore + 'static,
    C: dc::Clock + 'static,
{
    /// The Engine is invite-only for now (D2), and the client has no call that says otherwise.
    fn invite_required(&self) -> bool {
        true
    }

    /// The client at `engine-v0.2.1` has no call that checks an invite alone: the code is checked
    /// at the password step, which answers `invite_required` when it is not valid.
    async fn check_invite(&self, _invite: &str) -> Result<(), DeviceError> {
        Ok(())
    }

    async fn start_sign_up(
        &self,
        invite: &str,
        email: &str,
        password: &Password,
    ) -> Result<(), DeviceError> {
        let request = dc::SignUpRequest {
            email: email.to_owned(),
            password: engine_password(password),
            invite: (!invite.is_empty()).then(|| invite.to_owned()),
        };
        // The client refuses a computer that already belongs to an account (`StoreNotEmpty`), and
        // the account the server makes waits for its emailed code: nothing is live before it.
        let pending = self
            .shared
            .client
            .begin_sign_up(request)
            .await
            .map_err(map)?;
        *self.sign_up.lock().await = SignUp {
            pending: Some(pending),
            unsaved: false,
        };
        Ok(())
    }

    async fn resend_code(&self) -> Result<(), DeviceError> {
        let slot = self.sign_up.lock().await;
        let Some(pending) = slot.pending.as_ref() else {
            return Err(DeviceError::InvalidCode);
        };
        self.shared
            .client
            .resend_sign_up(pending)
            .await
            .map_err(map)
    }

    async fn verify_email_code(&self, code: &str) -> Result<SessionInfo, DeviceError> {
        let mut slot = self.sign_up.lock().await;
        let unsaved = slot.unsaved;
        let Some(pending) = slot.pending.as_mut() else {
            return Err(DeviceError::InvalidCode);
        };
        // A spent code is never sent again: the keys it made are saved by `retry_save`.
        let saved = if unsaved {
            self.shared.client.retry_save(pending)
        } else {
            self.shared.client.verify_sign_up(pending, code).await
        };
        match saved {
            Ok(_) => {
                *slot = SignUp::default();
                drop(slot);
                self.signed().await
            }
            Err(dc::DeviceError::Store(_)) => {
                slot.unsaved = true;
                Err(DeviceError::KeysNotSaved)
            }
            Err(error) => Err(map(error)),
        }
    }

    async fn sign_in(
        &self,
        email: &str,
        password: &Password,
    ) -> Result<SignInOutcome, DeviceError> {
        let outcome = self
            .shared
            .client
            .sign_in(email, &engine_password(password))
            .await
            .map_err(map)?;
        self.outcome(outcome).await
    }

    async fn second_factor(&self, code: &str) -> Result<SignInOutcome, DeviceError> {
        let mut slot = self.factor.lock().await;
        let Some(challenge) = slot.as_ref() else {
            return Err(DeviceError::InvalidCode);
        };
        let text = dc::Zeroizing::new(code.trim().to_owned());
        let factor = if text.len() == 6 && text.bytes().all(|b| b.is_ascii_digit()) {
            dc::SecondFactor::Code(text)
        } else {
            dc::SecondFactor::Recovery(text)
        };
        let outcome = self
            .shared
            .client
            .complete_second_factor(challenge, factor)
            .await
            .map_err(map)?;
        *slot = None;
        drop(slot);
        self.outcome(outcome).await
    }

    async fn approval_status(&self, request_id: &str) -> Result<ApprovalStatus, DeviceError> {
        self.poll_joined(request_id).await
    }

    async fn cancel_approval(&self, request_id: &str) -> Result<(), DeviceError> {
        self.cancel_joined(request_id).await
    }

    async fn approval_requests(&self) -> Result<Vec<IncomingApproval>, DeviceError> {
        self.pending_requests().await
    }

    async fn approve(&self, request_id: &str) -> Result<ApprovalDigits, DeviceError> {
        self.allow(request_id).await
    }

    fn time_left(&self, request_id: &str) -> Option<Duration> {
        self.trusted().left.get(request_id).copied()
    }

    async fn deny(&self, request_id: &str) -> Result<(), DeviceError> {
        self.refuse(request_id).await
    }

    async fn finish_approval(
        &self,
        request_id: &str,
        digits: &ApprovalDigits,
    ) -> Result<SessionInfo, DeviceError> {
        self.finish_joined(request_id, digits).await
    }

    /// "Confirm it's you": the network login again, which also gives the recent sign-in the
    /// Engine asks for before removing a device or signing every session out.
    async fn verify_password(&self, password: &Password) -> Result<(), DeviceError> {
        self.shared
            .client
            .reauthenticate(&engine_password(password))
            .await
            .map(drop)
            .map_err(map)
    }

    async fn devices(&self) -> Result<Vec<AccountDevice>, DeviceError> {
        let list = self.shared.client.devices().await.map_err(map)?;
        Ok(list
            .devices
            .iter()
            .filter(|device| device.state == dc::DeviceState::Trusted)
            .map(|device| AccountDevice {
                id: device.id.clone(),
                name: device.name.clone(),
                platform: device.platform.as_str().to_owned(),
                // The device's own claim, shown as a hint and never relied on.
                app: device.app.map_or("", dc::DeviceApp::as_str).to_owned(),
                trusted_since: device.created_at.clone(),
                current: device.current,
            })
            .collect())
    }

    async fn remove_device(&self, id: &str) -> Result<(), DeviceError> {
        let guard = self.shared.engine().await?;
        let Some(engine) = guard.as_ref() else {
            return Err(DeviceError::Unauthorized);
        };
        self.shared
            .client
            .remove_device(engine, id)
            .await
            .map_err(|error| match error {
                dc::DeviceError::InvalidInput(_) => DeviceError::RequestGone,
                other => map(other),
            })?;
        self.trusted().confirmed.remove(id);
        Ok(())
    }

    async fn sign_out_everywhere(&self) -> Result<(), DeviceError> {
        self.shared.client.sign_out_all().await.map_err(map)?;
        self.sign_out().await
    }

    /// Ends this device's session and forgets its keys, as the sign-out dialog says. The next
    /// sign-in is a new device that a trusted one has to let in.
    async fn sign_out(&self) -> Result<(), DeviceError> {
        let result = self.shared.client.sign_out(true).await.map_err(map);
        self.shared.forget().await;
        *self.sign_up.lock().await = SignUp::default();
        *self.factor.lock().await = None;
        *self.joining.lock().await = None;
        *self.trusted() = Trusted::default();
        result
    }
}
