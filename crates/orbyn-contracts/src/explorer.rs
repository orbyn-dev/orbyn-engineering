//! The left explorer: a project's worktrees and a read-only look at their files.
//! Nothing here writes; edits go through a thread's file routes.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire_enum! {
/// Why Orbyn must not write in a worktree.
pub enum ExplorerReadOnlyReason {
    /// The folder is gone, or Git marks the worktree prunable.
    Missing,
    /// `git worktree lock` was used on it.
    Locked,
    /// A different thread than the one asking has a run going in it.
    OtherSession,
}
}

wire_enum! {
/// The kind of change Git reports for one file; the web draws M, A, D, R, U and a conflict mark.
pub enum ExplorerChange {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
    Conflicted,
}
}

wire_enum! {
/// What an entry is. A link is listed as itself and never followed.
pub enum ExplorerEntryKind {
    File,
    Directory,
    Symlink,
}
}

wire_enum! {
/// Whether the change letters of a listing could be read.
pub enum ExplorerChanges {
    /// The letters are present.
    Ready,
    /// Git is still reading; the answer carries no letters and `data_changed`
    /// with `explorer:<project>` follows when they are ready.
    Pending,
    /// No letters exist: the folder is not in Git, or Git failed.
    Unavailable,
}
}

wire_enum! {
/// What a file read returned.
pub enum ExplorerFileKind {
    Text,
    Binary,
    /// Over the read cap; the size is reported without bytes.
    TooLarge,
}
}

wire! {
/// A thread that works in a worktree. Ids and titles only.
pub struct ExplorerThread {
    pub id: String,
    pub title: String,
    /// A run is going in this thread now.
    pub running: bool,
}
}

wire! {
/// How much of a worktree differs from HEAD, from its one cached Git status.
/// An untracked folder counts once.
#[derive(Default)]
pub struct ExplorerDirty {
    pub changed: u32,
    pub untracked: u32,
    pub conflicted: u32,
}
}

wire! {
pub struct ExplorerWorktree {
    /// Opaque and stable for the path. The only way to name this worktree in a route.
    pub id: String,
    /// Absolute path, for display.
    pub path: String,
    /// The folder's name.
    pub name: String,
    /// `None` when the head is detached.
    pub branch: Option<String>,
    pub detached: bool,
    /// The checked-out revision, abbreviated. Empty when unknown.
    pub head: String,
    /// The primary checkout.
    pub is_main: bool,
    /// The project's folder is a subfolder of a Git checkout: the only row, rooted at
    /// that folder. Nothing above it is shown, and it has no change letters.
    pub is_subfolder: bool,
    /// Orbyn must not write here; `read_only_reason` says why.
    pub read_only: bool,
    pub read_only_reason: Option<ExplorerReadOnlyReason>,
    /// Live threads whose folder is in this worktree.
    pub threads: Vec<ExplorerThread>,
    /// `None` until the first Git status has been read; a `data_changed` follows.
    pub dirty: Option<ExplorerDirty>,
}
}

wire! {
pub struct ExplorerWorktrees {
    pub project_id: String,
    /// False for a folder outside Git: one entry, no branch and no letters.
    pub git: bool,
    pub worktrees: Vec<ExplorerWorktree>,
}
}

wire! {
pub struct ExplorerEntry {
    pub name: String,
    /// Relative to the worktree, with `/` separators on every platform.
    pub path: String,
    pub kind: ExplorerEntryKind,
    /// Bytes, for a file; `None` for a folder or link.
    #[ts(type = "number | null")]
    pub size: Option<u64>,
    /// Git ignores it, or a folder above it.
    pub ignored: bool,
    /// The change of this file. A deleted file is listed though it is gone from disk.
    pub change: Option<ExplorerChange>,
    /// A folder with a changed file somewhere below it.
    pub has_changes: bool,
}
}

wire! {
/// One level of a worktree. Folders first, then names, each case-insensitively.
pub struct ExplorerTree {
    pub worktree_id: String,
    /// The folder listed, relative to the worktree; empty for its root.
    pub path: String,
    pub entries: Vec<ExplorerEntry>,
    /// More entries matched than one answer carries. Narrow with `filter`.
    pub truncated: bool,
    pub changes: ExplorerChanges,
    pub read_only: bool,
}
}

wire! {
/// A file for the explorer's tab. Read-only: it carries no way to save.
pub struct ExplorerFile {
    pub worktree_id: String,
    pub path: String,
    pub kind: ExplorerFileKind,
    /// Empty unless `kind` is `text`.
    pub content: String,
    #[ts(type = "number")]
    pub size: u64,
    /// Digest of the bytes of a text file.
    pub revision: Option<String>,
    pub change: Option<ExplorerChange>,
    pub read_only: bool,
}
}
