//! The sealed snapshot and the key store behind it: what the file holds, what is refused, and
//! that sign-out, removing this device and a session the service ended each destroy the keys
//! and the snapshot. Keys and tokens are generated at run time; no value here is a secret.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use orbyn_device_client as dc;
use orbyn_device_client::KeyStore as _;

use super::engine_server::{ACCOUNT, EMAIL, Server, Wire};
use super::*;
use crate::account::DeviceApi as _;
use crate::account::api::AccountApi;
use crate::account::dev::PASSWORD;
use crate::account::engine::{
    Anchor, EngineAccount, EngineZones, FileKeyStore, PURPOSE, Refusal, SealedSnapshots,
    SnapshotStore, VERSION, Vault, bound_for, parts_with,
};

pub(super) const FIRST: &str = "0a0a0a0a-0000-4000-8000-0000000000a1";
pub(super) const SECOND: &str = "0a0a0a0a-0000-4000-8000-0000000000b2";

pub(super) fn label(what: &str) -> String {
    format!("{what}-{}", crate::util::new_id())
}

fn snapshot(account: &str) -> dc::Snapshot {
    dc::Snapshot {
        account_id: account.to_owned(),
        device_id: label("device"),
        ..dc::Snapshot::default()
    }
}

fn identity(token: &str, device: &str) -> dc::StoredIdentity {
    dc::StoredIdentity {
        device_keys: dc::DeviceKeys::generate().unwrap().to_bytes(),
        account_id: ACCOUNT.to_owned(),
        device_id: device.to_owned(),
        email: EMAIL.to_owned(),
        session: Some(dc::StoredSession {
            token: dc::Zeroizing::new(token.to_owned()),
            expires_at: "2099-01-01T00:00:00Z".to_owned(),
        }),
        vault_key: dc::Zeroizing::new(crate::util::random_bytes::<32>().unwrap()),
    }
}

pub(super) struct Files {
    pub(super) keys: FileKeyStore,
    pub(super) snaps: Arc<SealedSnapshots>,
    pub(super) key_file: PathBuf,
    pub(super) snap_file: PathBuf,
}

fn files(dir: &Path, name: &str, profile: &str) -> Files {
    let key_file = dir.join(format!("{name}-keys.json"));
    let snap_file = dir.join(format!("{name}-snapshot.sealed"));
    let keys = FileKeyStore::new(&key_file);
    keys.save(&identity(&label("token"), FIRST)).unwrap();
    let snaps = Arc::new(SealedSnapshots::new(
        &snap_file,
        profile,
        Arc::new(keys.clone()),
    ));
    Files {
        keys,
        snaps,
        key_file,
        snap_file,
    }
}

#[test]
fn a_saved_snapshot_comes_back_and_the_file_holds_no_plain_text() {
    let dir = TempDir::new();
    let f = files(dir.path(), "a", "profile-a");
    let account = label("account");
    f.snaps.save(&snapshot(&account));
    let bytes = std::fs::read(&f.snap_file).unwrap();
    assert_eq!(bytes[0], VERSION);
    assert!(
        !bytes
            .windows(account.len())
            .any(|w| w == account.as_bytes())
    );
    assert_eq!(f.snaps.load().unwrap().account_id, account);
    assert_eq!(f.snaps.last_refusal(), None);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in [&f.snap_file, &f.key_file] {
            let mode = std::fs::metadata(path).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0, "{path:?} is readable by others");
        }
    }
}

#[test]
fn every_save_moves_the_counter_forward() {
    let dir = TempDir::new();
    let f = files(dir.path(), "a", "p");
    f.snaps.save(&snapshot("one"));
    let first = f.keys.anchor().unwrap().counter;
    f.snaps.save(&snapshot("two"));
    assert_eq!(f.keys.anchor().unwrap().counter, first + 1);
}

#[test]
fn an_older_snapshot_put_back_is_refused() {
    let dir = TempDir::new();
    let f = files(dir.path(), "a", "p");
    f.snaps.save(&snapshot("older"));
    let older = std::fs::read(&f.snap_file).unwrap();
    f.snaps.save(&snapshot("newer"));
    assert_eq!(f.snaps.load().unwrap().account_id, "newer");
    // An engine puts the earlier, validly sealed file back.
    std::fs::write(&f.snap_file, &older).unwrap();
    assert!(f.snaps.load().is_none());
    assert_eq!(f.snaps.last_refusal(), Some(Refusal::RolledBack));
}

