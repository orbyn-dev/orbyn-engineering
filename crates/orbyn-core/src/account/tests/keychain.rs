//! The keychain key store, over a fake keychain that can fail: what it keeps, that a delete is
//! never silent, and that sign-out, removing this device and an ended session each delete the
//! item. The real macOS keychain is checked by the ignored smoke test at the end (run by hand).
//! Keys and tokens are generated at run time; no value here is a secret.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use orbyn_device_client as dc;
use orbyn_device_client::KeyStore as _;

use super::engine_server::{ACCOUNT, EMAIL, Server, Wire};
use super::*;
use crate::account::DeviceApi as _;
use crate::account::api::AccountApi;
use crate::account::engine::{
    Anchor, DEV_SERVICE, EngineAccount, EngineZones, Failure, KeychainKeyStore, Kind, Paired,
    Probe, SealedSnapshots, SecretBackend, SnapshotStore, Vault, choose, parts_with,
};

const FIRST: &str = "0a0a0a0a-0000-4000-8000-0000000000a1";
const SECOND: &str = "0a0a0a0a-0000-4000-8000-0000000000b2";

/// A keychain in memory. `fail_delete` makes every delete fail; `fail_set` every write; `down` makes
/// every call fail the way a locked, refused or absent keychain does.
#[derive(Default)]
struct Fake {
    items: Mutex<BTreeMap<(String, String), Vec<u8>>>,
    fail_delete: AtomicBool,
    fail_set: AtomicBool,
    down: Mutex<Option<Failure>>,
}

impl Fake {
    fn items(&self) -> std::sync::MutexGuard<'_, BTreeMap<(String, String), Vec<u8>>> {
        self.items.lock().unwrap()
    }

    fn down(&self, failure: Option<Failure>) {
        *self.down.lock().unwrap() = failure;
    }

    fn check(&self) -> Result<(), Failure> {
        self.down.lock().unwrap().map_or(Ok(()), Err)
    }
}

impl SecretBackend for Fake {
    fn get(&self, service: &str, account: &str) -> Result<Option<dc::Zeroizing<Vec<u8>>>, Failure> {
        self.check()?;
        Ok(self
            .items()
            .get(&(service.to_owned(), account.to_owned()))
            .cloned()
            .map(dc::Zeroizing::new))
    }

    fn set(&self, service: &str, account: &str, bytes: &[u8]) -> Result<(), Failure> {
        self.check()?;
        if self.fail_set.load(Ordering::SeqCst) {
            return Err(Failure::Other);
        }
        self.items()
            .insert((service.to_owned(), account.to_owned()), bytes.to_vec());
        Ok(())
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), Failure> {
        self.check()?;
        if self.fail_delete.load(Ordering::SeqCst) {
            return Err(Failure::Other);
        }
        self.items()
            .remove(&(service.to_owned(), account.to_owned()));
        Ok(())
    }
}

