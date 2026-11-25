//! Autopilot over HTTP (Crew Preview, slice 4f): what the web screens read and the few answers
//! a person gives. Every type here is a view built by `orbyn-core` from what it already stores.
//! None carries an answer key, a secret or a check; the core scrubs each free-text field and
//! each stored JSON value before it is put in one (H1, H2). Times are Unix seconds unless the
//! name ends in `_at`, which is RFC 3339.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::background::{
    BackgroundBudget, BackgroundRunState, BackgroundStopReason, BackgroundTemplate,
};
use crate::crew::NeedsYouKind;

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
    /// The background runs that stopped without a merge and wait for the person, oldest first, one page. A merge's risk ranks
    /// the `items`; a background run has no risk, commit or diff, so it is listed beside them and carries its own state.
    #[serde(default)]
    #[ts(optional, as = "Option<Vec<AutopilotBackgroundReviewItem>>")]
    pub background_runs: Vec<AutopilotBackgroundReviewItem>,
    /// How many background runs wait in all. `background_runs` is the same `limit` and `offset` window of them as `items` is of
    /// the merges, and `next_offset` is set while either list has more.
    #[serde(default)]
    #[ts(optional, as = "Option<u32>")]
    pub background_total: u32,
    /// The waits the one needs-you field (KC3) names that this queue does not answer: the Inbox's items this queue does not
    /// answer, by kind, most first then by kind. The page says "N more wait for you in the Inbox" from these, and it is the
    /// same field the Inbox and the Taskboard read (not necessarily the same count: the board cards only work tasks). A review
    /// wait this queue owns is not here, and neither is a dismissed one.
    #[serde(default)]
    #[ts(optional, as = "Option<Vec<AutopilotWaitingElsewhere>>")]
    pub waiting_elsewhere: Vec<AutopilotWaitingElsewhere>,
    #[ts(type = "number")]
    pub now_unix: i64,
}
}

wire! {
/// One kind of wait the Review queue does not answer, and how many: the Inbox's items this queue does not answer, from the
/// one needs-you field.
pub struct AutopilotWaitingElsewhere {
    pub kind: NeedsYouKind,
    pub count: u32,
}
}

wire! {
/// One background run in the Review queue: it stopped without a merge and waits for the person. It has no commit, diff or
/// risk; the run's state, why it stopped and what it spent are what a person reads. It is not here while running, and not
/// when the person's own switch stopped it.
pub struct AutopilotBackgroundReviewItem {
    pub run_id: String,
    pub crew_id: String,
    pub project_id: String,
    pub crew_name: String,
    pub template: BackgroundTemplate,
    pub state: BackgroundRunState,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub stop_reason: Option<BackgroundStopReason>,
    /// Why the run waits, in the core's plain words. Absent when the reason is the stored stop reason itself.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub review_reason: Option<String>,
    pub turns_spent: u32,
    pub minutes_spent: u32,
    pub budget: BackgroundBudget,
    pub started_at: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub ended_at: Option<String>,
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
    /// For a revert of a forge merge: the exact commit the corrective PR will push.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub candidate: Option<String>,
    /// For a revert of a forge merge: the forge main commit that candidate was built on and is pinned to.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub base: Option<String>,
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
    /// Entries ahead of it that are queued or blocked on the same branch. A forge entry waiting on a person is not counted: the
    /// lane skips it.
    pub ahead: u32,
    /// The forge merge intent of this entry, when it has one: where it stands and the revision a person's answer must name.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub forge: Option<AutopilotForgeIntent>,
}
}

wire! {
/// Where a forge merge intent stands. `phase` is `prepared`, `publishing`, `awaiting_ci`, `merge_requested`, `merged`,
/// `needs_person` or `rejected`. A `needs_person` intent is answered with its `revision`.
pub struct AutopilotForgeIntent {
    pub phase: String,
    #[ts(type = "number")]
    pub revision: i64,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub pull: Option<u64>,
    /// For a corrective PR: what Core keeps until the correction is complete. Core never deletes the pushed branch on the forge.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub correction: Option<AutopilotCorrectionLeftovers>,
}
}

wire! {
/// The local checkout, object reference and forge branch a corrective PR leaves behind until it is complete and nothing reads it.
pub struct AutopilotCorrectionLeftovers {
    pub checkout: String,
    pub reference: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub branch: Option<String>,
}
}

wire_enum! {
/// What a person does with a forge merge that needs one. `reject` drops it; `retry` sends it back to be checked again.
pub enum CrewForgeAction {
    Reject,
    Retry,
}
}

wire! {
/// A person's answer for a forge merge that needs one (`POST /api/crew/autopilot/merge-queue/{id}/resolve`). `seen_revision` is the
/// intent's revision the person saw; an intent that has moved on is refused with `stale`.
pub struct CrewForgeResolve {
    pub action: CrewForgeAction,
    #[ts(type = "number")]
    pub seen_revision: i64,
}
}

