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
    Anchor, DEV_SERVICE, EngineAccount, EngineZones, KeychainKeyStore, SealedSnapshots,
    SecretBackend, SnapshotStore, Vault, parts_with,
};

const FIRST: &str = "0a0a0a0a-0000-4000-8000-0000000000a1";
const SECOND: &str = "0a0a0a0a-0000-4000-8000-0000000000b2";

/// A keychain in memory. `fail_delete` makes every delete fail; `fail_set` every write.
#[derive(Default)]
struct Fake {
    items: Mutex<BTreeMap<(String, String), Vec<u8>>>,
    fail_delete: AtomicBool,
    fail_set: AtomicBool,
}

impl Fake {
    fn items(&self) -> std::sync::MutexGuard<'_, BTreeMap<(String, String), Vec<u8>>> {
        self.items.lock().unwrap()
    }
}

impl SecretBackend for Fake {
    fn get(
        &self,
        service: &str,
        account: &str,
    ) -> Result<Option<dc::Zeroizing<Vec<u8>>>, dc::StoreError> {
        Ok(self
            .items()
            .get(&(service.to_owned(), account.to_owned()))
            .cloned()
            .map(dc::Zeroizing::new))
    }

    fn set(&self, service: &str, account: &str, bytes: &[u8]) -> Result<(), dc::StoreError> {
        if self.fail_set.load(Ordering::SeqCst) {
            return Err(dc::StoreError);
        }
        self.items()
            .insert((service.to_owned(), account.to_owned()), bytes.to_vec());
        Ok(())
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), dc::StoreError> {
        if self.fail_delete.load(Ordering::SeqCst) {
            return Err(dc::StoreError);
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
fn the_probe_tells_a_working_keychain_from_a_broken_one_and_leaves_nothing() {
    let fake = Arc::new(Fake::default());
    let keys = store(&fake, "p");
    assert!(keys.available());
    assert!(fake.items().is_empty());
    fake.fail_set.store(true, Ordering::SeqCst);
    assert!(!keys.available());
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
    assert!(keys.available());
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
