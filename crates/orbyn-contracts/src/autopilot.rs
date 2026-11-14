//! Autopilot over HTTP (Crew Preview, slice 4f): what the web screens read and the few answers
//! a person gives. Every type here is a view built by `orbyn-core` from what it already stores.
//! None carries an answer key, a secret or a check; the core scrubs each free-text field and
//! each stored JSON value before it is put in one (H1, H2). Times are Unix seconds unless the
//! name ends in `_at`, which is RFC 3339.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

wire_enum! {
/// How much Autopilot does: `off`, `on` or `full` (the default).
pub enum AutopilotMode {
    Off,
    On,
    Full,
}
}

wire_enum! {
/// The risk tier of a merge.
pub enum AutopilotRisk {
    Low,
    Medium,
    High,
}
}

wire_enum! {
/// Where a mode is chosen. A crew's choice beats its repository's, which beats the default.
pub enum AutopilotScopeKind {
    Default,
    Repo,
    Crew,
}
}

wire! {
/// One saved choice of mode.
pub struct AutopilotModeEntry {
    pub scope: AutopilotScopeKind,
    /// The project id for `repo`, the crew id for `crew`, absent for `default`.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub id: Option<String>,
    pub mode: AutopilotMode,
    pub updated_at: String,
}
}

wire! {
/// Whether Crew Preview is on. Every Crew route answers 404 when it is off, so this is only ever read as `true`.
pub struct CrewPreviewState {
    pub enabled: bool,
}
}

wire! {
/// The modes the person has chosen, and the one that applies when none is.
pub struct AutopilotModes {
    /// The person has chosen, or kept, the default: a default mode is saved. False shows the first-run banner.
    pub confirmed: bool,
    /// What a scope with no saved choice gets: the saved default, else `full`.
    pub effective_default: AutopilotMode,
    pub entries: Vec<AutopilotModeEntry>,
    /// More choices are saved than listed.
    pub truncated: bool,
}
}

wire! {
/// The body that sets a mode. It goes through the core's own setter; nothing else changes a mode.
pub struct AutopilotModeChange {
    pub scope: AutopilotScopeKind,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub id: Option<String>,
    pub mode: AutopilotMode,
}
}

wire! {
/// Merges and reverts of one area or engine in the window, with whether the sample is big enough to count.
pub struct AutopilotRate {
    pub merges: u32,
    pub reverts: u32,
    /// Reverts per thousand merges.
    pub revert_permille: u32,
    /// `merges` has reached the minimum sample, so the risk score reads this rate.
    pub counts: bool,
}
}

wire! {
pub struct AutopilotRateRow {
    pub name: String,
    pub rate: AutopilotRate,
}
}

wire! {
/// The trust record the risk score reads: revert rates by area and by engine over the window, on this device.
pub struct AutopilotTrust {
    pub window_days: u32,
    pub min_sample: u32,
    pub high_revert_permille: u32,
    pub total: AutopilotRate,
    pub areas: Vec<AutopilotRateRow>,
    pub engines: Vec<AutopilotRateRow>,
    /// The circuit breaker lowers an area from Full to On, and the merge gate reads it (slice 4j). Earning a higher mode is not built.
    pub autonomy_by_area: bool,
    pub truncated: bool,
}
}

wire! {
/// Why a merge has the risk it has, with the sample sizes the score read.
pub struct AutopilotRiskReport {
    pub task_id: String,
    /// `queued`, `blocked`, `waiting`, `merging`, `rejected` or `merged`.
    pub state: String,
    pub risk: AutopilotRisk,
    pub reasons: Vec<String>,
    pub samples: Vec<String>,
    pub areas: Vec<String>,
    pub engine: String,
    pub files: u32,
    pub lines: u32,
    pub mode: String,
    /// Why it is held or blocked, when it is.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub note: Option<String>,
    /// What the score does not measure.
    pub not_measured: Vec<String>,
    /// The mode each touched area is on, with the record behind it (slice 4j, enforcement).
    #[serde(default)]
    #[ts(optional, as = "Option<Vec<AutopilotAreaMode>>")]
    pub area_modes: Vec<AutopilotAreaMode>,
}
}