wire! {
/// What the answer did. `outcome` is `rejected`, `retry` (the intent is back in the pipeline) or `merged` (the forge already had the
/// merge, so the intent is recorded as merged and the answer asked for is not applied).
pub struct CrewForgeResolved {
    pub task_id: String,
    pub outcome: String,
    pub phase: String,
    #[ts(type = "number")]
    pub revision: i64,
}
}

wire! {
/// What the queue will do next, from what is stored, and what a dry run of the queued candidates' merges onto the branch
/// says: a candidate that conflicts now is reported before the queue reaches it. The dry run is a local `git merge-tree`;
/// it is not the dependency-graph forecast the plan draws, which needs the router's task graph.
pub struct AutopilotForecast {
    /// The queued entry that merges first when the queue is processed.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub next_task_id: Option<String>,
    pub queued: u32,
    pub blocked: u32,
    pub waiting_for_person: u32,
    /// The queued candidates the dry run covered, in queue order. A candidate whose repository or commit cannot be read
    /// locally is left out rather than guessed at, so this can be smaller than `queued + blocked`.
    pub checked: u32,
    /// The dry run says at least one covered candidate would conflict with the branch as it stands now.
    pub conflict_forecast: bool,
    /// The covered candidates that would conflict now, in queue order: which task, its candidate commit and the files the
    /// dry run reports. Empty when nothing conflicts, or when there was nothing to check locally.
    pub conflicts: Vec<AutopilotForecastConflict>,
}
}

wire! {
/// One queued candidate a dry-run merge says would conflict with the branch now.
pub struct AutopilotForecastConflict {
    pub task_id: String,
    /// The candidate commit the dry run merged onto the branch tip.
    pub merge_commit: String,
    /// The files the dry run reports as conflicting, at most 20.
    pub paths: Vec<String>,
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
/// A watch on main bisecting now, as it stood when the read was made. Progress is kept in memory only, so it lasts while the
/// watch runs: the outcome is stored as the merge rows and the ledger, and a restart keeps the outcome but not the progress.
pub struct AutopilotBisect {
    /// `checking_tip`, `checking_boundary`, `bisecting`, `confirming` or `checking_parent`.
    pub phase: String,
    /// The check runs that have finished, and the most this bisect can run. Each one is a real run of the repository's
    /// checks at that commit.
    pub tested: u32,
    pub steps: u32,
    /// While bisecting, the last commit known to pass and the first known to fail, which bound the cause.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub after: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub before: Option<String>,
    /// The commit whose checks are running now, while one is in flight.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub testing: Option<String>,
    /// Each finished run, oldest first: the commit and whether the checks passed there.
    pub checked: Vec<AutopilotBisectCheck>,
}
}

wire! {
/// One finished check run of a bisect in progress.
pub struct AutopilotBisectCheck {
    pub commit: String,
    /// Whether the repository's checks passed at that commit.
    pub passes: bool,
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
    /// The bisect a watch is running now, or none. It is kept in memory only.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub bisect: Option<AutopilotBisect>,
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
    /// Who decided, when a person or a local caller did: `user` is the person, confirmed in the desktop app; `local_caller` is a
    /// local API caller of the service (a deny, a dismiss or a merge reject), with no proof of who. Absent on rows the core or an engine decided.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub actor: Option<String>,
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
    /// The task's title as it stood when the row was written (the lead thread's title), so the person reads
    /// which work the row was for rather than an id alone. Absent when the title could not be read then.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub title: Option<String>,
    /// The engine of the run that asked, when a run asked: the answer is the same one the gate decided on.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub asker_engine: Option<String>,
    /// The model the asking run's thread was on, when the store knows one. Absent for a run with no model
    /// recorded and for rows no run asked for.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub asker_model: Option<String>,
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
pub struct AutopilotSecondEngineNotice {
    #[ts(type = "number")]
    pub id: i64,
    pub task_id: String,
    pub at: String,
    pub target: String,
    pub lead_engine: Option<String>,
    pub lead_family: Option<String>,
}
}

wire! {
/// Pending notices; continue before next_before's immutable ledger id, even if this page resolved items.
pub struct AutopilotSecondEngineNotices {
    pub items: Vec<AutopilotSecondEngineNotice>,
    #[ts(type = "number | null")]
    pub next_before: Option<i64>,
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

wire! {
/// The daily digest's send time, as Settings › Crew stores it: `HH:MM` in the offset the person saved it in.
/// `17:00` is the default. `utc_offset_minutes` is minutes east of UTC (`660` for UTC+11) and is `null`
/// until a person first saves, because nothing knows the local time before then.
pub struct AutopilotDigestSchedule {
    pub time: String,
    #[ts(type = "number | null")]
    pub utc_offset_minutes: Option<i32>,
}
}

wire! {
/// `POST /api/crew/autopilot/digest/schedule/set`: a person's save. Both fields, so the send time always
/// resolves in their local time; `utc_offset_minutes` is `-new Date().getTimezoneOffset()` in the page.
#[serde(deny_unknown_fields)]
pub struct AutopilotDigestScheduleInput {
    pub time: String,
    pub utc_offset_minutes: i32,
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
