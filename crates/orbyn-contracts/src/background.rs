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
    Watch,
}
}

wire_enum! {
/// What a `watch` crew does when its command turns failing: wait for the person (`needs_you`, the default behaviour) or start
/// its template's run (`template`).
pub enum BackgroundWatchAction {
    NeedsYou,
    Template,
}
}

wire! {
/// The settings of a trigger. Only the fields of its kind may be set, and the ones marked required must be:
/// `ci_failure` takes `branch` and `workflow` (both optional, absent means any), `issue_label` needs `label`,
/// `schedule` needs `every_hours` (1 to 720), `dependency_update` takes `ecosystem` (optional) and `watch` needs
/// `command` (1 to 500 characters), `every_hours` (1 to 720) and `on_failing` (`needs_you` or `template`). Any other field is refused.
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
    #[serde(default)]
    #[ts(optional = nullable)]
    pub command: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub on_failing: Option<BackgroundWatchAction>,
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

wire! {
/// The most one engine may spend on this crew in a day, set when the crew is created and never changed after: runs and turns,
/// counted per engine from the turns its runs ended with. The day is the person's (`utc_offset_minutes` of the create body), so the
/// budget resets at their midnight. An engine at its cap refuses new runs of the crew; a run already going is cancelled at the cap
/// with what the engine spent shown, never clamped.
#[serde(deny_unknown_fields)]
pub struct BackgroundDailyBudget {
    /// Runs 1 to 1000 for a new crew; 0 with 0 turns means no daily budget (a crew made before it).
    pub runs: u32,
    /// Turns 1 to 10000 for a new crew; 0 with 0 runs means no daily budget.
    pub turns: u32,
}
}

wire_enum! {
/// Where a background crew stands, in this order: `running` while a run is in progress; `needs_review` when its newest run needs
/// review; `stopped` when its newest run stopped for the budget or a failure (`budget`, `repeated_failure`, `engine_error`), even with
/// older reviews waiting; `needs_review` again when it has no run, or its newest run was stopped as `disabled`, while an older review
/// waits; otherwise `idle`.
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
    /// What each engine may spend on this crew in a day, reset at the person's midnight.
    pub daily: BackgroundDailyBudget,
    /// The person's clock, in minutes east of UTC (-840 to 840): the daily budget resets at their midnight.
    pub utc_offset_minutes: i32,
}
}

wire! {
/// The body that changes a background crew: its name and its trigger. The repository, template, budget and daily budget are fixed at
/// creation.
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
    /// What each engine may spend on this crew in a day, fixed at creation like the run budget.
    pub daily: BackgroundDailyBudget,
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
    /// Why a run never started, such as `disk_low`, and absent once a run starts: a not-started run is `stopped`
    /// with this code and no stop reason.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub reason_code: Option<String>,
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

wire! {
/// One run with the crew it belongs to, for "Open the run": the run's own state, times, reason and spend, and the crew's
/// name and template. The run read is one run, never a page.
pub struct BackgroundRunDetail {
    pub run: BackgroundRun,
    pub crew_id: String,
    /// The crew's name, shown the way every crew read shows it.
    pub name: String,
    pub template: BackgroundTemplate,
}
}

wire_enum! {
/// Where a watch's last check stands. The first check that runs is a baseline and raises nothing; after it, only a turn between
/// passing and failing is a change. Three checks in a row that could not run raise one Needs you row, and a check that runs
/// again resolves it.
pub enum BackgroundWatchState {
    NotChecked,
    Passing,
    Failing,
    CouldNotRun,
}
}

wire! {
/// A watch's last check: its state, when it last checked, what the check found and the redacted tail it kept. The command is the
/// crew's own trigger setting and is not repeated here.
pub struct BackgroundWatchStatus {
    pub crew_id: String,
    pub state: BackgroundWatchState,
    /// When the last check ran (or could not run), as RFC 3339; absent before the first one.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub last_checked_at: Option<String>,
    /// The exit code of the last check that ran, when it had one.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub last_exit_code: Option<i32>,
    /// Why the last check could not run: `timed out`, `could not start`, `refused`, `folder not trusted` or `printed too much`. Absent when it ran.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub reason: Option<String>,
    /// Whether a first check has run and set the baseline. A baseline never raises.
    pub baseline_done: bool,
    /// How many checks in a row could not run. Three raise one Needs you row.
    pub error_streak: u32,
    /// The last check's output, redacted, the last 20 lines and at most 2 KiB.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub tail: Option<String>,
}
}

wire_enum! {
/// What one row of a watch's change history records: the first check, a turn to failing, a recovery, or the person turning the
/// watch off. A different failure while it stays failing, a check that could not run, and a repeat of the same result are not rows.
pub enum BackgroundWatchChangeKind {
    Baseline,
    Failing,
    Passing,
    TurnedOff,
}
}

wire_enum! {
/// What Crew did when that change happened: raised the Needs you row, started the crew's template, resolved the row, or nothing.
pub enum BackgroundWatchChangeAction {
    NeedsYou,
    StartedTemplate,
    Resolved,
    None,
}
}

wire! {
/// One row of a watch's change history, with the exit code and redacted tail the check had at that change.
pub struct BackgroundWatchChange {
    #[ts(type = "number")]
    pub id: i64,
    pub kind: BackgroundWatchChangeKind,
    pub action: BackgroundWatchChangeAction,
    /// Why a template start was refused, as the fixed sentence core owns; absent when nothing was refused.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub reason: Option<String>,
    /// The fixed code behind `reason` (`disk_low`, `no_engine`, `folder_untrusted`, `run_in_progress`, `not_allowed`,
    /// `crew_gone` or `refused`); absent when nothing was refused.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub reason_code: Option<String>,
    /// When the change happened, as RFC 3339.
    pub at: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub exit_code: Option<i32>,
    /// The check's output at the change, redacted, the last 20 lines and at most 2 KiB.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub tail: Option<String>,
}
}

wire! {
/// A watch's changes, newest first, one page.
pub struct BackgroundWatchChanges {
    pub changes: Vec<BackgroundWatchChange>,
    pub total: u32,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub next_offset: Option<u32>,
}
}
