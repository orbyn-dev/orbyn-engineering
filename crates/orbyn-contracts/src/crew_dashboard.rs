//! The Crew dashboard read (`GET /api/crew/dashboard`, W12 in `docs/crew.md`, row G18 in
//! `docs/parity.md`): what the device's own Crew records say about one repository over a window.
//! The contract is [docs/crew-dashboard.md](../../../docs/crew-dashboard.md); this file is the
//! answer it names. Every type is a view the core builds from records it already stores and
//! writes nothing. Times are Unix seconds unless the name ends in `_at`. A figure the records
//! cannot measure is sent as `null` (`cycle`, `cost`) and never as `0`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::crew::{FlowWorking, NeedsYouKind};

wire! {
/// One repository's figures for one window, from this device's Crew records.
pub struct CrewDashboard {
    /// Echo of the project id asked for.
    pub project: String,
    /// The window actually read: 30, 60 or 90.
    pub days: u32,
    /// When the figures were taken, Unix seconds.
    #[ts(type = "number")]
    pub as_of: i64,
    /// A rate shows from this many merges, so the page never hard-codes it.
    pub min_rate_sample: u32,
    /// A median shows from this many merges.
    pub min_median_sample: u32,
    /// Merges in the window. Stopped and cancelled tasks are not merges and are counted nowhere.
    pub merges: u32,
    /// Merges that needed nothing from the person after the brief: no answered question, no
    /// approval, no override, no edit. Never more than `merges`.
    pub unaided: u32,
    /// Merges later reverted, by anyone.
    pub reverted: u32,
    /// The reverts the watch on main made by itself.
    pub reverted_auto: u32,
    /// The reverts a person asked for. `reverted_auto + reverted_by_person == reverted`.
    pub reverted_by_person: u32,
    /// Brief given to merge. `null` while no merge has both times.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub cycle: Option<CrewDashboardCycle>,
    /// At most 14 weeks, oldest first. Empty when `merges` is 0.
    pub weeks: Vec<CrewDashboardWeek>,
    /// At most 12 engines, most merges first.
    pub engines: Vec<CrewDashboardEngine>,
    /// At most 50 areas, most merges first.
    pub areas: Vec<CrewDashboardArea>,
    /// At most 20, newest first.
    pub reverts: Vec<CrewDashboardRevert>,
    /// At most 20, newest first. The page lists these for a repository under `min_rate_sample`
    /// merges, where a rate would say too little.
    pub recent: Vec<CrewDashboardRecent>,
    /// What waits and what works right now, on this device (KC3): a live count of now, not a figure
    /// over the window. It ignores `days`, and every count in it comes from the one needs-you field or
    /// the one working function, so it agrees with the Taskboard, the Inbox and the Review queue.
    pub live: CrewDashboardLive,
    /// A list above had more rows than it holds.
    pub truncated: bool,
}
}

wire! {
/// The dashboard's "Right now" row (KC3): what waits for the person now and which workers own live work.
/// The page draws one tile for `needs_you` (split by kind) and one for `working` (split by why).
/// Nothing here is windowed and nothing is stored: it is read from the records on every request.
pub struct CrewDashboardLive {
    /// What waits for the person now in this repository, by kind: the one needs-you list narrowed to this
    /// project, most items first then by kind, only kinds with at least one item. The same field the Inbox
    /// and the Taskboard read, not necessarily the same count: a background run's item has no card on the
    /// board and another repository's items are not here. A dismissed item is not here.
    pub needs_you: Vec<CrewDashboardWaiting>,
    /// The workers of this repository that own live work now (KC3), by why, most first then by why;
    /// their sum is the page's Working count. Not served: Idle. The store keeps no crew member that
    /// holds no task, so a genuine idle worker cannot be told from one that never joined.
    pub working: Vec<CrewDashboardWorking>,
    /// The live read cut older workers at its bound ([LIVE_WORKERS_MAX] in the store), so `working` covers the
    /// newest ones only. Never a figure the page should draw as complete.
    pub truncated: bool,
}
}

wire! {
/// One kind of wait that is in the inbox now, and how many of it: the page draws the kind's own words.
pub struct CrewDashboardWaiting {
    pub kind: NeedsYouKind,
    pub count: u32,
}
}

wire! {
/// One reason workers own live work now, and how many: the page draws the reason's own line.
pub struct CrewDashboardWorking {
    pub why: FlowWorking,
    pub count: u32,
}
}

wire! {
/// How long a merge took, in seconds, from the brief to the merge. A figure no merge measured is
/// `null`, never a zero.
pub struct CrewDashboardCycle {
    #[serde(default)]
    #[ts(optional = nullable, type = "number")]
    pub median_s: Option<i64>,
    #[serde(default)]
    #[ts(optional = nullable, type = "number")]
    pub p90_s: Option<i64>,
    #[serde(default)]
    #[ts(optional = nullable, type = "number")]
    pub fastest_s: Option<i64>,
    #[serde(default)]
    #[ts(optional = nullable, type = "number")]
    pub slowest_s: Option<i64>,
}
}

