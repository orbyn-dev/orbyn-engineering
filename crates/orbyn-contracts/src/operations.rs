//! Diagnostics, storage, cleanup and service information.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::{EnvironmentTheme, ReleaseChannel};

wire! {
pub struct DatabaseStorage {
    pub bytes: u64,
    pub reclaimable_bytes: u64,
    /// Reclaimable space has reached 1 GiB.
    pub repair_suggested: bool,
}
}

wire! {
pub struct Diagnostics {
    pub version: String,
    pub pid: u32,
    pub active_runs: usize,
    pub terminals: usize,
    pub database: DatabaseStorage,
    /// This service and everything it started, by process id.
    pub processes: Vec<ServiceProcess>,
    /// Oldest first, one sample a minute, in memory.
    pub history: Vec<ResourceSample>,
    /// Warnings and errors since the service started, newest first.
    pub events: Vec<ServiceEvent>,
}
}

wire! {
pub struct ServiceProcess {
    pub pid: u32,
    pub parent: Option<u32>,
    pub name: String,
    #[ts(type = "number")]
    pub memory_bytes: u64,
    #[ts(type = "number")]
    pub running_seconds: u64,
}
}

wire! {
pub struct ResourceSample {
    pub at: String,
    #[ts(type = "number")]
    pub memory_bytes: u64,
    pub processes: usize,
}
}

wire! {
pub struct ServiceEvent {
    pub at: String,
    pub level: String,
    pub target: String,
    pub message: String,
}
}

wire! {
pub struct OtlpCheck {
    pub url: String,
}
}

wire! {
pub struct OtlpResult {
    pub ok: bool,
    pub status: Option<u16>,
    pub message: Option<String>,
}
}

wire! {
pub struct CleanupEntry {
    /// `worktree`, `artifact` or `log`.
    pub kind: String,
    pub path: String,
    pub removed: bool,
    pub reason: String,
    /// A run now would remove it. False for one the rules name but cannot touch, such as
    /// a worktree with uncommitted work or another live thread in it.
    #[serde(default)]
    #[ts(as = "Option<bool>", optional)]
    pub eligible: bool,
    /// Its size, measured only by a preview.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub bytes: Option<u64>,
}
}

wire! {
pub struct CleanupGroup {
    /// `worktree`, `artifact` or `log`.
    pub kind: String,
    /// What a run now would remove.
    pub count: u64,
    pub bytes: u64,
    /// Named by the rules but kept: uncommitted work, or in use.
    pub held: u64,
}
}

wire! {
/// What Delete now, or Delete settled worktrees, would remove if run this moment.
/// Nothing is changed to produce it.
pub struct CleanupPreview {
    pub groups: Vec<CleanupGroup>,
    pub count: u64,
    pub bytes: u64,
    /// A worktree was too large to measure in full, so `bytes` is a floor.
    pub partial: bool,
    pub entries: Vec<CleanupEntry>,
}
}

wire! {
pub struct CleanupReport {
    pub dry_run: bool,
    pub ran_at: String,
    pub entries: Vec<CleanupEntry>,
}
}

wire! {
pub struct ApiError {
    pub code: String,
    pub message: String,
    /// Structured context; `already_decided` carries the existing `Approval`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detail: Option<Value>,
}
}

wire! {
/// The opening message for a triage thread, built by the running service.
pub struct TriagePlaybook {
    pub prompt: String,
    /// The source tree for this build, for reading, never fetched by Orbyn.
    pub source_url: String,
    /// False when the build named no revision and the link is the main branch.
    pub source_exact: bool,
}
}

wire! {
pub struct ServerInfo {
    pub version: String,
    pub remote: bool,
    pub platform: String,
    pub environment_label: String,
    /// The theme this environment publishes, applied once by each viewer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub environment_theme: Option<EnvironmentTheme>,
    /// `dev` for a debug build, `nightly` for a build made with
    /// `ORBYN_CHANNEL=nightly`, otherwise `stable`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub channel: Option<ReleaseChannel>,
    /// `environment_icon`: `laptop`, `desktop`, `tower` or `server`; absent is the plain monitor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub environment_icon: Option<String>,
    /// Set when this service start moved an existing database to a newer schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub database_upgrade: Option<DatabaseUpgrade>,
    /// `true` only when this service runs with the Crew Preview flag on. Absent means off. It is read-only here: the
    /// flag is set by the service's own start (`ORBYN_CREW_PREVIEW`), never by a request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub crew_preview: Option<bool>,
}
}

wire! {
/// The schema versions a service start moved the database between.
pub struct DatabaseUpgrade {
    #[ts(type = "number")]
    pub from: i64,
    #[ts(type = "number")]
    pub to: i64,
}
}