fn label(what: &str) -> String {
    format!("{what}-{}", crate::util::new_id())
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

fn store(fake: &Arc<Fake>, profile: &str) -> KeychainKeyStore {
    KeychainKeyStore::new(fake.clone(), DEV_SERVICE, profile)
}

#[test]
fn the_identity_and_the_anchor_round_trip_through_one_item_per_profile() {
    let fake = Arc::new(Fake::default());
    let keys = store(&fake, "p1");
    assert!(keys.load().unwrap().is_none());
    let id = identity(&label("token"), FIRST);
    keys.save(&id).unwrap();
    let back = keys.load().unwrap().unwrap();
    assert_eq!(back.device_id, FIRST);
    assert_eq!(back.vault_key.as_slice(), id.vault_key.as_slice());
    assert_eq!(
        back.session.unwrap().token.as_str(),
        id.session.as_ref().unwrap().token.as_str()
    );
    assert_eq!(fake.items().len(), 1);
    let ((service, account), _) = fake
        .items()
        .iter()
        .next()
        .map(|(k, v)| (k.clone(), v.clone()))
        .unwrap();
    assert_eq!(
        (service.as_str(), account.as_str()),
        (DEV_SERVICE, "profile-p1")
    );
    // The anchor lives with the vault key and survives a save of the identity.
    let anchor = Anchor {
        counter: 7,
        hash: [3; 32],
    };
    keys.set_anchor(anchor).unwrap();
    assert_eq!(keys.anchor(), Some(anchor));
    let mut again = keys.load().unwrap().unwrap();
    again.session = None;
    keys.save(&again).unwrap();
    assert_eq!(keys.anchor(), Some(anchor));
    assert_eq!(
        keys.vault_key().unwrap().as_slice(),
        id.vault_key.as_slice()
    );
    // Another profile is another item.
    let other = store(&fake, "p2");
    assert!(other.load().unwrap().is_none());
    other.save(&identity(&label("token"), SECOND)).unwrap();
    assert_eq!(fake.items().len(), 2);
    assert_eq!(keys.load().unwrap().unwrap().device_id, FIRST);
}

#[test]
fn clear_deletes_the_item() {
    let fake = Arc::new(Fake::default());
    let keys = store(&fake, "p");
    keys.save(&identity(&label("token"), FIRST)).unwrap();
    keys.clear().unwrap();
    assert!(fake.items().is_empty());
    assert!(keys.load().unwrap().is_none());
    assert!(keys.vault_key().is_none() && keys.anchor().is_none());
    keys.clear().unwrap();
}

#[test]
fn a_failed_delete_is_loud_leaves_no_usable_key_and_is_finished_later() {
    let fake = Arc::new(Fake::default());
    let keys = store(&fake, "p");
    keys.save(&identity(&label("token"), FIRST)).unwrap();
    fake.fail_delete.store(true, Ordering::SeqCst);
    assert!(keys.clear().is_err(), "the failure is returned");
    // The item is blanked: no read treats it as an identity, and the owed delete fails loudly.
    assert!(fake.items().values().all(Vec::is_empty));
    assert!(keys.load().is_err());
    assert!(keys.vault_key().is_none() && keys.anchor().is_none());
    // A new process (a new store over the same keychain) sees the same.
    assert!(store(&fake, "p").load().is_err());
    // Saving a new identity needs the old one gone first.
    assert!(keys.save(&identity(&label("token"), SECOND)).is_err());
    // Once deleting works, the first load finishes the wipe.
    fake.fail_delete.store(false, Ordering::SeqCst);
    assert!(store(&fake, "p").load().unwrap().is_none());
    assert!(fake.items().is_empty());
    // And a save over a tombstone finishes it too.
    fake.fail_delete.store(true, Ordering::SeqCst);
    keys.save(&identity(&label("token"), FIRST)).unwrap();
    assert!(keys.clear().is_err());
    fake.fail_delete.store(false, Ordering::SeqCst);
    keys.save(&identity(&label("token"), SECOND)).unwrap();
    assert_eq!(keys.load().unwrap().unwrap().device_id, SECOND);
}

#[test]
fn the_sealed_snapshot_uses_the_keychain_vault_and_refuses_an_older_file() {
    let dir = TempDir::new();
    let fake = Arc::new(Fake::default());
    let keys = store(&fake, "p");
    keys.save(&identity(&label("token"), FIRST)).unwrap();
    let file = dir.path().join("snapshot.sealed");
    let snaps = SealedSnapshots::new(&file, "p", Arc::new(keys.clone()));
    let snapshot = |account: &str| dc::Snapshot {
        account_id: account.to_owned(),
        ..dc::Snapshot::default()
    };
    snaps.save(&snapshot("older"));
    let older = std::fs::read(&file).unwrap();
    snaps.save(&snapshot("newer"));
    assert_eq!(snaps.load().unwrap().account_id, "newer");
    std::fs::write(&file, older).unwrap();
    assert!(snaps.load().is_none(), "an older file is refused");
}

#[test]
fn nothing_prints_a_key_a_token_or_the_record() {
    let fake = Arc::new(Fake::default());
    let keys = store(&fake, "p");
    let token = label("token");
    let id = identity(&token, FIRST);
    keys.save(&id).unwrap();
    let shown = format!("{keys:?} {id:?}");
    assert!(!shown.contains(&token));
    let stored = String::from_utf8(fake.items().values().next().unwrap().clone()).unwrap();
    assert!(stored.contains(&token), "the item holds it; only the item");
    assert!(!shown.contains(&stored));
}

#[test]
fn the_probe_tells_a_working_keychain_a_missing_one_and_one_that_said_no() {
    let fake = Arc::new(Fake::default());
    let keys = store(&fake, "p");
    assert_eq!(keys.probe(), Probe::Works);
    assert!(fake.items().is_empty(), "the probe leaves nothing");
    fake.down(Some(Failure::Unusable));
    assert_eq!(keys.probe(), Probe::Unusable);
    // Locked, interaction not allowed, a refused prompt: the keychain exists and may hold the identity.
    fake.down(Some(Failure::Blocked));
    assert_eq!(keys.probe(), Probe::Blocked);
    fake.down(Some(Failure::Other));
    assert_eq!(
        keys.probe(),
        Probe::Blocked,
        "a transient error is not an absent keychain"
    );
    fake.down(None);
    fake.fail_set.store(true, Ordering::SeqCst);
    assert_eq!(keys.probe(), Probe::Blocked);
}

#[test]
fn a_probe_that_could_not_delete_itself_is_overwritten_and_removed_by_the_next() {
    let fake = Arc::new(Fake::default());
    let keys = store(&fake, "p");
    fake.fail_delete.store(true, Ordering::SeqCst);
    assert_eq!(keys.probe(), Probe::Works);
    assert_eq!(fake.items().len(), 1, "a stale probe item");
    fake.fail_delete.store(false, Ordering::SeqCst);
    assert_eq!(keys.probe(), Probe::Works);
    assert!(fake.items().is_empty(), "the next probe swept it");
}

// ---- which store a profile uses ----

fn marker_text(dir: &std::path::Path, profile: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(format!("account-keys-{profile}.store"))).ok()
}

