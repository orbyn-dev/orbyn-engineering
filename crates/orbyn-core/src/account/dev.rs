//! A development stand-in for the Engine's account calls, in memory, for debug builds
//! and tests. It exists so the screens can be built and driven before the real device
//! client does. It is compiled out of release builds, answers only under the
//! `LocalOnly` gate, and its accounts are throwaway: never use a real password here.
//!
//! Known values, so a developer or an end-to-end spec can reach each state:
//!
//! | Input | Answer |
//! | --- | --- |
//! | invite `K7QM-2XWH-9QXA-3C2D` | accepted; any other well-formed invite is refused |
//! | emailed code `ABCD-EFGH` | accepted, after the password step, for an address with no account; any other is `InvalidCode`, the fifth wrong one in a row refuses every code until a new one is sent, and an address that already has an account is `InvalidCode` too and keeps its account |
//! | `dev@example.com` and the password below | signed in |
//! | `two@example.com` | asks for a code: `123456` or the recovery code `ABCDE-FGH23` |
//! | `slow@example.com` | `RateLimited` for 90 seconds |
//! | `down@example.com` | `Offline` |
//! | `busy@example.com` | the sign-up password step asks for a visible check first (1.5 s), then goes on |
//! | `hard@example.com` | the sign-up password step's check fails (`ProofFailed`) |
//! | `delete@example.com` | signs in; Sign out everywhere says deletion is scheduled |
//! | `approve@example.com` | asks a trusted device, which allows it 3 s later and shows the digits `418 207`; this device types them |
//! | `deny@example.com` | the same, and the trusted device refuses |
//! | `late@example.com` | the same, and nobody ever answers |
//!
//! Signed in, one request from an iPad waits for this device to answer (`approval_requests`).
//! Allowing it answers with six random digits that the stand-in keeps under the rule in
//! [`super::digits`]: three tries, single use, ten minutes.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use orbyn_contracts::AccountDevice;

use super::api::{
    AccountApi, ApprovalDigits, ApprovalStatus, Backend, IncomingApproval, Password,
    PendingApproval, SessionInfo, SignInOutcome,
};
use super::device_api::DeviceError;
use super::digits::{self, Check, Issued};
use super::fake::FakeDeviceApi;

pub const INVITE: &str = "K7QM2XWH9QXA3C2D";
pub const EMAIL_CODE: &str = "ABCDEFGH";
/// The digits a canned trusted device shows for the new device's own request.
pub const DEV_DIGITS: &str = "418207";
/// Wrong emailed codes in a row after which every code is refused until a new one is sent.
const EMAIL_CODE_TRIES: u8 = 5;
pub const PASSWORD: &str = "correct horse battery";
const SECOND_FACTOR: [&str; 2] = ["123456", "ABCDEFGH23"];
const WRONG_PASSWORDS_BEFORE_LIMIT: u8 = 5;

struct Account {
    email: String,
    password: String,
}

/// What a trusted device answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Allow,
    Deny,
}

/// This stand-in's own request to sign in, as the new device made it.
struct Mine {
    id: String,
    email: String,
    answer: Option<Answer>,
    /// The digits the trusted device showed, once it allowed the request.
    issued: Option<Issued>,
    /// A canned answer that arrives by itself, for a developer driving the screens by hand.
    auto: Option<(Instant, Answer)>,
}

#[derive(Default)]
struct State {
    mine: Option<Mine>,
    /// Requests from other devices, waiting for this one (as a trusted device) to answer.
    incoming: Vec<(IncomingApproval, Instant)>,
    /// What this device answered, newest last: the request id and whether it allowed it.
    decided: Vec<(String, bool)>,
    /// The digits this device showed for requests it allowed, by request id.
    shown: Vec<(String, Issued)>,
    cancelled: Vec<String>,
    accounts: Vec<Account>,
    signup: Option<SignUp>,
    pending_second_factor: Option<String>,
    session: Option<String>,
    removed: Vec<String>,
    wrong_passwords: u8,
}

/// An account created at the password step, waiting for its emailed code.
struct SignUp {
    email: String,
    password: String,
    wrong_codes: u8,
}

impl Drop for SignUp {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.password);
    }
}

