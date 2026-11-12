//! One worker per zone. It pulls, merges, applies, and uploads the outbox against the
//! adapter, then sleeps until a live notice, a local change or its backoff timer wakes
//! it. Its failures end in a status, never in an error to a caller, and nothing a
//! person does (a settings write, a run, a request) ever waits on it: they only touch
//! the local store. A failing zone backs off by itself; other zones are unaffected.

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::KeybindingFile;
use super::apply;
use super::device_api::{DeviceApi, DeviceError, DeviceStatus, ZoneSnapshot};
use super::merge::Record;
use super::settings_record::{self as names, MAX_RECORD_BYTES, record_bytes};
use super::status::{PauseReason, SignOutReason, StatusHub, SyncState};
use crate::error::CoreError;
use crate::store::Store;

/// Upload rounds in one cycle before giving the zone a rest, so edits that keep
/// arriving cannot hold the worker in a loop.
const MAX_ROUNDS: usize = 5;

#[derive(Clone, Copy, Debug)]
pub struct SyncTiming {
    pub backoff_base: Duration,
    pub backoff_max: Duration,
}

impl Default for SyncTiming {
    fn default() -> Self {
        Self {
            backoff_base: Duration::from_secs(2),
            backoff_max: Duration::from_secs(300),
        }
    }
}

pub(super) struct ZoneWorker {
    pub zone: &'static str,
    pub store: Arc<Store>,
    pub api: Arc<dyn DeviceApi>,
    pub status: Arc<StatusHub>,
    pub keybindings: KeybindingFile,
    pub timing: SyncTiming,
}

