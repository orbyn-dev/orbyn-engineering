//! What the worker leaves alone: local changes that were never queued, stamps from the
//! future, the first sync, record growth and old removal markers.

use super::*;
use crate::account::settings_record::{MAX_LIST, MAX_RECORD_BYTES, TOMBSTONE_HORIZON_MS};

async fn nudge(core: &Core, api: &FakeDeviceApi, n: usize) {
    // A local change to an unrelated key wakes the worker for one more cycle.
    set(core, "default_model", json!(format!("m{n}")));
    until(core, |core| online(core) && api.commits() >= n).await;
}

#[tokio::test]
async fn a_setting_that_went_over_its_limit_after_it_synced_is_not_put_back() {
    let dir = TempDir::new();
    let core = on(&dir, "a");
    let api = Arc::new(FakeDeviceApi::new());
    let _worker = start(&core, &api);
    set(&core, "branch_name_instructions", json!("short"));
    until(&core, |core| api.commits() == 1 && online(core)).await;
    let big = "x".repeat(record::MAX_VALUE_BYTES + 1);
    set(&core, "branch_name_instructions", json!(big));
    nudge(&core, &api, 2).await;
    // The account still holds the old value; this device keeps the person's.
    assert_eq!(
        api.zone(ZONE).unwrap().record["branch_name_instructions"].value,
        json!("short")
    );
    assert_eq!(stored(&core, "branch_name_instructions"), json!(big));
    peer(
        &api,
        future(),
        vec![("default_effort".into(), json!("low"))],
    );
    until(&core, |core| {
        stored(core, "default_effort") == json!("low") && online(core)
    })
    .await;
    assert_eq!(stored(&core, "branch_name_instructions"), json!(big));
}

#[tokio::test]
async fn a_change_that_never_reached_the_queue_is_not_put_back() {
    let dir = TempDir::new();
    let core = on(&dir, "a");
    let api = Arc::new(FakeDeviceApi::new());
    let _worker = start(&core, &api);
    set(&core, "default_effort", json!("high"));
    until(&core, |core| api.commits() == 1 && online(core)).await;
    // The write is made as a synced one is, which queues nothing: as if the queue failed.
    core.store
        .change_settings_synced(|_| {
            let mut writes = Vec::new();
            preferences::prepare_synced_write(&mut writes, "default_effort", Some(&json!("low")));
            Ok(((), writes))
        })
        .unwrap();
    nudge(&core, &api, 2).await;
    assert_eq!(stored(&core, "default_effort"), json!("low"));
    assert_eq!(waiting(&core), 0);
}

#[tokio::test]
async fn a_hand_edited_keybindings_file_is_not_put_back() {
    let dir = TempDir::new();
    let core = on(&dir, "a");
    let api = Arc::new(FakeDeviceApi::new());
    let synced = rule("mod+y", "sidebar.toggle");
    peer(&api, future(), vec![rule_field(&synced)]);
    let _worker = start(&core, &api);
    until(&core, |core| {
        core.keybindings().rules.contains(&synced) && online(core)
    })
    .await;
    let mine = rule("mod+q", "sidebar.toggle");
    std::fs::write(
        dir.path().join("a/keybindings.json"),
        serde_json::to_vec(&[&mine]).unwrap(),
    )
    .unwrap();
    peer(
        &api,
        future() + 1,
        vec![("default_effort".into(), json!("low"))],
    );
    until(&core, |core| {
        stored(core, "default_effort") == json!("low") && online(core)
    })
    .await;
    let rules = core.keybindings().rules;
    assert!(rules.contains(&mine) && !rules.contains(&synced));
}

#[tokio::test]
async fn a_stamp_from_the_future_is_kept_and_passed_on_but_never_applied() {
    let dir = TempDir::new();
    let core = on(&dir, "a");
    let api = Arc::new(FakeDeviceApi::new());
    // The largest stamp there is must survive the store and an upload unchanged.
    peer(
        &api,
        i64::MAX,
        vec![("default_effort".into(), json!("high"))],
    );
    let _worker = start(&core, &api);
    set(&core, "default_model", json!("m"));
    until(&core, |core| api.commits() == 1 && online(core)).await;
    assert_ne!(stored(&core, "default_effort"), json!("high"));
    assert_eq!(core.account.status().held_back, 1);
    let kept = core.store.account_record(ZONE).unwrap();
    assert_eq!(kept["default_effort"].stamp.wall_ms, i64::MAX);
    assert_eq!(
        api.zone(ZONE).unwrap().record["default_effort"]
            .stamp
            .wall_ms,
        i64::MAX
    );
    // No loop: a stable record uploads nothing more.
    nudge(&core, &api, 2).await;
    assert_eq!(api.commits(), 2);
    assert_eq!(core.account.status().held_back, 1);
}

