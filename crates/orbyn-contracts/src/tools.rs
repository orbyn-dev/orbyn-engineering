//! Engine tools and skill packs.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire_enum! {
/// Where a tool or connector stands. Only what the engine reports is shown; nothing here
/// is a switch.
pub enum ToolState {
    Ready,
    Starting,
    NeedsSignIn,
    Failed,
}
}

wire! {
/// One entry in a tools group: a server, connector, plugin, skill or tool.
pub struct ToolEntry {
    pub name: String,
    /// One line, when the engine gives one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    pub state: ToolState,
    /// A short reason in plain words for `failed` and `needs_sign_in`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reason: Option<String>,
    /// The longer message behind `failed`, for the Details view.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detail: Option<String>,
    /// Where the entry is defined, such as `.mcp.json` or `claude.ai connector`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source: Option<String>,
    /// How many tools the entry offers, when the engine says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tools: Option<u32>,
    /// The provider's own sign-in page, when the engine gives one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sign_in_url: Option<String>,
}
}

wire! {
/// A group of entries. `count` is what the group has; `items` may be shorter (the
/// built-in tools and skills are listed by count only).
pub struct ToolGroup {
    /// `builtin`, `orbyn`, `mcp`, `connectors`, `plugins` or `skills`.
    pub id: String,
    pub count: u32,
    pub items: Vec<ToolEntry>,
}
}

wire! {
/// What an engine reports as available, for the thread Tools tab and the Settings page.
pub struct EngineTools {
    pub engine: String,
    /// `run` when everything came from the frame a run reported, `check` when the engine was
    /// asked, `check+run` when the servers were asked and the rest came from a run's frame.
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub checked_at: Option<String>,
    /// Groups the engine reports, in display order. A group it does not report is absent.
    pub groups: Vec<ToolGroup>,
    /// Ids of groups the engine does not report, so the page can say so quietly.
    pub not_reported: Vec<String>,
    /// Set when the engine could not be asked; `groups` then holds the last list, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
    /// False before the first run of a thread whose engine reports on a run.
    pub reported: bool,
    /// True when a list was cut short (a deadline or page cap), so what is shown may not be all.
    #[serde(default)]
    pub incomplete: bool,
}
}

wire! {
pub struct SkillSummary {
    pub name: String,
    pub category: String,
    pub description: String,
    pub enabled: bool,
}
}

wire! {
pub struct SkillPackStatus {
    /// Short library revision, with `-dirty` when it has local edits.
    pub version: String,
    /// Full library revision.
    pub digest: String,
    /// True when the workspace library is valid.
    pub signature_valid: bool,
    pub source: Option<String>,
    pub skills: Vec<SkillSummary>,
}
}

wire! {
pub struct SkillProfile {
    pub id: String,
    pub name: String,
    pub skills: Vec<String>,
}
}
