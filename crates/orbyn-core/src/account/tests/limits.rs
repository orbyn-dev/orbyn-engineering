//! What the worker leaves alone: local changes that were never queued, stamps from the
//! future, the first sync, record growth and old removal markers.

use super::*;
use crate::account::settings_record::{MAX_LIST, MAX_RECORD_BYTES, TOMBSTONE_HORIZON_MS};

async fn nudge(core: &Core, api: &FakeDeviceApi, n: usize) {
    // A local change to an unrelated key wakes the worker for one more cycle.
    set(core, "branch_name_prefix", json!(format!("m{n}")));
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
        vec![("preview_appearance".into(), json!("light"))],
    );
    until(&core, |core| {
        stored(core, "preview_appearance") == json!("light") && online(core)
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
    set(&core, "preview_appearance", json!("dark"));
    until(&core, |core| api.commits() == 1 && online(core)).await;
    // The write is made as a synced one is, which queues nothing: as if the queue failed.
    core.store
        .change_settings_synced(|_| {
            let mut writes = Vec::new();
            preferences::prepare_synced_write(
                &mut writes,
                "preview_appearance",
                Some(&json!("light")),
            );
            Ok(((), writes))
        })
        .unwrap();
    nudge(&core, &api, 2).await;
    assert_eq!(stored(&core, "preview_appearance"), json!("light"));
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
        vec![("preview_appearance".into(), json!("light"))],
    );
    until(&core, |core| {
        stored(core, "preview_appearance") == json!("light") && online(core)
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
        vec![("preview_appearance".into(), json!("dark"))],
    );
    let _worker = start(&core, &api);
    set(&core, "branch_name_prefix", json!("m"));
    until(&core, |core| api.commits() == 1 && online(core)).await;
    assert_ne!(stored(&core, "preview_appearance"), json!("dark"));
    assert_eq!(core.account.status().held_back, 1);
    let kept = core.store.account_record(ZONE).unwrap();
    assert_eq!(kept["preview_appearance"].stamp.wall_ms, i64::MAX);
    assert_eq!(
        api.zone(ZONE).unwrap().record["preview_appearance"]
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
    set(&core, "preview_appearance", json!("light"));
    set(&core, "branch_name_prefix", json!("mine"));
    peer(
        &api,
        crate::util::unix_now_ms() - 1,
        vec![("preview_appearance".into(), json!("dark"))],
    );
    let _worker = start(&core, &api);
    until(&core, |core| {
        stored(core, "preview_appearance") == json!("dark") && online(core)
    })
    .await;
    assert_eq!(stored(&core, "branch_name_prefix"), json!("mine"));
    // What the account lacked is uploaded, and what it had is not overwritten.
    let server = api.zone(ZONE).unwrap().record;
    assert_eq!(server["preview_appearance"].value, json!("dark"));
    assert_eq!(server["branch_name_prefix"].value, json!("mine"));
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
    set(&core, "preview_appearance", json!("dark"));
    until(&core, |core| api.commits() == 1 && online(core)).await;
    let server = api.zone(ZONE).unwrap().record;
    assert!(!server.contains_key("branch_name_instructions"));
    assert_eq!(server["preview_appearance"].value, json!("dark"));
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
    set(&core, "preview_appearance", json!("dark"));
    peer(&api, future(), filler(8, 7_900));
    let _worker = start(&core, &api);
    until(&core, |core| api.commits() == 1 && online(core)).await;
    let server = api.zone(ZONE).unwrap().record;
    assert!(crate::account::settings_record::record_bytes(&server) <= MAX_RECORD_BYTES);
    assert!(!server.contains_key("branch_name_instructions"));
    assert_eq!(server["preview_appearance"].value, json!("dark"));
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
    set(&core, "preview_appearance", json!("dark"));
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

#[tokio::test]
async fn a_marker_one_device_compacted_is_dropped_by_the_other_not_uploaded_again() {
    let dir = TempDir::new();
    let (a, b) = (on(&dir, "a"), on(&dir, "b"));
    let api = Arc::new(FakeDeviceApi::new());
    let old = (
        record::element_name("favorite_models", "gone"),
        json!({"id": "gone", "removed": true}),
    );
    api.peer_write(
        ZONE,
        "peer",
        crate::util::unix_now_ms() - TOMBSTONE_HORIZON_MS - 1,
        vec![old.clone()],
    );
    let (_wa, _wb) = (start(&a, &api), start(&b, &api));
    let holds = |core: &Core| {
        core.store
            .account_record(ZONE)
            .unwrap()
            .contains_key(&old.0)
    };
    until(&a, |a| holds(a) && online(a)).await;
    until(&b, |b| holds(b) && online(b)).await;
    // A uploads something, and compacts the marker away on the way.
    set(&a, "branch_name_prefix", json!("m1"));
    until(&b, |b| {
        stored(b, "branch_name_prefix") == json!("m1") && online(b)
    })
    .await;
    assert!(!api.zone(ZONE).unwrap().record.contains_key(&old.0));
    // B dropped its copy, and did not upload it back.
    assert!(!holds(&b));
    assert_eq!(waiting(&b), 0);
    assert_eq!(api.commits(), 1);
}

#[tokio::test]
async fn a_setting_kept_local_because_it_did_not_fit_is_not_overwritten_by_the_older_value() {
    let dir = TempDir::new();
    let (a, b) = (on(&dir, "a"), on(&dir, "b"));
    let api = Arc::new(FakeDeviceApi::new());
    set(&a, "branch_name_instructions", json!("short"));
    let (_wa, _wb) = (start(&a, &api), start(&b, &api));
    until(&a, |a| api.commits() == 1 && online(a)).await;
    until(&b, |b| {
        stored(b, "branch_name_instructions") == json!("short") && online(b)
    })
    .await;
    // A changes it while offline, and meanwhile another device fills the record.
    api.set_online(false);
    let big = json!("y".repeat(7_000));
    set(&a, "branch_name_instructions", big.clone());
    peer(&api, future(), filler(8, 7_900));
    api.set_online(true);
    until(&a, |a| api.commits() == 2 && online(a)).await;
    assert_eq!(
        api.zone(ZONE).unwrap().record["branch_name_instructions"].value,
        json!("short")
    );
    assert_eq!(a.account.status().not_synced, 1);
    // More cycles: the person's value on A stays, and B still follows the account.
    nudge(&a, &api, 3).await;
    assert_eq!(stored(&a, "branch_name_instructions"), big);
    assert_eq!(stored(&b, "branch_name_instructions"), json!("short"));
}

#[tokio::test]
async fn a_local_edit_beats_a_remote_stamp_too_far_ahead_instead_of_being_discarded() {
    // The edit is made after the future field was merged, and before it is.
    for edit_first in [false, true] {
        let dir = TempDir::new();
        let core = on(&dir, "a");
        let api = Arc::new(FakeDeviceApi::new());
        let ahead = crate::util::unix_now_ms() + 10 * 24 * 60 * 60 * 1000;
        peer(
            &api,
            ahead,
            vec![("preview_appearance".into(), json!("dark"))],
        );
        if edit_first {
            set(&core, "preview_appearance", json!("light"));
        }
        let _worker = start(&core, &api);
        if !edit_first {
            until(&core, |core| {
                core.account.status().held_back == 1 && online(core)
            })
            .await;
            assert_ne!(stored(&core, "preview_appearance"), json!("dark"));
            set(&core, "preview_appearance", json!("light"));
        }
        until(&core, |core| api.commits() == 1 && online(core)).await;
        let server = api.zone(ZONE).unwrap().record;
        assert_eq!(
            server["preview_appearance"].value,
            json!("light"),
            "{edit_first}"
        );
        assert_eq!(stored(&core, "preview_appearance"), json!("light"));
        assert_eq!(core.account.status().held_back, 0);
    }
}