#[test]
fn a_keychain_that_is_locked_or_failing_never_switches_the_profile_to_the_file_store() {
    for failure in [Failure::Blocked, Failure::Other] {
        let dir = TempDir::new();
        let fake = Arc::new(Fake::default());
        let keys = store(&fake, "p");
        // The profile already has an identity in the keychain from an earlier run.
        keys.save(&identity(&label("token"), FIRST)).unwrap();
        fake.down(Some(failure));
        // Locked or failing: an error to retry, no marker, no choice, no second identity.
        assert!(
            choose(dir.path(), "p", Some(&keys), false).is_err(),
            "{failure:?}"
        );
        assert!(marker_text(dir.path(), "p").is_none());
        assert!(!dir.path().join("dev-keystore-p.json").exists());
        // Unlocked again: it is the keychain, and it is the same identity.
        fake.down(None);
        assert_eq!(
            choose(dir.path(), "p", Some(&keys), false),
            Ok(Kind::Keychain)
        );
        assert_eq!(keys.load().unwrap().unwrap().device_id, FIRST);
    }
}

#[test]
fn only_a_missing_keychain_lets_a_new_profile_use_the_file_store() {
    let dir = TempDir::new();
    let fake = Arc::new(Fake::default());
    let keys = store(&fake, "p");
    fake.down(Some(Failure::Unusable));
    assert_eq!(choose(dir.path(), "p", Some(&keys), false), Ok(Kind::File));
    assert_eq!(marker_text(dir.path(), "p").as_deref(), Some("file"));
    // No keychain backend at all (not macOS) is the same.
    assert_eq!(choose(dir.path(), "q", None, false), Ok(Kind::File));
    // A file store already on disk from before the marker existed is kept, and recorded.
    fake.down(None);
    assert_eq!(choose(dir.path(), "r", Some(&keys), true), Ok(Kind::File));
    assert_eq!(marker_text(dir.path(), "r").as_deref(), Some("file"));
    // A working keychain is the choice for a new profile, and recorded.
    assert_eq!(
        choose(dir.path(), "s", Some(&keys), false),
        Ok(Kind::Keychain)
    );
    assert_eq!(marker_text(dir.path(), "s").as_deref(), Some("keychain"));
}

