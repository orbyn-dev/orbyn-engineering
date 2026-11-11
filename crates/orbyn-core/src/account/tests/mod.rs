//! The sync core against the fake `DeviceApi`: offline-first behaviour, merge between
//! two devices, allowlist-only apply, the no-escalation rules and the error table.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use orbyn_contracts::{KeybindingRule, ShellEvent};
use serde_json::{Value, json};

mod allowlist;
mod limits;

use super::fake::FakeDeviceApi;
use super::settings_record::{self as record, ZONE};
use super::{
    AccountGate, DeviceError, DeviceStatus, SignOutReason, SyncHandle, SyncState, SyncTiming,
};
use crate::app::Core;
use crate::testing::TempDir;
use crate::{CoreConfig, settings::preferences};

fn core_at(dir: &TempDir, name: &str, gate: AccountGate) -> Core {
    let mut config = CoreConfig::new(dir.path().join(name), "http://127.0.0.1:1".into());
    config.engines.lookup_latest = false;
    config.account_gate = gate;
    Core::open(config).unwrap()
}

fn on(dir: &TempDir, name: &str) -> Core {
    core_at(dir, name, AccountGate::LocalOnly)
}

fn quick() -> SyncTiming {
    SyncTiming {
        backoff_base: Duration::from_millis(5),
        backoff_max: Duration::from_millis(50),
    }
}

fn start(core: &Core, api: &Arc<FakeDeviceApi>) -> SyncHandle {
    core.account
        .start_settings_sync(api.clone(), quick())
        .unwrap()
}

fn set(core: &Core, key: &str, value: Value) {
    core.set_preferences(None, &BTreeMap::from([(key.to_owned(), value)]))
        .unwrap();
}

fn waiting(core: &Core) -> usize {
    core.store.account_outbox_len(ZONE).unwrap()
}

fn online(core: &Core) -> bool {
    core.account.status().state == SyncState::Online && waiting(core) == 0
}

/// Waits, on the status stream, until `ready` holds. The status changes on every cycle,
/// so this wakes as often as the worker does and never sleeps in a loop.
async fn until(core: &Core, ready: impl Fn(&Core) -> bool) {
    let mut status = core.account.subscribe();
    tokio::time::timeout(Duration::from_secs(10), status.wait_for(|_| ready(core)))
        .await
        .expect("the worker did not get there")
        .unwrap();
}

fn rule(key: &str, command: &str) -> KeybindingRule {
    KeybindingRule {
        key: key.into(),
        command: command.into(),
        when: None,
        remove: false,
    }
}

fn rule_field(rule: &KeybindingRule) -> (String, Value) {
    let record::Staged::Field { name, value } = record::stage_keybinding(rule) else {
        panic!()
    };
    (name, value)
}

fn peer(api: &FakeDeviceApi, wall: i64, fields: Vec<(String, Value)>) {
    api.peer_write(ZONE, "peer", wall, fields);
}

fn future() -> i64 {
    crate::util::unix_now_ms() + 60_000
}

fn stored(core: &Core, key: &str) -> Value {
    core.preference(key, None).unwrap()
}

fn globals(core: &Core) -> BTreeMap<String, String> {
    core.store.settings_snapshot().unwrap().globals
}

/// A valid value for `key` that differs from its current one, as a peer could send.
fn foreign_value(core: &Core, key: &str) -> (Value, bool) {
    let candidates = [
        json!(true),
        json!(false),
        json!(7),
        json!("off"),
        json!("saver"),
        json!("laptop"),
        json!("on_return"),
        json!("merge"),
        json!("none"),
        json!("hold"),
        json!("custom"),
        json!("full_access"),
        json!("worktree"),
        json!(["x"]),
        json!({"a": "b"}),
        json!([{"name": "a", "command": "echo hi"}]),
    ];
    let current = stored(core, key);
    candidates
        .into_iter()
        .find(|value| preferences::check_synced(key, value).is_ok() && *value != current)
        .map_or((json!("unvalidated"), false), |value| (value, true))
}

fn not_allowlisted() -> Vec<&'static str> {
    preferences::catalog_keys()
        .filter(|key| !record::is_synced_key(key))
        .collect()
}

