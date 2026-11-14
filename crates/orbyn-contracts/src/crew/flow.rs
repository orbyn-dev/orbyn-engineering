//! Crew Flow over HTTP (Preview): what the Flow screens read. Every type is a view the core builds from what it already
//! stores; none can hold an answer key, a check, a repro, a path outside the repository, a thread id or a bus message
//! (H1). Free text is redacted and capped before it is put in one. Times are RFC 3339.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire_enum! {
/// The ten stages of a task's line, in order.
pub enum FlowStage {
    Brief,
    Route,
    Build,
    Checks,
    Code,
    Security,
    Finding,
    Merge,
    Main,
    You,
}
}

wire_enum! {
/// Where one stage stands. `review_waiting` is a merged change waiting for the person's review.
pub enum FlowCellState {
    Done,
    Running,
    NeedsYou,
    Failed,
    Pending,
    Skipped,
    ReviewWaiting,
}
}

wire_enum! {
/// The list filters. A task can be in more than one of `needs_you`, `reviewing` and `failed`.
pub enum FlowFilter {
    All,
    NeedsYou,
    Reviewing,
    Failed,
    Passed,
}
}

wire! {
/// One stage of one task. `count` is checks passed out of checks run; `lanes` is the reviewer lanes that ran.
pub struct FlowCell {
    pub stage: FlowStage,
    pub state: FlowCellState,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub passed: Option<u32>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub of: Option<u32>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub lanes: Option<u32>,
}
}

wire! {
/// One crew that has work tasks. Crews have no stored name, so `label` is the id shortened.
pub struct FlowCrewRow {
    pub id: String,
    pub label: String,
    pub tasks: u32,
    pub finished: u32,
    pub updated_at: String,
}
}

wire! {
pub struct FlowCrewList {
    pub crews: Vec<FlowCrewRow>,
    pub total: u32,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub next_offset: Option<u32>,
}
}

wire! {
/// One task of the Flow list: the line of ten stages and the round.
pub struct FlowTaskRow {
    pub id: String,
    pub name: String,
    pub kind: String,
    /// The engine routed to build it, when it is routed.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub engine: Option<String>,
    /// The lead's member name.
    pub owner: String,
    pub state: String,
    pub round: u32,
    pub cells: Vec<FlowCell>,
    pub created_at: String,
    pub updated_at: String,
}
}

wire! {
pub struct FlowCounts {
    pub all: u32,
    pub needs_you: u32,
    pub reviewing: u32,
    pub failed: u32,
    pub passed: u32,
}
}

wire! {
pub struct FlowTaskList {
    pub crew: String,
    /// Counts over the crew's tasks before `filter`, `engine` and `q` apply.
    pub counts: FlowCounts,
    pub tasks: Vec<FlowTaskRow>,
    /// Tasks that match, before the page.
    pub total: u32,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub next_offset: Option<u32>,
    /// The crew has more work tasks than one call reads; the newest are shown.
    pub truncated: bool,
}
}

wire! {
/// What the brief asked for. The files are the grants' patterns, never a path on this machine.
pub struct FlowBrief {
    pub purpose: String,
    pub files: Vec<String>,
    pub budget_turns: u32,
    pub budget_minutes: u32,
}
}

wire! {
pub struct FlowRoute {
    pub engine: String,
    pub model: String,
    pub tier: String,
}
}

wire! {
/// A reviewer lane in its latest round.
pub struct FlowLane {
    pub lane: String,
    /// `code` or `security`.
    pub group: String,
    pub task: String,
    pub round: u32,
    pub state: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub engine: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub model: Option<String>,
    /// Findings this lane filed in that round.
    pub findings: u32,
    pub confirmed: u32,
    pub unconfirmed: u32,
}
}

wire! {
/// A reviewer's finding. Only `check: confirmed` and `blocks: true` stops a task.
pub struct FlowFinding {
    pub id: String,
    pub lane: String,
    pub round: u32,
    pub severity: String,
    pub path: String,
    pub line: u32,
    pub claim: String,
    /// `confirmed`, `unconfirmed`, `pending` or `unchecked`.
    pub check: String,
    /// `open`, `fixed`, `regressed`, `superseded` or `unreported`.
    pub status: String,
    pub blocks: bool,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub fix_task: Option<String>,
}
}

wire! {
/// A decision at a stage. `reason` is shown once the task is final.
pub struct FlowGate {
    pub stage: String,
    pub decision: String,
    /// `code`, `confirmed_finding` or `person`.
    pub decided_by: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub lane: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub reason: Option<String>,
    pub at: String,
}
}

wire! {
/// One round of the build loop: round 1 is the task, each later round the fix that answered a finding.
pub struct FlowRound {
    pub n: u32,
    pub task: String,
    pub state: String,
}
}

wire! {
pub struct FlowMerge {
    pub state: String,
    pub risk: String,
    pub reasons: Vec<String>,
    /// The Autopilot mode the merge was decided under.
    pub mode: String,
}
}

wire! {
/// The person's part after the merge.
pub struct FlowYou {
    /// `waiting`, `looks_good`, `commented` or `reverted`.
    pub review_state: String,
    #[serde(default)]
    #[ts(type = "number | null", optional)]
    pub due_unix: Option<i64>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub needs_person: Option<String>,
}
}

wire! {
/// One task zoomed. No check, command, output excerpt or answer key is in it.
pub struct FlowTaskDetail {
    pub row: FlowTaskRow,
    pub brief: FlowBrief,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub route: Option<FlowRoute>,
    pub lanes: Vec<FlowLane>,
    pub findings: Vec<FlowFinding>,
    pub gates: Vec<FlowGate>,
    pub rounds: Vec<FlowRound>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub merge: Option<FlowMerge>,
    /// `unknown`, `green` or `broken` once merged.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub watch: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub you: Option<FlowYou>,
    /// A list was cut at its cap.
    pub truncated: bool,
}
}