#[test]
fn a_changed_file_or_counter_is_refused() {
    let dir = TempDir::new();
    let f = files(dir.path(), "a", "p");
    f.snaps.save(&snapshot("x"));
    let good = std::fs::read(&f.snap_file).unwrap();

    let mut flipped = good.clone();
    *flipped.last_mut().unwrap() ^= 1;
    std::fs::write(&f.snap_file, &flipped).unwrap();
    assert!(f.snaps.load().is_none());
    assert_eq!(f.snaps.last_refusal(), Some(Refusal::NotTheLatest));

    let mut bumped = good.clone();
    bumped[8] = bumped[8].wrapping_add(1);
    std::fs::write(&f.snap_file, &bumped).unwrap();
    assert!(f.snaps.load().is_none());
    assert_eq!(f.snaps.last_refusal(), Some(Refusal::NotTheLatest));

    std::fs::write(&f.snap_file, &good[..10]).unwrap();
    assert!(f.snaps.load().is_none());
    assert_eq!(f.snaps.last_refusal(), Some(Refusal::Unreadable));

    std::fs::write(&f.snap_file, &good).unwrap();
    assert!(f.snaps.load().is_some(), "the untouched file still opens");
}

#[test]
fn a_file_of_another_profile_or_another_vault_is_refused() {
    let dir = TempDir::new();
    let f = files(dir.path(), "a", "profile-a");
    f.snaps.save(&snapshot("mine"));
    // The same file and the same keys, read as another profile: the seal does not open.
    let other = SealedSnapshots::new(&f.snap_file, "profile-b", Arc::new(f.keys.clone()));
    assert!(other.load().is_none());
    assert_eq!(other.last_refusal(), Some(Refusal::DoesNotOpen));
    // The file moved to another data folder with its own keys and no anchor for it.
    let g = files(dir.path(), "b", "profile-a");
    std::fs::copy(&f.snap_file, &g.snap_file).unwrap();
    assert!(g.snaps.load().is_none());
    assert_eq!(g.snaps.last_refusal(), Some(Refusal::RolledBack));
    // Even with an anchor that matches the copied file, another vault key does not open it.
    let bytes = std::fs::read(&f.snap_file).unwrap();
    let counter = u64::from_be_bytes(bytes[1..9].try_into().unwrap());
    let hash: [u8; 32] = sha2::Sha256::digest(&bytes).into();
    g.keys.set_anchor(Anchor { counter, hash }).unwrap();
    assert!(g.snaps.load().is_none());
    assert_eq!(g.snaps.last_refusal(), Some(Refusal::DoesNotOpen));
}

#[test]
fn a_changed_format_version_is_refused() {
    let dir = TempDir::new();
    let f = files(dir.path(), "a", "p");
    f.snaps.save(&snapshot("x"));
    let mut bytes = std::fs::read(&f.snap_file).unwrap();
    bytes[0] = VERSION + 1;
    // Even when the vault is made to agree with the edited file, the version decides.
    let counter = u64::from_be_bytes(bytes[1..9].try_into().unwrap());
    let hash: [u8; 32] = sha2::Sha256::digest(&bytes).into();
    f.keys.set_anchor(Anchor { counter, hash }).unwrap();
    std::fs::write(&f.snap_file, &bytes).unwrap();
    assert!(f.snaps.load().is_none());
    assert_eq!(f.snaps.last_refusal(), Some(Refusal::WrongVersion));
}

#[test]
fn the_seal_binds_the_purpose_the_profile_and_the_counter() {
    use chacha20poly1305::aead::{Aead, KeyInit, Payload};
    use chacha20poly1305::{XChaCha20Poly1305, XNonce};
    let dir = TempDir::new();
    let f = files(dir.path(), "a", "profile-a");
    f.snaps.save(&snapshot("x"));
    let bytes = std::fs::read(&f.snap_file).unwrap();
    let counter = u64::from_be_bytes(bytes[1..9].try_into().unwrap());
    let nonce: [u8; 24] = bytes[9..33].try_into().unwrap();
    let key = f.keys.vault_key().unwrap();
    let open = |aad: Vec<u8>| {
        XChaCha20Poly1305::new(&(*key).into())
            .decrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: &bytes[33..],
                    aad: &aad,
                },
            )
            .is_ok()
    };
    assert!(open(bound_for(PURPOSE, "profile-a", counter)));
    assert!(!open(bound_for(
        "orbyn/engineering/other",
        "profile-a",
        counter
    )));
    assert!(!open(bound_for(PURPOSE, "profile-b", counter)));
    assert!(!open(bound_for(PURPOSE, "profile-a", counter + 1)));
}

