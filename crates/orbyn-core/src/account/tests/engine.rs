//! The adapter over the real device client, against a scripted server with no network: the
//! approval mapping end to end with two clients, the sync merge with two devices, the error
//! table, and the gate. Values are named by label; nothing here is a secret.

use std::sync::Arc;
use std::time::Duration;

use orbyn_device_client as dc;
use orbyn_device_client::KeyStore as _;
use serde_json::json;

use super::engine_server::{ACCOUNT, EMAIL, Server, Wire};
use super::*;
use crate::account::DeviceApi as _;
use crate::account::SyncHandle;
use crate::account::api::{AccountApi, ApprovalDigits, ApprovalStatus};
use crate::account::engine::{EngineAccount, EngineZones, connect, parts};

type Account = EngineAccount<Wire, dc::MemoryStore, dc::SystemClock>;
type Zones = EngineZones<Wire, dc::MemoryStore, dc::SystemClock>;

const APPROVER: &str = "0a0a0a0a-0000-4000-8000-0000000000a1";
const NEWCOMER: &str = "0a0a0a0a-0000-4000-8000-0000000000b2";

struct Device {
    account: Arc<Account>,
    zones: Arc<Zones>,
}

fn device(server: &Arc<Server>, id: &str, name: &str, trusted: bool) -> Device {
    let keys = dc::DeviceKeys::generate().unwrap();
    let token = server.add_device(id, name, Some("engineering"), &keys.public(), trusted);
    let store = dc::MemoryStore::new();
    store
        .save(&dc::StoredIdentity {
            device_keys: keys.to_bytes(),
            account_id: ACCOUNT.to_owned(),
            device_id: id.to_owned(),
            email: EMAIL.to_owned(),
            session: Some(dc::StoredSession {
                token: dc::Zeroizing::new(token),
                expires_at: "2099-01-01T00:00:00Z".to_owned(),
            }),
            vault_key: dc::Zeroizing::new(crate::util::random_bytes::<32>().unwrap()),
        })
        .unwrap();
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
    let (account, zones) = parts(
        client,
        Duration::from_millis(300),
        Duration::from_millis(10),
    );
    Device { account, zones }
}

/// The two devices of an account, the second still waiting to be let in.
fn pair() -> (Arc<Server>, Device, Device) {
    let server = Server::new();
    let approver = device(&server, APPROVER, "Mac mini", true);
    let newcomer = device(&server, NEWCOMER, "Laptop", false);
    (server, approver, newcomer)
}

/// The new device asks until the trusted one has allowed it.
async fn wait_allowed(newcomer: &Device) {
    loop {
        match newcomer.account.approval_status(NEWCOMER).await.unwrap() {
            ApprovalStatus::Allowed => return,
            ApprovalStatus::Waiting => tokio::time::sleep(Duration::from_millis(10)).await,
            _ => panic!("the request ended"),
        }
    }
}

/// Allow on the trusted device while the new device keeps asking, as its screen does.
async fn allow(approver: &Device, newcomer: &Device) -> ApprovalDigits {
    newcomer.account.expect_approval(NEWCOMER).await;
    let (digits, ()) = tokio::join!(
        async { approver.account.approve(NEWCOMER).await.unwrap() },
        wait_allowed(newcomer)
    );
    digits
}

/// Digits that differ from `right` in every place.
fn other_than(right: &ApprovalDigits) -> ApprovalDigits {
    ApprovalDigits::new(
        right
            .expose()
            .chars()
            .map(|c| char::from(b'0' + (c.to_digit(10).unwrap_or(0) as u8 + 1) % 10))
            .collect(),
    )
}

