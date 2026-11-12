//! The sync half over the real client: the Engine's per-record model.
//!
//! The person's settings are one record, `person` of type `person_settings`, in the zone
//! `engineering.settings` (the Engine's `orbyn-engineering` names them; the app may not depend on
//! that crate, so they are spelled here, once). The Engine is the source of truth for the merge:
//! [`DeviceApi::push`] stages this device's changes with `Engine::stage`, whose merge decides what
//! wins, runs a pass, and answers with the Engine's merged record. Whatever differs from what was
//! sent comes back as [`DeviceError::Conflict`], so the worker takes the Engine's result into the
//! store, and its diff of the queued edits against that result is the "offline edit lost" list.
//!
//! A stamp crosses lossless: the app's `(wall_ms, counter, device)` is the Engine's `(millis,
//! counter, device)` with the device as sixteen hex digits, which the store keeps as this zone's
//! device name ([`DeviceApi::device_tag`]), so the two orders agree.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use orbyn_device_client as dc;
use sha2::{Digest, Sha256};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use super::errors::map;
use super::shared::Shared;
use crate::account::device_api::{DeviceApi, DeviceError, DeviceStatus, Version, ZoneSnapshot};
use crate::account::merge::{Field, Record, Stamp};
use crate::account::settings_record::ZONE;

const RECORD_TYPE: &str = "person_settings";
const RECORD_ID: &str = "person";

pub struct EngineZones<T, S, C> {
    shared: Arc<Shared<T, S, C>>,
    status: watch::Sender<DeviceStatus>,
    notices: watch::Sender<u64>,
    version: Mutex<(Version, Record)>,
    bridges: Mutex<Vec<JoinHandle<()>>>,
}

impl<T, S, C> Drop for EngineZones<T, S, C> {
    fn drop(&mut self) {
        for task in self
            .bridges
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .drain(..)
        {
            task.abort();
        }
    }
}

