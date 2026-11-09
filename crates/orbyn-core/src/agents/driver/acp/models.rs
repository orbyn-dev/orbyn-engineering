use orbyn_contracts::{ModelInfo, ModelOption, ModelOptionValue};
use serde_json::{Value, json};

use super::events::field;
use super::{CALL_TIMEOUT, CONFIG_PREFIX, MAX_PROBED_MODELS};
use crate::agents::driver::rpc::RpcClient;
use crate::agents::selection::MODE;
use crate::error::{CoreError, Result};

/// A value the session no longer lists fails the turn. Running under something else
/// could mean running under a looser mode than the one chosen.
pub(super) fn require_offered(session: &Value, option: &str, value: &str) -> Result<()> {
    let offered = parse_session_options(session);
    match offered.iter().find(|known| known.id == option) {
        Some(known) if known.values.iter().any(|choice| choice.id == value) => Ok(()),
        _ => Err(CoreError::UnsupportedOption(format!(
            "This agent no longer offers {value} for {option}."
        ))),
    }
}

/// Sets the model through whichever route this session lists it on. Returns the
/// session state to read later steps from, which a config reply replaces.
pub(super) async fn select_model(
    rpc: &RpcClient,
    id: &str,
    session: &Value,
    model: &str,
) -> Result<Value> {
    if let Some(config) = model_config(session) {
        let config_id = config.get("id").and_then(Value::as_str).unwrap_or_default();
        let params = json!({ "sessionId": id, "configId": config_id, "value": model });
        let reply = rpc
            .call("session/set_config_option", params, CALL_TIMEOUT)
            .await?;
        return Ok(if reply.get("configOptions").is_some() {
            reply
        } else {
            session.clone()
        });
    }
    let listed = parse_models(session);
    if !listed.is_empty() && !listed.iter().any(|known| known.id == model) {
        return Err(CoreError::UnsupportedOption(format!(
            "This agent no longer offers {model}."
        )));
    }
    let params = json!({ "sessionId": id, "modelId": model });
    match rpc.call("session/set_model", params, CALL_TIMEOUT).await {
        Ok(reply) if reply.get("configOptions").is_some() => {
            let mut next = session.clone();
            next["configOptions"] = reply["configOptions"].clone();
            Ok(next)
        }
        Ok(_) => Ok(session.clone()),
        Err(error) => Err(CoreError::EngineUnavailable(format!(
            "This agent did not take the model {model}: {error}"
        ))),
    }
}

fn config_options(session: &Value) -> impl Iterator<Item = &Value> {
    session
        .get("configOptions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

/// The config option that picks the model, for an agent that lists models there.
fn model_config(session: &Value) -> Option<&Value> {
    config_options(session)
        .find(|option| option.get("category").and_then(Value::as_str) == Some("model"))
}

/// A config option's choices. They can be flat or sorted into named groups.
fn config_values(option: &Value) -> Vec<&Value> {
    option
        .get("options")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(
            |entry| match entry.get("options").and_then(Value::as_array) {
                Some(grouped) => grouped.iter().collect(),
                None => vec![entry],
            },
        )
        .collect()
}

/// What a session offers beyond the model: its modes and its other config options.
pub(super) fn parse_session_options(session: &Value) -> Vec<ModelOption> {
    let mut options = Vec::new();
    let current_mode = session
        .pointer("/modes/currentModeId")
        .and_then(Value::as_str);
    let modes: Vec<ModelOptionValue> = session
        .pointer("/modes/availableModes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|mode| {
            let id = mode.get("id")?.as_str()?;
            Some(choice(mode, "name", id, current_mode == Some(id)))
        })
        .collect();
    if !modes.is_empty() {
        options.push(ModelOption {
            id: MODE.to_owned(),
            label: "Mode".to_owned(),
            values: modes,
        });
    }
    for option in config_options(session) {
        let category = option.get("category").and_then(Value::as_str);
        let Some(id) = option.get("id").and_then(Value::as_str) else {
            continue;
        };
        if category == Some("model") || category == Some(MODE) {
            continue;
        }
        let current = option.get("currentValue").and_then(Value::as_str);
        let values: Vec<ModelOptionValue> = config_values(option)
            .into_iter()
            .filter_map(|entry| {
                let value = entry.get("value")?.as_str()?;
                Some(choice(entry, "name", value, current == Some(value)))
            })
            .collect();
        if values.is_empty() {
            continue;
        }
        options.push(ModelOption {
            id: format!("{CONFIG_PREFIX}{id}"),
            label: field(option, "name"),
            values,
        });
    }
    options
}

fn choice(entry: &Value, name_key: &str, id: &str, default: bool) -> ModelOptionValue {
    ModelOptionValue {
        id: id.to_owned(),
        label: entry
            .get(name_key)
            .and_then(Value::as_str)
            .unwrap_or(id)
            .to_owned(),
        default,
        description: entry
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_owned),
    }
}

