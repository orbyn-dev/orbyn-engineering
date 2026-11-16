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
#[cfg(any(test, debug_assertions))]
mod keychain;
#[cfg(any(test, debug_assertions))]
mod keystore;
#[cfg(any(test, debug_assertions))]
mod profile;
#[cfg(any(test, debug_assertions))]
mod sealed;
mod shared;
mod snapshot;
mod zones;

#[cfg(test)]
pub(crate) use errors::map_for_test;

pub use account::EngineAccount;
#[cfg(all(target_os = "macos", any(test, debug_assertions)))]
pub use keychain::SystemKeychain;
#[cfg(any(test, debug_assertions))]
pub use keychain::{DEV_SERVICE, KeychainKeyStore};
#[cfg(test)]
pub(crate) use keychain::{Failure, Probe, SecretBackend};
#[cfg(any(test, debug_assertions))]
pub use keystore::FileKeyStore;
#[cfg(test)]
pub(crate) use keystore::{Anchor, Vault};
#[cfg(test)]
pub(crate) use profile::{Kind, Paired, choose};
#[cfg(test)]
pub(crate) use sealed::{PURPOSE, Refusal, SealedSnapshots, VERSION, bound_for};
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

/// The real client for a development run that keeps its keys and a sealed snapshot in `dir`:
/// the file key store and the sealed snapshot, named by `profile` (the data folder's random,
/// non-secret profile id). Debug builds and tests only; a release has no file key store, and
/// real sign-in stays off. Same loopback rule as [`connect`].
#[cfg(any(test, debug_assertions))]
pub fn connect_sealed(
    address: &str,
    device_name: &str,
    dir: &std::path::Path,
    profile: &str,
) -> Result<Backend, DeviceError> {
    let keys = FileKeyStore::new(dir.join(format!("dev-keystore-{profile}.json")));
    connect_sealed_with(address, device_name, dir, profile, keys)
}

/// Which store a profile's keys live in is decided once and recorded (`profile::choose`): the macOS
/// login keychain when it answers, the development file store when there is no keychain to ask or
/// the profile already has one. A keychain that is locked, refused or failing is
/// "unavailable, retry", never a reason to fall back: a fallback would make a second identity and
/// leave the keychain's to return on the next run. A wipe clears both stores. The file store is
/// debug-build only, so a release has neither it nor this function. Real sign-in stays off.
#[cfg(any(test, debug_assertions))]
pub fn connect_local(
    address: &str,
    device_name: &str,
    dir: &std::path::Path,
    profile: &str,
) -> Result<Backend, DeviceError> {
    #[cfg(target_os = "macos")]
    let keychain = Some(KeychainKeyStore::system(profile));
    #[cfg(not(target_os = "macos"))]
    let keychain: Option<KeychainKeyStore> = None;
    connect_local_with(address, device_name, dir, profile, keychain)
}

/// [`connect_local`] over the keychain it is given (`None` where there is none), so a test can
/// stand in for the keychain.
#[cfg(any(test, debug_assertions))]
pub(crate) fn connect_local_with(
    address: &str,
    device_name: &str,
    dir: &std::path::Path,
    profile: &str,
    keychain: Option<KeychainKeyStore>,
) -> Result<Backend, DeviceError> {
    use profile::{Kind, Paired};
    // Before anything is chosen or recorded: an address that is not loopback decides nothing.
    if !dc::is_loopback_literal(address) {
        return Err(DeviceError::Offline);
    }
    let unavailable = || DeviceError::Server { retryable: true };
    let file_path = dir.join(format!("dev-keystore-{profile}.json"));
    let kind = profile::choose(dir, profile, keychain.as_ref(), file_path.exists())
        .map_err(|_| unavailable())?;
    let file = FileKeyStore::new(&file_path);
    match kind {
        Kind::File => {
            let other = keychain.clone();
            let keys = Paired {
                primary: file,
                other: Arc::new(move || {
                    if let Some(keychain) = &other {
                        let _ = dc::KeyStore::clear(keychain);
                    }
                }),
            };
            connect_sealed_with(address, device_name, dir, profile, keys)
        }
        Kind::Keychain => {
            let keychain = keychain.ok_or_else(unavailable)?;
            let other = file.clone();
            let keys = Paired {
                primary: keychain,
                other: Arc::new(move || {
                    let _ = dc::KeyStore::clear(&other);
                }),
            };
            connect_sealed_with(address, device_name, dir, profile, keys)
        }
    }
}

/// The sealed snapshot beside `dir`, the keys wherever `keys` keeps them.
#[cfg(any(test, debug_assertions))]
pub(crate) fn connect_sealed_with<K>(
    address: &str,
    device_name: &str,
    dir: &std::path::Path,
    profile: &str,
    keys: K,
) -> Result<Backend, DeviceError>
where
    K: dc::KeyStore + keystore::Vault + Clone + 'static,
{
    if !dc::is_loopback_literal(address) {
        return Err(DeviceError::Offline);
    }
    let transport = dc::HttpApi::new(address).map_err(|_| DeviceError::Offline)?;
    let snapshots = sealed::SealedSnapshots::new(
        dir.join(format!("account-snapshot-{profile}.sealed")),
        profile,
        Arc::new(keys.clone()),
    );
    let platform = match std::env::consts::OS {
        "macos" => dc::DevicePlatform::Macos,
        "windows" => dc::DevicePlatform::Windows,
        _ => dc::DevicePlatform::Linux,
    };
    let client = dc::DeviceClient::new(
        transport,
        keys,
        dc::SystemClock,
        dc::ClientConfig {
            device_name: device_name.to_owned(),
            platform,
            app: Some(dc::DeviceApp::Engineering),
        },
    );
    Ok(backend(client, Arc::new(snapshots)))
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
    parts_with(client, Arc::new(MemorySnapshots::default()), wait, step)
}

/// [`parts`] with the snapshot kept where the test says.
#[cfg(test)]
pub(crate) fn parts_with<T, S, C>(
    client: dc::DeviceClient<T, S, C>,
    snapshots: Arc<dyn SnapshotStore>,
    wait: std::time::Duration,
    step: std::time::Duration,
) -> Parts<T, S, C>
where
    T: dc::DeviceApi + 'static,
    S: dc::KeyStore + 'static,
    C: dc::Clock + 'static,
{
    let shared = Arc::new(shared::Shared::new(client, snapshots));
    (
        Arc::new(EngineAccount::new(shared.clone(), wait, step)),
        Arc::new(EngineZones::new(shared)),
    )
}
