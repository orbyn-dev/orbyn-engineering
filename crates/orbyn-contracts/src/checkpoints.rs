//! Checkpoints, restore points and the file changes a run made.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire! {
/// The workspace before and after one run, kept in hidden refs.
pub struct Checkpoint {
    pub run_id: String,
    pub start: String,
    /// Absent until the run ends.
    pub end: Option<String>,
    pub created_at: String,
}
}

wire! {
pub struct CommitInfo {
    pub sha: String,
    pub subject: String,
    pub author: String,
    pub date: String,
}
}

wire_enum! {
pub enum RestorePoint {
    /// As the run found it.
    Start,
    /// As the run left it.
    End,
}
}

wire! {
pub struct RestoreWorkspace {
    pub run_id: String,
    pub point: RestorePoint,
}
}

wire! {
pub struct RewindThread {
    /// The conversation continues from just before this run.
    pub run_id: String,
    pub restore_files: bool,
}
}

wire! {
/// Files a run's engine said it wrote. Nothing is inferred from the checkout, so a
/// thread sharing the folder adds nothing here. A command's own writes are not known.
pub struct RunChanges {
    pub run_id: String,
    pub paths: Vec<String>,
    /// More files were reported than are listed.
    pub truncated: bool,
    /// What was kept of them, relative to the checkout.
    #[serde(default)]
    pub files: Vec<ChangedFile>,
}
}

wire_enum! {
/// `Unknown` means an end of the change was not kept: too large, over the run's
/// budget, or written before Orbyn could look.
pub enum ChangeStatus {
    Added,
    Modified,
    Deleted,
    Unchanged,
    Unknown,
}
}

wire! {
pub struct ChangedFile {
    pub path: String,
    pub status: ChangeStatus,
    /// Both ends were kept, so this file can be put back either way.
    pub restorable: bool,
}
}

wire! {
/// A file as a run found and left it, from what was kept then, never the folder as
/// it is now. A text is empty when absent, unknown or binary.
pub struct ChangedFileContent {
    pub path: String,
    pub status: ChangeStatus,
    pub before: Option<String>,
    pub after: Option<String>,
    pub binary: bool,
}
}

wire! {
pub struct RestoreChanges {
    /// `start` puts back what the run found; `end` what it left.
    pub point: RestorePoint,
    /// Absent means every file the run changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub paths: Option<Vec<String>>,
}
}

wire_enum! {
pub enum RestoreOutcome {
    Restored,
    /// It already matches.
    Unchanged,
    /// It holds something the run did not leave, so it was left alone.
    Conflict,
    /// That end of the change was not kept.
    NotKept,
}
}

wire! {
pub struct RestoredFile {
    pub path: String,
    pub outcome: RestoreOutcome,
}
}
