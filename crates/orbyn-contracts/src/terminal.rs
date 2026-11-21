//! Terminals, project scripts and the project file.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire! {
pub struct TerminalInfo {
    pub id: String,
    pub thread_id: String,
    pub title: String,
    /// Terminals with the same group share a split.
    pub group: String,
    /// `horizontal` or `vertical`, for a terminal split from another.
    pub direction: Option<String>,
    pub alive: bool,
    pub exit_code: Option<i32>,
    /// Clients attached right now, observers included.
    pub clients: usize,
    /// The project script this runs, if any.
    pub script: Option<String>,
    pub created_at: String,
}
}

wire! {
pub struct OpenTerminal {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    /// Joins this terminal's split group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub split_of: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub direction: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub cols: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub rows: Option<u16>,
}
}

wire! {
/// Opens the engine's own console resumed on the thread's session.
pub struct OpenConsole {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub cols: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub rows: Option<u16>,
}
}

wire! {
pub struct RenameTerminal {
    pub title: String,
}
}

wire! {
pub struct TerminalScrollback {
    /// What the terminal printed, control sequences included.
    pub text: String,
}
}

wire! {
/// A team script from the project file.
pub struct ProjectScript {
    // Stable action identity; absent in legacy project files and saved lists.
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub run_on_worktree_create: bool,
    #[serde(default = "script_is_async", rename = "async")]
    #[ts(rename = "async")]
    pub asynchronous: bool,
    #[serde(default)]
    pub run_on_settle: bool,
    #[serde(default)]
    pub preview_url: Option<String>,
    #[serde(default)]
    pub auto_open_preview: bool,
}
}

fn script_is_async() -> bool {
    true
}

wire! {
/// `orbyn.json` at the checkout's root. Orbyn only reads it.
pub struct ProjectFile {
    #[serde(default)]
    pub icon_path: Option<String>,
    /// `local` or `worktree`.
    #[serde(default)]
    pub default_thread_env_mode: Option<String>,
    /// `recursive`, `top-level` or `none`. Older spellings are normalized on read.
    #[serde(default)]
    pub worktree_submodules: Option<String>,
    #[serde(default)]
    pub scripts: Vec<ProjectScript>,
}
}

wire! {
/// Actual project-file contents plus the effective `scripts` list after settings
/// overrides. Resolving scripts never rewrites `file`.
pub struct ProjectFileView {
    /// The file that was read, relative to the checkout.
    pub path: Option<String>,
    pub file: Option<ProjectFile>,
    /// Why a file that exists could not be used.
    pub problem: Option<String>,
    pub scripts: Vec<ProjectScript>,
}
}

wire! {
pub struct RunScript {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
}
}

wire! {
/// One frame of `GET /api/threads/{id}/terminals/{terminal}/output/events`: a
/// terminal's output as it is produced. Text is UTF-8, so a character split across
/// two PTY reads arrives whole in the next frame; control sequences are kept and
/// other bytes read as U+FFFD, the same as `TerminalScrollback.text`.
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommandOutputEvent {
    /// Output kept before the subscription began, oldest first: zero or more frames.
    Snapshot { text: String },
    /// Output produced after the snapshots.
    Chunk { text: String },
    /// The terminal's output ended, so the subscription closes after this frame.
    /// `exit_code` is the child's exit code as reported by then, and `None` when it
    /// had not been reported yet.
    Ended { exit_code: Option<i32> },
    /// This client fell behind the live output and lost `dropped` chunks, so the
    /// stream ends. Connect again for the retained output.
    Overflow {
        #[ts(type = "number")]
        dropped: u64,
    },
}
}