wire! {
/// One area a merge touched and the mode it is on, with a sentence naming the record: "Area web/app is on On: 2 reverts in 14 days · 9 merges".
pub struct AutopilotAreaMode {
    /// The area, scrubbed for display.
    pub area: String,
    /// The mode the merge gate reads for the area: the lower of the ceiling and what the area's record or a person's override allows.
    pub mode: AutopilotMode,
    pub line: String,
}
}

wire! {
/// One merge in the after-merge Review queue.
pub struct AutopilotReviewItem {
    pub task_id: String,
    pub risk: AutopilotRisk,
    pub reasons: Vec<String>,
    pub merge_commit: String,
    pub before_commit: String,
    pub candidate: String,
    pub files: u32,
    pub lines: u32,
    #[ts(type = "number")]
    pub merged_unix: i64,
    #[serde(default)]
    #[ts(type = "number | null", optional)]
    pub due_unix: Option<i64>,
    pub overdue: bool,
    /// The checks, the reviewers by engine, the findings and what was fixed.
    pub proof: Value,
    pub not_verified: Vec<String>,
    /// The task's own usage as its result recorded it.
    pub cost: Value,
    /// `unknown`, `green` or `broken`.
    pub watch: String,
    /// A break the crew was not certain enough to revert; the person decides.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub needs_person: Option<String>,
    /// The areas of this merge that are lowered by the circuit breaker now, each with its record. Empty when none is.
    #[serde(default)]
    #[ts(optional, as = "Option<Vec<AutopilotAreaMode>>")]
    pub area_modes: Vec<AutopilotAreaMode>,
}
}

wire! {
pub struct AutopilotReviewQueue {
    pub items: Vec<AutopilotReviewItem>,
    pub total: u32,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub next_offset: Option<u32>,
    #[ts(type = "number")]
    pub now_unix: i64,
}
}

wire! {
/// A later merge on the same branch that a revert has to leave alone or ask about.
pub struct AutopilotLaterMerge {
    pub task_id: String,
    pub merge_commit: String,
    pub risk: AutopilotRisk,
}
}

wire! {
/// What a revert would meet, read before the person asks for it. The check that decides is the core's: it makes the revert in a
/// scratch checkout and runs the repository's checks on the result, and it refuses when a later merge needs the one reverted.
pub struct AutopilotRevertCheck {
    pub task_id: String,
    pub merge_commit: String,
    pub already_reverted: bool,
    /// Merges after this one on the same branch, oldest first. Any of them may depend on it.
    pub later_merges: Vec<AutopilotLaterMerge>,
    pub truncated: bool,
}
}

wire! {
/// The body of an answer to a merge in the Review queue. `merge_commit` must be the one the person saw, so an answer
/// to a merge that has changed since is refused.
pub struct AutopilotAnswerBody {
    pub merge_commit: String,
    /// The comment, for the comment answer only: 1 to 2000 characters.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub text: Option<String>,
}
}

wire! {
pub struct AutopilotAnswered {
    pub task_id: String,
    /// `looks_good`, `commented` or `reverted`.
    pub review_state: String,
    /// The commit that reverted the merge, for a revert.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub revert_commit: Option<String>,
}
}

wire! {
/// One task asked to merge.
pub struct AutopilotQueueEntry {
    pub task_id: String,
    pub crew_id: String,
    /// `queued`, `blocked`, `waiting`, `merging`, `merged` or `rejected`.
    pub state: String,
    pub risk: AutopilotRisk,
    pub reasons: Vec<String>,
    pub areas: Vec<String>,
    pub samples: Vec<String>,
    pub engine: String,
    pub files: u32,
    pub lines: u32,
    pub mode: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub note: Option<String>,
    /// A person let it go.
    pub released: bool,
    #[ts(type = "number")]
    pub queued_unix: i64,
    /// Entries ahead of it that are queued or blocked on the same branch.
    pub ahead: u32,
}
}

wire! {
/// What the queue will do next, from what is stored. Conflicts are found when a merge is tried, not forecast.
pub struct AutopilotForecast {
    /// The queued entry that merges first when the queue is processed.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub next_task_id: Option<String>,
    pub queued: u32,
    pub blocked: u32,
    pub waiting_for_person: u32,
    /// There is no conflict forecast from the dependency graph yet.
    pub conflict_forecast: bool,
}
}

