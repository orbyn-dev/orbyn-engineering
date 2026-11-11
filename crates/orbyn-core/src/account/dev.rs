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
//! | emailed code `ABCD-EFGH` | accepted; any other is `InvalidCode` |
//! | `dev@example.com` and the password below | signed in |
//! | `two@example.com` | asks for a code: `123456` or the recovery code `ABCDE-FGH23` |
//! | `slow@example.com` | `RateLimited` for 90 seconds |
//! | `down@example.com` | `Offline` |
//! | `busy@example.com` | sign-up asks for a visible check first (1.5 s), then goes on |
//! | `hard@example.com` | sign-up's check fails (`ProofFailed`) |
//! | `delete@example.com` | signs in; Sign out everywhere says deletion is scheduled |
//! | `approve@example.com` | asks a trusted device (digits `418 207`), which allows it 3 s later |
//! | `deny@example.com` | the same, and the trusted device refuses |
//! | `mismatch@example.com` | the same, and the trusted device says the codes differ |
//! | `late@example.com` | the same, and nobody ever answers |
//!
//! Signed in, one request from an iPad waits for this device to answer (`approval_requests`).

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use orbyn_contracts::AccountDevice;

use super::api::{
    AccountApi, ApprovalStatus, Backend, IncomingApproval, Password, PendingApproval, SessionInfo,
    SignInOutcome,
};
use super::device_api::DeviceError;
use super::fake::FakeDeviceApi;

pub const INVITE: &str = "K7QM2XWH9QXA3C2D";
pub const EMAIL_CODE: &str = "ABCDEFGH";
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
    CodesDiffer,
}

/// This stand-in's own request to sign in, as the new device made it.
struct Mine {
    id: String,
    email: String,
    answer: Option<Answer>,
    /// A canned answer that arrives by itself, for a developer driving the screens by hand.
    auto: Option<(Instant, Answer)>,
}

