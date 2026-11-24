//! The Crew "needs you" inbox over HTTP (Crew Preview): one list of everything, across every crew, that waits for the person.
//! An item names only ids, a kind, a time and at most one redacted line: it carries no task content, no answer key, no check,
//! no path and no thread id. The read is built from what Crew already stores, so it holds no state of its own; a dismissal is
//! the person's local choice and changes nothing behind it. Times are RFC 3339.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire_enum! {
/// Why an item is in the inbox: the first matching rule for a task, or the wait of a merge, a background run or a watch.
pub enum NeedsYouKind {
    TaskNeedsPerson,
    CouldNotStart,
    ManualCheck,
    MergeWaiting,
    BackgroundReview,
    /// A watch's command turned failing and asked for the person (or its template could not start).
    WatchFailing,
    /// Three checks in a row of a watch could not run. A check that runs again resolves it.
    WatchCouldNotRun,
    TripwireStop,
    WorkerStoppedTwice,
    /// The task's last cancel attempt could not stop its worker; the task keeps its state and the stop is tried again.
    CouldNotStop,
}
}

wire! {
/// One thing that waits for the person. `id` is the task's id, a background run's id or a watch crew's id; `since` is when it
/// began to wait. `name`, `engine`, `model` and `task_kind` are set only for a `could_not_stop` item.
pub struct NeedsYouItem {
    /// The task's id, a background run's id, or a watch crew's id.
    pub id: String,
    pub kind: NeedsYouKind,
    /// The project the item belongs to, when its task or run still names one.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub project_id: Option<String>,
    /// The crew the item belongs to: a task's crew, a merge's crew, a background crew, or a watch crew.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub crew_id: Option<String>,
    /// When the item began to wait, as RFC 3339.
    pub since: String,
    /// The person may dismiss it on this device; the others are settled by answering them.
    pub dismissible: bool,
    /// One redacted line, at most 160 characters, never task content: a task's own latest scrubbed `needs_person` reason when
    /// it has one (the question, the refusal that kept it from starting, a manual check), and the core's fixed words for a
    /// kind whose task has none (a merge that waits, a background run's review, a stop).
    #[serde(default)]
    #[ts(optional = nullable)]
    pub reason: Option<String>,
    /// The task's brief outcome, one redacted line of at most 160 characters, for a `could_not_stop` item only.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub name: Option<String>,
    /// The engine routed to build the task, for a `could_not_stop` item whose task is routed.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub engine: Option<String>,
    /// The routed model, set with `engine`.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub model: Option<String>,
    /// The brief's kind as its wire name, set only for a `could_not_stop` item.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub task_kind: Option<String>,
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

wire! {
/// One item a window wants to announce: its kind and id, as the list names them.
pub struct NeedsYouRef {
    pub kind: NeedsYouKind,
    pub id: String,
}
}

wire! {
/// A window asks to announce these items (at most 50) with a desktop notification.
pub struct NeedsYouNotifyBody {
    pub items: Vec<NeedsYouRef>,
}
}

wire! {
/// The items this window may announce: the ones that are current items of the list, not dismissed, and not announced yet at
/// their current wait. Each is marked announced before the answer, so no other window is given the same item.
pub struct NeedsYouNotified {
    pub claimed: Vec<NeedsYouRef>,
}
}
