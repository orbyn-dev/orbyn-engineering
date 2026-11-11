//! The Orbyn account: settings that follow the person across their computers. Phase 1
//! builds the sync core only: the record, its merge, the allowlist-only apply, the
//! local store and outbox, and one worker per zone, all against an adapter shaped like
//! the Engine's `DeviceApi`. There is no network, sign-in, keychain or route here.
//! The design is [docs/account.md](../../../../docs/account.md) and its
//! [threat model](../../../../docs/account-threat-model.md).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::error::{CoreError, Result};
use crate::store::{Store, SyncConflict};

pub mod api;
pub mod apply;
pub mod changes;
pub mod codes;
pub mod device_api;
pub mod flow;
pub mod merge;
pub mod policy;
pub mod settings_record;
pub mod status;
pub mod sync;

#[cfg(any(test, debug_assertions))]
mod dev;
#[cfg(any(test, debug_assertions))]
mod fake;
#[cfg(test)]
mod tests;

pub use api::{AccountApi, Backend, Password, SessionInfo, SignInOutcome};
pub use device_api::{DeviceApi, DeviceError, DeviceStatus, Version, ZoneSnapshot};
pub use flow::{APPROVAL_LIFETIME, ConfirmFor, Confirmed};
pub use status::{AccountStatus, PauseReason, SignOutReason, StatusHub, SyncState};
pub use sync::{SyncHandle, SyncTiming};

/// Whether the account may do anything. Real sign-in stays `Off` until every
/// condition in [docs/account.md](../../../../docs/account.md#sign-in-gate) holds, and
/// `On` is a separate, reviewed change that does not exist yet. Under `Off` no worker
/// starts, nothing is queued and no request goes to any Engine.
///
/// `LocalOnly` is for tests and local development against a loopback Engine with
/// throwaway accounts, on the three conditions in that rule: no non-loopback Engine
/// (the address check belongs to the sign-in change), no real sign-in, and this
/// comment. It is compiled out of release builds, so a release can only be `Off`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AccountGate {
    #[default]
    Off,
    #[cfg(any(test, debug_assertions))]
    LocalOnly,
}

/// How this computer's settings join the account's the first time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FirstSync {
    /// One setting at a time. The account's value wins where it has one; settings only this
    /// computer has go up. The default.
    #[default]
    Combine,
    /// This computer's settings are as new as an edit made now, so they win everywhere.
    UseDevice,
    /// This computer's settings are not uploaded; the account's are applied.
    UseAccount,
}

/// Which backend the account talks to. `Development` is an in-memory stand-in for the
/// Engine, for debug builds and tests only; it is compiled out of release builds, so a
/// release has none, and the real device client is not part of this build yet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AccountBackendKind {
    #[default]
    None,
    #[cfg(any(test, debug_assertions))]
    Development,
}

// A release build has one gate value and one backend. Adding a variant outside the debug
// cfg makes these matches non-exhaustive and breaks the release build, so `LocalOnly`, the
// development stand-in and `ORBYN_ACCOUNT_DEV` cannot be selected in a release.
#[cfg(not(any(test, debug_assertions)))]
const _: () = {
    #[allow(dead_code)]
    fn only_off(gate: AccountGate) {
        match gate {
            AccountGate::Off => {}
        }
    }
    #[allow(dead_code)]
    fn only_none(kind: AccountBackendKind) {
        match kind {
            AccountBackendKind::None => {}
        }
    }
};

/// Where the person's keybindings file is and the lock every writer of it takes.
#[derive(Clone)]
pub(crate) struct KeybindingFile {
    pub path: PathBuf,
    pub writer: Arc<Mutex<()>>,
}

/// The one handle `Core` holds for the account.
pub struct AccountCenter {
    gate: AccountGate,
    backend: Option<Backend>,
    store: Arc<Store>,
    keybindings: KeybindingFile,
    status: Arc<StatusHub>,
    inner: tokio::sync::Mutex<flow::Inner>,
    confirmed: Mutex<Option<flow::Grant>>,
    approval_life: Mutex<std::time::Duration>,
    /// Whether the person has been told the service ended their session, and whether they ended
    /// it themselves everywhere. In memory only, and reset by the next sign-in.
    ended: Mutex<flow::Ended>,
    /// The development stand-in itself, so a test can play the trusted device.
    #[cfg(test)]
    stand_in: Option<Arc<dev::DevAccount>>,
}

