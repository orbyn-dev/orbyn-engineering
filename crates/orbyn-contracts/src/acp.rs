//! Agent Client Protocol agents and their native sessions.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire! {
/// An agent the ACP registry lists. Nothing here is installed by being listed.
pub struct AcpRegistryEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    /// What this machine can install it through, best first: `binary`, `npx`, `uvx`.
    pub installable: Vec<String>,
    /// The engine it became, once installed.
    pub engine: Option<String>,
}
}

wire! {
pub struct InstallAcpAgent {
    /// `binary`, `npx` or `uvx`. Absent takes the best this machine can use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub distribution: Option<String>,
}
}

wire! {
#[derive(Default)]
/// The exact management methods advertised by this ACP agent, without signing in.
pub struct AcpSessionCapabilities {
    pub can_list: bool,
    pub can_load: bool,
    pub can_resume: bool,
    pub can_delete: bool,
}
}

wire! {
/// Agent-owned conversation metadata. Reported directories are data, not access grants.
pub struct AcpNativeSession {
    pub session_id: String,
    pub cwd: String,
    pub additional_directories: Vec<String>,
    pub title: Option<String>,
    pub updated_at: Option<String>,
    pub imported_thread_id: Option<String>,
}
}

wire! {
pub struct AcpNativeSessionPage {
    pub sessions: Vec<AcpNativeSession>,
    pub next_cursor: Option<String>,
    pub can_load: bool,
    pub can_resume: bool,
    pub can_delete: bool,
}
}

wire! {
#[serde(deny_unknown_fields)]
/// The service resolves the registered project; callers cannot supply a launch directory.
pub struct AcpNativeSessionsQuery {
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub cursor: Option<String>,
}
}

wire! {
#[serde(deny_unknown_fields)]
pub struct ImportAcpNativeSession {
    pub project_id: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub updated_at: Option<String>,
}
}

wire! {
pub struct AcpNativeSessionImported {
    pub thread_id: String,
    pub imported: bool,
}
}

wire! {
#[serde(deny_unknown_fields)]
pub struct DeleteAcpNativeSession {
    pub project_id: String,
    pub session_id: String,
}
}