#[test]
fn the_marker_is_final_in_both_directions() {
    let dir = TempDir::new();
    let fake = Arc::new(Fake::default());
    let keys = store(&fake, "p");
    assert_eq!(
        choose(dir.path(), "p", Some(&keys), false),
        Ok(Kind::Keychain)
    );
    // The keychain goes away, even for good: the profile is unavailable, not moved to a file.
    for failure in [Failure::Unusable, Failure::Blocked, Failure::Other] {
        fake.down(Some(failure));
        assert!(
            choose(dir.path(), "p", Some(&keys), false).is_err(),
            "{failure:?}"
        );
        assert!(
            choose(dir.path(), "p", Some(&keys), true).is_err(),
            "{failure:?}"
        );
    }
    assert_eq!(marker_text(dir.path(), "p").as_deref(), Some("keychain"));
    // A file profile stays a file profile even when a keychain starts answering.
    fake.down(None);
    assert_eq!(choose(dir.path(), "f", None, false), Ok(Kind::File));
    assert_eq!(choose(dir.path(), "f", Some(&keys), false), Ok(Kind::File));
    // A marker that says something else is refused.
    std::fs::write(dir.path().join("account-keys-x.store"), "somewhere").unwrap();
    assert!(choose(dir.path(), "x", Some(&keys), false).is_err());
}

#[test]
fn two_identities_for_one_profile_are_refused_not_picked() {
    let dir = TempDir::new();
    let fake = Arc::new(Fake::default());
    let keys = store(&fake, "p");
    keys.save(&identity(&label("token"), FIRST)).unwrap();
    assert!(choose(dir.path(), "p", Some(&keys), true).is_err());
    assert!(marker_text(dir.path(), "p").is_none());
}

#[test]
fn a_wipe_clears_both_stores_and_returns_the_failure_of_the_one_in_use() {
    let fake = Arc::new(Fake::default());
    let keys = store(&fake, "p");
    keys.save(&identity(&label("token"), FIRST)).unwrap();
    let dir = TempDir::new();
    let file = crate::account::engine::FileKeyStore::new(dir.path().join("k.json"));
    file.save(&identity(&label("token"), SECOND)).unwrap();
    let other = file.clone();
    let paired = Paired {
        primary: keys.clone(),
        other: Arc::new(move || {
            let _ = other.clear();
        }),
    };
    paired.clear().unwrap();
    assert!(fake.items().is_empty(), "the one in use");
    assert!(!dir.path().join("k.json").exists(), "the other");

    // The one in use failing is returned, and the other is still cleared.
    keys.save(&identity(&label("token"), FIRST)).unwrap();
    file.save(&identity(&label("token"), SECOND)).unwrap();
    fake.fail_delete.store(true, Ordering::SeqCst);
    assert!(paired.clear().is_err());
    assert!(!dir.path().join("k.json").exists());
    assert!(
        fake.items().values().all(Vec::is_empty),
        "blanked, no usable key"
    );
}

// ---- the adapter: every wipe path deletes the item ----

type Account = EngineAccount<Wire, KeychainKeyStore, dc::SystemClock>;
type Zones = EngineZones<Wire, KeychainKeyStore, dc::SystemClock>;

struct Device {
    account: Arc<Account>,
    zones: Arc<Zones>,
    fake: Arc<Fake>,
    snap_file: std::path::PathBuf,
}

