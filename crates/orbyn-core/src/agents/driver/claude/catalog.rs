use futures_util::StreamExt;
use orbyn_contracts::{EngineAgent, ModelInfo, ModelOption, ModelOptionValue};
use serde_json::Value;
use tokio::io::AsyncRead;
use tokio_util::codec::{FramedRead, LinesCodec};

use super::events::text_of;
use super::{DOCUMENTED_EFFORTS, MAX_INITIALIZE_BYTES};
use crate::agents::driver::{ModelCatalog, effort_option};
use crate::agents::selection::FAST_MODE;
use crate::error::{CoreError, Result};

pub(super) fn probe_failed() -> CoreError {
    CoreError::EngineUnavailable("Claude Code did not report a usable control response.".into())
}

/// Read only the matching success body. Never return a CLI error payload or log
/// frames: initialization and usage replies may also carry account information.
pub(super) async fn read_control<R: AsyncRead + Unpin>(
    stream: &mut FramedRead<R, LinesCodec>,
    id: &str,
) -> Result<Value> {
    let mut seen = 0;
    while let Some(line) = stream.next().await {
        let line = line.map_err(|_| probe_failed())?;
        seen += line.len() + 1;
        if seen > MAX_INITIALIZE_BYTES {
            return Err(probe_failed());
        }
        let Ok(frame) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if frame.get("type").and_then(Value::as_str) != Some("control_response") {
            continue;
        }
        let Some(reply) = frame.get("response") else {
            continue;
        };
        if reply.get("request_id").and_then(Value::as_str) != Some(id) {
            continue;
        }
        if reply.get("subtype").and_then(Value::as_str) != Some("success") {
            return Err(probe_failed());
        }
        return reply
            .get("response")
            .filter(|body| body.is_object())
            .cloned()
            .ok_or_else(probe_failed);
    }
    Err(probe_failed())
}

/// Turns an `initialize` response into a catalog. Each model's effort list is its
/// own, and a model that reports no effort support gets no effort option.
pub fn parse_capabilities(response: &Value) -> ModelCatalog {
    let list = |key: &str| response.get(key).and_then(Value::as_array);
    let reported = list("models").into_iter().flatten();
    let mut models: Vec<ModelInfo> = reported.filter_map(parse_claude_model).collect();
    if !models.iter().any(|model| model.is_default)
        && let Some(first) = models.first_mut()
    {
        first.is_default = true;
    }
    ModelCatalog {
        models,
        commands: list("commands")
            .into_iter()
            .flatten()
            .filter_map(|command| {
                Some((
                    text_of(command, "name")?,
                    text_of(command, "description").unwrap_or_default(),
                ))
            })
            .collect(),
        agents: list("agents")
            .into_iter()
            .flatten()
            .filter_map(|agent| {
                Some(EngineAgent {
                    name: text_of(agent, "name")?,
                    description: text_of(agent, "description").unwrap_or_default(),
                    model: text_of(agent, "model"),
                })
            })
            .collect(),
        output_styles: list("available_output_styles")
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        output_style: text_of(response, "output_style"),
        ..ModelCatalog::engine(Vec::new())
    }
}

fn parse_claude_model(model: &Value) -> Option<ModelInfo> {
    let id = text_of(model, "value")?;
    let flag = |key: &str| model.get(key).and_then(Value::as_bool) == Some(true);
    let mut options = Vec::new();
    let levels: Vec<&str> = model
        .get("supportedEffortLevels")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if flag("supportsEffort") && !levels.is_empty() {
        options.push(effort_option(levels, None));
    }
    if flag("supportsFastMode") {
        options.push(fast_option());
    }
    Some(ModelInfo {
        label: text_of(model, "displayName").unwrap_or_else(|| id.clone()),
        description: text_of(model, "description"),
        is_default: id == "default",
        custom: false,
        options,
        input_modalities: Vec::new(),
        upgrade: None,
        id,
    })
}

fn fast_option() -> ModelOption {
    let value = |id: &str, label: &str, default: bool| ModelOptionValue {
        id: id.to_owned(),
        label: label.to_owned(),
        default,
        description: None,
    };
    ModelOption {
        id: FAST_MODE.to_owned(),
        label: "Speed".to_owned(),
        values: vec![value("off", "Standard", true), value("on", "Fast", false)],
    }
}

/// The aliases the CLI's docs name, for when it cannot be asked. An alias tracks the
/// vendor's latest, and any full model name also works as a custom model.
pub(super) fn documented_catalog(reason: String) -> ModelCatalog {
    let model = |id: &str, label: &str, description: &str, is_default: bool| ModelInfo {
        id: id.to_owned(),
        label: label.to_owned(),
        description: Some(description.to_owned()),
        is_default,
        custom: false,
        options: vec![effort_option(DOCUMENTED_EFFORTS.iter().copied(), None)],
        input_modalities: Vec::new(),
        upgrade: None,
    };
    ModelCatalog {
        source: "documented",
        fallback: Some(reason),
        models: vec![
            model(
                "default",
                "Default",
                "What Claude Code picks for your plan",
                true,
            ),
            model("sonnet", "Sonnet", "The latest Sonnet", false),
            model("opus", "Opus", "The latest Opus", false),
            model("haiku", "Haiku", "The latest Haiku", false),
            model(
                "sonnet[1m]",
                "Sonnet (1M context)",
                "Sonnet with a 1M context window",
                false,
            ),
            model(
                "opus[1m]",
                "Opus (1M context)",
                "Opus with a 1M context window",
                false,
            ),
            model(
                "opusplan",
                "Opus for planning",
                "Opus in plan mode, Sonnet otherwise",
                false,
            ),
        ],
        ..ModelCatalog::default()
    }
}
