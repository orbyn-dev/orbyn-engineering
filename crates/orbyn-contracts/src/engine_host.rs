//! The engine host's status: one entry per family, read from `GET /api/engine-host`.
//! It carries no environment, no arguments, no key and no path.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire_enum! {
/// Where a family's engine host is.
pub enum EngineHostState {
    /// Never started in this app run; it starts on the first run.
    NotStarted,
    Starting,
    Running,
    /// Stopped for being idle; it starts again on the next run.
    Stopped,
    /// Crashed, and the person asked again; it starts on the next run.
    Crashed,
    /// Crashed; the app is starting it again after a wait.
    Restarting,
    /// Failed repeatedly; no restart until the person asks.
    CantStart,
    /// The host speaks another protocol than this app. Final until Orbyn is updated.
    VersionMismatch,
    /// The family runs on its Rust driver.
    NotUsed,
}
}

wire_enum! {
/// Why a family does not use the host.
pub enum EngineHostNotUsed {
    /// The family's flag is off.
    FlagOff,
    /// The host does not run on this machine.
    UnsupportedPlatform,
}
}

wire_enum! {
/// How the last host failed.
pub enum EngineHostErrorKind {
    /// The process exited.
    Exited,
    /// It closed its output without exiting.
    ClosedOutput,
    /// It stopped answering pings.
    Stalled,
    /// Too many malformed frames.
    Malformed,
    /// A frame over 8 MiB.
    Oversize,
    /// No hello or ready in time.
    HelloTimeout,
    /// It did not start.
    StartFailed,
}
}

wire_enum! {
/// What a family's host last failed a run with, from the closed set of the protocol's error kinds, so a
/// screen can word it ("Sign in again") without reading a message. Separate from how the host process failed.
pub enum EngineHostRunError {
    Auth,
    RateLimit,
    Network,
    InvalidRequest,
    Unsupported,
    Cancelled,
    Engine,
    Host,
    /// The engine no longer holds the conversation.
    SessionLost,
}
}

wire_enum! {
/// Where one thread's work on a host is.
pub enum EngineHostThreadState {
    Starting,
    Running,
    Restarting,
    Unavailable,
}
}

wire! {
/// A thread the host serves.
pub struct EngineHostThread {
    pub thread_id: String,
    pub family: String,
    pub state: EngineHostThreadState,
}
}

wire! {
/// The body of the person's stop or restart of a family's host (`POST /api/engine-host/{family}/stop` and
/// `/restart`). A host that still has a background task open (a shell, a subagent, a workflow) is left
/// running unless `stop_background` is true, because stopping it ends that work.
pub struct EngineHostControl {
    #[serde(default)]
    #[ts(optional, as = "Option<bool>")]
    pub stop_background: bool,
}
}

wire! {
/// The last time a family's host failed.
pub struct EngineHostCrash {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub exit_code: Option<i32>,
    /// The signal that ended it, on Unix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub signal: Option<i32>,
    /// RFC 3339.
    pub at: String,
    pub error_kind: EngineHostErrorKind,
    pub message: String,
    /// The host's last words on stderr, credential-looking lines dropped, at most 1024 characters.
    pub stderr_tail: String,
}
}

wire! {
/// One family's engine host.
pub struct EngineHostStatus {
    pub family: String,
    pub state: EngineHostState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub host_version: Option<String>,
    /// Restarts after a crash since the app started.
    pub restart_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_crash: Option<EngineHostCrash>,
    /// For `version_mismatch`: the protocol this app speaks and the host's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub protocol_app: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub protocol_host: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub not_used: Option<EngineHostNotUsed>,
    /// A plain sentence for a state that stops runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detail: Option<String>,
    /// What the person can do, when there is one thing (`Update Orbyn`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub action: Option<String>,
    /// The threads with a run open on the host now, so a screen can say which are working.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub threads: Option<Vec<EngineHostThread>>,
    /// What the family's latest finished run failed with; absent after a run that completed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_error_kind: Option<EngineHostRunError>,
}
}
