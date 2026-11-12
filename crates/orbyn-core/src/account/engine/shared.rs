//! What the account half and the sync half share: the client, the Engine's state and where its
//! snapshot is kept.

use std::sync::{Arc, OnceLock};

use orbyn_device_client as dc;
use tokio::sync::{Mutex, MutexGuard};

use super::errors::map;
use super::snapshot::SnapshotStore;
use crate::account::device_api::DeviceError;

pub(super) struct Shared<T, S, C> {
    pub(super) client: dc::DeviceClient<T, S, C>,
    engine: Mutex<Option<dc::Engine>>,
    snapshots: Arc<dyn SnapshotStore>,
    /// The name this device's stamps carry, known once the Engine is open.
    tag: OnceLock<String>,
    /// Zone keys this device owes a device it just approved, because sending them failed.
    owed: std::sync::Mutex<std::collections::BTreeSet<String>>,
}

impl<T, S, C> Shared<T, S, C>
where
    T: dc::DeviceApi + 'static,
    S: dc::KeyStore + 'static,
    C: dc::Clock + 'static,
{
    pub(super) fn new(
        client: dc::DeviceClient<T, S, C>,
        snapshots: Arc<dyn SnapshotStore>,
    ) -> Self {
        Self {
            client,
            engine: Mutex::new(None),
            snapshots,
            tag: OnceLock::new(),
            owed: std::sync::Mutex::default(),
        }
    }

    /// The Engine's state, opened from the snapshot (or empty) the first time it is needed.
    pub(super) async fn engine(&self) -> Result<MutexGuard<'_, Option<dc::Engine>>, DeviceError> {
        let mut guard = self.engine.lock().await;
        if guard.is_none() {
            let mut opened = self
                .client
                .open_engine(self.snapshots.load())
                .map_err(map)?;
            let now = u64::try_from(unix_secs()).unwrap_or(0).saturating_mul(1000);
            let _ = self.tag.set(format!("{:016x}", opened.tick(now).device));
            *guard = Some(opened);
        }
        Ok(guard)
    }

    pub(super) fn save(&self, engine: &dc::Engine) {
        self.snapshots.save(&engine.snapshot());
    }

    /// Forgets the Engine's state: a sign-out, or an account that is gone.
    pub(super) async fn forget(&self) {
        *self.engine.lock().await = None;
        self.snapshots.clear();
    }

    pub(super) fn tag(&self) -> Option<String> {
        self.tag.get().cloned()
    }

    fn owed(&self) -> std::sync::MutexGuard<'_, std::collections::BTreeSet<String>> {
        self.owed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// A device was approved: it is owed every zone key once the Engine lists it as trusted,
    /// which is after it has typed the digits.
    pub(super) fn owe_keys(&self, device: &str) {
        self.owed().insert(device.to_owned());
    }

    pub(super) fn owes_keys(&self) -> bool {
        !self.owed().is_empty()
    }

    /// Devices that are owed keys and are trusted now; they stop being owed once sent.
    pub(super) fn settle(&self, trusted: &[String]) -> Vec<String> {
        let owed = self.owed();
        owed.iter()
            .filter(|id| trusted.contains(id))
            .cloned()
            .collect()
    }

    pub(super) fn paid(&self, ids: &[String]) {
        let mut owed = self.owed();
        for id in ids {
            owed.remove(id);
        }
    }

    /// This device's ids and the app's view of its session.
    pub(super) fn session(
        &self,
        device_name: &str,
    ) -> Result<crate::account::SessionInfo, DeviceError> {
        let (_, device_id, email) = self
            .client
            .identity()
            .map_err(map)?
            .ok_or(DeviceError::Unauthorized)?;
        Ok(crate::account::SessionInfo {
            email,
            device_id,
            device_name: device_name.to_owned(),
        })
    }
}

pub(super) fn unix_secs() -> i64 {
    crate::util::unix_now()
}
