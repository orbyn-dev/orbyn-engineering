//! Real, sanitized Codex catalog ids qualify the router's existing default words.

use orbyn_contracts::RouteTier;
use serde_json::Value;

use super::fake::model;
use crate::crew::tiers::{RouterConfig, model_for_tier, tier_of_model};

#[test]
fn observed_codex_models_match_the_default_tiers() {
    let catalog: Value =
        serde_json::from_str(include_str!("codex-0.160.0-windows-models.json")).unwrap();
    let models: Vec<_> = catalog["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            model(
                entry["id"].as_str().unwrap(),
                entry["label"].as_str().unwrap(),
                false,
            )
        })
        .collect();
    let config = RouterConfig::default();
    let expected = [
        ("gpt-6.1-terra", Some(RouteTier::Worker)),
        ("gpt-6.1-sol", Some(RouteTier::Lead)),
        ("gpt-6-astra", None),
        ("gpt-6-sol", Some(RouteTier::Lead)),
        ("gpt-6-luna", Some(RouteTier::Scout)),
        ("gpt-5.6-sol", Some(RouteTier::Lead)),
        ("gpt-5.6-terra", Some(RouteTier::Worker)),
        ("gpt-5.6-luna", Some(RouteTier::Scout)),
    ];
    assert_eq!(models.len(), expected.len());
    for (model, (id, tier)) in models.iter().zip(expected) {
        assert_eq!(model.id, id);
        assert_eq!(tier_of_model(&config, "codex", model), tier, "{id}");
    }
    for (tier, id) in [
        (RouteTier::Lead, "gpt-6.1-sol"),
        (RouteTier::Worker, "gpt-6.1-terra"),
        (RouteTier::Scout, "gpt-6-luna"),
    ] {
        assert_eq!(
            model_for_tier(&config, "codex", &models, tier, false)
                .unwrap()
                .id,
            id
        );
    }
}
