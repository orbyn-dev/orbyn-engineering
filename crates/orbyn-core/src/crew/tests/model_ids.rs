//! Real, sanitized Claude and Codex catalog ids qualify the router's default tier words.
//!
//! Each list was captured from the installed CLI on this machine through the same
//! request the driver sends, so a word that stops matching an observed id, or starts
//! matching one it should not, fails here instead of rejecting an engine on a real
//! install. One account's catalog on one day is not a guarantee for another account or
//! a later CLI.

use orbyn_contracts::{ModelInfo, RouteTier};
use serde_json::Value;

use super::fake::model;
use crate::crew::tiers::{RouterConfig, model_for_tier, tier_of_model};

/// Reads a captured catalog. Every entry carries the fields the driver reads.
fn captured(file: &str, id: &str, label: &str, default: &str) -> Vec<ModelInfo> {
    let catalog: Value = serde_json::from_str(file).unwrap();
    catalog["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            model(
                entry[id].as_str().unwrap(),
                entry[label].as_str().unwrap(),
                entry[default].as_bool().unwrap_or(false),
            )
        })
        .collect()
}

fn claude() -> Vec<ModelInfo> {
    let mut models = captured(
        include_str!("claude-2.1.317-windows-models.json"),
        "value",
        "displayName",
        "isDefault",
    );
    // The CLI marks its own `default` entry, not a flag on the entry: see
    // `parse_claude_model` in crates/orbyn-core/src/agents/driver/claude/catalog.rs.
    for entry in models.iter_mut() {
        entry.is_default = entry.id == "default";
    }
    models
}

fn codex() -> Vec<ModelInfo> {
    captured(
        include_str!("codex-0.160.0-windows-models.json"),
        "id",
        "label",
        "isDefault",
    )
}

#[test]
fn observed_codex_models_match_the_default_tiers() {
    let models = codex();
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
    // The CLI's own default fills the worker tier; the lead and scout picks are the
    // first listed model of their word, the only ones this catalog offers there.
    assert_eq!(
        models
            .iter()
            .find(|model| model.is_default)
            .map(|model| model.id.as_str()),
        Some("gpt-6.1-terra")
    );
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

#[test]
fn observed_claude_models_match_the_default_tiers() {
    let models = claude();
    let config = RouterConfig::default();
    let expected = [
        // The CLI's own default names no tier word: its label and value are `default`.
        ("default", None),
        // The aliases the picker offers first.
        ("opus", Some(RouteTier::Lead)),
        ("sonnet", Some(RouteTier::Worker)),
        // A family with no tier word stays unassigned rather than borrowing one.
        ("claude-fable-5-2", None),
        ("haiku", Some(RouteTier::Scout)),
        ("fable", None),
        // Dated ids and the alias list's later entries carry the same words.
        ("claude-haiku-4-5-20251001", Some(RouteTier::Scout)),
        ("claude-sonnet-5", Some(RouteTier::Worker)),
        ("claude-opus-5", Some(RouteTier::Lead)),
        ("claude-fable-5", None),
        ("claude-opus-4-8", Some(RouteTier::Lead)),
        ("claude-opus-4-7", Some(RouteTier::Lead)),
        ("claude-opus-4-6", Some(RouteTier::Lead)),
        ("claude-sonnet-4-6", Some(RouteTier::Worker)),
    ];
    assert_eq!(models.len(), expected.len());
    for (model, (id, tier)) in models.iter().zip(expected) {
        assert_eq!(model.id, id);
        assert_eq!(tier_of_model(&config, "claude", model), tier, "{id}");
    }
    // Each tier picks the alias the CLI lists first, which tracks the latest model of
    // its family; the earlier dated ids are never chosen over it.
    for (tier, id) in [
        (RouteTier::Lead, "opus"),
        (RouteTier::Worker, "sonnet"),
        (RouteTier::Scout, "haiku"),
    ] {
        assert_eq!(
            model_for_tier(&config, "claude", &models, tier, false)
                .unwrap()
                .id,
            id
        );
    }
}