#[tokio::test]
async fn under_the_off_gate_nothing_starts_queues_or_reaches_the_network() {
    let dir = TempDir::new();
    let core = core_at(&dir, "a", AccountGate::default());
    let api = Arc::new(FakeDeviceApi::new());
    let error = core
        .account
        .start_settings_sync(api.clone(), quick())
        .err()
        .unwrap();
    assert_eq!(error.code(), "account_disabled");
    set(&core, "preview_appearance", json!("dark"));
    assert_eq!(stored(&core, "preview_appearance"), json!("dark"));
    assert_eq!(waiting(&core), 0);
    assert!(core.store.account_record(ZONE).unwrap().is_empty());
    assert_eq!(api.pulls(), 0);
    assert_eq!(
        core.account.status().state,
        SyncState::SignedOut { reason: None }
    );
}

#[tokio::test]
async fn a_setting_follows_the_person_to_another_device_and_back() {
    let dir = TempDir::new();
    let (a, b) = (on(&dir, "a"), on(&dir, "b"));
    let api = Arc::new(FakeDeviceApi::new());
    set(&a, "preview_appearance", json!("dark"));
    set(&a, "favorite_models", json!(["m1"]));
    a.save_keybinding(&rule("mod+y", "sidebar.toggle")).unwrap();
    let (_wa, _wb) = (start(&a, &api), start(&b, &api));
    until(&b, |b| {
        stored(b, "preview_appearance") == json!("dark")
            && stored(b, "favorite_models") == json!(["m1"])
            && b.keybindings()
                .rules
                .iter()
                .any(|r| r.key == "mod+y" && r.command == "sidebar.toggle")
    })
    .await;
    // What arrived from the account is not queued again.
    until(&b, online).await;
    assert_eq!(waiting(&b), 0);
    // Two devices adding different models both keep theirs, and a removal travels.
    set(&b, "favorite_models", json!(["m1", "m2"]));
    until(&a, |a| stored(a, "favorite_models") == json!(["m1", "m2"])).await;
    set(&a, "favorite_models", json!(["m2"]));
    until(&b, |b| stored(b, "favorite_models") == json!(["m2"])).await;
    until(&a, online).await;
    until(&b, online).await;
}

#[tokio::test]
async fn a_network_cut_mid_sync_leaves_the_app_working_and_the_queue_drains_later() {
    let dir = TempDir::new();
    let core = on(&dir, "a");
    let mut windows = core.shell.watch();
    let api = Arc::new(FakeDeviceApi::new());
    let _worker = start(&core, &api);
    until(&core, online).await;
    api.cut_on_next_push();
    set(&core, "preview_appearance", json!("dark"));
    let mut status = core.account.subscribe();
    let down = status.wait_for(|s| matches!(s.state, SyncState::Offline { .. }) && s.waiting == 1);
    tokio::time::timeout(Duration::from_secs(10), down)
        .await
        .unwrap()
        .unwrap();
    // Settings still write and read, and nothing waits for the network.
    set(&core, "branch_name_prefix", json!("m"));
    assert_eq!(stored(&core, "preview_appearance"), json!("dark"));
    assert_eq!(stored(&core, "branch_name_prefix"), json!("m"));
    assert_eq!(waiting(&core), 2);
    assert_eq!(api.commits(), 0);
    // The network comes back: the queue drains in one upload, with no duplicate.
    api.set_online(true);
    until(&core, online).await;
    assert_eq!(api.commits(), 1);
    let server = api.zone(ZONE).unwrap().record;
    assert_eq!(server["preview_appearance"].value, json!("dark"));
    assert_eq!(server["branch_name_prefix"].value, json!("m"));
    // The windows were told, over the existing event stream, that the account changed.
    let mut said = Vec::new();
    while let Ok(event) = windows.events.try_recv() {
        said.push(event);
    }
    assert!(said.contains(&ShellEvent::DataChanged {
        what: "account".into()
    }));
}

#[tokio::test]
async fn a_lost_answer_after_the_server_committed_does_not_upload_twice() {
    let dir = TempDir::new();
    let core = on(&dir, "a");
    let api = Arc::new(FakeDeviceApi::new());
    let _worker = start(&core, &api);
    until(&core, online).await;
    api.lose_next_answer();
    set(&core, "preview_appearance", json!("dark"));
    until(&core, |core| api.commits() == 1 && online(core)).await;
    assert_eq!(api.commits(), 1);
    assert_eq!(api.pushes().len(), 1);
}