/// Stops its worker when dropped, or when `stop` is awaited.
pub struct SyncHandle {
    cancel: CancellationToken,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl SyncHandle {
    pub async fn stop(mut self) {
        self.cancel.cancel();
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

impl Drop for SyncHandle {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

pub(super) fn spawn(worker: ZoneWorker) -> SyncHandle {
    let cancel = CancellationToken::new();
    let task = tokio::spawn(worker.run(cancel.clone()));
    SyncHandle {
        cancel,
        task: Some(task),
    }
}

enum Flow {
    /// Nothing more to do until something wakes the worker.
    Idle,
    Retry(Duration),
    Stop,
}

/// Why a cycle ended early.
enum Failure {
    Device(DeviceError),
    Local(CoreError),
    /// More edits arrived than a cycle will chase.
    Busy,
}

impl From<DeviceError> for Failure {
    fn from(error: DeviceError) -> Self {
        Self::Device(error)
    }
}

impl From<CoreError> for Failure {
    fn from(error: CoreError) -> Self {
        Self::Local(error)
    }
}

/// A receiver that never fires once its sender is gone, instead of firing for ever.
async fn changed<T>(receiver: &mut watch::Receiver<T>) {
    if receiver.changed().await.is_err() {
        std::future::pending::<()>().await;
    }
}

impl ZoneWorker {
    async fn run(self, cancel: CancellationToken) {
        let mut local = self.store.subscribe_account_changes();
        let mut notices = self.api.notices();
        let mut device = self.api.status();
        let (mut failures, mut offline_since) = (0u32, None::<i64>);
        loop {
            local.borrow_and_update();
            notices.borrow_and_update();
            device.borrow_and_update();
            let wait = match self.cycle(&mut failures, &mut offline_since).await {
                Flow::Stop => return,
                Flow::Idle => None,
                Flow::Retry(delay) => Some(delay),
            };
            tokio::select! {
                () = cancel.cancelled() => return,
                () = changed(&mut local) => {}
                () = changed(&mut notices) => {}
                () = changed(&mut device) => {}
                () = async {
                    match wait {
                        Some(delay) => tokio::time::sleep(delay).await,
                        None => std::future::pending().await,
                    }
                } => {}
            }
        }
    }

    /// Counts that are true whatever the network does.
    fn counts(&self) -> (usize, usize, usize, usize) {
        let waiting = self.store.account_outbox_len(self.zone).unwrap_or(0);
        let skipped = self.store.account_skipped_len(self.zone).unwrap_or(0);
        let conflicts = self
            .store
            .account_conflicts(self.zone)
            .map_or(0, |list| list.len());
        let held_back = self.store.account_held_back_len(self.zone).unwrap_or(0);
        (waiting, skipped, conflicts, held_back)
    }

    fn show(&self, state: SyncState) {
        let (waiting, not_synced, conflicts, held_back) = self.counts();
        self.status.publish(super::AccountStatus {
            state,
            waiting,
            not_synced,
            conflicts,
            held_back,
            last_synced: None,
        });
    }

    async fn cycle(&self, failures: &mut u32, offline_since: &mut Option<i64>) -> Flow {
        let client = self.api.status().borrow().clone();
        match client {
            DeviceStatus::SignedOut => {
                self.show(SyncState::SignedOut { reason: None });
                return Flow::Idle;
            }
            DeviceStatus::SigningIn => {
                self.show(SyncState::SigningIn);
                return Flow::Idle;
            }
            DeviceStatus::Paused { .. } => {
                self.show(SyncState::Paused {
                    reason: PauseReason::ZoneFailed,
                });
                return Flow::Idle;
            }
            DeviceStatus::Online | DeviceStatus::Offline { .. } => {}
        }
        self.show(SyncState::Syncing);
        match self.sync_once().await {
            Ok(keyboard_ok) => {
                *failures = 0;
                *offline_since = None;
                self.show(if keyboard_ok {
                    SyncState::Online
                } else {
                    SyncState::Paused {
                        reason: PauseReason::InvalidKeyboardFile,
                    }
                });
                Flow::Idle
            }
            Err(failure) => self.failed(failure, failures, offline_since),
        }
    }

    /// The client's errors as app behaviour: see the table in docs/account.md.
    fn failed(
        &self,
        failure: Failure,
        failures: &mut u32,
        offline_since: &mut Option<i64>,
    ) -> Flow {
        let pause = |reason| {
            self.show(SyncState::Paused { reason });
            Flow::Idle
        };
        match failure {
            Failure::Device(DeviceError::Offline | DeviceError::Server { retryable: true })
            | Failure::Busy => {
                *failures += 1;
                let since = *offline_since.get_or_insert_with(crate::util::unix_now);
                self.show(SyncState::Offline { since_unix: since });
                Flow::Retry(self.backoff(*failures))
            }
            Failure::Device(DeviceError::RateLimited { retry_after }) => {
                self.show(SyncState::Throttled {
                    retry_after_secs: retry_after.as_secs(),
                });
                Flow::Retry(retry_after)
            }
            Failure::Device(DeviceError::TooLarge) => pause(PauseReason::TooLarge),
            Failure::Device(DeviceError::ProtectionAdvanced) => {
                pause(PauseReason::ProtectionAdvanced)
            }
            Failure::Device(DeviceError::DeletionScheduled) => {
                pause(PauseReason::DeletionScheduled)
            }
            Failure::Device(DeviceError::Unauthorized) => {
                // Destroying key material belongs to the sign-out path; settings and the
                // outbox stay, so signing in again resumes where this left off.
                self.show(SyncState::SignedOut {
                    reason: Some(SignOutReason::SessionEnded),
                });
                Flow::Stop
            }
            Failure::Device(DeviceError::AccountGone) => {
                self.show(SyncState::SignedOut {
                    reason: Some(SignOutReason::AccountGone),
                });
                Flow::Stop
            }
            Failure::Device(
                DeviceError::Server { retryable: false }
                | DeviceError::InvalidKey
                | DeviceError::Conflict(_)
                | DeviceError::InviteRequired
                | DeviceError::WrongPassword
                | DeviceError::InvalidCode
                | DeviceError::RequestGone
                | DeviceError::WrongDigits { .. }
                | DeviceError::DigitsLocked
                | DeviceError::ProofFailed,
            ) => pause(PauseReason::ZoneFailed),
            Failure::Local(error) => {
                tracing::warn!(
                    code = error.code(),
                    zone = self.zone,
                    "account sync could not use the local store"
                );
                *failures += 1;
                self.show(SyncState::Paused {
                    reason: PauseReason::ZoneFailed,
                });
                Flow::Retry(self.backoff(*failures))
            }
        }
    }

    /// Exponential backoff with jitter of up to a fifth either way.
    fn backoff(&self, failures: u32) -> Duration {
        let doubled = self
            .timing
            .backoff_base
            .saturating_mul(1u32 << failures.saturating_sub(1).min(16));
        let capped = doubled.min(self.timing.backoff_max);
        let jitter = crate::util::random_bytes::<1>().map_or(128, |bytes| bytes[0]);
        capped.mul_f64(0.8 + 0.4 * f64::from(jitter) / 255.0)
    }

    /// One pull, merge, apply and upload. `Ok(false)` means everything synced but the
    /// keybindings file could not be written, which the status says.
    async fn sync_once(&self) -> Result<bool, Failure> {
        // What the account held at the last pull or conflict, so a setting left out of an
        // upload can keep the value the account has.
        let mut remote = Record::new();
        if let Some(snapshot) = self.api.pull(self.zone).await? {
            self.merge(&snapshot)?;
            remote = snapshot.record;
        }
        let mut keyboard = self.apply()?;
        for _ in 0..MAX_ROUNDS {
            if self.store.account_outbox_len(self.zone)? == 0 {
                return Ok(keyboard);
            }
            let record = self.uploadable(&remote)?;
            let base = self.store.account_version(self.zone)?;
            match self.api.push(self.zone, base, &record).await {
                Ok(version) => {
                    self.store.account_pushed(self.zone, &record, version)?;
                }
                Err(DeviceError::Conflict(current)) => {
                    self.merge(&current)?;
                    remote = current.record;
                    keyboard &= self.apply()?;
                }
                Err(error) => return Err(error.into()),
            }
        }
        if self.store.account_outbox_len(self.zone)? == 0 {
            Ok(keyboard)
        } else {
            Err(Failure::Busy)
        }
    }

    /// The record to upload. The Engine contract uploads the whole record, so this is the
    /// merged record, not only this device's changes, and another device's value is never
    /// left out of it, with two deliberate exceptions: a set-element removal marker older
    /// than the horizon is compacted away, and a record still over the limit loses the
    /// largest settings waiting to upload (they stay local, the status counts them, and the
    /// account keeps the older value it holds, which is passed on but never applied here). A record
    /// that is too large without this device's changes pauses the zone.
    fn uploadable(&self, remote: &Record) -> Result<Record, Failure> {
        self.store
            .account_compact(self.zone, crate::util::unix_now_ms())?;
        let mut record = self.store.account_record(self.zone)?;
        while record_bytes(&record) > MAX_RECORD_BYTES {
            let queued = self.store.account_outbox_names(self.zone)?;
            let Some(biggest) = queued
                .into_iter()
                .filter(|name| record.contains_key(name))
                .max_by_key(|name| {
                    record_bytes(&Record::from([(name.clone(), record[name].clone())]))
                })
            else {
                return Err(DeviceError::TooLarge.into());
            };
            self.store.account_keep_local(self.zone, &biggest)?;
            // The account keeps the value it has; only this device's newer one stays here.
            match remote.get(&biggest) {
                Some(held) => record.insert(biggest, held.clone()),
                None => record.remove(&biggest),
            };
        }
        Ok(record)
    }

    fn merge(&self, snapshot: &ZoneSnapshot) -> Result<(), CoreError> {
        let report = self
            .store
            .account_merge(self.zone, &snapshot.record, snapshot.version)?;
        if report.conflicts > 0 {
            self.show(SyncState::Syncing);
        }
        Ok(())
    }

    /// Applies what the account changed since this device last applied it: only the
    /// allowlist, through the apply plan, in one transaction with the read, and without
    /// queuing anything again. A field is applied once, so nothing the person did here
    /// since (a value over its limit, a keybindings file edited by hand, a change that
    /// failed to queue) is put back. Returns whether the keybindings file took its rules.
    fn apply(&self) -> Result<bool, CoreError> {
        let (rules, names) = self.store.change_settings_synced(|snapshot| {
            let stored = |key: &str| {
                snapshot
                    .globals
                    .get(&format!("pref:{key}"))
                    .and_then(|text| serde_json::from_str::<Value>(text).ok())
            };
            let plan = apply::plan(&snapshot.account);
            let mut writes = Vec::new();
            for (key, value) in &plan.values {
                if stored(key) != *value {
                    crate::settings::preferences::prepare_synced_write(
                        &mut writes,
                        key,
                        value.as_ref(),
                    );
                }
            }
            for (key, edit) in &plan.sets {
                let before = stored(key);
                let list = Value::Array(apply::edited_list(before.as_ref(), edit));
                let valid = crate::settings::preferences::check_synced(key, &list).is_ok();
                let unchanged = before.as_ref() == Some(&list)
                    || (before.is_none() && list.as_array().is_some_and(Vec::is_empty));
                if valid && !unchanged {
                    crate::settings::preferences::prepare_synced_write(
                        &mut writes,
                        key,
                        Some(&list),
                    );
                }
            }
            let names: Vec<String> = snapshot
                .account
                .keys()
                .filter(|name| names::is_keybinding_name(name))
                .cloned()
                .collect();
            Ok(((plan.keybindings, names), writes))
        })?;
        if names.is_empty() {
            return Ok(true);
        }
        if rules.is_empty() {
            self.store.account_mark_applied(self.zone, &names)?;
            return Ok(true);
        }
        let wrote = crate::settings::keybindings::write_synced_rules(
            &self.keybindings.path,
            &self.keybindings.writer,
            &rules,
        );
        if wrote.is_ok() {
            self.store.account_mark_applied(self.zone, &names)?;
        }
        Ok(wrote.is_ok())
    }
}
