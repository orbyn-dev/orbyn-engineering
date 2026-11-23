//! Code Mode run records and the per-engine switch (docs/code-mode.md, "Run record"). A run's script, logs,
//! result and call previews come from a model or a tool and are untrusted text.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

wire_enum! {
pub enum CodeRunStatus {
    Running,
    Paused,
    Completed,
    Error,
}
}

wire! {
/// Why a run ended badly. `kind` is `syntax`, `runtime`, `limit`, `denied`, `lost` or `cancelled`.
pub struct CodeRunError {
    pub kind: String,
    pub message: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub line: Option<u32>,
}
}

wire! {
/// One inner call. `status` is `running`, `ok`, `failed` or `denied`; `preview` is the first 4 KiB of the result.
pub struct CodeRunCall {
    pub index: u32,
    pub tool: String,
    pub status: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub preview: Option<String>,
    /// The size of the whole result, when the preview is only a part of it.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub result_bytes: Option<u64>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub duration_ms: Option<u64>,
}
}

wire! {
/// A call in flight.
pub struct CodeRunPending {
    pub tool: String,
    pub call: u32,
}
}

wire! {
/// `GET /api/code-runs/{id}`: one run. Kept 7 days.
pub struct CodeRunRecord {
    pub run_id: String,
    pub status: CodeRunStatus,
    /// The script, at most 64 KiB. Untrusted.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub script: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub script_sha256: Option<String>,
    pub started_at: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub ended_at: Option<String>,
    pub calls: Vec<CodeRunCall>,
    pub logs: Vec<String>,
    /// How many lines the script printed in all, when the record kept fewer.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub logs_total: Option<u64>,
    /// The return value; a result over 64 KiB is `{ truncated: true, bytes, preview }`.
    #[serde(default)]
    #[ts(optional = nullable, type = "unknown")]
    pub result: Option<Value>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub error: Option<CodeRunError>,
    pub pending: Vec<CodeRunPending>,
    pub truncated: bool,
    /// How many times a call picked the run up again after it paused or was lost.
    pub resumes: u32,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub waited_ms: Option<u64>,
    /// The run belongs to a Crew worker's thread.
    pub crew_worker: bool,
}
}

wire! {
/// One row of a thread's runs, newest first.
pub struct CodeRunSummary {
    pub run_id: String,
    pub status: CodeRunStatus,
    pub started_at: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub ended_at: Option<String>,
    pub calls: u32,
    /// The first 120 characters of the script.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub script_head: Option<String>,
    /// The total time the run waited for a person, in milliseconds; `null` for a run stored before
    /// waits were recorded.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub waited_ms: Option<u64>,
}
}

wire! {
pub struct CodeRunList {
    pub runs: Vec<CodeRunSummary>,
}
}

wire! {
/// An engine's Code Mode switch. Off until a person turns it on.
pub struct CodeModeSetting {
    pub enabled: bool,
}
}