pub struct DevAccount {
    state: Mutex<State>,
    /// How long a canned answer takes to arrive.
    auto_delay: Mutex<Duration>,
    /// How long a request from another device waits for an answer: ten minutes, as the Engine's.
    incoming_life: Mutex<Duration>,
    #[cfg(test)]
    zones: Arc<FakeDeviceApi>,
}

/// The backend, and the stand-in itself so a test can play the trusted device.
pub fn handle() -> (Backend, Arc<DevAccount>) {
    let accounts = [
        "dev@example.com",
        "two@example.com",
        "slow@example.com",
        "down@example.com",
        "delete@example.com",
        "approve@example.com",
        "deny@example.com",
        "late@example.com",
    ]
    .map(|email| Account {
        email: email.to_owned(),
        password: PASSWORD.to_owned(),
    });
    let zones = Arc::new(FakeDeviceApi::new());
    let account = Arc::new(DevAccount {
        state: Mutex::new(State {
            accounts: accounts.into(),
            incoming: vec![(
                IncomingApproval {
                    id: "req-ipad".to_owned(),
                    device_name: "iPad".to_owned(),
                    app: "planner".to_owned(),
                    near: Some("Near Melbourne".to_owned()),
                    asked_at: "2026-11-11T09:40:00Z".to_owned(),
                    allowed: false,
                    expires_in_secs: 0,
                },
                Instant::now(),
            )],
            ..State::default()
        }),
        auto_delay: Mutex::new(Duration::from_secs(3)),
        incoming_life: Mutex::new(Duration::from_secs(10 * 60)),
        #[cfg(test)]
        zones: zones.clone(),
    });
    (
        Backend {
            account: account.clone(),
            device: zones,
        },
        account,
    )
}

/// A trusted device answering: allowing shows the canned digits for this device's own request.
fn decide(mine: &mut Mine, answer: Answer) {
    mine.answer = Some(answer);
    if answer == Answer::Allow && mine.issued.is_none() {
        mine.issued = Some(Issued::new(DEV_DIGITS.to_owned(), digits::LIFETIME));
    }
}

fn session(email: &str) -> SessionInfo {
    SessionInfo {
        email: email.to_owned(),
        device_id: "dev-this".to_owned(),
        device_name: "This computer".to_owned(),
    }
}

fn device(
    id: &str,
    name: &str,
    platform: &str,
    app: &str,
    since: &str,
    current: bool,
) -> AccountDevice {
    AccountDevice {
        id: id.into(),
        name: name.into(),
        platform: platform.into(),
        app: app.into(),
        trusted_since: since.into(),
        current,
    }
}

impl DevAccount {
    /// Makes a request from another device last only this long.
    #[cfg(test)]
    pub fn set_incoming_life(&self, life: Duration) {
        *self
            .incoming_life
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = life;
    }

    /// The other device gave its request up.
    #[cfg(test)]
    pub fn cancel_incoming(&self, id: &str) {
        self.lock().incoming.retain(|(request, _)| request.id != id);
    }

    /// The Engine ends this device's session without the person doing it.
    #[cfg(test)]
    pub fn sign_out_elsewhere(&self) {
        self.lock().session = None;
    }

    /// Whether the stand-in still holds a session for this device.
    #[cfg(test)]
    pub fn signed_in(&self) -> bool {
        self.lock().session.is_some()
    }

    #[cfg(test)]
    pub fn zones(&self) -> Arc<FakeDeviceApi> {
        self.zones.clone()
    }

    /// Plays the trusted device answering this device's own request. Allowing shows
    /// [`DEV_DIGITS`], as a test reads them with [`Self::shown_digits`].
    #[cfg(test)]
    pub fn answer(&self, answer: Answer) {
        if let Some(mine) = self.lock().mine.as_mut() {
            decide(mine, answer);
        }
    }

    /// The digits the trusted device shows for this device's request, as a person reads them off
    /// that screen. `None` before it allowed, and after the digits are spent or locked.
    #[cfg(test)]
    pub fn shown_digits(&self) -> Option<String> {
        let state = self.lock();
        let issued = state.mine.as_ref()?.issued.as_ref()?;
        issued.shown().map(str::to_owned)
    }

    /// Tries left on the digits of this device's request, as the server counts them.
    #[cfg(test)]
    pub fn digit_tries_left(&self) -> Option<u8> {
        let state = self.lock();
        Some(state.mine.as_ref()?.issued.as_ref()?.tries_left())
    }

