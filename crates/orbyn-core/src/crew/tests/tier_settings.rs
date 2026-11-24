//! ACP assignments are explicit, scoped and model-pinned; native words are unchanged.

use orbyn_contracts::{EngineTier, RejectionWhy, RouteTier};
use serde_json::json;

use super::fake::engine;
use super::harness::{Turn, world_tiered};
use super::review_support::arc;
use crate::crew::filters::{Want, screen};
use crate::crew::{AcpTierAssignment, RouterConfig, TierWords};
use crate::error::CoreError;

const NATIVE: &str = "native engines use their model-family tier map";
const UNAVAIL: &str = "an assignment names an unavailable engine";
const SHAPE: &str = "give seen and assignments with engine, tier and expected_default_model";
const CHANGED: &str =
    "the engine's default model changed; review the current default before saving";
const STALE: &str = "ACP tier settings are stale; reload before saving";
const DEFAULT: &str = "the engine must have exactly one visible explicitly marked default model";

/// The screen refused the engine for exactly this reason, not for some other one.
fn refused<T>(result: Result<T, orbyn_contracts::RouteRejection>, why: RejectionWhy) {
    match result {
        Err(rejection) => assert_eq!(rejection.why, why, "{}", rejection.text),
        Ok(_) => panic!("the screen admitted an engine it must refuse for {why:?}"),
    }
}

/// The call refused the assignment with exactly this message.
fn invalid<T>(result: Result<T, CoreError>, message: &str) {
    match result {
        Err(CoreError::Invalid(got)) => assert_eq!(got, message),
        Err(other) => panic!("refused with {other:?}, not {message}"),
        Ok(_) => panic!("accepted what must be refused: {message}"),
    }
}

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
    refused(
        screen(&view, &lead(), None, &config),
        RejectionWhy::TierBelowLead,
    );
    refused(
        screen(&view, &lead(), Some("mini-1"), &config),
        RejectionWhy::TierBelowLead,
    );
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
    refused(
        screen(&view, &lead(), Some("big-1"), &config),
        RejectionWhy::TierBelowLead,
    );
    let mut sibling = view.clone();
    sibling.id = orbyn_contracts::CrewToken::new("opencode-alt").unwrap();
    refused(
        screen(&sibling, &lead(), None, &config),
        RejectionWhy::TierBelowLead,
    );
    let mut drift = view.clone();
    for m in drift.models.as_mut().unwrap() {
        m.is_default = m.id == "big-1";
    }
    refused(
        screen(&drift, &lead(), None, &config),
        RejectionWhy::TierBelowLead,
    );
    refused(
        screen(&drift, &lead(), Some("mini-1"), &config),
        RejectionWhy::TierBelowLead,
    );
    let mut hidden = view.clone();
    hidden.models.as_mut().unwrap().retain(|m| m.id != "mini-1");
    refused(
        screen(&hidden, &lead(), None, &config),
        RejectionWhy::TierBelowLead,
    );
    let mut quota = view.clone();
    quota.used_percent = Some(100);
    refused(
        screen(&quota, &lead(), None, &config),
        RejectionWhy::QuotaExhausted,
    );
    config.acp_tiers.clear();
    let worker = Want {
        tier: RouteTier::Worker,
        ..lead()
    };
    assert!(screen(&view, &worker, None, &config).is_ok());
    refused(
        screen(&view, &lead(), None, &config),
        RejectionWhy::TierBelowLead,
    );
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
    invalid(
        resolve_acp_assignment(&engine("claude"), RouteTier::Lead),
        NATIVE,
    );
    let view = engine("opencode");
    assert_eq!(
        resolve_acp_assignment(&view, RouteTier::Lead)
            .unwrap()
            .model,
        "mini-1"
    );
    let mut off = view.clone();
    off.turned_off = true;
    invalid(resolve_acp_assignment(&off, RouteTier::Lead), UNAVAIL);
    let mut gone = view.clone();
    gone.installed = false;
    invalid(resolve_acp_assignment(&gone, RouteTier::Lead), UNAVAIL);
    let mut out = view.clone();
    out.auth = orbyn_contracts::AuthState::LoggedOut;
    invalid(resolve_acp_assignment(&out, RouteTier::Lead), UNAVAIL);
    let mut hidden = view;
    hidden.models = Some(Vec::new());
    invalid(resolve_acp_assignment(&hidden, RouteTier::Lead), DEFAULT);
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
        invalid(resolve_acp_assignment(&view, RouteTier::Lead), DEFAULT);
        refused(
            screen(&view, &lead(), None, &config),
            RejectionWhy::TierBelowLead,
        );
        refused(
            screen(&view, &lead(), Some("mini-1"), &config),
            RejectionWhy::TierBelowLead,
        );
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
        refused(
            screen(
                &view,
                &Want {
                    tier: RouteTier::Scout,
                    ..lead()
                },
                None,
                &scout,
            ),
            RejectionWhy::MissingCapability,
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
    invalid(
        w.core
            .crew_set_engine_tiers(&json!({"seen":0,"assignments":[]}))
            .await,
        STALE,
    );
    assert_eq!(w.core.crew_engine_tiers().unwrap(), saved);
    for (bad, message) in [
        (
            json!({"seen":1,"assignments":[{"engine":"alpha","tier":"lead","expected_default_model":"alpha-model"},{"engine":"alpha","tier":"scout","expected_default_model":"alpha-model"}]}),
            "give each engine once",
        ),
        (
            json!({"seen":1,"assignments":[{"engine":"missing","tier":"lead","expected_default_model":"missing-model"}]}),
            "an assignment names an unconfigured engine",
        ),
        (
            json!({"seen":1,"assignments":[{"engine":"alpha","tier":"lead","expected_default_model":"alpha-model","model":"guessed-opus"}]}),
            SHAPE,
        ),
        (json!({"seen":1,"assignments":[],"actor":"person"}), SHAPE),
        (
            json!({"seen":1,"assignments":[{"engine":"alpha","tier":"lead"}]}),
            SHAPE,
        ),
        (
            json!({"seen":1,"assignments":[{"engine":"alpha","tier":"lead","expected_default_model":"a-model-the-person-did-not-see"}]}),
            CHANGED,
        ),
    ] {
        invalid(w.core.crew_set_engine_tiers(&bad).await, message);
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
    invalid(w.core.crew_set_engine_tiers(&changed).await, CHANGED);
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
