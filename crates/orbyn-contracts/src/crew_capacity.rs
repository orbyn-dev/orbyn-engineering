//! Review capacity and review coverage (Crew Preview). Counts, limits and line numbers only: no money, no path of a
//! file, no prompt. A value the core could not measure is absent, never zero.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire_enum! {
/// How much of the low-risk work is picked for a closer look.
pub enum CapacitySampling { Off, OneInFour }
}

wire_enum! {
/// A task's priority. Only Low can be paused.
pub enum CapacityPriority { High, Medium, Low }
}

wire! {
/// What a new Low-priority start that Crew began itself would meet right now: the merges waiting for a person and the
/// limit. A refused start is not stored, so this is the current decision and not a list of held tasks.
pub struct PausedStart {
    pub waiting: u32,
    pub limit: u32,
}
}

wire! {
/// One row of the sampling plan: a batch of `from` to `to` low-risk merges has `sampled` of them chosen.
pub struct SamplePlan {
    pub from: u32,
    pub to: u32,
    pub sampled: u32,
}
}

wire! {
/// `GET /api/crew/capacity?project=`: a project's capacity settings and the facts the pause rule reads.
pub struct CapacityState {
    pub project: String,
    /// High and Medium merges that wait for a person.
    pub waiting: u32,
    /// The limit. 8 until a person sets one (`settings_saved` is false then).
    pub limit: u32,
    pub sampling: CapacitySampling,
    /// Whether new Low-priority Crew starts may pause at all. Off until a person turns it on.
    pub pause_enabled: bool,
    /// Whether a person ever saved settings for this project.
    pub settings_saved: bool,
    /// Set while a new Low-priority Crew start would pause now: pausing is on and the waiting count is over the limit.
    #[ts(optional = nullable)]
    pub paused: Option<PausedStart>,
    pub plan: Vec<SamplePlan>,
}
}

wire! {
/// `POST /api/crew/capacity/settings`: a person's capacity settings for one project.
#[serde(deny_unknown_fields)]
pub struct CapacitySettingsInput {
    pub project: String,
    pub limit: u32,
    pub sampling: CapacitySampling,
    pub pause_enabled: bool,
}
}

wire! {
/// `POST /api/crew/capacity/tasks/{id}`: a person sets a task's priority, or whether Crew began it. Either field may be left out.
#[serde(deny_unknown_fields)]
pub struct CapacityTaskInput {
    #[serde(default)]
    #[ts(optional = nullable)]
    pub priority: Option<CapacityPriority>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub crew_started: Option<bool>,
}
}

wire! {
/// The answer to a task write: what is stored now.
pub struct CapacityTaskAnswer {
    pub task: String,
    pub priority: CapacityPriority,
    pub crew_started: bool,
}
}

wire! {
/// Agent-written lines of one area, counted per merge hunk. "Reviewed" means the merge was marked Looks good or
/// Commented: review at merge level, never per line.
pub struct AreaCoverage {
    pub area: String,
    pub merges: u32,
    pub agent_lines: u32,
    pub reviewed_lines: u32,
    /// Lines of a High or Medium merge that still waits for a person.
    pub in_queue_lines: u32,
    /// Merges of this area before provenance began or with no note: counted here, adding no lines.
    pub not_measured_merges: u32,
}
}

wire! {
/// Agent-written lines of one engine and model.
pub struct EngineCoverage {
    pub engine: String,
    #[ts(optional = nullable)]
    pub model: Option<String>,
    pub agent_lines: u32,
    pub reviewed_lines: u32,
}
}

wire! {
/// `GET /api/crew/capacity/coverage?project=&since=`. Lines are those the merges wrote, not lines still on the branch.
pub struct ReviewCoverage {
    pub project: String,
    /// When provenance first began on this device. Absent: it never has, and nothing is measured.
    #[ts(type = "number", optional = nullable)]
    pub began_unix: Option<i64>,
    #[ts(type = "number")]
    pub as_of_unix: i64,
    /// Merges with measured lines.
    pub merges: u32,
    pub not_measured_merges: u32,
    pub agent_lines: u32,
    pub reviewed_lines: u32,
    pub in_queue_lines: u32,
    pub areas: Vec<AreaCoverage>,
    pub engines: Vec<EngineCoverage>,
}
}
