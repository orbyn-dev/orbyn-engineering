//! Where the Engine's snapshot (zone keys and the record cache) is kept between runs. The client
//! does no IO, so the app does, and the snapshot is sealed under the device's vault key before it
//! touches disk. The sealed store is part of the change that turns sign-in on; until then only the
//! in-memory one exists, and a run that ends loses it.

use std::sync::Mutex;

use orbyn_device_client as dc;

pub trait SnapshotStore: Send + Sync {
    fn load(&self) -> Option<dc::Snapshot>;
    fn save(&self, snapshot: &dc::Snapshot);
    fn clear(&self);
}

#[derive(Default)]
pub struct MemorySnapshots(Mutex<Option<dc::Snapshot>>);

impl MemorySnapshots {
    fn slot(&self) -> std::sync::MutexGuard<'_, Option<dc::Snapshot>> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl SnapshotStore for MemorySnapshots {
    fn load(&self) -> Option<dc::Snapshot> {
        self.slot().clone()
    }

    fn save(&self, snapshot: &dc::Snapshot) {
        *self.slot() = Some(snapshot.clone());
    }

    fn clear(&self) {
        *self.slot() = None;
    }
}