/// Some agents list what a model offers only once it is chosen, so a probe chooses each
/// in turn, in a session nobody will use, and keeps what differs from the session's own
/// options. One that refuses is left with those.
pub(super) async fn probe_model_options(
    rpc: &RpcClient,
    id: &str,
    session: &Value,
    baseline: &[ModelOption],
    models: &mut [ModelInfo],
) {
    let mut state = session.clone();
    for model in models.iter_mut().take(MAX_PROBED_MODELS) {
        let Ok(reply) = select_model(rpc, id, &state, &model.id).await else {
            continue;
        };
        model.options = parse_session_options(&reply)
            .into_iter()
            .filter(|option| !baseline.contains(option))
            .collect();
        state = reply;
    }
}

/// Removes the session a probe opened, so an agent that keeps sessions does not show
/// it among real work. An agent may offer neither call, and a failure here is only a
/// leftover in a temp folder that is about to go.
pub(super) async fn delete_probe_session(rpc: &RpcClient, init: &Value, id: &str) {
    let offered = |name: &str| {
        init.pointer("/agentCapabilities/sessionCapabilities")
            .and_then(|capabilities| capabilities.get(name))
            .is_some_and(|capability| !capability.is_null())
    };
    let method = if offered("delete") {
        "session/delete"
    } else if offered("close") {
        "session/close"
    } else {
        return;
    };
    let params = json!({ "sessionId": id });
    if let Err(error) = rpc.call(method, params, CALL_TIMEOUT).await {
        tracing::debug!(%error, "could not remove a probe session");
    }
}

/// Reads the models a session reply lists, marking the agent's current one. An
/// agent can list them as a config option instead of a `models` block.
pub(super) fn parse_models(session: &Value) -> Vec<ModelInfo> {
    if let Some(config) = model_config(session) {
        let current = config.get("currentValue").and_then(Value::as_str);
        return config_values(config)
            .into_iter()
            .filter_map(|entry| {
                let id = entry.get("value")?.as_str()?;
                let picked = choice(entry, "name", id, current == Some(id));
                Some(ModelInfo {
                    id: picked.id,
                    label: picked.label,
                    description: picked.description,
                    is_default: picked.default,
                    custom: false,
                    options: Vec::new(),
                    input_modalities: Vec::new(),
                    upgrade: None,
                })
            })
            .collect();
    }
    let current = session
        .pointer("/models/currentModelId")
        .and_then(Value::as_str);
    session
        .pointer("/models/availableModels")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|model| {
            let id = model.get("modelId")?.as_str()?.to_owned();
            Some(ModelInfo {
                label: model
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(&id)
                    .to_owned(),
                description: model
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                is_default: current == Some(id.as_str()),
                custom: false,
                options: Vec::new(),
                input_modalities: Vec::new(),
                upgrade: None,
                id,
            })
        })
        .collect()
}