fn device(server: &Arc<Server>, dir: &std::path::Path, id: &str, name: &str) -> Device {
    let keys = dc::DeviceKeys::generate().unwrap();
    let token = server.add_device(id, name, Some("engineering"), &keys.public(), true);
    let fake = Arc::new(Fake::default());
    let store = store(&fake, name);
    let mut stored = identity(&token, id);
    stored.device_keys = keys.to_bytes();
    store.save(&stored).unwrap();
    let snap_file = dir.join(format!("{name}.sealed"));
    let snaps = Arc::new(SealedSnapshots::new(
        &snap_file,
        name,
        Arc::new(store.clone()),
    ));
    let client = dc::DeviceClient::new(
        Wire(server.clone()),
        store,
        dc::SystemClock,
        dc::ClientConfig {
            device_name: name.to_owned(),
            platform: dc::DevicePlatform::Macos,
            app: Some(dc::DeviceApp::Engineering),
        },
    );
    let (account, zones) = parts_with(
        client,
        snaps,
        Duration::from_millis(300),
        Duration::from_millis(10),
    );
    Device {
        account,
        zones,
        fake,
        snap_file,
    }
}

async fn synced(server: &Arc<Server>, dir: &std::path::Path, id: &str, name: &str) -> Device {
    let d = device(server, dir, id, name);
    d.zones.pull(ZONE).await.unwrap();
    assert!(d.snap_file.exists() && d.fake.items().len() == 1);
    d
}

fn gone(d: &Device) -> bool {
    d.fake.items().is_empty() && !d.snap_file.exists()
}

#[tokio::test]
async fn sign_out_removing_this_device_and_an_ended_session_each_delete_the_keychain_item() {
    let dir = TempDir::new();
    let server = Server::new();

    let d = synced(&server, dir.path(), FIRST, "one").await;
    d.account.sign_out().await.unwrap();
    assert!(gone(&d), "sign-out");

    let one = synced(&server, dir.path(), FIRST, "uno").await;
    let _two = synced(&server, dir.path(), SECOND, "dos").await;
    one.account.remove_device(SECOND).await.unwrap();
    assert!(
        !one.fake.items().is_empty(),
        "removing another device keeps the keys"
    );
    one.account.remove_device(FIRST).await.unwrap();
    assert!(gone(&one), "removing this device");

    let d = synced(&server, dir.path(), FIRST, "tres").await;
    server.refuse("/v1/zones", 401, "unauthorized");
    assert_eq!(
        d.zones.pull(ZONE).await.err(),
        Some(DeviceError::Unauthorized)
    );
    d.zones.wipe_keys().await;
    server.clear_refusal();
    assert!(gone(&d), "an ended session");

    let d = synced(&server, dir.path(), FIRST, "cuatro").await;
    d.account.wipe_keys().await;
    assert!(gone(&d), "an account that is gone");
}

#[tokio::test]
async fn a_failed_keychain_delete_blocks_the_client_and_the_retry_wipes() {
    let dir = TempDir::new();
    let server = Server::new();
    let d = synced(&server, dir.path(), FIRST, "one").await;
    d.fake.fail_delete.store(true, Ordering::SeqCst);
    d.account.wipe_keys().await;
    assert!(
        !d.fake.items().is_empty(),
        "the item is still there, blanked"
    );
    assert!(d.fake.items().values().all(Vec::is_empty));
    let calls = server.calls("/v1/");
    assert_eq!(
        d.zones.pull(ZONE).await.err(),
        Some(DeviceError::Unauthorized)
    );
    assert_eq!(
        server.calls("/v1/"),
        calls,
        "the client was not opened again"
    );
    d.fake.fail_delete.store(false, Ordering::SeqCst);
    assert_eq!(
        d.zones.pull(ZONE).await.err(),
        Some(DeviceError::Unauthorized)
    );
    assert!(gone(&d));
}

