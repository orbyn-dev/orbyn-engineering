//! ACP assignments are explicit, scoped and model-pinned; native words are unchanged.

use orbyn_contracts::{EngineTier, RouteTier};
use serde_json::json;

use super::fake::engine;
use super::harness::{Turn, world_tiered};
use super::review_support::arc;
use crate::crew::filters::{Want, screen};
use crate::crew::{AcpTierAssignment, RouterConfig, TierWords};

fn lead() -> Want {
    Want {
        writes: false,
        needs: Vec::new(),
        image: false,
        tier: RouteTier::Lead,
        reviewer: false,
        crew_worker: false,
    }
}

fn project(w: &super::harness::World) -> String {
    w.core.store.thread(&w.lead).unwrap().project_id
}

#[test]
fn acp_tiers_never_come_from_names_or_promote_sibling_engines_or_models() {
    let view = engine("opencode");
    let mut config = RouterConfig::default();
    // A legacy family word is not an explicit ACP assignment.
    config.tiers.insert(
        "opencode".into(),
        TierWords {
            lead: Some("mini".into()),
            ..TierWords::default()
        },
    );
    assert!(screen(&view, &lead(), None, &config).is_err());
    assert!(screen(&view, &lead(), Some("mini-1"), &config).is_err());
    config.acp_tiers.insert(
        "opencode".into(),
        AcpTierAssignment {
            tier: RouteTier::Lead,
            model: "mini-1".into(),
        },
    );
    let admitted = screen(&view, &lead(), None, &config).unwrap();
    assert_eq!(admitted.model, "mini-1");
    assert_eq!(admitted.tier, RouteTier::Lead);
    assert!(screen(&view, &lead(), Some("big-1"), &config).is_err());
    let mut sibling = view.clone();
    sibling.id = orbyn_contracts::CrewToken::new("opencode-alt").unwrap();
    assert!(screen(&sibling, &lead(), None, &config).is_err());
    let mut drift = view.clone();
    for m in drift.models.as_mut().unwrap() {
        m.is_default = m.id == "big-1";
    }
    assert!(screen(&drift, &lead(), None, &config).is_err());
    assert!(screen(&drift, &lead(), Some("mini-1"), &config).is_err());
    let mut hidden = view.clone();
    hidden.models.as_mut().unwrap().retain(|m| m.id != "mini-1");
    assert!(screen(&hidden, &lead(), None, &config).is_err());
    let mut quota = view.clone();
    quota.used_percent = Some(100);
    assert!(screen(&quota, &lead(), None, &config).is_err());
    config.acp_tiers.clear();
    let worker = Want {
        tier: RouteTier::Worker,
        ..lead()
    };
    assert!(screen(&view, &worker, None, &config).is_ok());
    assert!(screen(&view, &lead(), None, &config).is_err());
    let native = engine("claude");
    assert_eq!(native.tier, EngineTier::Native);
    assert_eq!(
        screen(&native, &lead(), None, &config).unwrap().model,
        "claude-opus-4"
    );
}

#[test]
fn settings_refuses_native_unavailable_and_unlisted_model_assignments() {
    use crate::crew::tier_settings::resolve_acp_assignment;
    assert!(resolve_acp_assignment(&engine("claude"), RouteTier::Lead).is_err());
    let view = engine("opencode");
    assert_eq!(
        resolve_acp_assignment(&view, RouteTier::Lead)
            .unwrap()
            .model,
        "mini-1"
    );
    let mut off = view.clone();
    off.turned_off = true;
    assert!(resolve_acp_assignment(&off, RouteTier::Lead).is_err());
    let mut gone = view.clone();
    gone.installed = false;
    assert!(resolve_acp_assignment(&gone, RouteTier::Lead).is_err());
    let mut out = view.clone();
    out.auth = orbyn_contracts::AuthState::LoggedOut;
    assert!(resolve_acp_assignment(&out, RouteTier::Lead).is_err());
    let mut hidden = view;
    hidden.models = Some(Vec::new());
    assert!(resolve_acp_assignment(&hidden, RouteTier::Lead).is_err());
}

#[test]
fn missing_or_ambiguous_defaults_never_gain_an_acp_tier() {
    use crate::crew::tier_settings::resolve_acp_assignment;
    let original = engine("opencode");
    let mut config = RouterConfig::default();
    config.acp_tiers.insert(
        "opencode".into(),
        AcpTierAssignment {
            tier: RouteTier::Lead,
            model: "mini-1".into(),
        },
    );
    for marked in [false, true] {
        let mut view = original.clone();
        for model in view.models.as_mut().unwrap() {
            model.is_default = marked;
        }
        assert!(resolve_acp_assignment(&view, RouteTier::Lead).is_err());
        assert!(screen(&view, &lead(), None, &config).is_err());
        assert!(screen(&view, &lead(), Some("mini-1"), &config).is_err());
        assert!(
            screen(
                &view,
                &Want {
                    tier: RouteTier::Worker,
                    ..lead()
                },
                None,
                &config
            )
            .is_ok(),
            "existing builder baseline is retained"
        );
        let mut scout = config.clone();
        scout.acp_tiers.get_mut("opencode").unwrap().tier = RouteTier::Scout;
        assert!(
            screen(
                &view,
                &Want {
                    tier: RouteTier::Scout,
                    ..lead()
                },
                None,
                &scout
            )
            .is_err()
        );
    }
}