#[test]
fn a_counter_edit_the_vault_agrees_with_still_does_not_open() {
    let dir = TempDir::new();
    let f = files(dir.path(), "a", "p");
    f.snaps.save(&snapshot("x"));
    let mut bytes = std::fs::read(&f.snap_file).unwrap();
    bytes[8] = bytes[8].wrapping_add(1);
    let counter = u64::from_be_bytes(bytes[1..9].try_into().unwrap());
    let hash: [u8; 32] = sha2::Sha256::digest(&bytes).into();
    // Someone who could also move the vault's anchor still cannot forge the counter: it is
    // part of what the seal authenticates.
    f.keys.set_anchor(Anchor { counter, hash }).unwrap();
    std::fs::write(&f.snap_file, &bytes).unwrap();
    assert!(f.snaps.load().is_none());
    assert_eq!(f.snaps.last_refusal(), Some(Refusal::DoesNotOpen));
}

#[test]
fn a_crash_between_the_anchor_and_the_file_starts_empty_and_the_next_save_recovers() {
    let dir = TempDir::new();
    let f = files(dir.path(), "a", "p");
    f.snaps.save(&snapshot("before"));
    let kept = f.keys.anchor().unwrap();
    // The state a crash leaves: the anchor moved to the next save, the file did not.
    f.keys
        .set_anchor(Anchor {
            counter: kept.counter + 1,
            hash: [7; 32],
        })
        .unwrap();
    assert!(f.snaps.load().is_none());
    assert_eq!(f.snaps.last_refusal(), Some(Refusal::RolledBack));
    f.snaps.save(&snapshot("after"));
    assert_eq!(f.snaps.load().unwrap().account_id, "after");
    assert_eq!(f.keys.anchor().unwrap().counter, kept.counter + 2);
}

fn marker(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".wipe");
    PathBuf::from(name)
}

#[test]
fn a_delete_that_fails_half_way_leaves_no_readable_key_and_the_next_one_finishes_it() {
    let dir = TempDir::new();
    let f = files(dir.path(), "a", "p");
    f.snaps.save(&snapshot("x"));
    f.keys.fail_deletes(1);
    assert!(f.keys.clear().is_err());
    // The file is still there, but empty, and a marker says a wipe is owed.
    assert_eq!(std::fs::metadata(&f.key_file).unwrap().len(), 0);
    assert!(marker(&f.key_file).exists());
    assert!(f.keys.load().is_err());
    assert!(f.keys.vault_key().is_none() && f.keys.anchor().is_none());
    // A whole, readable key file and a marker, as a crash leaves them, stay shut: nothing could be
    // overwritten or removed.
    f.keys.fail_deletes(0);
    f.keys.save(&identity(&label("token"), FIRST)).unwrap();
    f.keys.fail_deletes(3);
    assert!(f.keys.clear().is_err());
    assert!(std::fs::metadata(&f.key_file).unwrap().len() > 0);
    assert!(f.keys.load().is_err() && f.keys.vault_key().is_none());
    f.keys.fail_deletes(1);
    assert!(f.keys.clear().is_err());
    // A new process, once deleting works, finishes the owed wipe on its first load.
    let fresh = FileKeyStore::new(&f.key_file);
    assert!(fresh.load().unwrap().is_none());
    assert!(!f.key_file.exists() && !marker(&f.key_file).exists());
    // Saving a new identity over an owed wipe finishes the old wipe first.
    f.keys.fail_deletes(0);
    f.keys.save(&identity(&label("token"), FIRST)).unwrap();
    f.keys.fail_deletes(1);
    assert!(f.keys.clear().is_err());
    f.keys.fail_deletes(0);
    let next = identity(&label("token"), SECOND);
    f.keys.save(&next).unwrap();
    assert!(!marker(&f.key_file).exists());
    assert_eq!(f.keys.load().unwrap().unwrap().device_id, SECOND);
    assert!(
        f.keys.anchor().is_none(),
        "the old anchor went with the old key"
    );
    // And a plain retry of the clear finishes it too.
    f.keys.fail_deletes(1);
    assert!(f.keys.clear().is_err());
    f.keys.fail_deletes(0);
    f.keys.clear().unwrap();
    assert!(!f.key_file.exists() && !marker(&f.key_file).exists());
}

