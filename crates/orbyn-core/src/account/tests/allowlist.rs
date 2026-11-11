//! What a peer cannot make this device do: allowlist-only apply, the keybinding and
//! model-option rules and the branch prefix check, through the whole worker.

use super::*;

#[tokio::test]
async fn each_catalog_key_outside_the_allowlist_from_a_peer_is_not_applied_and_is_kept() {
    let dir = TempDir::new();
    let core = on(&dir, "a");
    let api = Arc::new(FakeDeviceApi::new());
    let foreign = not_allowlisted();
    assert_eq!(foreign.len(), 42);
    let mut sent = Vec::new();
    let mut valid = 0;
    for key in &foreign {
        let (value, ok) = foreign_value(&core, key);
        valid += usize::from(ok);
        sent.push(((*key).to_owned(), value));
    }
    assert!(
        valid >= 30,
        "the test should send values that would pass the value check ({valid})"
    );
    sent.push(("default_effort".into(), json!("high")));
    sent.push(("a_key_from_a_newer_app".into(), json!({"kept": true})));
    peer(&api, future(), sent.clone());
    let before = globals(&core);
    let _worker = start(&core, &api);
    until(&core, |core| {
        stored(core, "default_effort") == json!("high") && online(core)
    })
    .await;
    // Only the allowlisted key changed, and nothing was queued by applying.
    let mut expected = before;
    expected.insert("pref:default_effort".into(), "\"high\"".into());
    assert_eq!(globals(&core), expected);
    assert_eq!(waiting(&core), 0);
    // Every other field is kept in the record, byte for byte.
    let kept = core.store.account_record(ZONE).unwrap();
    for (key, value) in sent.iter().filter(|(key, _)| key != "default_effort") {
        assert_eq!(kept[key].value, *value, "{key}");
    }
    // And it is passed on with the next upload.
    set(&core, "default_model", json!("m"));
    until(&core, |core| api.commits() == 1 && online(core)).await;
    let uploaded = &api.pushes()[0].1;
    for (key, value) in sent.iter().filter(|(key, _)| key != "default_effort") {
        assert_eq!(uploaded[key].value, *value, "{key}");
    }
}

#[tokio::test]
async fn a_local_write_of_a_key_outside_the_allowlist_is_never_queued() {
    let dir = TempDir::new();
    let core = on(&dir, "a");
    let api = Arc::new(FakeDeviceApi::new());
    api.set_status(DeviceStatus::SignedOut);
    let _worker = start(&core, &api);
    let mut written = 0;
    for key in not_allowlisted() {
        let (value, _) = foreign_value(&core, key);
        written += usize::from(
            core.set_preferences(None, &BTreeMap::from([(key.to_owned(), value)]))
                .is_ok(),
        );
    }
    assert!(written >= 30, "{written}");
    assert_eq!(waiting(&core), 0);
    assert!(core.store.account_record(ZONE).unwrap().is_empty());
    // An allowlisted key is queued, as the control.
    set(&core, "default_effort", json!("high"));
    assert_eq!(waiting(&core), 1);
}

#[tokio::test]
async fn a_peers_keybinding_to_an_approval_command_or_a_dialog_key_is_kept_never_applied() {
    let dir = TempDir::new();
    let core = on(&dir, "a");
    let api = Arc::new(FakeDeviceApi::new());
    let bad = [
        rule("mod+y", "approval.approve"),
        rule("mod+n", "approval.deny"),
        rule("mod+m", "permissions.setMode"),
        rule("mod+u", "composer.sendAlternate"),
        rule("enter", "sidebar.toggle"),
        rule("escape", "navigation.back"),
    ];
    let good = rule("mod+e", "sidebar.toggle");
    let mut sent: Vec<_> = bad.iter().map(rule_field).collect();
    sent.push(rule_field(&good));
    peer(&api, future(), sent.clone());
    let _worker = start(&core, &api);
    until(&core, |core| {
        core.keybindings().rules.contains(&good) && online(core)
    })
    .await;
    let view = core.keybindings();
    for rule in &bad {
        assert!(!view.rules.contains(rule), "{rule:?}");
    }
    assert!(
        !view
            .rules
            .iter()
            .any(|rule| rule.command.starts_with("approval"))
    );
    let kept = core.store.account_record(ZONE).unwrap();
    for (name, value) in &sent {
        assert_eq!(&kept[name].value, value);
    }
    assert_eq!(waiting(&core), 0);
}

#[tokio::test]
async fn a_default_model_option_that_means_sandbox_approval_or_skip_permissions_is_dropped_for_each_adapter()
 {
    let cases = [
        (
            "claude",
            json!({"fast_mode": "on", "dangerously_skip_permissions": "true", "permission_mode": "bypassPermissions"}),
            json!({"fast_mode": "on"}),
        ),
        (
            "codex",
            json!({"service_tier": "fast", "sandbox": "danger-full-access", "approval_policy": "never"}),
            json!({"service_tier": "fast"}),
        ),
        (
            "an-acp-agent",
            json!({"mode": "bypass", "config:approval": "never", "fast_mode": "on"}),
            json!({}),
        ),
    ];
    for (engine, sent, kept) in cases {
        let dir = TempDir::new();
        let core = on(&dir, "a");
        let api = Arc::new(FakeDeviceApi::new());
        peer(
            &api,
            future(),
            vec![
                ("default_engine".into(), json!(engine)),
                ("default_model_options".into(), sent),
            ],
        );
        let _worker = start(&core, &api);
        until(&core, |core| {
            stored(core, "default_engine") == json!(engine) && online(core)
        })
        .await;
        assert_eq!(stored(&core, "default_model_options"), kept, "{engine}");
    }
}

#[tokio::test]
async fn a_branch_prefix_that_is_not_a_git_ref_part_is_not_applied() {
    let dir = TempDir::new();
    let core = on(&dir, "a");
    let api = Arc::new(FakeDeviceApi::new());
    peer(
        &api,
        future(),
        vec![
            ("branch_name_prefix".into(), json!("--upload-pack=x")),
            ("default_effort".into(), json!("low")),
        ],
    );
    let _worker = start(&core, &api);
    until(&core, |core| {
        stored(core, "default_effort") == json!("low") && online(core)
    })
    .await;
    assert_eq!(stored(&core, "branch_name_prefix"), json!("feat"));
    peer(
        &api,
        future() + 1,
        vec![("branch_name_prefix".into(), json!("team/feat"))],
    );
    until(&core, |core| {
        stored(core, "branch_name_prefix") == json!("team/feat")
    })
    .await;
}
