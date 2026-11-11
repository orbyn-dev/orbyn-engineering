//! What the worker sees: the outcome wanted and the limits it works inside. A brief has
//! no field for the answer key (H1).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::common::{Verdict, count, envelope, text, unique, within};
use super::lease::LeaseMode;
use super::text::{CrewHash, CrewLongText, CrewPath, CrewPathPattern, CrewShortText, CrewToken};

wire_enum! {
pub enum BriefKind {
    Plan,
    Implement,
    Review,
    Test,
    Research,
    Ui,
    Docs,
}
}

wire_enum! {
/// What a brief asks for. It never overrides the never-allowed list (SPEC.md section 7.1).
pub enum BriefEffect {
    ReadRepo,
    WriteLeased,
    RunChecks,
    RunCommands,
    Network,
    InstallDependencies,
}
}

wire! {
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BriefInput {
    Text { text: CrewLongText },
    Path { path: CrewPath },
    Ref {
        #[serde(rename = "ref")]
        reference: CrewHash,
    },
}
}

wire! {
/// One path the brief asks to lease.
#[serde(deny_unknown_fields)]
pub struct FileGrant {
    pub path: CrewPathPattern,
    pub mode: LeaseMode,
}
}

wire! {
#[serde(deny_unknown_fields)]
pub struct BriefBudget {
    /// 1 to 1000.
    pub turns: u32,
    /// 1 to 1440.
    pub minutes: u32,
}
}

wire_enum! {
pub enum BriefAssigner {
    Person,
    Lead,
}
}

wire! {
/// An explicit assignment. It still passes the router's hard filters.
#[serde(deny_unknown_fields)]
pub struct EngineAssignment {
    pub id: CrewToken,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<CrewToken>,
    pub assigned_by: BriefAssigner,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tag: Option<CrewShortText>,
}
}

crew_object! {
pub struct Brief {
    pub kind: BriefKind,
    pub outcome: String,
    pub inputs: Vec<BriefInput>,
    /// The file lease the brief asks for. The core grants it as a `Lease`.
    pub files: Vec<FileGrant>,
    pub allowed_effects: Vec<BriefEffect>,
    pub stop_when: Vec<CrewShortText>,
    pub budget: BriefBudget,
    /// Capabilities the engine must have, for example `image` or `mcp`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub needs: Option<Vec<CrewToken>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub engine: Option<EngineAssignment>,
    /// Objects this brief builds on, such as the findings a fix brief answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub refs: Option<Vec<CrewHash>>,
}
}

impl Brief {
    pub fn validate(&self) -> Verdict {
        envelope(self.v, &self.author)?;
        text("outcome", &self.outcome, 4000)?;
        count("inputs", self.inputs.len(), 0, 64)?;
        count("files", self.files.len(), 0, 256)?;
        unique("allowed_effects", &self.allowed_effects)?;
        count("stop_when", self.stop_when.len(), 1, 16)?;
        within("budget.turns", self.budget.turns.into(), 1, 1000)?;
        within("budget.minutes", self.budget.minutes.into(), 1, 1440)?;
        if let Some(needs) = &self.needs {
            count("needs", needs.len(), 0, 16)?;
            unique("needs", needs)?;
        }
        if let Some(refs) = &self.refs {
            count("refs", refs.len(), 0, 64)?;
        }
        Ok(())
    }

    /// The paths the brief asks to write.
    pub fn write_paths(&self) -> impl Iterator<Item = &CrewPathPattern> {
        self.files
            .iter()
            .filter(|f| f.mode == LeaseMode::Write)
            .map(|f| &f.path)
    }
}
