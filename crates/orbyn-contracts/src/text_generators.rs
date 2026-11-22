//! One-shot text generation: the models each engine offers and the model a
//! thread's title or source-control text resolves to.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ModelInfo;

wire_enum! {
/// Why a one-shot text call uses the model it does.
pub enum TextModelReason {
    /// The per-engine choice in `text_generation_models`.
    Chosen,
    /// The legacy single `text_generation_model` setting.
    Legacy,
    /// The engine's own small model.
    Small,
    /// The thread's model.
    Thread,
}
}

wire! {
/// One engine's one-shot text model choice.
pub struct TextGeneratorEngine {
    pub engine: String,
    pub name: String,
    /// What the engine catalog returned, capped at 200 for this read.
    pub models: Vec<ModelInfo>,
    /// The model `text_generation_models` names for this engine in the global
    /// scope, or null.
    pub chosen: Option<String>,
    /// The model a one-shot call resolves to in the global scope, or null. This
    /// read has no thread, so `thread` never resolves here.
    pub resolved: Option<String>,
    /// Why `resolved` was picked, or null when nothing resolves.
    pub reason: Option<TextModelReason>,
    /// The project's own choice, when `?project=` named one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub project: Option<TextGeneratorProject>,
}
}

wire! {
/// One project's effective one-shot text model choice for an engine.
pub struct TextGeneratorProject {
    pub project_id: String,
    /// The model the project's effective map names for this engine, or null.
    pub chosen: Option<String>,
    /// The model a one-shot call resolves to in the project, or null.
    pub resolved: Option<String>,
    /// Why `resolved` was picked, or null when nothing resolves.
    pub reason: Option<TextModelReason>,
}
}

wire! {
/// Every known, enabled engine and its text generator models.
pub struct TextGenerators {
    pub engines: Vec<TextGeneratorEngine>,
}
}