#[tokio::test]
async fn two_offline_edits_to_one_setting_converge_on_the_newer_and_surface_the_loss() {
    let dir = TempDir::new();
    let (a, b) = (on(&dir, "a"), on(&dir, "b"));
    let mut seen_by_b = b.shell.watch();
    let api = Arc::new(FakeDeviceApi::new());
    set(&a, "preview_appearance", json!("system"));
    let (_wa, _wb) = (start(&a, &api), start(&b, &api));
    until(&b, |b| {
        stored(b, "preview_appearance") == json!("system") && online(b)
    })
    .await;
    until(&a, online).await;
    api.set_online(false);
    set(&a, "preview_appearance", json!("light"));
    std::thread::sleep(Duration::from_millis(5));
    set(&b, "preview_appearance", json!("dark"));
    api.set_online(true);
    until(&a, |a| {
        stored(a, "preview_appearance") == json!("dark") && online(a)
    })
    .await;
    until(&b, |b| {
        stored(b, "preview_appearance") == json!("dark") && online(b)
    })
    .await;
    // The device that met the other's edit while its own was still queued surfaces it.
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
    let mut said = Vec::new();
    while let Ok(event) = seen_by_b.events.try_recv() {
        said.push(event);
    }
    let conflict_event = ShellEvent::DataChanged {
        what: "account:conflicts".into(),
    };
    let in_a = a.account.conflicts().unwrap().len() == 1;
    assert!(in_a || said.contains(&conflict_event));
    // An edit made after seeing the other's value is not a conflict.
    set(&a, "preview_appearance", json!("system"));
    until(&b, |b| {
        stored(b, "preview_appearance") == json!("system") && online(b)
    })
    .await;
    let total = a.account.conflicts().unwrap().len() + b.account.conflicts().unwrap().len();
    assert_eq!(total, 1);
    a.account.clear_conflicts().unwrap();
    assert!(a.account.conflicts().unwrap().is_empty());
}

#[tokio::test]
async fn rebinding_a_command_leaves_a_removal_marker_so_no_device_keeps_the_old_key() {
    let dir = TempDir::new();
    let (a, b) = (on(&dir, "a"), on(&dir, "b"));
    let api = Arc::new(FakeDeviceApi::new());
    let (_wa, _wb) = (start(&a, &api), start(&b, &api));
    a.save_keybinding(&rule("mod+y", "sidebar.toggle")).unwrap();
    until(&b, |b| {
        b.keybindings().rules.iter().any(|r| r.key == "mod+y")
    })
    .await;
    a.save_keybinding(&rule("mod+e", "sidebar.toggle")).unwrap();
    until(&b, |b| {
        let rules = b.keybindings().rules;
        rules.iter().any(|r| r.key == "mod+e")
            && !rules.iter().any(|r| r.key == "mod+y" && !r.remove)
    })
    .await;
    let (old, _) = rule_field(&rule("mod+y", "sidebar.toggle"));
    assert_eq!(
        api.zone(ZONE).unwrap().record[&old].value["remove"],
        json!(true)
    );
}

#[tokio::test]
async fn a_setting_over_a_limit_stays_local_and_the_status_counts_it() {
    let dir = TempDir::new();
    let core = on(&dir, "a");
    let api = Arc::new(FakeDeviceApi::new());
    let _worker = start(&core, &api);
    until(&core, online).await;
    set(
        &core,
        "branch_name_instructions",
        json!("x".repeat(record::MAX_VALUE_BYTES + 1)),
    );
    set(&core, "preview_appearance", json!("dark"));
    until(&core, |core| api.commits() == 1 && online(core)).await;
    assert!(
        !api.zone(ZONE)
            .unwrap()
            .record
            .contains_key("branch_name_instructions")
    );
    assert_eq!(core.account.status().not_synced, 1);
    assert_eq!(
        stored(&core, "branch_name_instructions")
            .as_str()
            .unwrap()
            .len(),
        record::MAX_VALUE_BYTES + 1
    );
}

