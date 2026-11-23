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
/// The SDK a family's engine host runs, as its `hello` names it.
pub struct EngineHostSdk {
    pub name: String,
    pub version: String,
}
}

wire! {
/// The automatic restart a family's host waits for.
pub struct EngineHostRestart {
    /// The restart being waited for, counting from one inside the window.
    pub attempt: u32,
    /// The most restarts allowed inside the window.
    pub limit: u32,
    /// Whole seconds until the next try; absent when there is no deadline to report.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub next_try_seconds: Option<u32>,
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
    /// RFC 3339, when the live host started running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub since: Option<String>,
    /// The SDK package and version the live host's `hello` named.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sdk: Option<EngineHostSdk>,
    /// The requests the live host's `hello` says it implements.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub capabilities: Option<Vec<String>>,
    /// One host process serves every thread of the family.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(optional, as = "Option<bool>")]
    pub pooled: bool,
    /// Present only while the family waits to restart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub restart: Option<EngineHostRestart>,
    /// How many times the host stopped inside the restart window, for a family that gave up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub stops_in_window: Option<u32>,
    /// A Rust driver wraps the host for this family, so the person can run it by choosing the CLI path. Nothing
    /// switches to it on its own: a down host under the SDK path still fails the run.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(optional, as = "Option<bool>")]
    pub has_driver: bool,
    /// This build carries no SDK for the family; the host refused a run saying so.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(optional, as = "Option<bool>")]
    pub sdk_missing: bool,
}
}