#[test]
fn with_no_keys_nothing_is_written_and_nothing_loads() {
    let dir = TempDir::new();
    let f = files(dir.path(), "a", "p");
    f.snaps.save(&snapshot("x"));
    f.keys.clear().unwrap();
    std::fs::remove_file(&f.snap_file).unwrap();
    // A stale engine saving after a wipe cannot bring a snapshot back.
    f.snaps.save(&snapshot("late"));
    assert!(!f.snap_file.exists());
    assert!(!f.key_file.exists());
    assert!(f.snaps.load().is_none());
}

#[test]
fn the_key_store_keeps_the_anchor_through_a_save_and_forgets_all_on_clear() {
    let dir = TempDir::new();
    let f = files(dir.path(), "a", "p");
    f.snaps.save(&snapshot("x"));
    let anchor = f.keys.anchor().unwrap();
    let mut again = f.keys.load().unwrap().unwrap();
    again.session = None;
    f.keys.save(&again).unwrap();
    assert_eq!(f.keys.anchor(), Some(anchor));
    assert!(f.keys.load().unwrap().unwrap().session.is_none());
    f.keys.clear().unwrap();
    assert!(f.keys.load().unwrap().is_none());
    assert!(f.keys.anchor().is_none() && f.keys.vault_key().is_none());
    assert!(!f.key_file.exists());
    f.keys.clear().unwrap();
}

#[test]
fn nothing_prints_a_key_a_token_or_a_value() {
    let dir = TempDir::new();
    let token = label("token");
    let account = label("account");
    let keys = FileKeyStore::new(dir.path().join("k.json"));
    let id = identity(&token, FIRST);
    let vault_text = format!("{:?}", id.vault_key.as_slice());
    keys.save(&id).unwrap();
    let snaps = SealedSnapshots::new(dir.path().join("s.sealed"), "p", Arc::new(keys.clone()));
    snaps.save(&snapshot(&account));
    let anchor = Vault::anchor(&keys);
    let shown = format!("{keys:?} {snaps:?} {anchor:?} {id:?}");
    for secret in [&token, &account, &vault_text] {
        assert!(!shown.contains(secret.as_str()));
    }
    assert!(!shown.contains(&B64.encode(id.vault_key.as_slice())));
}

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use sha2::Digest as _;

// ---- the adapter: sign-out, removing this device and a session the service ended ----

pub(super) type Account = EngineAccount<Wire, FileKeyStore, dc::SystemClock>;
pub(super) type Zones = EngineZones<Wire, FileKeyStore, dc::SystemClock>;

pub(super) struct Device {
    pub(super) account: Arc<Account>,
    pub(super) zones: Arc<Zones>,
    pub(super) files: Files,
}

fn device(server: &Arc<Server>, dir: &Path, id: &str, name: &str) -> Device {
    let keys = dc::DeviceKeys::generate().unwrap();
    let token = server.add_device(id, name, Some("engineering"), &keys.public(), true);
    let key_file = dir.join(format!("{name}-keys.json"));
    let snap_file = dir.join(format!("{name}-snapshot.sealed"));
    let store = FileKeyStore::new(&key_file);
    let mut stored = identity(&token, id);
    stored.device_keys = keys.to_bytes();
    store.save(&stored).unwrap();
    let snaps = Arc::new(SealedSnapshots::new(
        &snap_file,
        "profile",
        Arc::new(store.clone()),
    ));
    let client = dc::DeviceClient::new(
        Wire(server.clone()),
        store.clone(),
        dc::SystemClock,
        dc::ClientConfig {
            device_name: name.to_owned(),
            platform: dc::DevicePlatform::Macos,
            app: Some(dc::DeviceApp::Engineering),
        },
    );
    let (account, zones) = parts_with(
        client,
        snaps.clone(),
        Duration::from_millis(300),
        Duration::from_millis(10),
    );
    Device {
        account,
        zones,
        files: Files {
            keys: store,
            snaps,
            key_file,
            snap_file,
        },
    }
}

/// A device that has synced once, so its keys and its sealed snapshot both exist.
pub(super) async fn synced(server: &Arc<Server>, dir: &Path, id: &str, name: &str) -> Device {
    let d = device(server, dir, id, name);
    d.zones.pull(ZONE).await.unwrap();
    assert!(
        d.files.snap_file.exists(),
        "the sync saved a sealed snapshot"
    );
    assert!(d.files.key_file.exists());
    d
}