#[tokio::test]
async fn an_invalid_keyboard_file_pauses_with_a_reason_and_still_uploads() {
    let dir = TempDir::new();
    let core = on(&dir, "a");
    std::fs::write(dir.path().join("a/keybindings.json"), "{ not rules").unwrap();
    let api = Arc::new(FakeDeviceApi::new());
    peer(
        &api,
        future(),
        vec![rule_field(&rule("mod+e", "sidebar.toggle"))],
    );
    let _worker = start(&core, &api);
    set(&core, "preview_appearance", json!("dark"));
    let mut status = core.account.subscribe();
    let paused = status.wait_for(|s| {
        s.state
            == SyncState::Paused {
                reason: super::PauseReason::InvalidKeyboardFile,
            }
    });
    tokio::time::timeout(Duration::from_secs(10), paused)
        .await
        .unwrap()
        .unwrap();
    until(&core, |_| {
        api.zone(ZONE)
            .is_some_and(|zone| zone.record.contains_key("preview_appearance"))
    })
    .await;
    assert_eq!(waiting(&core), 0);
}

#[tokio::test]
async fn each_client_error_becomes_the_status_and_behaviour_in_the_table() {
    use super::PauseReason::{DeletionScheduled, ProtectionAdvanced, TooLarge, ZoneFailed};
    let hour = Duration::from_secs(3600);
    let cases = [
        (DeviceError::Offline, "offline"),
        (DeviceError::Server { retryable: true }, "offline"),
        (DeviceError::Server { retryable: false }, "zone"),
        (DeviceError::InvalidKey, "zone"),
        (DeviceError::TooLarge, "large"),
        (DeviceError::RateLimited { retry_after: hour }, "throttled"),
        (DeviceError::ProtectionAdvanced, "protection"),
        (DeviceError::DeletionScheduled, "deletion"),
        (DeviceError::Unauthorized, "ended"),
        (DeviceError::AccountGone, "gone"),
    ];
    for (error, expect) in cases {
        let dir = TempDir::new();
        let core = on(&dir, "a");
        let api = Arc::new(FakeDeviceApi::new());
        set(&core, "preview_appearance", json!("dark"));
        api.fail_every_call(Some(error.clone()));
        let timing = SyncTiming {
            backoff_base: Duration::from_millis(300),
            backoff_max: Duration::from_secs(1),
        };
        let _worker = core
            .account
            .start_settings_sync(api.clone(), timing)
            .unwrap();
        let wanted = match expect {
            "offline" => SyncState::Offline { since_unix: 0 },
            "zone" => SyncState::Paused { reason: ZoneFailed },
            "large" => SyncState::Paused { reason: TooLarge },
            "throttled" => SyncState::Throttled {
                retry_after_secs: 3600,
            },
            "protection" => SyncState::Paused {
                reason: ProtectionAdvanced,
            },
            "deletion" => SyncState::Paused {
                reason: DeletionScheduled,
            },
            "ended" => SyncState::SignedOut {
                reason: Some(SignOutReason::SessionEnded),
            },
            _ => SyncState::SignedOut {
                reason: Some(SignOutReason::AccountGone),
            },
        };
        let mut status = core.account.subscribe();
        let reached = status.wait_for(|s| match (&s.state, &wanted) {
            (SyncState::Offline { .. }, SyncState::Offline { .. }) => true,
            (state, wanted) => state == wanted,
        });
        tokio::time::timeout(Duration::from_secs(10), reached)
            .await
            .unwrap_or_else(|_| panic!("{error:?}"))
            .unwrap();
        // Whatever happened, the person's setting and the queue are intact and writable.
        assert_eq!(
            stored(&core, "preview_appearance"),
            json!("dark"),
            "{error:?}"
        );
        set(&core, "branch_name_prefix", json!("m"));
        assert_eq!(waiting(&core), 2, "{error:?}");
        assert_eq!(api.commits(), 0, "{error:?}");
    }
}

#[tokio::test]
async fn stopping_the_worker_keeps_queuing_local_changes() {
    let dir = TempDir::new();
    let core = on(&dir, "a");
    let api = Arc::new(FakeDeviceApi::new());
    let worker = start(&core, &api);
    until(&core, online).await;
    worker.stop().await;
    set(&core, "preview_appearance", json!("dark"));
    assert_eq!(waiting(&core), 1);
    assert_eq!(api.commits(), 0);
}