    /// Plays a server that never locks: the digits of this device's request get their tries back.
    #[cfg(test)]
    pub fn refresh_digit_tries(&self) {
        if let Some(mine) = self.lock().mine.as_mut()
            && let Some(old) = mine.issued.take()
        {
            let digits = old.shown().map(str::to_owned).unwrap_or_default();
            mine.issued = Some(Issued::new(digits, digits::LIFETIME));
        }
    }

    /// The digits this device showed as a trusted one when it allowed `id`.
    #[cfg(test)]
    pub fn digits_shown_for(&self, id: &str) -> Option<String> {
        let state = self.lock();
        let (_, issued) = state.shown.iter().find(|(request, _)| request == id)?;
        issued.shown().map(str::to_owned)
    }

    /// The requests this device gave up.
    #[cfg(test)]
    pub fn cancelled(&self) -> Vec<String> {
        self.lock().cancelled.clone()
    }

    /// What this device answered as a trusted one: the request id, and whether it allowed it.
    #[cfg(test)]
    pub fn decided(&self) -> Vec<(String, bool)> {
        self.lock().decided.clone()
    }

    fn incoming_life(&self) -> Duration {
        *self
            .incoming_life
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// A request is allowed once, while it lasts: an expired, answered or given-up one is gone.
    /// It stays listed, as allowed, until it is closed, so the dialog can cancel it.
    fn allow_incoming(&self, id: &str) -> Result<ApprovalDigits, DeviceError> {
        let life = self.incoming_life();
        let mut state = self.lock();
        let live = state
            .incoming
            .iter()
            .any(|(request, asked)| request.id == id && asked.elapsed() < life && !request.allowed);
        if !live {
            return Err(DeviceError::RequestGone);
        }
        let digits = Issued::random_digits();
        if digits.is_empty() {
            return Err(DeviceError::Server { retryable: true });
        }
        if let Some((request, _)) = state.incoming.iter_mut().find(|(r, _)| r.id == id) {
            request.allowed = true;
        }
        let left = life.saturating_sub(
            state
                .incoming
                .iter()
                .find(|(request, _)| request.id == id)
                .map_or(Duration::ZERO, |(_, asked)| asked.elapsed()),
        );
        state
            .shown
            .push((id.to_owned(), Issued::new(digits.clone(), left)));
        state.decided.push((id.to_owned(), true));
        Ok(ApprovalDigits::new(digits))
    }

    fn deny_incoming(&self, id: &str) -> Result<(), DeviceError> {
        let life = self.incoming_life();
        let mut state = self.lock();
        let live = state
            .incoming
            .iter()
            .any(|(request, asked)| request.id == id && asked.elapsed() < life);
        if !live {
            return Err(DeviceError::RequestGone);
        }
        state.incoming.retain(|(request, _)| request.id != id);
        state.shown.retain(|(request, _)| request != id);
        state.decided.push((id.to_owned(), false));
        Ok(())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn email(&self) -> Result<String, DeviceError> {
        self.lock().session.clone().ok_or(DeviceError::Unauthorized)
    }
}

#[async_trait]
impl AccountApi for DevAccount {
    fn invite_required(&self) -> bool {
        true
    }

    async fn check_invite(&self, invite: &str) -> Result<(), DeviceError> {
        if invite == INVITE {
            Ok(())
        } else {
            Err(DeviceError::InviteRequired)
        }
    }

    async fn start_sign_up(
        &self,
        invite: &str,
        email: &str,
        password: &Password,
    ) -> Result<(), DeviceError> {
        self.check_invite(invite).await?;
        // The Engine answers 428 when one network starts too many sign-ups in an hour, and
        // the client then does a short proof of work. The stand-in only takes its time.
        match email {
            "busy@example.com" => tokio::time::sleep(Duration::from_millis(1500)).await,
            "hard@example.com" => {
                tokio::time::sleep(Duration::from_millis(300)).await;
                return Err(DeviceError::ProofFailed);
            }
            _ => {}
        }
        // The account is made here and the code is emailed; the sign-in waits for the code.
        self.lock().signup = Some(SignUp {
            email: email.to_owned(),
            password: password.expose().to_owned(),
            wrong_codes: 0,
        });
        Ok(())
    }

    async fn resend_code(&self) -> Result<(), DeviceError> {
        match self.lock().signup.as_mut() {
            Some(signup) => {
                // A new code replaces the old one, so the wrong tries start again.
                signup.wrong_codes = 0;
                Ok(())
            }
            None => Err(DeviceError::InvalidCode),
        }
    }

    async fn verify_email_code(&self, code: &str) -> Result<SessionInfo, DeviceError> {
        let mut state = self.lock();
        let Some(signup) = state.signup.as_mut() else {
            return Err(DeviceError::InvalidCode);
        };
        // Every failure answers the same way: a wrong code, a spent limit, no code at all.
        if signup.wrong_codes >= EMAIL_CODE_TRIES {
            return Err(DeviceError::InvalidCode);
        }
        if code != EMAIL_CODE {
            signup.wrong_codes += 1;
            return Err(DeviceError::InvalidCode);
        }
        let Some(signup) = state.signup.take() else {
            return Err(DeviceError::InvalidCode);
        };
        let email = signup.email.clone();
        // The Engine makes no live account before the code, and never replaces one that exists: a
        // sign-up for an address that has an account ends here, with the same answer as a wrong
        // code, and the account it met is untouched (issue 489, item 5).
        if state
            .accounts
            .iter()
            .any(|account| account.email.eq_ignore_ascii_case(&email))
        {
            return Err(DeviceError::InvalidCode);
        }
        state.accounts.push(Account {
            email: email.clone(),
            password: signup.password.clone(),
        });
        state.session = Some(email.clone());
        Ok(session(&email))
    }

    async fn sign_in(
        &self,
        email: &str,
        password: &Password,
    ) -> Result<SignInOutcome, DeviceError> {
        match email {
            "slow@example.com" => {
                return Err(DeviceError::RateLimited {
                    retry_after: Duration::from_secs(90),
                });
            }
            "down@example.com" => return Err(DeviceError::Offline),
            _ => {}
        }
        let mut state = self.lock();
        let known = state
            .accounts
            .iter()
            .any(|a| a.email.eq_ignore_ascii_case(email.trim()) && a.password == password.expose());
        if !known {
            return Err(DeviceError::WrongPassword);
        }
        if email == "two@example.com" {
            state.pending_second_factor = Some(email.to_owned());
            return Ok(SignInOutcome::SecondFactor);
        }
        let canned = match email {
            "approve@example.com" => Some(Some(Answer::Allow)),
            "deny@example.com" => Some(Some(Answer::Deny)),
            "late@example.com" => Some(None),
            _ => None,
        };
        if let Some(canned) = canned {
            let delay = *self
                .auto_delay
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.mine = Some(Mine {
                id: "req-mine".to_owned(),
                email: email.to_owned(),
                answer: None,
                issued: None,
                auto: canned.map(|answer| (Instant::now() + delay, answer)),
            });
            return Ok(SignInOutcome::AwaitingApproval(PendingApproval {
                request_id: "req-mine".to_owned(),
            }));
        }
        state.session = Some(email.to_owned());
        Ok(SignInOutcome::Signed(session(email)))
    }

    async fn second_factor(&self, code: &str) -> Result<SignInOutcome, DeviceError> {
        let mut state = self.lock();
        let normalized = code.replace('-', "").to_ascii_uppercase();
        if state.pending_second_factor.is_some() && SECOND_FACTOR.contains(&normalized.as_str()) {
            let email = state.pending_second_factor.take().unwrap_or_default();
            state.session = Some(email.clone());
            Ok(SignInOutcome::Signed(session(&email)))
        } else {
            Err(DeviceError::InvalidCode)
        }
    }

    async fn approval_status(&self, request_id: &str) -> Result<ApprovalStatus, DeviceError> {
        let mut state = self.lock();
        let Some(mine) = state.mine.as_mut().filter(|m| m.id == request_id) else {
            return Err(DeviceError::RequestGone);
        };
        if let (None, Some((at, answer))) = (mine.answer, mine.auto)
            && Instant::now() >= at
        {
            decide(mine, answer);
        }
        // Allowing never signs the new device in: only the right digits do.
        let locked = mine
            .issued
            .as_ref()
            .is_some_and(|issued| issued.tries_left() == 0);
        match mine.answer {
            None => Ok(ApprovalStatus::Waiting),
            Some(Answer::Deny) => Ok(ApprovalStatus::Denied),
            Some(Answer::Allow) if locked => Ok(ApprovalStatus::Locked),
            Some(Answer::Allow) => Ok(ApprovalStatus::Allowed),
        }
    }

    async fn cancel_approval(&self, request_id: &str) -> Result<(), DeviceError> {
        let mut state = self.lock();
        if state.mine.as_ref().is_some_and(|m| m.id == request_id) {
            state.mine = None;
            state.cancelled.push(request_id.to_owned());
        }
        Ok(())
    }

    async fn approval_requests(&self) -> Result<Vec<IncomingApproval>, DeviceError> {
        self.email()?;
        let life = self.incoming_life();
        Ok(self
            .lock()
            .incoming
            .iter()
            .filter(|(_, asked)| asked.elapsed() < life)
            // No time left is reported: the Engine does not expose one yet.
            .map(|(request, _)| IncomingApproval {
                expires_in_secs: 0,
                ..request.clone()
            })
            .collect())
    }

    async fn approve(&self, request_id: &str) -> Result<ApprovalDigits, DeviceError> {
        self.email()?;
        self.allow_incoming(request_id)
    }

    async fn deny(&self, request_id: &str) -> Result<(), DeviceError> {
        self.email()?;
        self.deny_incoming(request_id)
    }

    async fn finish_approval(
        &self,
        request_id: &str,
        digits: &ApprovalDigits,
    ) -> Result<SessionInfo, DeviceError> {
        let mut state = self.lock();
        let Some(mine) = state.mine.as_mut().filter(|m| m.id == request_id) else {
            return Err(DeviceError::RequestGone);
        };
        // Only a request a trusted device allowed has digits; nothing signs in without them.
        let (Some(Answer::Allow), Some(issued)) = (mine.answer, mine.issued.as_mut()) else {
            return Err(DeviceError::RequestGone);
        };
        match issued.check(digits.expose()) {
            Check::Accepted => {
                let email = mine.email.clone();
                state.mine = None;
                state.session = Some(email.clone());
                Ok(session(&email))
            }
            Check::Wrong { tries_left } => Err(DeviceError::WrongDigits { tries_left }),
            Check::Locked => Err(DeviceError::DigitsLocked),
            Check::Expired => Err(DeviceError::RequestGone),
        }
    }

    async fn verify_password(&self, password: &Password) -> Result<(), DeviceError> {
        let email = self.email()?;
        let mut state = self.lock();
        if state.wrong_passwords >= WRONG_PASSWORDS_BEFORE_LIMIT {
            return Err(DeviceError::RateLimited {
                retry_after: Duration::from_secs(600),
            });
        }
        let right = state
            .accounts
            .iter()
            .any(|a| a.email == email && a.password == password.expose());
        if right {
            state.wrong_passwords = 0;
            Ok(())
        } else {
            state.wrong_passwords += 1;
            Err(DeviceError::WrongPassword)
        }
    }

    async fn devices(&self) -> Result<Vec<AccountDevice>, DeviceError> {
        self.email()?;
        let removed = self.lock().removed.clone();
        let mut list = vec![device(
            "dev-this",
            "This computer",
            "macOS 15",
            "engineering",
            "2026-07-12T09:00:00Z",
            true,
        )];
        for other in [
            device(
                "dev-phone",
                "iPhone",
                "iOS 18",
                "planner",
                "2026-08-08T09:00:00Z",
                false,
            ),
            device(
                "dev-mini",
                "Mac mini",
                "macOS 15",
                "engineering",
                "2026-09-26T09:00:00Z",
                false,
            ),
        ] {
            if !removed.contains(&other.id) {
                list.push(other);
            }
        }
        Ok(list)
    }

    async fn remove_device(&self, id: &str) -> Result<(), DeviceError> {
        self.email()?;
        if id == "dev-this" {
            return Err(DeviceError::Server { retryable: false });
        }
        self.lock().removed.push(id.to_owned());
        Ok(())
    }

    async fn sign_out_everywhere(&self) -> Result<(), DeviceError> {
        let email = self.email()?;
        if email == "delete@example.com" {
            return Err(DeviceError::DeletionScheduled);
        }
        self.lock().session = None;
        Ok(())
    }

    async fn sign_out(&self) -> Result<(), DeviceError> {
        self.lock().session = None;
        Ok(())
    }
}