impl AccountCenter {
    pub(crate) fn new(
        gate: AccountGate,
        kind: AccountBackendKind,
        store: Arc<Store>,
        keybindings_path: PathBuf,
        keybinding_writer: Arc<Mutex<()>>,
    ) -> Self {
        #[cfg(test)]
        let mut stand_in = None;
        let backend = match (gate, kind) {
            #[cfg(any(test, debug_assertions))]
            (AccountGate::LocalOnly, AccountBackendKind::Development) => {
                let (backend, _handle) = dev::handle();
                #[cfg(test)]
                {
                    stand_in = Some(_handle);
                }
                Some(backend)
            }
            _ => None,
        };
        Self {
            gate,
            backend,
            #[cfg(test)]
            stand_in,
            store,
            keybindings: KeybindingFile {
                path: keybindings_path,
                writer: keybinding_writer,
            },
            status: Arc::new(StatusHub::default()),
            inner: tokio::sync::Mutex::default(),
            confirmed: Mutex::new(None),
            approval_life: Mutex::new(flow::APPROVAL_LIFETIME),
            ended: Mutex::new(flow::Ended::default()),
        }
    }

    #[cfg(test)]
    pub(crate) fn stand_in(&self) -> Arc<dev::DevAccount> {
        self.stand_in
            .clone()
            .expect("a core on the development stand-in")
    }

    #[cfg(test)]
    pub(crate) fn set_approval_life(&self, life: std::time::Duration) {
        *self
            .approval_life
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = life;
    }

    pub fn gate(&self) -> AccountGate {
        self.gate
    }

    /// Lets status changes reach the windows over the shell event stream.
    pub(crate) fn set_notify(&self, notify: &crate::util::swr::Notify) {
        self.status.set_notify(notify.clone());
    }

    pub fn status(&self) -> AccountStatus {
        self.status.current()
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<AccountStatus> {
        self.status.subscribe()
    }

    /// The offline edits that lost, newest last, for the person to look at.
    pub fn conflicts(&self) -> Result<Vec<SyncConflict>> {
        self.store.account_conflicts(settings_record::ZONE)
    }

    pub fn clear_conflicts(&self) -> Result<()> {
        self.store.account_clear_conflicts(settings_record::ZONE)?;
        self.status.update(|status| status.conflicts = 0);
        Ok(())
    }

    /// Starts the worker for the settings zone against `api`. Settings the person has
    /// set so far are queued the first time, and every later change is queued as it is
    /// made. Nothing here waits for the network: the worker is a background task.
    /// Answers `account_disabled` under the `Off` gate, before anything is queued.
    pub fn start_settings_sync(
        &self,
        api: Arc<dyn DeviceApi>,
        timing: SyncTiming,
    ) -> Result<SyncHandle> {
        self.start_first_sync(api, timing, FirstSync::Combine)
    }

    /// As [`Self::start_settings_sync`], with the person's choice for how this computer's
    /// settings join the account's the first time. After the first time the choice is moot:
    /// the computer already has a record.
    pub fn start_first_sync(
        &self,
        api: Arc<dyn DeviceApi>,
        timing: SyncTiming,
        first: FirstSync,
    ) -> Result<SyncHandle> {
        if self.gate == AccountGate::Off {
            return Err(CoreError::AccountDisabled);
        }
        let seed = if first == FirstSync::UseAccount {
            Vec::new()
        } else {
            self.seed()?
        };
        self.store
            .account_enable(settings_record::ZONE, &seed, first == FirstSync::UseDevice)?;
        Ok(sync::spawn(sync::ZoneWorker {
            zone: settings_record::ZONE,
            store: self.store.clone(),
            api,
            status: self.status.clone(),
            keybindings: self.keybindings.clone(),
            timing,
        }))
    }

    /// Whether this computer has settings of its own to join to the account's.
    pub(crate) fn has_settings(&self) -> Result<bool> {
        Ok(!self.seed()?.is_empty())
    }

    /// What the person has set so far, as first-sync fields: only person-level values of
    /// the allowlisted keys, and their own keybindings.
    fn seed(&self) -> Result<Vec<settings_record::Staged>> {
        let snapshot = self.store.settings_snapshot()?;
        let mut staged = Vec::new();
        for key in settings_record::SYNCED_KEYS {
            let stored = snapshot
                .globals
                .get(&format!("pref:{key}"))
                .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok());
            if let Some(value) = stored {
                staged.extend(settings_record::stage_preference(key, None, Some(&value)));
            }
        }
        staged.extend(
            crate::settings::keybindings::own_rules(&self.keybindings.path)
                .iter()
                .map(settings_record::stage_keybinding),
        );
        Ok(staged)
    }
}