wire! {
pub struct AutopilotMergeQueue {
    pub entries: Vec<AutopilotQueueEntry>,
    pub total: u32,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub next_offset: Option<u32>,
    pub forecast: AutopilotForecast,
}
}

wire! {
/// What the watch on main found for one merge.
pub struct AutopilotWatchRow {
    pub task_id: String,
    pub merge_commit: String,
    pub risk: AutopilotRisk,
    /// `unknown` (not watched yet), `green` or `broken`.
    pub watch: String,
    /// `waiting`, `looks_good`, `commented` or `reverted`.
    pub review_state: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub needs_person: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub reverted_commit: Option<String>,
    #[ts(type = "number")]
    pub merged_unix: i64,
}
}

wire! {
pub struct AutopilotWatch {
    pub rows: Vec<AutopilotWatchRow>,
    pub total: u32,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub next_offset: Option<u32>,
    pub broken: u32,
    pub unwatched: u32,
    /// The latest breaks, reverts and notices, newest first.
    pub recent: Vec<AutopilotLogRow>,
    /// Bisect progress is not stored; the outcome is, as the rows above and `recent`.
    pub bisect_progress: bool,
}
}

wire! {
/// A lead's or a second key's answer to a request.
pub struct AutopilotAnswer {
    pub approve: bool,
    pub reason: String,
}
}

wire! {
pub struct AutopilotSecondKey {
    pub engine: String,
    pub approve: bool,
    pub reason: String,
}
}

wire! {
/// One row of the approval ledger, redacted when it was written and again when it is read.
pub struct AutopilotLogRow {
    #[ts(type = "number")]
    pub id: i64,
    pub task_id: String,
    pub crew_id: String,
    /// `allowed`, `denied`, `person`, `disagreement`, `tripwire`, `merge_queued`, `merge_waiting`, `merged`,
    /// `merge_rejected`, `main_broken`, `reverted`, `person_told`, `held`, `approved_once`, `denied_by_person`,
    /// `dismissed_by_person`, `hold_used`, `hold_ended` or `command_exit`.
    pub kind: String,
    pub at: String,
    /// `hard_rule`, `tier`, `lead`, `two_keys`, `person` and so on, when the row says.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub decided_by: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub tier: Option<String>,
    /// The always-denied rule, such as `N5`.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub rule: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub mode: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub action_kind: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub target: Option<String>,
    pub paths: Vec<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub lead: Option<AutopilotAnswer>,
    /// The second key's answer, when the action needed two.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub second: Option<AutopilotSecondKey>,
    /// The second key was missing, so the person is told to sign in to a second engine.
    pub needs_second_engine: bool,
    /// The row as stored, cleaned.
    pub body: Value,
}
}

wire! {
pub struct AutopilotLog {
    pub rows: Vec<AutopilotLogRow>,
    pub total: u32,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub next_offset: Option<u32>,
}
}

wire! {
/// A running task and how close it is to a trip wire.
pub struct AutopilotTripwireTask {
    pub task_id: String,
    pub crew_id: String,
    pub state: String,
    pub in_a_row: u32,
    pub in_a_run: u32,
}
}

wire! {
pub struct AutopilotTripwires {
    pub in_a_row_limit: u32,
    pub in_a_run_limit: u32,
    pub running: Vec<AutopilotTripwireTask>,
    /// The latest wires that fired, newest first.
    pub recent: Vec<AutopilotLogRow>,
}
}

wire! {
pub struct AutopilotRule {
    pub id: String,
    pub rule: String,
}
}

wire! {
pub struct AutopilotReviewWindow {
    pub risk: AutopilotRisk,
    /// `None`: batched, no deadline.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub hours: Option<u32>,
}
}

wire! {
/// The limits Autopilot works within. All are fixed in code; a person cannot raise them from here.
pub struct AutopilotGuards {
    pub always_denied: Vec<AutopilotRule>,
    pub in_a_row_limit: u32,
    pub in_a_run_limit: u32,
    pub review_windows: Vec<AutopilotReviewWindow>,
    pub large_files: u32,
    pub large_lines: u32,
    pub high_revert_permille: u32,
    pub min_sample: u32,
    pub window_days: u32,
    pub default_mode: AutopilotMode,
    /// The engine that answers for the lead, when one is wired.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub lead_engine: Option<String>,
    /// Engines that may be a second key.
    pub second_key_engines: Vec<String>,
}
}

