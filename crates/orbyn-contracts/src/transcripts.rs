//! The transcript file (docs/attachments-transcripts.md, "Transcript export and import"): what `GET /api/threads/{id}/export`
//! writes and `POST /api/threads/import` reads. The file is plain JSON; it never holds a secret, an environment value, an
//! attachment's bytes, an engine session id or a path outside the project.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Attachment, Thread};

/// The `format` member of every transcript file.
pub const TRANSCRIPT_FORMAT: &str = "orbyn-transcript";
/// The version this build writes and the newest it reads.
pub const TRANSCRIPT_VERSION: u32 = 1;
/// The file ending. The web saves a transcript under `<title>` plus this.
pub const TRANSCRIPT_FILE_ENDING: &str = ".orbyn.json";
/// The media type of a saved transcript file. The HTTP answer of the export route is `application/json`.
pub const TRANSCRIPT_MEDIA_TYPE: &str = "application/vnd.orbyn.transcript+json";

wire! {
/// The thread facts a transcript carries. `engine` and `model` are labels for the reader: an import never trusts them to pick
/// an engine session or a model.
pub struct TranscriptThread {
    pub title: String,
    pub engine: String,
    pub model: Option<String>,
    pub created_at: String,
}
}

wire! {
/// One message: `role` is `user` or `assistant`, `text` is plain text, already redacted.
pub struct TranscriptMessage {
    pub role: String,
    pub ts: String,
    pub text: String,
}
}

wire! {
/// The file. Unknown members are dropped on import.
pub struct TranscriptDocument {
    pub format: String,
    pub version: u32,
    pub exported_at: String,
    pub thread: TranscriptThread,
    /// Oldest first.
    pub messages: Vec<TranscriptMessage>,
    /// How many older messages were left out so the file stays within the import's size cap.
    pub omitted_earlier: u32,
}
}

wire! {
/// One thing an import dropped or changed. `kind` is `message`, `title`, `thread` or `document`; `index` is the message's place
/// in the file when `kind` is `message`. Never the text.
pub struct TranscriptProblem {
    pub kind: String,
    pub index: Option<u32>,
    pub code: String,
}
}

wire! {
/// `POST /api/threads/import`. `thread` is absent on a dry run.
pub struct TranscriptImportAnswer {
    pub dry_run: bool,
    pub thread: Option<Thread>,
    pub title: String,
    pub message_count: u32,
    pub dropped: u32,
    pub problems: Vec<TranscriptProblem>,
}
}

wire! {
/// `POST /api/threads/{id}/transcripts`. `attachment` is absent on a dry run. The thread itself is unchanged: the attachment
/// rides on the thread's next message like any other.
pub struct TranscriptAttachAnswer {
    pub dry_run: bool,
    pub attachment: Option<Attachment>,
    pub message_count: u32,
    pub dropped: u32,
    pub problems: Vec<TranscriptProblem>,
}
}