/// The Engine's record as the app's.
fn to_app(fields: Option<&dc::Fields>) -> Record {
    fields
        .map(|fields| {
            fields
                .0
                .iter()
                .filter(|(name, _)| !name.starts_with('_'))
                .map(|(name, field)| {
                    (
                        name.clone(),
                        Field {
                            value: field.value.clone(),
                            stamp: Stamp {
                                wall_ms: i64::try_from(field.stamp.millis).unwrap_or(i64::MAX),
                                counter: field.stamp.counter,
                                device: format!("{:016x}", field.stamp.device),
                            },
                        },
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The app's stamp as the Engine's. A device name that is not sixteen hex digits (one from before
/// this zone named its device by the Engine's) is hashed, so the mapping is total.
fn to_engine(stamp: &Stamp) -> dc::Stamp {
    let named = if stamp.device.len() == 16 {
        u64::from_str_radix(&stamp.device, 16).ok()
    } else {
        None
    };
    let device = named.unwrap_or_else(|| {
        let digest = Sha256::digest(stamp.device.as_bytes());
        u64::from_be_bytes(digest[..8].try_into().unwrap_or([0; 8]))
    });
    dc::Stamp {
        millis: u64::try_from(stamp.wall_ms).unwrap_or(0),
        counter: stamp.counter,
        device,
    }
}

/// Whether the Engine's record holds something the app's copy does not, apart from a removal
/// marker the app compacted away on purpose (the Engine keeps every field for ever).
fn differs(merged: &Record, sent: &Record, now_ms: i64) -> bool {
    merged.iter().any(|(name, field)| match sent.get(name) {
        Some(mine) => mine != field,
        None => !crate::store::is_old_tombstone(name, field, now_ms),
    })
}

impl<T, S, C> EngineZones<T, S, C>
where
    T: dc::DeviceApi + 'static,
    S: dc::KeyStore + 'static,
    C: dc::Clock + 'static,
{
    pub(super) fn new(shared: Arc<Shared<T, S, C>>) -> Self {
        Self {
            shared,
            status: watch::channel(DeviceStatus::Online).0,
            notices: watch::channel(0).0,
            version: Mutex::default(),
            bridges: Mutex::default(),
        }
    }

    /// Opens the Engine's state, so this device's stamps are named before sync is switched on.
    /// The sign-in paths do it; a caller that starts sync some other way must too.
    pub async fn open(&self) -> Result<(), DeviceError> {
        drop(self.shared.engine().await?);
        Ok(())
    }

    /// The client's status and live notices, forwarded as the worker reads them. Started with the
    /// first pass, which runs inside the runtime.
    fn start_bridges(&self) {
        let mut bridges = self
            .bridges
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !bridges.is_empty() {
            return;
        }
        let mut from = self.shared.client.status();
        let status = self.status.clone();
        bridges.push(tokio::spawn(async move {
            loop {
                let next = match from.borrow_and_update().state {
                    dc::SyncState::Offline => DeviceStatus::Offline {
                        since_unix: crate::util::unix_now(),
                    },
                    dc::SyncState::SignedOut => DeviceStatus::SignedOut,
                    // The errors that end a session come back from the pass itself, where the
                    // worker maps them; the status stays online so it keeps running them.
                    _ => DeviceStatus::Online,
                };
                // Only a real change wakes the worker, or its own passes would wake it again.
                status.send_if_modified(|now| {
                    let same = std::mem::discriminant(now) == std::mem::discriminant(&next);
                    if !same {
                        *now = next;
                    }
                    !same
                });
                if from.changed().await.is_err() {
                    return;
                }
            }
        }));
        // No stream on a transport that has no address, or before a session: passes still run.
        if let Ok(mut stream) = self.shared.client.notices(dc::NoticeConfig::default()) {
            let notices = self.notices.clone();
            bridges.push(tokio::spawn(async move {
                while let Some(notice) = stream.next().await {
                    if !matches!(notice, dc::Notice::Disconnected) {
                        notices.send_modify(|count| *count += 1);
                    }
                }
            }));
        }
    }

    /// One pass over the account, with this device's record staged first when there is one. The
    /// Engine's state is kept after it, whatever it ended in.
    async fn pass(&self, stage: Option<&Record>) -> Result<Record, DeviceError> {
        self.start_bridges();
        let mut guard = self.shared.engine().await?;
        let Some(engine) = guard.as_mut() else {
            return Err(DeviceError::Unauthorized);
        };
        if self.shared.owes_keys()
            && let Ok(devices) = self.shared.client.devices().await
        {
            let trusted: Vec<String> = devices
                .devices
                .iter()
                .filter(|d| d.state == dc::DeviceState::Trusted)
                .map(|d| d.id.clone())
                .collect();
            let ready = self.shared.settle(&trusted);
            if !ready.is_empty() {
                let zones: Vec<String> = engine.zones().map(str::to_owned).collect();
                let mut all = true;
                for zone in zones {
                    all &= self
                        .shared
                        .client
                        .send_keys(engine, &zone, &devices)
                        .await
                        .is_ok();
                }
                if all {
                    self.shared.paid(&ready);
                }
            }
        }
        if let Some(record) = stage {
            let ours = engine.record(ZONE, RECORD_ID);
            let mut changes = dc::Fields::default();
            let mut any = false;
            for (name, field) in record {
                let stamp = to_engine(&field.stamp);
                let same = ours
                    .and_then(|fields| fields.0.get(name))
                    .is_some_and(|held| held.value == field.value && held.stamp == stamp);
                if !same {
                    changes.set(name, field.value.clone(), stamp);
                    any = true;
                }
            }
            if any {
                engine
                    .stage(ZONE, RECORD_TYPE, RECORD_ID, &changes)
                    .map_err(|_| DeviceError::Server { retryable: false })?;
            }
        }
        let result = self.shared.client.sync_all(engine).await;
        self.shared.save(engine);
        result.map_err(map)?;
        Ok(to_app(engine.record(ZONE, RECORD_ID)))
    }

    /// The version moves when the record's content does, so the worker sees a change as one.
    fn versioned(&self, record: &Record) -> Version {
        let mut held = self
            .version
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if held.1 != *record {
            *held = (held.0 + 1, record.clone());
        }
        held.0
    }
}

#[async_trait]
impl<T, S, C> DeviceApi for EngineZones<T, S, C>
where
    T: dc::DeviceApi + 'static,
    S: dc::KeyStore + 'static,
    C: dc::Clock + 'static,
{
    async fn pull(&self, zone: &str) -> Result<Option<ZoneSnapshot>, DeviceError> {
        if zone != ZONE {
            return Ok(None);
        }
        let record = self.pass(None).await?;
        if record.is_empty() {
            return Ok(None);
        }
        let version = self.versioned(&record);
        Ok(Some(ZoneSnapshot { version, record }))
    }

    async fn push(
        &self,
        zone: &str,
        _base: Option<Version>,
        record: &Record,
    ) -> Result<Version, DeviceError> {
        if zone != ZONE {
            return Err(DeviceError::InvalidKey);
        }
        let merged = self.pass(Some(record)).await?;
        let version = self.versioned(&merged);
        if differs(&merged, record, crate::util::unix_now_ms()) {
            return Err(DeviceError::Conflict(ZoneSnapshot {
                version,
                record: merged,
            }));
        }
        Ok(version)
    }

    fn status(&self) -> watch::Receiver<DeviceStatus> {
        self.status.subscribe()
    }

    fn notices(&self) -> watch::Receiver<u64> {
        self.notices.subscribe()
    }

    fn device_tag(&self) -> Option<String> {
        self.shared.tag()
    }
}
