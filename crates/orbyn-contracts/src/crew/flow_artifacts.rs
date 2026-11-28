//! One Flow task's artifacts, check results and diff over HTTP (Preview): the evidence rows of the build under way, the
//! artifacts those rows name, the change the checks read, and the commit the checkout is on now, which an artifact pinned to
//! an earlier commit is stale against. Every type is a view the core builds from what it already stores; none can hold an
//! answer key or any part of a check: not its command, its output, what it proves or an Evidence id (H1). Free text is
//! redacted and capped before it is put in one. Times are RFC 3339.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire! {
/// One artifact a check's evidence names. Its bytes are not served: the store keeps the hash only, so a pruned artifact and
/// one never written look the same here.
pub struct FlowArtifact {
    /// `screenshot`, `diff`, `report` or `log`.
    pub kind: crate::ArtifactKind,
    /// The name its producer wrote, redacted and capped; absent when it wrote none.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub name: Option<String>,
    /// The content hash of the bytes, which no route serves.
    pub hash: String,
}
}

wire! {
/// One check of a task's build as it ran: which check, how it went, on which commit, and the artifacts it named. Never the
/// command it ran, its output, what it proves, or any part of the answer key beyond the check's own id and kind.
pub struct FlowCheck {
    /// The check's id, as the answer key gives it; never its text.
    pub id: String,
    /// The check's kind, such as `test` or `lint`, when the task's key names the check; absent for a check the key does not,
    /// such as a lead's repro of a finding run in a fix's verification. Never the check's params.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub kind: Option<crate::CheckKind>,
    /// The task whose verification ran it: the task itself, or the fix that answered a finding.
    pub task: String,
    /// `pass`, `fail`, `error` or `skipped`.
    pub outcome: crate::CheckOutcome,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub exit_code: Option<u32>,
    #[ts(type = "number")]
    pub duration_ms: u64,
    /// The commit the checks read: the checkout's head when the verification ran, not the change itself.
    pub commit: String,
    /// The artifacts this row named. The store caps a row at 64.
    pub artifacts: Vec<FlowArtifact>,
}
}

wire! {
/// One file the change touched, as counts. Never a patch: the Flow draws a diff summary.
pub struct FlowDiffFile {
    /// The path inside the repository; never a path on this machine.
    pub path: String,
    /// `added`, `deleted` or `modified`.
    pub status: String,
    pub added: u32,
    pub removed: u32,
}
}

wire! {
/// The change a task's checks read: the files the checkout changed since it started, as counts. Read from git when this is
/// asked for, so it is the checkout as it is now, not a record of the verification.
pub struct FlowTaskDiff {
    /// The commit the change is read against: the one the task started from.
    pub base: String,
    /// The files that changed, by path, up to the read's cap.
    pub files: Vec<FlowDiffFile>,
    /// Files the change touched, before the cap.
    pub files_total: u32,
    /// Lines added over every changed file, before the cap.
    pub added: u32,
    /// Lines removed over every changed file, before the cap.
    pub removed: u32,
    /// The file list was cut at its cap.
    pub truncated: bool,
}
}

wire! {
/// One task's checks, the artifacts they named, the change they read and the commit the checkout is on now: what the Flow's
/// checks bar, artifacts list and artifact diff draw. A file the task left untracked is not in the diff, which reads the
/// files git tracks. The checkout's head is what an artifact or a diff pinned to an earlier commit is stale against: it is
/// stale once the checkout has moved on by itself, and this read never says whether it has, only which commit it is on.
pub struct FlowTaskArtifacts {
    pub crew: String,
    pub task: String,
    /// The evidence rows of the build under way, oldest first: the latest fix's when it has any, else the task's own.
    pub checks: Vec<FlowCheck>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub diff: Option<FlowTaskDiff>,
    /// Why there is no diff: `no_checkout`, `no_base` or `unreadable`. Absent when `diff` is there.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub diff_absent: Option<String>,
    /// The commit the task's checkout is on now, read from git when this is asked for, so it is the checkout as it is now and
    /// not a record of the verification.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub head: Option<String>,
    /// Why there is no head: `no_checkout` or `unreadable`. Absent when `head` is there.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub head_absent: Option<String>,
    /// A list was cut at its cap.
    pub truncated: bool,
}
}