fn gone(d: &Device) -> bool {
    !d.files.snap_file.exists()
        && !d.files.key_file.exists()
        && !marker(&d.files.key_file).exists()
        && d.files.keys.load().unwrap().is_none()
}

#[tokio::test]
async fn sign_out_destroys_the_keys_and_the_sealed_snapshot() {
    let dir = TempDir::new();
    let server = Server::new();
    let d = synced(&server, dir.path(), FIRST, "one").await;
    d.account.sign_out().await.unwrap();
    assert!(gone(&d));
}

#[tokio::test]
async fn removing_this_device_destroys_its_keys_and_removing_another_does_not() {
    let dir = TempDir::new();
    let server = Server::new();
    let one = synced(&server, dir.path(), FIRST, "one").await;
    let two = synced(&server, dir.path(), SECOND, "two").await;
    one.account.remove_device(SECOND).await.unwrap();
    assert!(
        one.files.snap_file.exists() && one.files.key_file.exists(),
        "removing another device keeps this device's keys"
    );
    one.account.remove_device(FIRST).await.unwrap();
    assert!(gone(&one));
    assert!(
        two.files.key_file.exists(),
        "the other device's files are its own"
    );
}

#[tokio::test]
async fn a_session_the_service_ended_destroys_the_keys_through_the_worker_and_the_screens() {
    let dir = TempDir::new();
    let server = Server::new();
    let d = synced(&server, dir.path(), FIRST, "one").await;
    server.refuse("/v1/zones", 401, "unauthorized");
    assert_eq!(
        d.zones.pull(ZONE).await.err(),
        Some(DeviceError::Unauthorized)
    );
    d.zones.wipe_keys().await;
    assert!(gone(&d));

    // The screens' side of the same adapter.
    let server = Server::new();
    let d = synced(&server, dir.path(), FIRST, "again").await;
    d.account.wipe_keys().await;
    assert!(gone(&d));
}

#[tokio::test]
async fn the_worker_destroys_keys_for_an_ended_session_or_a_gone_account_and_for_nothing_else() {
    for (error, wipes) in [
        (DeviceError::Unauthorized, 1),
        (DeviceError::AccountGone, 1),
        (DeviceError::Offline, 0),
        (DeviceError::Server { retryable: false }, 0),
        (DeviceError::ProtectionAdvanced, 0),
    ] {
        let dir = TempDir::new();
        let core = on(&dir, "a");
        let api = Arc::new(FakeDeviceApi::new());
        api.fail_every_call(Some(error.clone()));
        let _worker = start(&core, &api);
        if wipes == 1 {
            until(&core, |_| api.wipes() >= 1).await;
        } else {
            until(&core, |c| {
                matches!(
                    c.account.status().state,
                    SyncState::Offline { .. } | SyncState::Paused { .. }
                )
            })
            .await;
        }
        assert_eq!(api.wipes(), wipes, "{error:?}");
    }
}

#[tokio::test]
async fn a_session_ended_on_a_screen_call_destroys_the_keys() {
    let dir = TempDir::new();
    let core = dev(&dir, "ended");
    let a = &core.account;
    a.sign_in(
        "dev@example.com",
        crate::account::Password::new(PASSWORD.to_owned()),
    )
    .await
    .unwrap();
    a.stand_in().sign_out_elsewhere();
    assert!(
        matches!(
            a.devices().await,
            Err(crate::error::CoreError::Account {
                code: "session_ended",
                ..
            })
        ),
        "a session ended elsewhere answers session_ended"
    );
    assert_eq!(a.stand_in().wipes(), 1);
}

#[derive(Clone, Copy, Debug)]
enum Trigger {
    OwnRemoval,
    EndedSession,
    AccountGone,
}

