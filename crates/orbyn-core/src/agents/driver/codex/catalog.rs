use orbyn_contracts::{ModelInfo, ModelOption, ModelOptionValue};
use serde_json::Value;

use super::text;
use crate::agents::driver::effort_option;
use crate::agents::selection::{SERVICE_TIER, STANDARD_TIER};

/// Reads one `model/list` entry. Hidden models stay out of the picker.
pub(super) fn parse_model(value: &Value) -> Option<ModelInfo> {
    if value.get("hidden").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let id = text(value, "model").or_else(|| text(value, "id"))?;
    let default_effort = text(value, "defaultReasoningEffort");
    let reported = value
        .get("supportedReasoningEfforts")
        .and_then(Value::as_array);
    let mut effort = effort_option(
        reported
            .into_iter()
            .flatten()
            .filter_map(|option| option.get("reasoningEffort")?.as_str()),
        default_effort.as_deref(),
    );
    for (choice, option) in effort.values.iter_mut().zip(reported.into_iter().flatten()) {
        choice.description = text(option, "description").filter(|text| !text.is_empty());
    }
    let mut options = Vec::new();
    if !effort.values.is_empty() {
        options.push(effort);
    }
    options.extend(tier_option(value));
    Some(ModelInfo {
        label: text(value, "displayName").unwrap_or_else(|| id.clone()),
        description: text(value, "description"),
        is_default: value.get("isDefault").and_then(Value::as_bool) == Some(true),
        custom: false,
        options,
        input_modalities: value
            .get("inputModalities")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        upgrade: text(value, "upgrade"),
        id,
    })
}

/// Service tiers, or the older speed tier list from a CLI that has no `serviceTiers`.
/// Standard is always offered, and a model with neither has no such choice at all.
fn tier_option(value: &Value) -> Option<ModelOption> {
    let list = |key: &str| value.get(key).and_then(Value::as_array);
    let mut tiers: Vec<ModelOptionValue> = list("serviceTiers")
        .into_iter()
        .flatten()
        .filter_map(|tier| {
            let id = text(tier, "id")?;
            Some(ModelOptionValue {
                label: text(tier, "name").unwrap_or_else(|| id.clone()),
                default: false,
                description: text(tier, "description").filter(|text| !text.is_empty()),
                id,
            })
        })
        .collect();
    if tiers.is_empty() {
        tiers = list("additionalSpeedTiers")
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(|id| ModelOptionValue {
                id: id.to_owned(),
                label: if id == "fast" { "Fast" } else { id }.to_owned(),
                default: false,
                description: None,
            })
            .collect();
    }
    if tiers.is_empty() {
        return None;
    }
    let default = text(value, "defaultServiceTier")
        .filter(|id| tiers.iter().any(|tier| tier.id == *id))
        .unwrap_or_else(|| STANDARD_TIER.to_owned());
    tiers.insert(
        0,
        ModelOptionValue {
            id: STANDARD_TIER.to_owned(),
            label: "Standard".to_owned(),
            default: false,
            description: None,
        },
    );
    for tier in &mut tiers {
        tier.default = tier.id == default;
    }
    Some(ModelOption {
        id: SERVICE_TIER.to_owned(),
        label: "Speed".to_owned(),
        values: tiers,
    })
}

/// The tier in the person's Codex config becomes the default wherever a model
/// offers it. Config says `fast` for what a catalog can call `priority`.
pub(super) fn apply_configured_tier(models: &mut [ModelInfo], configured: &str) {
    let options = models.iter_mut().flat_map(|model| &mut model.options);
    for option in options.filter(|option| option.id == SERVICE_TIER) {
        let found = option
            .values
            .iter()
            .find(|value| value.id == configured)
            .or_else(|| {
                let alias = (configured == "fast").then_some("priority")?;
                option.values.iter().find(|value| value.id == alias)
            })
            .map(|value| value.id.clone());
        if let Some(found) = found {
            for value in &mut option.values {
                value.default = value.id == found;
            }
        }
    }
}