wire! {
pub struct AutopilotDigestMerge {
    pub task_id: String,
    pub risk: AutopilotRisk,
    pub merge_commit: String,
    #[ts(type = "number")]
    pub merged_unix: i64,
}
}

wire! {
/// What merged, what waits for review and what was reverted in a window.
pub struct AutopilotDigest {
    #[ts(type = "number")]
    pub since_unix: i64,
    #[ts(type = "number")]
    pub until_unix: i64,
    pub merged: Vec<AutopilotDigestMerge>,
    pub merged_count: u32,
    pub reverted: Vec<AutopilotDigestMerge>,
    pub reverted_count: u32,
    /// Merges waiting for the person right now, whenever they merged.
    pub waiting_review: u32,
    pub overdue: u32,
    pub needs_person: u32,
    /// Requests denied and wires fired in the window.
    pub denied: u32,
    pub tripwires: u32,
    /// Areas whose mode changed in the window: lowered, returned, or a person's override set or cleared. Newest first.
    #[serde(default)]
    #[ts(optional, as = "Option<Vec<AutopilotAreaChanged>>")]
    pub areas_changed: Vec<AutopilotAreaChanged>,
    /// Nothing happened, so no notification is due.
    pub quiet: bool,
}
}

wire_enum! {
/// Where a held command stands. A held command is decided once: `approved`, `denied` or `dismissed`. An approval is then
/// `used` by the one retry it covers, or ends unused as `expired`.
pub enum AutopilotHeldState {
    Held,
    Approved,
    Denied,
    Dismissed,
    Used,
    Expired,
}
}

wire! {
/// A command Autopilot held for the person. `command` is redacted, with the answer key's text withheld, and is what the person
/// reads before approving; when it was too long to show whole, `readable` is false and approving once is refused. The hold is
/// bound to the exact command, its arguments and the worker's thread, which this view does not carry.
pub struct AutopilotHeld {
    pub id: String,
    pub task_id: String,
    pub crew_id: String,
    pub kind: String,
    pub command: String,
    pub reason: String,
    pub state: AutopilotHeldState,
    pub readable: bool,
    /// Seconds since it was held.
    pub age_secs: u32,
    /// Seconds left to decide it (`held`), or to use the approval (`approved`); 0 once it ended.
    pub expires_in_secs: u32,
}
}

wire! {
/// The held commands that are still open or were decided lately, newest first.
pub struct AutopilotHeldList {
    pub items: Vec<AutopilotHeld>,
    /// How long a hold waits for a decision, and how long an approval lasts, in seconds.
    pub hold_secs: u32,
    pub approval_secs: u32,
}
}

wire_enum! {
/// Where an area stands under the circuit breaker. `demoted` (just lowered, nothing clean yet), `recovering` (lowered, the way back in progress) and
/// `criteria_met` (the way back is met; it returns at the next merge) are read by the merge gate.
pub enum AutopilotAreaState {
    Follows,
    Demoted,
    Recovering,
    CriteriaMet,
    NotEnoughData,
    CeilingLower,
    Overridden,
}
}

wire! {
/// The record the breaker reads for one area: merges made in the window and how many of them were reverted. Escapes are not measured.
pub struct AutopilotAreaRecord {
    pub merges: u32,
    pub reverts: u32,
    /// Always absent: there is no escape backend, so nothing counts escapes.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub escapes: Option<u32>,
    pub window_days: u32,
}
}

wire! {
/// What a lowered area still needs to return to its ceiling: days without a revert and merges since, none reverted.
pub struct AutopilotAreaRecovery {
    pub clean_days: u32,
    pub days_needed: u32,
    pub merges: u32,
    pub merges_needed: u32,
}
}

wire! {
/// A person's override of an area: `keep_ceiling` ignores the breaker's lowering, `off` makes every merge in the area wait.
pub struct AutopilotAreaOverride {
    /// `keep_ceiling` or `off`.
    pub choice: String,
    #[ts(type = "number")]
    pub at: i64,
}
}

