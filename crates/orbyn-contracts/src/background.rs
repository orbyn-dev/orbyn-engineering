//! Background crews over HTTP (Crew Preview, slice 4h-state): a crew the person defines once for a repository, with the template it
//! follows, what would start it, a run budget set at creation and an on/off switch. They are kept in the service's data folder,
//! never in the repository. This slice stores and serves them; no trigger fires and no engine starts (slice 4h).
//!
//! Every free-text field has been through the locked redactor before it is stored. Times are RFC 3339 UTC strings.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire_enum! {
/// What a background crew does when it starts.
pub enum BackgroundTemplate {
    FixFailingCi,
    FlakyTest,
    DependencyBump,
    SecurityAdvisory,
}
}

wire_enum! {
/// What would start a background crew. Nothing fires in this slice.
pub enum BackgroundTriggerKind {
    CiFailure,
    IssueLabel,
    Schedule,
    DependencyUpdate,
}
}

wire! {
/// The settings of a trigger. Only the fields of its kind may be set, and the ones marked required must be:
/// `ci_failure` takes `branch` and `workflow` (both optional, absent means any), `issue_label` needs `label`,
/// `schedule` needs `every_hours` (1 to 720) and `dependency_update` takes `ecosystem` (optional). Any other field is refused.
#[serde(deny_unknown_fields)]
pub struct BackgroundTrigger {
    pub kind: BackgroundTriggerKind,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub branch: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub workflow: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub label: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub every_hours: Option<u32>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub ecosystem: Option<String>,
}
}

wire! {
/// The most one run may spend, set when the crew is created and never changed after. A run that reaches either number stops with
/// the reason `budget` and goes to review.
#[serde(deny_unknown_fields)]
pub struct BackgroundBudget {
    /// Agent turns, 1 to 1000.
    pub turns: u32,
    /// Minutes of work, 1 to 1440.
    pub minutes: u32,
}
}

wire_enum! {
/// Where a background crew stands: `running` while a run is in progress, `needs_review` when a run waits for review, `stopped` when
/// its newest run stopped for the budget or a failure, and `idle` otherwise: nothing running, nothing waiting, and the newest run (if
/// any) was stopped by the person turning the crew off.
pub enum BackgroundState {
    Idle,
    Running,
    Stopped,
    NeedsReview,
}
}

wire_enum! {
/// Why a run stopped.
pub enum BackgroundStopReason {
    Budget,
    RepeatedFailure,
    EngineError,
    /// The person turned the crew off while it ran.
    Disabled,
}
}

wire! {
/// The body that creates a background crew. It starts off: turning it on is its own step.
#[serde(deny_unknown_fields)]
pub struct BackgroundCrewCreate {
    /// The project (repository) it works on.
    pub project_id: String,
    /// 1 to 80 characters, unique within the project.
    pub name: String,
    pub template: BackgroundTemplate,
    pub trigger: BackgroundTrigger,
    pub budget: BackgroundBudget,
}
}

wire! {
/// The body that changes a background crew: its name and its trigger. The repository, template and budget are fixed at creation.
#[serde(deny_unknown_fields)]
pub struct BackgroundCrewUpdate {
    #[serde(default)]
    #[ts(optional = nullable)]
    pub name: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub trigger: Option<BackgroundTrigger>,
}
}

wire! {
/// One background crew.
pub struct BackgroundCrew {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub template: BackgroundTemplate,
    pub trigger: BackgroundTrigger,
    pub budget: BackgroundBudget,
    pub enabled: bool,
    pub state: BackgroundState,
    /// Runs of this crew waiting for the person's review.
    pub reviews_waiting: u32,
    pub created_at: String,
    pub updated_at: String,
}
}

wire! {
/// The background crews, one page.
pub struct BackgroundCrewList {
    pub crews: Vec<BackgroundCrew>,
    pub total: u32,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub next_offset: Option<u32>,
}
}

wire_enum! {
/// Where one run stands.
pub enum BackgroundRunState {
    Running,
    Stopped,
    NeedsReview,
}
}

wire! {
/// One run of a background crew: its state, what it spent and the budget it had.
pub struct BackgroundRun {
    pub id: String,
    pub crew_id: String,
    pub state: BackgroundRunState,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub stop_reason: Option<BackgroundStopReason>,
    pub turns_spent: u32,
    pub minutes_spent: u32,
    pub budget: BackgroundBudget,
    /// The run is in the review queue, waiting for the person.
    pub review_waiting: bool,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub review_reason: Option<String>,
    pub started_at: String,
    pub updated_at: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub ended_at: Option<String>,
}
}

wire! {
/// The runs of one background crew, newest first, one page.
pub struct BackgroundRunList {
    pub runs: Vec<BackgroundRun>,
    pub total: u32,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub next_offset: Option<u32>,
}
}
