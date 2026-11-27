//! The Taskboard over HTTP (Crew Preview): a crew's work tasks grouped into the seven columns. Since KC3 a card sits in Needs you
//! because the one needs-you field names it (`needs_you`), not because of its run state, and its `working` word comes from the one
//! `working` function; every other column is the task's run state. A card is a view the core builds from what it already stores;
//! none can hold an answer key, a check, a check's text, a repro, a thread id or a path (H1). Free text is redacted and capped
//! before it is put in one. Times are RFC 3339.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{NeedsYouKind, StopDetail};

wire_enum! {
/// The seven columns, in the order work moves through them.
pub enum FlowColumnId {
    Queued,
    Routed,
    Running,
    Verifying,
    NeedsYou,
    Passed,
    StoppedCancelled,
}
}

wire! {
/// Checks passed out of checks run in a task's final verification, shown for a passed task only: two counts, never a check, its id, its command or its text.
pub struct FlowChecks {
    pub passed: u32,
    pub of: u32,
}
}

wire_enum! {
/// Why the worker owns live work now (KC3 PR2): the one `working` function's answer, so the Taskboard, the Map and the
/// dashboard cannot disagree about who is Working. The words are the page's; the task's column still says its run state.
pub enum FlowWorking {
    /// A live turn: a build running, a reviewer lane's round, or a background run in flight.
    Running,
    /// The verifier is running the task's checks.
    Verifying,
    /// A retry is queued after a failed check (`failed`, or the once-in-a-life approach change).
    Retry,
    /// The task itself has ended and a fix of a review finding has not finished.
    Fix,
}
}

wire! {
/// One work task as a card. The two flags are advisory: the write routes decide.
pub struct FlowCard {
    pub id: String,
    pub name: String,
    pub kind: String,
    /// The lead's member name.
    pub owner: String,
    /// The engine routed to build it; absent until it is routed ("Not routed yet").
    #[serde(default)]
    #[ts(optional = nullable)]
    pub engine: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub model: Option<String>,
    /// `low` or `medium`, as stored.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub effort: Option<String>,
    pub state: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub stop_reason: Option<String>,
    /// The typed detail of an `engine_error` stop, when it has one (KC8): `worker_stopped_twice`.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub stop_detail: Option<StopDetail>,
    /// When the task last changed state.
    pub updated_at: String,
    /// The one needs-you field's line for this card (one redacted line, at most 160 characters): the task's own scrubbed
    /// result for a task that waits on a person or could not start, or the core's fixed words for a merge that waits. `null`
    /// when the field does not name the card, which is the case for a task whose dismissible wait was dismissed on this device:
    /// it still sits in Needs you because its run state is `needs_person`, and it draws no sentence.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub reason: Option<String>,
    /// The worker never ran: the task was refused at its start (an engine the app cannot restrict, no route, a setting that
    /// blocks it) and waits so the person can retry or dismiss it. Not an approval: nothing is asked of the person but that
    /// choice. `reason` holds the refusal while the one needs-you field names the card; after a dismissal the card keeps its
    /// place in Needs you (the task still cannot run) and carries no reason.
    pub could_not_start: bool,
    /// The task has not ended, so a cancel may apply.
    pub can_cancel: bool,
    /// Its checks may be run again: it passed, or it stopped for a benign reason after its checks ran, and it has a checkout.
    pub can_rerun: bool,
    /// Checks passed out of run in the final verification; present only on a passed task. A task still verifying, failed or stopped
    /// has none: the core keeps no record per verification attempt to count from.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub checks: Option<FlowChecks>,
    /// The wait this card answers, when the one needs-you field names it (KC3): the kind decides the shared sentence. `null` when
    /// the field does not name the card — a task whose dismissible wait was dismissed on this device still sits in Needs you
    /// because its run state is `needs_person`, and it carries no kind and no reason.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub needs_you: Option<NeedsYouKind>,
    /// Why its worker owns live work now (KC3), from the one `working` function; `null` when it owns none. Not the column:
    /// the column still says the task's run state.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub working: Option<FlowWorking>,
}
}

wire! {
pub struct FlowColumn {
    pub column: FlowColumnId,
    /// Cards in this column after the filters.
    pub count: u32,
    pub cards: Vec<FlowCard>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub next_offset: Option<u32>,
}
}

wire! {
/// How many of the crew's tasks an engine or a kind has, before the filters, for the header chips.
pub struct FlowFacet {
    pub name: String,
    pub count: u32,
}
}

wire! {
pub struct FlowBoard {
    pub crew: String,
    /// The crew's work tasks read, before the filters.
    pub total: u32,
    pub engines: Vec<FlowFacet>,
    pub kinds: Vec<FlowFacet>,
    /// The seven columns, or the one asked for.
    pub columns: Vec<FlowColumn>,
    /// The crew has more work tasks than one call reads; the newest are shown.
    pub truncated: bool,
}
}
