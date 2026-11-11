//! A fake `DeviceApi` for tests. It keeps each zone's record and version in memory,
//! follows the contract's error set, and can lie like the Engine's test server: cut
//! the network in the middle of an upload, lose an answer after the server committed,
//! return any error once, and change a zone as another device would.

use std::collections::BTreeMap;
use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::watch;

use super::device_api::{DeviceApi, DeviceError, DeviceStatus, Version, ZoneSnapshot};
use super::merge::{Field, Record, Stamp, merge_records};

#[derive(Default)]
struct Inner {
    zones: BTreeMap<String, ZoneSnapshot>,
    offline: bool,
    cut_next_push: bool,
    lose_next_answer: bool,
    error: Option<DeviceError>,
    commits: usize,
    pushes: Vec<(Option<Version>, Record)>,
    pulls: usize,
}

pub struct FakeDeviceApi {
    inner: Mutex<Inner>,
    status: watch::Sender<DeviceStatus>,
    notices: watch::Sender<u64>,
}

#[cfg_attr(not(test), allow(dead_code))]
impl FakeDeviceApi {
    pub fn new() -> Self {
        Self {
            inner: Mutex::default(),
            status: watch::channel(DeviceStatus::Online).0,
            notices: watch::channel(0).0,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn set_online(&self, online: bool) {
        self.lock().offline = !online;
        self.status.send_replace(if online {
            DeviceStatus::Online
        } else {
            DeviceStatus::Offline { since_unix: 1 }
        });
    }

    pub fn set_status(&self, status: DeviceStatus) {
        self.status.send_replace(status);
    }

    /// The next upload finds the network gone: nothing is committed and the client
    /// is offline from then on.
    pub fn cut_on_next_push(&self) {
        self.lock().cut_next_push = true;
    }

    /// The next upload is committed, but the answer is lost on the way back.
    pub fn lose_next_answer(&self) {
        self.lock().lose_next_answer = true;
    }

    /// Every call, of any kind, returns `error` until this is called again with none.
    pub fn fail_every_call(&self, error: Option<DeviceError>) {
        self.lock().error = error;
    }

    /// Another device uploads these fields, as of `wall_ms`, and the live notice fires.
    pub fn peer_write(&self, zone: &str, device: &str, wall_ms: i64, fields: Vec<(String, Value)>) {
        let theirs: Record = fields
            .into_iter()
            .map(|(name, value)| {
                (
                    name,
                    Field {
                        value,
                        stamp: Stamp {
                            wall_ms,
                            counter: 0,
                            device: device.into(),
                        },
                    },
                )
            })
            .collect();
        {
            let mut inner = self.lock();
            let current = inner.zones.get(zone).cloned();
            let version = current.as_ref().map_or(0, |zone| zone.version) + 1;
            let record = merge_records(
                &current.map(|zone| zone.record).unwrap_or_default(),
                &theirs,
            )
            .record;
            inner
                .zones
                .insert(zone.to_owned(), ZoneSnapshot { version, record });
        }
        self.notices.send_modify(|count| *count += 1);
    }

    pub fn zone(&self, zone: &str) -> Option<ZoneSnapshot> {
        self.lock().zones.get(zone).cloned()
    }

    /// Uploads the server accepted.
    pub fn commits(&self) -> usize {
        self.lock().commits
    }

    /// Every upload that reached the server, with its base version.
    pub fn pushes(&self) -> Vec<(Option<Version>, Record)> {
        self.lock().pushes.clone()
    }

    pub fn pulls(&self) -> usize {
        self.lock().pulls
    }
}

#[async_trait]
impl DeviceApi for FakeDeviceApi {
    async fn pull(&self, zone: &str) -> Result<Option<ZoneSnapshot>, DeviceError> {
        let mut inner = self.lock();
        if let Some(error) = inner.error.clone() {
            return Err(error);
        }
        if inner.offline {
            return Err(DeviceError::Offline);
        }
        inner.pulls += 1;
        Ok(inner.zones.get(zone).cloned())
    }

    async fn push(
        &self,
        zone: &str,
        base: Option<Version>,
        record: &Record,
    ) -> Result<Version, DeviceError> {
        let mut inner = self.lock();
        if let Some(error) = inner.error.clone() {
            return Err(error);
        }
        if inner.cut_next_push {
            inner.cut_next_push = false;
            inner.offline = true;
            self.status
                .send_replace(DeviceStatus::Offline { since_unix: 1 });
            return Err(DeviceError::Offline);
        }
        if inner.offline {
            return Err(DeviceError::Offline);
        }
        let current = inner.zones.get(zone).cloned();
        if current.as_ref().map(|zone| zone.version) != base {
            return Err(DeviceError::Conflict(current.unwrap_or(ZoneSnapshot {
                version: 0,
                record: Record::new(),
            })));
        }
        let version = base.unwrap_or(0) + 1;
        inner.pushes.push((base, record.clone()));
        inner.zones.insert(
            zone.to_owned(),
            ZoneSnapshot {
                version,
                record: record.clone(),
            },
        );
        inner.commits += 1;
        // A live notice to every device, the pusher included.
        self.notices.send_modify(|count| *count += 1);
        if inner.lose_next_answer {
            inner.lose_next_answer = false;
            return Err(DeviceError::Offline);
        }
        Ok(version)
    }

    fn status(&self) -> watch::Receiver<DeviceStatus> {
        self.status.subscribe()
    }

    fn notices(&self) -> watch::Receiver<u64> {
        self.notices.subscribe()
    }
}
