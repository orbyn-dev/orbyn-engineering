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

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use orbyn_contracts::AccountDevice;

use super::api::{AccountApi, Backend, Password, SessionInfo, SignInOutcome};
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

#[derive(Default)]
struct State {
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
}

pub fn backend() -> Backend {
    let accounts = [
        "dev@example.com",
        "two@example.com",
        "slow@example.com",
        "down@example.com",
        "delete@example.com",
    ]
    .map(|email| Account {
        email: email.to_owned(),
        password: PASSWORD.to_owned(),
    });
    Backend {
        account: Arc::new(DevAccount {
            state: Mutex::new(State {
                accounts: accounts.into(),
                ..State::default()
            }),
        }),
        device: Arc::new(FakeDeviceApi::new()),
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
