//! Importing sessions found in an engine's own history.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire! {
/// One saved conversation an engine keeps in its own history folder.
pub struct HistorySession {
    /// `claude` or `codex`.
    pub engine: String,
    pub session_id: String,
    /// The first prompt, cut to 120 characters, or the session id.
    pub title: String,
    /// The folder the engine ran in, as its history recorded it.
    pub cwd: String,
    pub updated_at: String,
    /// The registered project at exactly that folder, if any.
    pub project_id: Option<String>,
    /// Whether that project is trusted, so the session can be imported.
    pub importable: bool,
    /// The thread already holding this session.
    pub thread_id: Option<String>,
}
}

wire! {
pub struct HistoryScan {
    pub sessions: Vec<HistorySession>,
    /// More files existed than the scan reads.
    pub truncated: bool,
}
}

wire! {
pub struct ImportHistorySession {
    pub engine: String,
    pub session_id: String,
}
}

wire! {
pub struct HistorySessionImported {
    pub thread_id: String,
    pub imported: bool,
}
}

wire! {
/// One message an imported thread had before it was imported. `text` is plain text and never markup.
pub struct EarlierMessage {
    /// `user`, `assistant` or `tool`.
    pub role: String,
    pub text: String,
}
}

wire! {
/// One page of an imported thread's earlier messages, oldest first for the page.
pub struct EarlierPage {
    /// The provider the history came from, `Claude Code` or `Codex`.
    pub source: String,
    /// How many messages the history holds in all.
    pub total: u32,
    /// When the thread was imported.
    pub imported_at: String,
    /// The cursor for the page before this one, or `null` at the start of the history.
    pub next_before: Option<String>,
    pub messages: Vec<EarlierMessage>,
}
}