wire! {
/// One week of the window, counted from its local Monday.
pub struct CrewDashboardWeek {
    /// Unix seconds of the week's first day, the device's local Monday.
    #[ts(type = "number")]
    pub start: i64,
    pub merges: u32,
    pub unaided: u32,
    pub reverted: u32,
}
}

wire! {
/// One engine's record in the window, with its cost in its own unit.
pub struct CrewDashboardEngine {
    /// The engine that did the work, scrubbed and capped.
    pub name: String,
    /// The model the brief named, scrubbed and capped. `null` when it named none.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub model: Option<String>,
    pub merges: u32,
    pub unaided: u32,
    pub reverted: u32,
    /// `null` when the engine reports no cost, and `null` too when its merges were reported in
    /// different units: a cost is never added across units.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub cost: Option<CrewDashboardCost>,
}
}

wire! {
/// A cost in the engine's own unit. The page adds one engine's unit across repositories; nothing
/// adds two units together.
pub struct CrewDashboardCost {
    /// `tokens`, `micros` or another unit an engine reports, shown as given.
    pub unit: String,
    #[ts(type = "number")]
    pub amount: u64,
}
}

wire! {
/// One area's record in the window. The first two folders of a path, as in Autopilot by area.
/// No cost: one area's work can use engines counted in different units.
pub struct CrewDashboardArea {
    /// The area, scrubbed and capped.
    pub name: String,
    pub merges: u32,
    pub unaided: u32,
    pub reverted: u32,
}
}

wire_enum! {
/// Who reverted a merge.
pub enum CrewRevertBy {
    Person,
    Auto,
}
}

wire! {
/// One merge that was reverted, newest first.
pub struct CrewDashboardRevert {
    /// The brief's outcome: text from a person or an engine, scrubbed and capped. Never markup.
    pub title: String,
    /// When the revert was recorded, Unix seconds.
    #[ts(type = "number")]
    pub at: i64,
    pub by: CrewRevertBy,
    /// Whether the cause was proved certain. `null` when nothing recorded it.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub certain: Option<bool>,
}
}

wire_enum! {
/// What a merge needed from the person after its brief.
pub enum CrewNeeded {
    Nothing,
    /// The person answered an engine's question.
    Answer,
    /// The person allowed, denied or dismissed a held command.
    Approval,
    /// Anything else that waited on the person, such as a merge that needed an answer.
    Other,
}
}

wire_enum! {
/// Whether the task's own checks passed, as its result recorded them.
pub enum CrewChecks {
    Passed,
    Failed,
    #[serde(rename = "none")]
    #[ts(rename = "none")]
    None_,
}
}

wire! {
/// One recent merge of the window, newest first.
pub struct CrewDashboardRecent {
    /// The brief's outcome, scrubbed and capped. Never markup.
    pub title: String,
    /// The engine that did the work, scrubbed and capped.
    pub engine: String,
    #[ts(type = "number")]
    pub at: i64,
    pub needed: CrewNeeded,
    pub checks: CrewChecks,
}
}

wire! {
/// The dashboard's project menu: how many merges each repository the service lists made in one
/// window, so the menu draws a count beside a name without a request per repository first. Counts
/// only - no rate, no cost, no per-repository figure; the [`CrewDashboard`] read of one repository
/// is what answers those, and a rate or a median is never worked out from this answer because a
/// count alone is not the sample the page states under a figure.
pub struct CrewDashboardProjects {
    /// The window actually read: 30, 60 or 90.
    pub days: u32,
    /// When the counts were taken, Unix seconds.
    #[ts(type = "number")]
    pub as_of: i64,
    /// One row per repository the service lists, in the service's own project order. A repository
    /// the person has never used Crew in is here with `merges: 0`, never left out: no merges is a
    /// figure the records can state, unlike a cost or a cycle time they cannot measure.
    pub projects: Vec<CrewDashboardProjectCount>,
    /// The project list had more repositories than one answer holds, so the rows beyond the bound
    /// are not here. The menu says the counts are partial rather than drawing a cut list as whole.
    pub truncated: bool,
}
}

wire! {
/// One repository's merges in the window: the same figure [`CrewDashboard`] answers as `merges` for
/// that repository, so the menu and the page never disagree.
pub struct CrewDashboardProjectCount {
    /// The project id the count is keyed by. The name is the project list's, not this read's: an
    /// answer over every repository carries no text to scrub.
    pub project: String,
    /// Merges in the window.
    pub merges: u32,
}
}

/// The default window, in days: `days` on the request.
pub const CREW_DASHBOARD_DAYS_DEFAULT: u32 = 60;

/// The windows the read accepts.
pub const CREW_DASHBOARD_DAYS: [u32; 3] = [30, 60, 90];