#[derive(Default)]
struct State {
    mine: Option<Mine>,
    /// Requests from other devices, waiting for this one (as a trusted device) to answer.
    incoming: Vec<(IncomingApproval, Instant)>,
    /// What this device answered, newest last: the request id and whether it allowed it.
    decided: Vec<(String, Option<bool>)>,
    cancelled: Vec<String>,
    accounts: Vec<Account>,
    signup: Option<String>,
    verified: bool,
    pending_second_factor: Option<String>,
    session: Option<String>,
    removed: Vec<String>,
    wrong_passwords: u8,
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
        "mismatch@example.com",
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
                    digits: "418207".to_owned(),
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
    seen: &str,
    current: bool,
) -> AccountDevice {
    AccountDevice {
        id: id.into(),
        name: name.into(),
        platform: platform.into(),
        app: app.into(),
        trusted_since: since.into(),
        last_seen: seen.into(),
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

    #[cfg(test)]
    pub fn zones(&self) -> Arc<FakeDeviceApi> {
        self.zones.clone()
    }

    /// Plays the trusted device answering this device's own request.
    #[cfg(test)]
    pub fn answer(&self, answer: Answer) {
        if let Some(mine) = self.lock().mine.as_mut() {
            mine.answer = Some(answer);
        }
    }

    /// The requests this device gave up.
    #[cfg(test)]
    pub fn cancelled(&self) -> Vec<String> {
        self.lock().cancelled.clone()
    }

    /// What this device answered as a trusted one: the request id, and `Some(true)` for
    /// allowed, `Some(false)` for refused with the codes differing, `None` for refused.
    #[cfg(test)]
    pub fn decided(&self) -> Vec<(String, Option<bool>)> {
        self.lock().decided.clone()
    }

    fn incoming_life(&self) -> Duration {
        *self
            .incoming_life
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// A request is answered once, while it lasts: an expired, answered or given-up one is gone.
    fn answer_incoming(&self, id: &str, decision: Option<bool>) -> Result<(), DeviceError> {
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
        state.decided.push((id.to_owned(), decision));
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

    async fn start_sign_up(&self, invite: &str, email: &str) -> Result<(), DeviceError> {
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
        let mut state = self.lock();
        state.signup = Some(email.to_owned());
        state.verified = false;
        Ok(())
    }

    async fn resend_code(&self) -> Result<(), DeviceError> {
        self.lock()
            .signup
            .as_ref()
            .map(|_| ())
            .ok_or(DeviceError::InvalidCode)
    }

    async fn verify_email_code(&self, code: &str) -> Result<(), DeviceError> {
        let mut state = self.lock();
        if state.signup.is_some() && code == EMAIL_CODE {
            state.verified = true;
            Ok(())
        } else {
            Err(DeviceError::InvalidCode)
        }
    }

    async fn register(&self, password: &Password) -> Result<SessionInfo, DeviceError> {
        let mut state = self.lock();
        if !state.verified {
            return Err(DeviceError::InvalidCode);
        }
        let email = state.signup.take().ok_or(DeviceError::InvalidCode)?;
        state.accounts.retain(|account| account.email != email);
        state.accounts.push(Account {
            email: email.clone(),
            password: password.expose().to_owned(),
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
            .any(|a| a.email == email && a.password == password.expose());
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
            "mismatch@example.com" => Some(Some(Answer::CodesDiffer)),
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
                auto: canned.map(|answer| (Instant::now() + delay, answer)),
            });
            return Ok(SignInOutcome::AwaitingApproval(PendingApproval {
                request_id: "req-mine".to_owned(),
                digits: "418207".to_owned(),
            }));
        }
        state.session = Some(email.to_owned());
        Ok(SignInOutcome::Signed(session(email)))
    }

    async fn second_factor(&self, code: &str) -> Result<SessionInfo, DeviceError> {
        let mut state = self.lock();
        let normalized = code.replace('-', "").to_ascii_uppercase();
        if state.pending_second_factor.is_some() && SECOND_FACTOR.contains(&normalized.as_str()) {
            let email = state.pending_second_factor.take().unwrap_or_default();
            state.session = Some(email.clone());
            Ok(session(&email))
        } else {
            Err(DeviceError::InvalidCode)
        }
    }

    async fn approval_status(&self, request_id: &str) -> Result<ApprovalStatus, DeviceError> {
        let mut state = self.lock();
        let Some(mine) = state.mine.as_mut().filter(|m| m.id == request_id) else {
            return Err(DeviceError::InvalidCode);
        };
        if let (None, Some((at, answer))) = (mine.answer, mine.auto)
            && Instant::now() >= at
        {
            mine.answer = Some(answer);
        }
        let email = mine.email.clone();
        match mine.answer {
            None => Ok(ApprovalStatus::Waiting),
            Some(Answer::Deny) => Ok(ApprovalStatus::Denied),
            Some(Answer::CodesDiffer) => Ok(ApprovalStatus::CodesDiffer),
            Some(Answer::Allow) => {
                state.mine = None;
                state.session = Some(email.clone());
                Ok(ApprovalStatus::Approved(session(&email)))
            }
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
            .map(|(request, asked)| IncomingApproval {
                expires_in_secs: life.saturating_sub(asked.elapsed()).as_secs(),
                ..request.clone()
            })
            .collect())
    }

    async fn approve(&self, request_id: &str) -> Result<(), DeviceError> {
        self.email()?;
        self.answer_incoming(request_id, Some(true))
    }

    async fn deny(&self, request_id: &str, codes_differ: bool) -> Result<(), DeviceError> {
        self.email()?;
        self.answer_incoming(request_id, if codes_differ { Some(false) } else { None })
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
            "2026-11-11T09:38:00Z",
            true,
        )];
        for other in [
            device(
                "dev-phone",
                "iPhone",
                "iOS 18",
                "planner",
                "2026-08-08T09:00:00Z",
                "2026-11-11T09:36:00Z",
                false,
            ),
            device(
                "dev-mini",
                "Mac mini",
                "macOS 15",
                "engineering",
                "2026-09-26T09:00:00Z",
                "2026-11-10T18:00:00Z",
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