#[tokio::test]
async fn a_first_sync_never_beats_a_newer_account_value() {
    let dir = TempDir::new();
    let core = on(&dir, "a");
    let api = Arc::new(FakeDeviceApi::new());
    // This device set both before the account was on; the account has a newer value of one.
    set(&core, "default_effort", json!("low"));
    set(&core, "default_model", json!("mine"));
    peer(
        &api,
        crate::util::unix_now_ms() - 1,
        vec![("default_effort".into(), json!("high"))],
    );
    let _worker = start(&core, &api);
    until(&core, |core| {
        stored(core, "default_effort") == json!("high") && online(core)
    })
    .await;
    assert_eq!(stored(&core, "default_model"), json!("mine"));
    // What the account lacked is uploaded, and what it had is not overwritten.
    let server = api.zone(ZONE).unwrap().record;
    assert_eq!(server["default_effort"].value, json!("high"));
    assert_eq!(server["default_model"].value, json!("mine"));
    assert!(core.account.conflicts().unwrap().is_empty());
}

fn filler(n: usize, each: usize) -> Vec<(String, Value)> {
    (0..n)
        .map(|i| (format!("filler_{i}"), json!("x".repeat(each))))
        .collect()
}

#[tokio::test]
async fn a_full_record_keeps_only_the_new_setting_local_and_does_not_pause_the_zone() {
    let dir = TempDir::new();
    let core = on(&dir, "a");
    let api = Arc::new(FakeDeviceApi::new());
    peer(&api, future(), filler(8, 7_900));
    let _worker = start(&core, &api);
    until(&core, online).await;
    set(&core, "branch_name_instructions", json!("y".repeat(6_000)));
    set(&core, "default_effort", json!("high"));
    until(&core, |core| api.commits() == 1 && online(core)).await;
    let server = api.zone(ZONE).unwrap().record;
    assert!(!server.contains_key("branch_name_instructions"));
    assert_eq!(server["default_effort"].value, json!("high"));
    assert_eq!(core.account.status().not_synced, 1);
    assert_eq!(
        stored(&core, "branch_name_instructions")
            .as_str()
            .unwrap()
            .len(),
        6_000
    );
}

#[tokio::test]
async fn a_record_that_grew_past_the_limit_by_merging_drops_only_the_queued_setting() {
    let dir = TempDir::new();
    let core = on(&dir, "a");
    let api = Arc::new(FakeDeviceApi::new());
    // Queued while the account held little; another device then filled the record.
    set(&core, "branch_name_instructions", json!("y".repeat(7_000)));
    set(&core, "default_effort", json!("high"));
    peer(&api, future(), filler(8, 7_900));
    let _worker = start(&core, &api);
    until(&core, |core| api.commits() == 1 && online(core)).await;
    let server = api.zone(ZONE).unwrap().record;
    assert!(crate::account::settings_record::record_bytes(&server) <= MAX_RECORD_BYTES);
    assert!(!server.contains_key("branch_name_instructions"));
    assert_eq!(server["default_effort"].value, json!("high"));
    assert_eq!(
        server
            .keys()
            .filter(|name| name.starts_with("filler_"))
            .count(),
        8
    );
    assert_eq!(core.account.status().not_synced, 1);
}

#[tokio::test]
async fn old_removal_markers_are_compacted_when_uploading_and_recent_ones_stay() {
    let dir = TempDir::new();
    let core = on(&dir, "a");
    let api = Arc::new(FakeDeviceApi::new());
    let now = crate::util::unix_now_ms();
    let marker = |id: &str| {
        (
            record::element_name("favorite_models", id),
            json!({"id": id, "removed": true}),
        )
    };
    let (old, recent) = (marker("old"), marker("recent"));
    api.peer_write(
        ZONE,
        "peer",
        now - TOMBSTONE_HORIZON_MS - 1,
        vec![old.clone()],
    );
    api.peer_write(ZONE, "peer", now - 1000, vec![recent.clone()]);
    let _worker = start(&core, &api);
    set(&core, "default_effort", json!("high"));
    until(&core, |core| api.commits() == 1 && online(core)).await;
    let server = api.zone(ZONE).unwrap().record;
    assert!(!server.contains_key(&old.0));
    assert!(server.contains_key(&recent.0));
}

#[tokio::test]
async fn a_peer_cannot_grow_a_list_past_the_limit() {
    let dir = TempDir::new();
    let core = on(&dir, "a");
    let api = Arc::new(FakeDeviceApi::new());
    let fields = (0..MAX_LIST + 100)
        .map(|n| {
            let id = format!("m{n}");
            (
                record::element_name("favorite_models", &id),
                json!({ "id": id }),
            )
        })
        .collect();
    peer(&api, future(), fields);
    let _worker = start(&core, &api);
    until(&core, |core| {
        stored(core, "favorite_models")
            .as_array()
            .is_some_and(|list| !list.is_empty())
            && online(core)
    })
    .await;
    assert_eq!(
        stored(&core, "favorite_models").as_array().unwrap().len(),
        MAX_LIST
    );
}