#[tokio::test]
async fn a_pending_device_is_listed_as_a_request_and_allow_ends_in_the_typed_check() {
    let (server, approver, newcomer) = pair();
    let listed = approver.account.approval_requests().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, NEWCOMER);
    assert_eq!(listed[0].device_name, "Laptop");
    assert_eq!(
        (listed[0].app.as_str(), listed[0].allowed),
        ("engineering", false)
    );
    assert_eq!(listed[0].expires_in_secs, 0, "no time left is claimed");
    assert!(
        approver
            .account
            .devices()
            .await
            .unwrap()
            .iter()
            .all(|d| d.id == APPROVER)
    );

    let digits = allow(&approver, &newcomer).await;
    assert_eq!(digits.expose().len(), 6);
    // The Engine's count of the time left is passed on for display and decides nothing.
    let left = approver.account.time_left(NEWCOMER).unwrap();
    assert!(left <= Duration::from_secs(600) && left > Duration::from_secs(500));
    // Allowed and confirmed, not yet trusted: only the typed digits finish it.
    assert!(!server.is_trusted(NEWCOMER));
    assert!(approver.account.approval_requests().await.unwrap()[0].allowed);

    // Wrong digits cost a try and leave the approval open for the right ones.
    let wrong = other_than(&digits);
    let first = newcomer.account.finish_approval(NEWCOMER, &wrong).await;
    assert_eq!(
        first.err(),
        Some(DeviceError::WrongDigits { tries_left: 2 })
    );
    let second = newcomer.account.finish_approval(NEWCOMER, &wrong).await;
    assert_eq!(
        second.err(),
        Some(DeviceError::WrongDigits { tries_left: 1 })
    );
    assert!(server.approval_open());
    let session = newcomer
        .account
        .finish_approval(NEWCOMER, &digits)
        .await
        .unwrap();
    assert_eq!(
        (session.email.as_str(), session.device_id.as_str()),
        (EMAIL, NEWCOMER)
    );
    assert!(server.is_trusted(NEWCOMER));
    // Used up: nothing signs in a second time, and the request is no longer listed.
    let again = newcomer.account.finish_approval(NEWCOMER, &digits).await;
    assert_eq!(again.err(), Some(DeviceError::RequestGone));
    assert!(
        approver
            .account
            .approval_requests()
            .await
            .unwrap()
            .is_empty()
    );
    let names: Vec<_> = approver
        .account
        .devices()
        .await
        .unwrap()
        .into_iter()
        .map(|d| d.id)
        .collect();
    assert!(names.contains(&NEWCOMER.to_owned()));
}

#[tokio::test]
async fn three_wrong_entries_close_the_approval_and_never_reach_the_engine_as_a_guess() {
    let (server, approver, newcomer) = pair();
    let digits = allow(&approver, &newcomer).await;
    let wrong = other_than(&digits);
    for _ in 0..2 {
        newcomer
            .account
            .finish_approval(NEWCOMER, &wrong)
            .await
            .unwrap_err();
    }
    let last = newcomer.account.finish_approval(NEWCOMER, &wrong).await;
    assert_eq!(last.err(), Some(DeviceError::DigitsLocked));
    assert!(!server.approval_open(), "the approval was cancelled");
    assert!(!server.is_trusted(NEWCOMER));
    assert_eq!(server.calls("/code"), 0, "no digits were sent");
}

