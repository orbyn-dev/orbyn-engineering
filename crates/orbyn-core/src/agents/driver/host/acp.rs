//! The ACP family's side of the host mapping: the agent an `open` names, the catalog a
//! `models` reply carries, and the one place an `open` failure becomes a `CoreError`.

use std::path::Path;

use orbyn_contracts::{CustomEngine, ModelInfo, ModelOption, ModelOptionValue};
use serde_json::{Value, json};

use super::frames::ErrorKind;
use crate::agents::driver::ModelCatalog;
use crate::agents::driver::acp::resolve_agent;
use crate::agents::engines::known::KnownEngine;
use crate::error::{CoreError, Result};
use crate::host::process::ResolvedCommand;

/// An ACP engine as the host's `open.options.agent`: the core resolves the program and
/// owns the arguments and environment; the host only starts it.
#[derive(Clone)]
pub struct HostAcpAgent {
    /// The entry in the host's `acpAgents.mjs`, or `default` for an unknown engine.
    pub id: String,
    /// The program the core resolved, before its arguments.
    pub command: String,
    /// The engine's own arguments. Kiro gets none, so the host adds its launch arguments.
    pub args: Vec<String>,
    /// Environment set for this engine alone, never a credential.
    pub env: Vec<(String, String)>,
    /// The known engine this was made from, which names a program that must not run.
    pub known: Option<&'static KnownEngine>,
}

impl HostAcpAgent {
    pub fn new(
        id: impl Into<String>,
        command: impl Into<String>,
        args: Vec<String>,
        env: Vec<(String, String)>,
        known: Option<&'static KnownEngine>,
    ) -> Self {
        Self {
            id: id.into(),
            command: command.into(),
            args,
            env,
            known,
        }
    }

    /// The agent for a native ACP engine. Unknown engines take the host's `default`
    /// entry; Kiro's launch arguments stay in the host, so the core sends none.
    pub fn for_engine(engine: &CustomEngine, known: Option<&'static KnownEngine>) -> Self {
        let id = known.map_or("default", |known| known.id);
        let args = if known.is_some_and(|known| known.id == "kiro") {
            Vec::new()
        } else {
            engine.args.clone()
        };
        let env: Vec<(String, String)> = known.map_or_else(Vec::new, |known| {
            known
                .env
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect()
        });
        Self {
            id: id.to_owned(),
            command: engine.target.clone(),
            args,
            env,
            known,
        }
    }

    /// The program to start, resolved exactly as the Rust driver resolves it.
    pub fn resolved(&self) -> Result<ResolvedCommand> {
        resolve_agent(&self.command, self.known)
    }

    /// The `agent` object of an `open` or `models` request: the resolved binary, the
    /// arguments, the environment and the host's agent id.
    pub fn json(&self) -> Result<Value> {
        let command = self.resolved()?;
        let env: serde_json::Map<String, Value> = self
            .env
            .iter()
            .map(|(name, value)| (name.clone(), json!(value)))
            .collect();
        Ok(json!({
            "command": command.program,
            "args": self.args,
            "env": env,
            "id": self.id,
        }))
    }

    /// The fields of a host-wide `models` probe: the agent and a throwaway folder.
    pub fn probe_fields(&self, cwd: &Path) -> Result<Value> {
        Ok(json!({ "agent": self.json()?, "cwd": cwd }))
    }
}

/// The sentence a person reads when the agent needs a sign-in. It is the core's own
/// wording, never what the agent or its stderr said.
pub fn auth_sentence() -> String {
    crate::agents::driver::diagnostics::failure_message("authentication required", None)
}

/// What a failed `open` becomes for the person.
///
/// This is the one place an ACP open failure is translated. The engine id is here for a
/// later status hook (a start issue or a signed-out engine); today it changes nothing.
pub(crate) fn open_failed(engine_id: &str, kind: ErrorKind, message: Option<&str>) -> CoreError {
    let _ = engine_id;
    match kind {
        ErrorKind::SessionLost => CoreError::SessionLost,
        ErrorKind::Auth => CoreError::EngineUnavailable(auth_sentence()),
        ErrorKind::Unsupported => CoreError::EngineUnavailable(
            message
                .unwrap_or("The engine host cannot start this agent.")
                .to_owned(),
        ),
        _ => CoreError::EngineUnavailable(
            message
                .unwrap_or("The engine host refused to start this agent.")
                .to_owned(),
        ),
    }
}

/// A host `models` reply as the core's catalog: every model keeps the efforts it
/// reported, under the generic `effort` option, with the engine's default marked.
pub fn catalog(value: &Value) -> ModelCatalog {
    let models = value
        .get("models")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|model| {
            let id = model.get("id").and_then(Value::as_str)?.to_owned();
            let default_effort = model.get("defaultEffort").and_then(Value::as_str);
            let efforts = model
                .get("efforts")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|effort| {
                    let id = effort.get("id").and_then(Value::as_str)?.to_owned();
                    let label = effort
                        .get("label")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .unwrap_or_else(|| id.clone());
                    let default = effort.get("isDefault").and_then(Value::as_bool) == Some(true)
                        || default_effort == Some(id.as_str());
                    Some(ModelOptionValue {
                        id,
                        label,
                        default,
                        description: effort
                            .get("description")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                    })
                })
                .collect::<Vec<_>>();
            let options = if efforts.is_empty() {
                Vec::new()
            } else {
                vec![ModelOption {
                    id: crate::agents::selection::EFFORT.to_owned(),
                    label: "Effort".to_owned(),
                    values: efforts,
                }]
            };
            Some(ModelInfo {
                label: model
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| id.clone()),
                description: model
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                is_default: model.get("isDefault").and_then(Value::as_bool) == Some(true),
                custom: false,
                options,
                input_modalities: Vec::new(),
                upgrade: None,
                id,
            })
        })
        .collect();
    ModelCatalog::engine(models)
}