/// A wipe whose key delete fails must not leave a usable key: the client is not opened again
/// (no call reaches the server), and the next try finishes the wipe.
async fn failed_delete_then_retry(trigger: Trigger, mode: u8) {
    let dir = TempDir::new();
    let server = Server::new();
    let d = synced(&server, dir.path(), FIRST, "one").await;
    let _other = synced(&server, dir.path(), SECOND, "two").await;
    d.files.keys.fail_deletes(mode);
    match trigger {
        Trigger::OwnRemoval => {
            // Removed on the server, but the local keys could not be deleted: told so.
            assert!(matches!(
                d.account.remove_device(FIRST).await,
                Err(crate::account::DeviceError::Server { retryable: true })
            ));
        }
        Trigger::EndedSession => {
            server.refuse("/v1/zones", 401, "unauthorized");
            assert_eq!(
                d.zones.pull(ZONE).await.err(),
                Some(DeviceError::Unauthorized)
            );
            d.zones.wipe_keys().await;
            server.clear_refusal();
        }
        Trigger::AccountGone => d.account.wipe_keys().await,
    }
    assert!(d.files.key_file.exists(), "{trigger:?}: the delete failed");
    let calls = server.calls("/v1/");
    for _ in 0..2 {
        // Not signed in again, and nothing reached the Engine.
        assert_eq!(
            d.zones.pull(ZONE).await.err(),
            Some(DeviceError::Unauthorized),
            "{trigger:?} mode {mode}"
        );
        assert_eq!(
            server.calls("/v1/"),
            calls,
            "{trigger:?}: the client was reopened"
        );
    }
    // The next try, once deleting works again, wipes everything.
    d.files.keys.fail_deletes(0);
    assert_eq!(
        d.zones.pull(ZONE).await.err(),
        Some(DeviceError::Unauthorized)
    );
    assert!(gone(&d), "{trigger:?} mode {mode}: the retry did not wipe");
    assert_eq!(server.calls("/v1/"), calls);
}

#[tokio::test]
async fn a_failed_key_delete_blocks_the_client_and_is_retried() {
    for trigger in [
        Trigger::OwnRemoval,
        Trigger::EndedSession,
        Trigger::AccountGone,
    ] {
        // 2: nothing could be deleted, so the key file is still whole and only the owed wipe
        // keeps the client shut. 1: it was emptied but not removed.
        for mode in [2, 1] {
            failed_delete_then_retry(trigger, mode).await;
        }
    }
}

/// The same files, a new run: a new key store, a new client and a new `Shared`, as after a restart.
fn restarted(server: &Arc<Server>, d: &Device, name: &str) -> Device {
    let store = FileKeyStore::new(&d.files.key_file);
    let snaps = Arc::new(SealedSnapshots::new(
        &d.files.snap_file,
        "profile",
        Arc::new(store.clone()),
    ));
    let client = dc::DeviceClient::new(
        Wire(server.clone()),
        store.clone(),
        dc::SystemClock,
        dc::ClientConfig {
            device_name: name.to_owned(),
            platform: dc::DevicePlatform::Macos,
            app: Some(dc::DeviceApp::Engineering),
        },
    );
    let (account, zones) = parts_with(
        client,
        snaps.clone(),
        Duration::from_millis(300),
        Duration::from_millis(10),
    );
    Device {
        account,
        zones,
        files: Files {
            keys: store,
            snaps,
            key_file: d.files.key_file.clone(),
            snap_file: d.files.snap_file.clone(),
        },
    }
}

#[tokio::test]
async fn an_owed_wipe_is_finished_by_the_first_use_after_a_restart() {
    let dir = TempDir::new();
    let server = Server::new();
    let d = synced(&server, dir.path(), FIRST, "one").await;
    // The run ended with the wipe owed: the marker is down and the key file is still whole.
    std::fs::write(marker(&d.files.key_file), b"wipe owed").unwrap();
    assert!(std::fs::metadata(&d.files.key_file).unwrap().len() > 0);
    let calls = server.calls("/v1/");

    // The sync worker's first pass finishes it, and nothing reaches the Engine.
    let again = restarted(&server, &d, "one");
    assert_eq!(
        again.zones.pull(ZONE).await.err(),
        Some(DeviceError::Unauthorized)
    );
    assert!(gone(&again), "the first pass did not finish the wipe");
    assert_eq!(server.calls("/v1/"), calls, "{:?}", server.calls("/v1/"));

    // A sign-in loads the store first; the same marker is finished there.
    let d = synced(&server, dir.path(), FIRST, "two").await;
    std::fs::write(marker(&d.files.key_file), b"wipe owed").unwrap();
    let again = restarted(&server, &d, "two");
    assert!(again.files.keys.load().unwrap().is_none());
    assert!(!again.files.key_file.exists() && !marker(&again.files.key_file).exists());
    // The sealed file the store cannot know about stays until the client next opens, but with
    // its vault key gone it cannot be opened.
    assert!(again.files.snaps.load().is_none());
}