/// The real login keychain, by hand: `cargo test -p orbyn-core --features engine-client --lib
/// keychain::the_real -- --ignored`. It makes, reads and deletes one throwaway item.
#[cfg(target_os = "macos")]
#[test]
#[ignore = "touches the real login keychain"]
fn the_real_macos_keychain_round_trips_and_deletes() {
    let profile = label("smoke");
    let keys = KeychainKeyStore::system(&profile);
    assert_eq!(keys.probe(), Probe::Works);
    let token = label("token");
    keys.save(&identity(&token, FIRST)).unwrap();
    assert_eq!(keys.load().unwrap().unwrap().device_id, FIRST);
    keys.set_anchor(Anchor {
        counter: 1,
        hash: [9; 32],
    })
    .unwrap();
    assert!(keys.anchor().is_some());
    keys.clear().unwrap();
    assert!(keys.load().unwrap().is_none());
    keys.clear().unwrap();
}

// ---- connect_local: the choice, end to end ----

#[tokio::test]
async fn connect_local_over_a_locked_keychain_is_unavailable_and_makes_no_second_identity() {
    use crate::account::engine::connect_local_with;
    let dir = TempDir::new();
    let fake = Arc::new(Fake::default());
    let keys = store(&fake, "p");
    keys.save(&identity(&label("token"), FIRST)).unwrap();
    fake.down(Some(Failure::Blocked));
    let locked = connect_local_with(
        "http://127.0.0.1:1",
        "x",
        dir.path(),
        "p",
        Some(keys.clone()),
    );
    assert!(matches!(
        locked,
        Err(DeviceError::Server { retryable: true })
    ));
    assert!(marker_text(dir.path(), "p").is_none());
    assert!(
        !dir.path().join("dev-keystore-p.json").exists(),
        "no file store, no second identity"
    );
    // Unlocked again, it is the keychain and the same identity, and a marker says so.
    fake.down(None);
    assert!(
        connect_local_with(
            "http://127.0.0.1:1",
            "x",
            dir.path(),
            "p",
            Some(keys.clone())
        )
        .is_ok()
    );
    assert_eq!(marker_text(dir.path(), "p").as_deref(), Some("keychain"));
    assert_eq!(keys.load().unwrap().unwrap().device_id, FIRST);
    // The profile stays on the keychain when it goes away later.
    fake.down(Some(Failure::Unusable));
    assert!(matches!(
        connect_local_with("http://127.0.0.1:1", "x", dir.path(), "p", Some(keys)),
        Err(crate::account::DeviceError::Server { retryable: true })
    ));
}

#[tokio::test]
async fn connect_local_without_a_keychain_uses_the_file_store_and_a_wipe_clears_both() {
    use crate::account::engine::connect_local_with;
    let dir = TempDir::new();
    let fake = Arc::new(Fake::default());
    let keys = store(&fake, "p");
    fake.down(Some(Failure::Unusable));
    let backend = connect_local_with(
        "http://127.0.0.1:1",
        "x",
        dir.path(),
        "p",
        Some(keys.clone()),
    )
    .unwrap();
    assert_eq!(marker_text(dir.path(), "p").as_deref(), Some("file"));
    // A stray identity in the keychain (it came back) and one in the file store; sign-out clears both.
    fake.down(None);
    keys.save(&identity(&label("token"), FIRST)).unwrap();
    let file = crate::account::engine::FileKeyStore::new(dir.path().join("dev-keystore-p.json"));
    file.save(&identity(&label("token"), SECOND)).unwrap();
    let _ = backend.account.sign_out().await;
    assert!(
        !dir.path().join("dev-keystore-p.json").exists(),
        "the file store"
    );
    assert!(fake.items().is_empty(), "the keychain too");
}

#[test]
fn a_non_loopback_address_builds_nothing_whatever_the_keychain() {
    use crate::account::engine::connect_local_with;
    let dir = TempDir::new();
    let fake = Arc::new(Fake::default());
    let keys = store(&fake, "p");
    let refused = connect_local_with("http://example.com:1", "x", dir.path(), "p", Some(keys));
    assert!(matches!(refused, Err(DeviceError::Offline)));
    assert!(
        marker_text(dir.path(), "p").is_none(),
        "nothing was chosen or recorded"
    );
}