#[tokio::test]
async fn person_tier_save_is_cas_rejects_duplicates_and_clears_to_builder_default() {
    let w = world_tiered(true, arc(|_, _| Turn::Hang), &[("alpha", false)], true).await;
    let initial = w.core.crew_engine_tiers().unwrap();
    assert_eq!(initial["revision"], 0);
    let discovery = w.core.crew_engine_tier_view().await.unwrap();
    assert_eq!(serde_json::to_value(&discovery.settings).unwrap(), initial);
    let alpha = discovery.engines.iter().find(|e| e.id == "alpha").unwrap();
    assert!(!alpha.native && alpha.available);
    assert_eq!(alpha.defaults, vec!["alpha-model"]);
    assert_eq!(
        w.core.crew_engine_tiers().unwrap(),
        initial,
        "discovery cannot persist a mapping"
    );
    assert!(
        !serde_json::to_string(&discovery)
            .unwrap()
            .contains("/bin/sh")
    );
    let saved = w.core.crew_set_engine_tiers(&json!({"seen":0,"assignments":[{"engine":"alpha","tier":"scout","expected_default_model":"alpha-model"}]})).await.unwrap();
    assert_eq!(saved["revision"], 1);
    assert_eq!(saved["assignments"]["alpha"]["tier"], "scout");
    assert_eq!(saved["assignments"]["alpha"]["model"], "alpha-model");
    assert!(
        w.core
            .crew_set_engine_tiers(&json!({"seen":0,"assignments":[]}))
            .await
            .is_err()
    );
    assert_eq!(w.core.crew_engine_tiers().unwrap(), saved);
    for bad in [
        json!({"seen":1,"assignments":[{"engine":"alpha","tier":"lead","expected_default_model":"alpha-model"},{"engine":"alpha","tier":"scout","expected_default_model":"alpha-model"}]}),
        json!({"seen":1,"assignments":[{"engine":"missing","tier":"lead","expected_default_model":"missing-model"}]}),
        json!({"seen":1,"assignments":[{"engine":"alpha","tier":"lead","expected_default_model":"alpha-model","model":"guessed-opus"}]}),
        json!({"seen":1,"assignments":[],"actor":"person"}),
        json!({"seen":1,"assignments":[{"engine":"alpha","tier":"lead"}]}),
        json!({"seen":1,"assignments":[{"engine":"alpha","tier":"lead","expected_default_model":"a-model-the-person-did-not-see"}]}),
    ] {
        assert!(w.core.crew_set_engine_tiers(&bad).await.is_err());
        assert_eq!(w.core.crew_engine_tiers().unwrap(), saved);
    }
    let cleared = w
        .core
        .crew_set_engine_tiers(&json!({"seen":1,"assignments":[]}))
        .await
        .unwrap();
    assert_eq!(cleared["revision"], 2);
    assert_eq!(cleared["assignments"], json!({}));
    assert!(
        w.core
            .crew_router_config_with_mappings(&project(&w))
            .unwrap()
            .acp_tiers
            .is_empty()
    );
    let (_, error) = w
        .lead("crew_set_engine_tiers", json!({"seen":2,"assignments":[]}))
        .await;
    assert!(error, "no model tool writes a person tier assignment");
}

#[tokio::test]
async fn one_changed_expectation_rejects_the_whole_map_without_repinning_other_rows() {
    let w = world_tiered(
        true,
        arc(|_, _| Turn::Hang),
        &[("alpha", false), ("beta", false)],
        true,
    )
    .await;
    let saved = w
        .core
        .crew_set_engine_tiers(&json!({"seen":0,"assignments":[
            {"engine":"alpha","tier":"lead","expected_default_model":"alpha-model"},
            {"engine":"beta","tier":"scout","expected_default_model":"beta-model"}
        ]}))
        .await
        .unwrap();
    let changed = json!({"seen":1,"assignments":[
        {"engine":"alpha","tier":"scout","expected_default_model":"alpha-model"},
        {"engine":"beta","tier":"scout","expected_default_model":"older-reviewed-beta-model"}
    ]});
    assert!(w.core.crew_set_engine_tiers(&changed).await.is_err());
    assert_eq!(
        w.core.crew_engine_tiers().unwrap(),
        saved,
        "a default expectation mismatch cannot change pins, tiers or map revision"
    );
}

#[tokio::test]
async fn an_initial_clear_overrides_config_seed_and_persists_a_revision() {
    let w = world_tiered(true, arc(|_, _| Turn::Hang), &[("alpha", false)], true).await;
    assert!(
        !w.core
            .crew_router_config_with_mappings(&project(&w))
            .unwrap()
            .acp_tiers
            .is_empty()
    );
    let cleared = w
        .core
        .crew_set_engine_tiers(&json!({"seen":0,"assignments":[]}))
        .await
        .unwrap();
    assert_eq!(cleared["revision"], 1);
    assert!(
        w.core
            .crew_router_config_with_mappings(&project(&w))
            .unwrap()
            .acp_tiers
            .is_empty()
    );
}
