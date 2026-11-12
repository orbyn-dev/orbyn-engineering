//! The Engine's device client behind the account traits. [`EngineAccount`] answers the
//! [`AccountApi`](super::AccountApi) the screens use, and [`EngineZones`] answers the
//! [`DeviceApi`](super::DeviceApi) the sync worker uses, both over one
//! `orbyn_device_client::DeviceClient`. This is the only place the app names the Engine, and it
//! names only that one crate (tag `engine-v0.2.4`, docs/account.md "Engine pin"; a test fails
//! when anything else is named).
//!
//! Nothing here is reachable yet: real sign-in stays off. The gate must be `LocalOnly`, which
//! only debug and test builds have, and [`connect`] refuses any address that is not a loopback
//! IP literal. Under `Off` nothing in this module is constructed or called.
//!
//! What the Engine's API makes different from the traits is listed in docs/account.md, "The
//! adapter as built". Secrets: a password goes in as `Password::new(text)` and is wiped by the
//! client. A `Local`, `Trashed` or `Quarantined` value and the approval digits are never
//! formatted, logged or put in an error; the digits reach a screen only through the approve
//! answer.

use std::sync::Arc;

use orbyn_device_client as dc;

use super::api::Backend;
use super::device_api::DeviceError;

mod account;
mod approval;
mod errors;
mod shared;
mod snapshot;
mod zones;

#[cfg(test)]
pub(crate) use errors::map_for_test;

pub use account::EngineAccount;
pub use snapshot::{MemorySnapshots, SnapshotStore};
pub use zones::EngineZones;

/// How long an approval is waited for on the trusted side before it is given up. A new device
/// joins when it next asks, which the page does every couple of seconds.
const APPROVE_WAIT: std::time::Duration = std::time::Duration::from_secs(20);
const APPROVE_STEP: std::time::Duration = std::time::Duration::from_millis(500);

/// Every Engine call goes through one client; the two halves share it.
pub fn backend<T, S, C>(
    client: dc::DeviceClient<T, S, C>,
    snapshots: Arc<dyn SnapshotStore>,
) -> Backend
where
    T: dc::DeviceApi + 'static,
    S: dc::KeyStore + 'static,
    C: dc::Clock + 'static,
{
    backend_with_wait(client, snapshots, APPROVE_WAIT, APPROVE_STEP)
}

pub(crate) fn backend_with_wait<T, S, C>(
    client: dc::DeviceClient<T, S, C>,
    snapshots: Arc<dyn SnapshotStore>,
    wait: std::time::Duration,
    step: std::time::Duration,
) -> Backend
where
    T: dc::DeviceApi + 'static,
    S: dc::KeyStore + 'static,
    C: dc::Clock + 'static,
{
    let shared = Arc::new(shared::Shared::new(client, snapshots));
    Backend {
        account: Arc::new(EngineAccount::new(shared.clone(), wait, step)),
        device: Arc::new(EngineZones::new(shared)),
    }
}

// How many real clients `connect` built on this thread, so a test can show the `Off` gate builds none.
#[cfg(test)]
thread_local! {
    pub(crate) static CONNECTED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The real client over HTTP for a local Engine, with the keys in memory only (never the real
/// keychain) and a snapshot kept in memory. Only a loopback IP literal is accepted: a name can
/// be made to point anywhere. The device says it is Engineering, as a hint and no more.
pub fn connect(address: &str, device_name: &str) -> Result<Backend, DeviceError> {
    if !dc::is_loopback_literal(address) {
        return Err(DeviceError::Offline);
    }
    let transport = dc::HttpApi::new(address).map_err(|_| DeviceError::Offline)?;
    let platform = match std::env::consts::OS {
        "macos" => dc::DevicePlatform::Macos,
        "windows" => dc::DevicePlatform::Windows,
        _ => dc::DevicePlatform::Linux,
    };
    let client = dc::DeviceClient::new(
        transport,
        dc::MemoryStore::new(),
        dc::SystemClock,
        dc::ClientConfig {
            device_name: device_name.to_owned(),
            platform,
            app: Some(dc::DeviceApp::Engineering),
        },
    );
    #[cfg(test)]
    CONNECTED.with(|n| n.set(n.get() + 1));
    Ok(backend(client, Arc::new(MemorySnapshots::default())))
}

#[cfg(test)]
pub(crate) type Parts<T, S, C> = (Arc<EngineAccount<T, S, C>>, Arc<EngineZones<T, S, C>>);

/// Both halves over one client, kept concrete so a test can drive each directly.
#[cfg(test)]
pub(crate) fn parts<T, S, C>(
    client: dc::DeviceClient<T, S, C>,
    wait: std::time::Duration,
    step: std::time::Duration,
) -> Parts<T, S, C>
where
    T: dc::DeviceApi + 'static,
    S: dc::KeyStore + 'static,
    C: dc::Clock + 'static,
{
    let shared = Arc::new(shared::Shared::new(
        client,
        Arc::new(MemorySnapshots::default()),
    ));
    (
        Arc::new(EngineAccount::new(shared.clone(), wait, step)),
        Arc::new(EngineZones::new(shared)),
    )
}
