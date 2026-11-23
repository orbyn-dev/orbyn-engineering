//! The Crew "needs you" inbox over HTTP (Crew Preview): one list of everything, across every crew, that waits for the person.
//! An item names only ids, a kind, a time and at most one redacted line: it carries no task content, no answer key, no check,
//! no path and no thread id. The read is built from what Crew already stores, so it holds no state of its own; a dismissal is
//! the person's local choice and changes nothing behind it. Times are RFC 3339.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire_enum! {
/// Why an item is in the inbox: the first matching rule for a task, or the wait of a merge or a background run.
pub enum NeedsYouKind {
    TaskNeedsPerson,
    CouldNotStart,
    ManualCheck,
    MergeWaiting,
    BackgroundReview,
    TripwireStop,
    WorkerStoppedTwice,
    /// The task's last cancel attempt could not stop its worker; the task keeps its state and the stop is tried again.
    CouldNotStop,
}
}

wire! {
/// One thing that waits for the person. `id` is the task's id, or a background run's id; `since` is when it began to wait.
pub struct NeedsYouItem {
    /// The task's id, or a background run's id.
    pub id: String,
    pub kind: NeedsYouKind,
    /// The project the item belongs to, when its task or run still names one.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub project_id: Option<String>,
    /// The crew the item belongs to: a task's crew, a merge's crew, or a background crew.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub crew_id: Option<String>,
    /// When the item began to wait, as RFC 3339.
    pub since: String,
    /// The person may dismiss it on this device; the others are settled by answering them.
    pub dismissible: bool,
    /// One redacted line, at most 160 characters, never task content: the core's fixed words for the kind, or, for a task that
    /// waits on a person, the task's latest scrubbed reason.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub reason: Option<String>,
}
}

wire! {
/// Everything that waits for the person, newest first, at most 200 items.
pub struct NeedsYouList {
    pub items: Vec<NeedsYouItem>,
    /// How many items there are (the length of `items`).
    pub count: u32,
}
}

wire! {
/// A person's dismissal of one item on this device.
pub struct NeedsYouDismissBody {
    pub kind: NeedsYouKind,
    pub id: String,
}
}

wire! {
/// The answer to a dismissal: it is hidden on this device, and nothing behind it changed.
pub struct NeedsYouDismissed {
    pub dismissed: bool,
}
}
