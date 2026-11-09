//! App, library and engine update status.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{IncomingCommit, VersionSupport};

wire! {
pub struct LibraryUpdateStatus {
    pub current: Option<String>,
    pub latest: Option<String>,
    pub behind: u32,
    pub ahead: u32,
    pub dirty: bool,
    pub incoming: Vec<IncomingCommit>,
    pub can_apply: bool,
    pub blocked_reason: Option<String>,
}
}

wire_enum! {
pub enum AppMode {
    Desktop,
    Server,
}
}

wire! {
pub struct AppUpdateStatus {
    pub current: String,
    pub latest: Option<String>,
    pub notes: Option<String>,
    pub mode: AppMode,
    pub can_apply: bool,
    pub blocked_reason: Option<String>,
    pub unconfigured: bool,
    /// Bytes fetched so far while the shell downloads, when it reports them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub downloaded: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub total: Option<u64>,
}
}

wire! {
pub struct EngineUpdateStatus {
    pub id: String,
    pub current: Option<String>,
    pub latest: Option<String>,
    pub support: VersionSupport,
    pub can_apply: bool,
    pub blocked_reason: Option<String>,
}
}

wire! {
pub struct UpdateStatus {
    pub library: LibraryUpdateStatus,
    pub app: AppUpdateStatus,
    pub engines: Vec<EngineUpdateStatus>,
    /// When the last check fully succeeded.
    pub checked_at: Option<String>,
    /// Why the last check fell short. Earlier results stay in place.
    pub error: Option<String>,
}
}

wire! {
pub struct UpdateStep {
    /// `engine:<id>`, `library` or `app`.
    pub target: String,
    pub ok: bool,
    pub message: Option<String>,
}
}

wire! {
pub struct ApplyAllResult {
    /// Steps in the order they ran. A failed step is the last one.
    pub steps: Vec<UpdateStep>,
}
}

wire_enum! {
pub enum ReleaseChannel {
    Stable,
    Nightly,
    Dev,
}
}