#[tokio::test]
async fn cancel_on_the_digits_removes_the_device_and_the_new_device_sees_it_ended() {
    let (server, approver, newcomer) = pair();
    allow(&approver, &newcomer).await;
    approver.account.deny(NEWCOMER).await.unwrap();
    assert!(server.was_removed(NEWCOMER));
    let status = newcomer.account.approval_status(NEWCOMER).await;
    assert_eq!(status.err(), Some(DeviceError::RequestGone));
    assert!(
        approver
            .account
            .approval_requests()
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn refusing_before_an_approval_exists_hides_the_request_and_sends_nothing() {
    let (server, approver, _newcomer) = pair();
    approver.account.deny(NEWCOMER).await.unwrap();
    assert!(
        approver
            .account
            .approval_requests()
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(server.calls("/approvals"), 0);
    assert!(!server.was_removed(NEWCOMER));
}

#[tokio::test]
async fn allow_gives_up_when_the_new_device_never_joins_and_cancels_what_it_started() {
    let (server, approver, _newcomer) = pair();
    let outcome = approver.account.approve(NEWCOMER).await;
    assert_eq!(outcome.err(), Some(DeviceError::RequestGone));
    assert!(!server.approval_open());
    assert!(!server.is_trusted(NEWCOMER));
}

#[tokio::test]
async fn an_ended_approval_is_gone_for_the_new_device() {
    let (server, approver, newcomer) = pair();
    newcomer.account.expect_approval(NEWCOMER).await;
    assert_eq!(
        newcomer.account.approval_status(NEWCOMER).await.unwrap(),
        ApprovalStatus::Waiting
    );
    let (_, ()) = tokio::join!(
        async {
            let _ = approver.account.approve(NEWCOMER).await;
        },
        wait_allowed(&newcomer)
    );
    server.refuse("/devices/current", 410, "approval_expired");
    let status = newcomer.account.approval_status(NEWCOMER).await;
    assert!(status.is_err());
    server.clear_refusal();
}

#[test]
fn the_engines_errors_become_the_apps_with_nothing_attached() {
    use crate::account::engine::map_for_test as map;
    let cases = [
        (dc::DeviceError::Transport, DeviceError::Offline),
        (
            dc::DeviceError::ReauthenticationRequired,
            DeviceError::ReauthRequired,
        ),
        (dc::DeviceError::LoginFailed, DeviceError::WrongPassword),
        (dc::DeviceError::StoreNotEmpty, DeviceError::AlreadySignedIn),
        (dc::DeviceError::ApprovalExpired, DeviceError::RequestGone),
        (
            dc::DeviceError::ApprovalMismatch,
            DeviceError::ApprovalRefused,
        ),
        (
            dc::DeviceError::LastTrustedDevice,
            DeviceError::LastTrustedDevice,
        ),
        (dc::DeviceError::SessionExpired, DeviceError::Unauthorized),
        (dc::DeviceError::AccountGone, DeviceError::AccountGone),
    ];
    for (engine, app) in cases {
        assert_eq!(map(engine), app);
    }
    assert_eq!(
        map(dc::DeviceError::RateLimited { retry_after: None }),
        DeviceError::RateLimited {
            retry_after: Duration::from_secs(60)
        }
    );
}

#[tokio::test]
async fn removing_the_last_trusted_device_and_a_stale_sign_in_have_their_own_answers() {
    let (server, approver, _newcomer) = pair();
    server.refuse("/v1/devices/", 409, "last_trusted_device");
    let last = approver.account.remove_device(NEWCOMER).await;
    assert_eq!(last.err(), Some(DeviceError::LastTrustedDevice));
    server.refuse("/v1/devices/", 401, "reauthentication_required");
    let stale = approver.account.remove_device(NEWCOMER).await;
    assert_eq!(stale.err(), Some(DeviceError::ReauthRequired));
    assert_eq!(
        crate::account::api::refusal(&DeviceError::ReauthRequired).code(),
        "reauth_required"
    );
}

/// Two trusted devices, each running the real worker against its own store. The first sets a
/// value and syncs, so its zone exists when it lets the second in and hands over the zone key.
async fn trusted_pair(dir: &TempDir) -> (Core, Core, Arc<Server>, [SyncHandle; 2]) {
    let (server, a, b) = pair();
    let (core_a, core_b) = (on(dir, "a"), on(dir, "b"));
    set(&core_a, "preview_appearance", json!("system"));
    a.zones.open().await.unwrap();
    let first = core_a
        .account
        .start_settings_sync(a.zones.clone(), quick())
        .unwrap();
    until(&core_a, online).await;
    let digits = allow(&a, &b).await;
    b.account.finish_approval(NEWCOMER, &digits).await.unwrap();
    // The first device's next pass hands the zone key to the device it just let in.
    a.zones.pull(ZONE).await.unwrap();
    b.zones.open().await.unwrap();
    let seen = b.zones.pull(ZONE).await;
    assert!(
        matches!(&seen, Ok(Some(_))),
        "second device saw {:?} calls {}",
        seen.as_ref().map(|s| s.is_some()),
        server.calls("keychain")
    );
    let second = core_b
        .account
        .start_settings_sync(b.zones.clone(), quick())
        .unwrap();
    until(&core_b, |core| {
        stored(core, "preview_appearance") == json!("system") && online(core)
    })
    .await;
    (core_a, core_b, server, [first, second])
}

#[tokio::test]
async fn two_offline_edits_through_the_engine_converge_and_surface_the_loss() {
    let dir = TempDir::new();
    let (a, b, server, _workers) = trusted_pair(&dir).await;
    server.set_offline(true);
    set(&a, "preview_appearance", json!("light"));
    std::thread::sleep(Duration::from_millis(5));
    set(&b, "preview_appearance", json!("dark"));
    server.set_offline(false);
    // No live stream here, so a device learns of the other's push when its own pass runs.
    until(&b, |b| {
        stored(b, "preview_appearance") == json!("dark") && online(b)
    })
    .await;
    a.store.account_poke();
    until(&a, |a| {
        stored(a, "preview_appearance") == json!("dark") && online(a)
    })
    .await;
    // The Engine decided the winner; the list is the diff of the queued edit against its result.
    let lost: Vec<_> = a
        .account
        .conflicts()
        .unwrap()
        .into_iter()
        .chain(b.account.conflicts().unwrap())
        .collect();
    assert_eq!(lost.len(), 1, "{lost:?}");
    assert_eq!(
        (lost[0].kept.clone(), lost[0].lost.clone()),
        (json!("dark"), json!("light"))
    );
}

#[tokio::test]
async fn a_setting_that_never_syncs_stays_on_its_device_through_the_engine() {
    let dir = TempDir::new();
    let (a, b, _server, _workers) = trusted_pair(&dir).await;
    set(&a, "default_permission_mode", json!("full_access"));
    set(&a, "preview_appearance", json!("light"));
    until(&a, online).await;
    b.store.account_poke();
    until(&b, |b| {
        stored(b, "preview_appearance") == json!("light") && online(b)
    })
    .await;
    assert_ne!(stored(&b, "default_permission_mode"), json!("full_access"));
}

#[tokio::test]
async fn under_the_off_gate_nothing_builds_or_calls_the_real_client() {
    let dir = TempDir::new();
    let before = crate::account::engine::CONNECTED.with(std::cell::Cell::get);
    let off = crate::account::AccountCenter::new(
        AccountGate::Off,
        crate::account::AccountBackendKind::Engine {
            address: "http://127.0.0.1:9".into(),
        },
        Arc::new(crate::store::Store::open(&dir.path().join("off.db"), "t").unwrap()),
        dir.path().join("kb.json"),
        Arc::new(std::sync::Mutex::new(())),
    );
    assert!(!off.available());
    assert_eq!(
        crate::account::engine::CONNECTED.with(std::cell::Cell::get),
        before
    );

    // A backend over a counting server, behind the Off gate: every call refuses, nothing is sent.
    let server = Server::new();
    let d = device(&server, APPROVER, "Mac mini", true);
    let backend = crate::account::api::Backend {
        account: d.account.clone(),
        device: d.zones.clone(),
    };
    let gated = crate::account::AccountCenter::with_backend(
        AccountGate::Off,
        backend,
        Arc::new(crate::store::Store::open(&dir.path().join("off2.db"), "t").unwrap()),
        dir.path().join("kb2.json"),
        Arc::new(std::sync::Mutex::new(())),
    );
    assert_eq!(
        gated.devices().await.err().unwrap().code(),
        "account_disabled"
    );
    assert_eq!(
        gated.approval_requests().await.err().unwrap().code(),
        "account_disabled"
    );
    assert!(gated.start_settings_sync(d.zones.clone(), quick()).is_err());
    assert_eq!(server.calls("/"), 0);
}

#[test]
fn only_a_loopback_address_builds_the_real_client() {
    for refused in [
        "https://api.orbyn.dev",
        "http://localhost:1",
        "http://192.168.1.2",
        "nonsense",
    ] {
        assert!(connect(refused, "This computer").is_err(), "{refused}");
    }
    assert!(connect("http://127.0.0.1:9", "This computer").is_ok());
    assert!(connect("http://[::1]:9", "This computer").is_ok());
}
