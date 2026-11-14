//! The Taskboard over HTTP (Crew Preview): a crew's work tasks grouped by run state. A card is a view the core builds from what
//! it already stores; none can hold an answer key, a check, a count of checks, a repro, a thread id or a path (H1). Free text is
//! redacted and capped before it is put in one. Times are RFC 3339.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

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
    /// When the task last changed state.
    pub updated_at: String,
    /// Why the task needs the person, for a task in Needs you.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub reason: Option<String>,
    /// The task has not ended, so a cancel may apply.
    pub can_cancel: bool,
    /// Its checks may be run again: it passed, or it stopped for a benign reason after its checks ran, and it has a checkout.
    pub can_rerun: bool,
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