wire! {
/// One area of a project: the ceiling, what the breaker would make of it now, and why, in a sentence.
pub struct AutopilotAreaRow {
    /// The first two folders of the paths merges touched, or the first one, or the file at the root, scrubbed for display.
    pub area: String,
    /// The area as stored: the name to pass as `area` to the history route.
    pub area_key: String,
    pub ceiling: AutopilotMode,
    /// The mode the merge gate reads for the area, before the crew's own mode is applied.
    pub now: AutopilotMode,
    pub state: AutopilotAreaState,
    pub line: String,
    /// When the breaker lowered the area, for `demoted`, `recovering` and `criteria_met`.
    #[serde(default)]
    #[ts(type = "number | null", optional)]
    pub since: Option<i64>,
    pub record: AutopilotAreaRecord,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub recovery: Option<AutopilotAreaRecovery>,
    #[serde(default, rename = "override")]
    #[ts(optional = nullable)]
    pub person_override: Option<AutopilotAreaOverride>,
}
}

wire! {
/// The areas of one project under the circuit breaker (slice 4j). Read-only; the thresholds are proposed.
pub struct AutopilotAreas {
    pub project: String,
    /// The repository's mode, else the default, else `full`: the most the breaker can lower from.
    pub ceiling: AutopilotMode,
    #[ts(type = "number")]
    pub as_of: i64,
    /// True: the merge gate reads each area's mode. Earning a higher mode is not built.
    pub enforced: bool,
    pub total: u32,
    pub rows: Vec<AutopilotAreaRow>,
}
}

wire! {
/// One row of an area's trail, newest first.
pub struct AutopilotAreaEvent {
    #[ts(type = "number")]
    pub id: i64,
    /// `demoted`, `recovery_reset`, `restored`, `override_set` or `override_cleared`.
    pub kind: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub from_mode: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub to_mode: Option<String>,
    /// `breaker` or `person`.
    pub decided_by: String,
    /// The revert commit, `restore:<recovery start>` or `person:<audit id>`.
    pub cause: String,
    pub record: AutopilotAreaRecord,
    #[ts(type = "number")]
    pub at: i64,
}
}

wire! {
/// The trail of one area.
pub struct AutopilotAreaHistory {
    pub project: String,
    pub area: String,
    pub events: Vec<AutopilotAreaEvent>,
    /// More rows exist than listed.
    pub truncated: bool,
}
}

wire! {
/// The breaker's constants, served as they are. They are Engineering's proposal, not yet confirmed.
pub struct AutopilotThresholds {
    pub window_days: u32,
    /// Merges the window needs before the breaker decides anything.
    pub min_merges: u32,
    /// Reverted merges in the window that lower an area.
    pub trip_reverts: u32,
    /// Days without a revert before a lowered area can return.
    pub recovery_days: u32,
    /// Merges since the lowering, none reverted, before it can return.
    pub recovery_merges: u32,
    pub proposed: bool,
}
}

wire! {
/// The record a person saw when they chose: the merges and reverts in the window and when the breaker lowered the area. A choice made
/// on an old record is refused with `stale`.
pub struct AutopilotAreaSeen {
    pub merges: u32,
    pub reverts: u32,
    #[serde(default)]
    #[ts(type = "number | null", optional)]
    pub tripped_at: Option<i64>,
}
}

wire! {
/// A person's override of one area. Never raises the mode above the ceiling.
pub struct AutopilotAreaOverrideBody {
    pub project: String,
    /// The area as stored (`area_key` of the areas read).
    pub area: String,
    /// `keep_ceiling` or `off`.
    pub choice: String,
    /// Must be true: the person read what the choice does.
    pub acknowledged: bool,
    pub seen: AutopilotAreaSeen,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub reason: Option<String>,
}
}

wire! {
/// A person's return of one area to following the record.
pub struct AutopilotAreaFollowBody {
    pub project: String,
    pub area: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub seen: Option<AutopilotAreaSeen>,
}
}

wire! {
/// One change of an area's mode in a digest window.
pub struct AutopilotAreaChanged {
    pub project: String,
    pub area: String,
    /// `demoted`, `restored`, `override_set` or `override_cleared`.
    pub kind: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub from_mode: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub to_mode: Option<String>,
    /// `breaker` or `person`.
    pub decided_by: String,
    #[ts(type = "number")]
    pub at: i64,
}
}
