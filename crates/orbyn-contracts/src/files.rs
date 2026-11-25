//! Files, editors, outside paths and folder listings.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ExplorerEntryKind;

wire! {
pub struct FileEntry {
    pub name: String,
    /// Relative to the checkout.
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub ignored: bool,
}
}

wire! {
pub struct FileContent {
    pub path: String,
    /// Empty when `binary` or `truncated`.
    pub content: String,
    /// Digest of the file's bytes; send it back to write safely.
    pub revision: String,
    pub size: u64,
    pub binary: bool,
    pub truncated: bool,
}
}

wire! {
/// A file outside the thread's checkout that the person opened to look at. It
/// carries no revision because nothing can save it: `read_only` and `outside`
/// are always true, so a client never offers edit or save.
pub struct OutsideFile {
    /// The canonical absolute path, with links resolved.
    pub path: String,
    /// Empty when `binary` or `truncated`.
    pub content: String,
    pub size: u64,
    pub binary: bool,
    pub truncated: bool,
    pub outside: bool,
    pub read_only: bool,
}
}

wire_enum! {
pub enum ThreadPathBasis {
    Thread,
    Repository,
}
}

wire! {
/// Navigation metadata only; ordinary file reads still require a checkout path.
#[serde(deny_unknown_fields)]
pub struct ResolveThreadPath {
    pub path: String,
    pub basis: ThreadPathBasis,
    // The caller's current thread cwd, fenced against moves during resolution.
    pub cwd: String,
}
}

wire! {
pub struct ResolvedThreadPath {
    pub cwd: String,
    // An existing safe file inside the selected thread cwd, never a sibling.
    pub checkout_path: Option<String>,
    // Git-relative metadata can name missing/deleted files and checkout siblings.
    pub repository_path: Option<String>,
}
}

wire! {
pub struct WriteFile {
    pub path: String,
    pub content: String,
    /// The revision this edit started from; `None` means the file must not exist.
    pub base_revision: Option<String>,
    /// A multi-repository thread's repository; absent is the primary one (docs/multi-repo.md).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub repo_id: Option<String>,
}
}

wire! {
pub struct SearchHit {
    pub path: String,
    pub line: u32,
    pub text: String,
}
}

wire! {
pub struct EditorInfo {
    pub id: String,
    pub name: String,
    /// `path`, `goto` or `line_column`; `path` editors take `file:line:col`.
    #[serde(default)]
    pub launch_style: String,
    /// The system file manager, which reveals a file and has no line target.
    #[serde(default)]
    pub file_manager: bool,
}
}

wire! {
pub struct OpenInEditor {
    pub path: String,
    pub editor: String,
    /// One-based position to open at. A column needs a line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub column: Option<u32>,
    /// A multi-repository thread's repository; absent is the primary one (docs/multi-repo.md).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub repo_id: Option<String>,
}
}

wire! {
pub struct FileDiff {
    pub path: String,
    pub status: String,
    pub additions: u32,
    pub deletions: u32,
    pub patch: String,
    /// The repository of a multi-repository thread the file is in; absent for an ordinary thread.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub repo_id: Option<String>,
}
}

wire! {
pub struct FsEntry {
    pub name: String,
    pub path: String,
    pub is_git: bool,
}
}

wire! {
pub struct FsListing {
    pub path: String,
    pub parent: Option<String>,
    /// Directories only, sorted by name.
    pub entries: Vec<FsEntry>,
}
}

wire! {
/// One path in a thread's checkout for the file browser: the entry itself, with no bytes
/// read and the last link never followed. A path with nothing there is an answer
/// (`exists: false`), not an error; a path that leaves the checkout is refused.
pub struct FileMetadata {
    /// The path asked for, relative to the checkout with `/` separators; empty is its root.
    pub path: String,
    /// False when nothing exists at the path; everything else is then empty.
    pub exists: bool,
    /// The entry's own kind. `None` when nothing exists there, or the entry is a device,
    /// pipe or socket, which is never opened.
    pub kind: Option<ExplorerEntryKind>,
    /// Bytes for a file; a folder's own record size; a link's target text, never its
    /// target's bytes.
    #[ts(type = "number")]
    pub size: u64,
    /// Unix permission bits (`0o777`); `None` where the platform has none.
    pub mode: Option<u32>,
    /// True when the entry cannot be written, which every platform reports.
    pub read_only: bool,
    /// Last modified time, RFC 3339 UTC; `None` when the platform cannot say.
    pub modified: Option<String>,
    /// False for a link that resolves nowhere, lands outside the checkout or lands in a
    /// place a file read refuses (Git's folder, the library's `repo/`): the link is still
    /// reported as itself, and nothing about where it points is returned.
    pub inside: bool,
}
}
