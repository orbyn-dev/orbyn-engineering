//! Files, editors, outside paths and folder listings.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

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
}
}

wire! {
pub struct FileDiff {
    pub path: String,
    pub status: String,
    pub additions: u32,
    pub deletions: u32,
    pub patch: String,
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
