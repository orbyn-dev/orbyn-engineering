//! What an install, an update or a removal of an engine did, kept for the provider page.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire_enum! {
pub enum InstallKind {
    Install,
    Update,
    Remove,
}
}

wire! {
/// One attempt that started: an install, an update or a removal. A refusal before
/// anything ran (a thread is running on the engine) is not an attempt and is not kept.
pub struct InstallOutcome {
    pub id: String,
    pub engine: String,
    pub kind: InstallKind,
    pub started_at: String,
    pub finished_at: String,
    pub ok: bool,
    /// Why it failed, in plain words. Absent when it worked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reason: Option<String>,
    /// The command that ran, as shown to a person: no secrets, no query strings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub command: Option<String>,
    /// The installer's exit status. Absent when no installer ran or it was stopped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub exit_status: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub from_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub to_version: Option<String>,
    /// The last lines the installer printed, with anything secret hidden.
    pub output: Vec<String>,
}
}

wire! {
/// The vendor's own install page, for an engine Orbyn does not install itself.
pub struct VendorPage {
    pub url: String,
    /// The address's host, which the card shows beside the link.
    pub domain: String,
}
}

wire! {
/// `GET /api/engines/{id}/installs`: the vendor page when the engine is not installed, and
/// the kept outcomes, newest first (at most 20 per engine).
pub struct EngineInstalls {
    pub engine: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub vendor_page: Option<VendorPage>,
    pub outcomes: Vec<InstallOutcome>,
}
}
