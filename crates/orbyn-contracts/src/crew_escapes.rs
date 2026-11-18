//! The escape log over HTTP (Crew Preview, slice 4k, PR 1; docs/crew/escapes.md): merges that got past review and checks and were
//! later found defective, read-only. Ids, enums, areas, engine and model names and one short reason only: no diff, test body,
//! command output, prompt or secret. Possible and confirmed escapes are always separate numbers; the log is display only and
//! nothing here feeds enforcement. Times are Unix seconds.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire_enum! {
/// Where an escape was found. Every source starts `possible`; only `person_marked` is confirmed at once, because the person's mark
/// is the confirm.
pub enum EscapeSource {
    Revert,
    PersonMarked,
    PostMergeCheck,
    FollowUpFix,
}
}

wire_enum! {
/// Where an escape stands. A revert is `possible` at once and `confirmed` only when a person confirms.
pub enum EscapeState {
    Possible,
    Confirmed,
    Dismissed,
}
}

wire_enum! {
/// The gap the escape shows: a missing test (plain input and output) or a missing repository rule or tool step.
pub enum EscapeGap {
    TestGap,
    ToolingGap,
}
}

wire_enum! {
/// Who set or decided something: the core's own detection or a person.
pub enum EscapeActor {
    Core,
    Person,
}
}

wire_enum! {
/// One kind of row in an escape's audit trail.
pub enum EscapeEventKind {
    Detected,
    Confirmed,
    Dismissed,
    Reclassified,
    CheckProposed,
    CheckFailed,
    CheckProven,
    CheckMerged,
    CheckWithdrawn,
}
}

wire! {
/// One escape.
pub struct EscapeRow {
    pub id: String,
    pub project: String,
    /// The merge it is about (a Review queue item's task id).
    pub merge_task_id: String,
    pub source: EscapeSource,
    pub state: EscapeState,
    pub gap: Option<EscapeGap>,
    pub gap_by: Option<EscapeActor>,
    /// Copied from the merge when the escape was logged; a later change to the merge does not move it.
    pub engine: String,
    /// Absent until a model is recorded for the merge.
    pub model: Option<String>,
    pub areas: Vec<String>,
    pub lane: Option<String>,
    /// At most 200 characters, redacted.
    pub reason: Option<String>,
    /// One plain sentence that says what this is and that a possible escape is not confirmed.
    pub line: String,
    #[ts(type = "number")]
    pub found_unix: i64,
    #[ts(type = "number | null")]
    pub decided_unix: Option<i64>,
    pub decided_by: Option<EscapeActor>,
}
}

wire! {
/// One engine's counts in the window. `possible`, `confirmed` and `dismissed` are never added together. There is no rate in this
/// slice; `low_sample` is true when the engine has fewer merges in the window than the threshold, so no rate is honest yet.
pub struct EscapeEngineCounts {
    pub engine: String,
    pub merges: u32,
    pub possible: u32,
    pub confirmed: u32,
    pub dismissed: u32,
    pub low_sample: bool,
}
}

wire! {
/// The escape log of one project: counts by engine for the window, and one page of rows, newest first.
pub struct EscapeLog {
    pub project: String,
    #[ts(type = "number")]
    pub as_of: i64,
    /// The window the counts cover, in days (the stored `report_days`).
    pub window_days: u32,
    /// Merges in the window.
    pub merges: u32,
    pub possible: u32,
    pub confirmed: u32,
    pub dismissed: u32,
    /// True when the window holds fewer merges than `min_merges`.
    pub low_sample: bool,
    pub min_merges: u32,
    pub engines: Vec<EscapeEngineCounts>,
    /// Rows matching the `state` filter, in all.
    pub total: u32,
    pub rows: Vec<EscapeRow>,
}
}

wire! {
/// The possible escapes of one project (`GET /api/crew/escapes/possible`), newest found first: what the scan found and a person has
/// not decided. Each row says why in its `line` and `reason`; none is confirmed, and none counts as an escape until a person confirms it.
/// A dismissed escape is not listed and is not offered again.
pub struct EscapePossible {
    pub project: String,
    #[ts(type = "number")]
    pub as_of: i64,
    /// Possible escapes in all, whatever the page.
    pub total: u32,
    pub rows: Vec<EscapeRow>,
}
}

wire! {
/// One row of an escape's audit trail, oldest first.
pub struct EscapeEventRow {
    #[ts(type = "number")]
    pub id: i64,
    pub kind: EscapeEventKind,
    pub from_state: Option<EscapeState>,
    pub to_state: Option<EscapeState>,
    pub decided_by: EscapeActor,
    pub cause: String,
    #[ts(type = "number")]
    pub at: i64,
}
}

wire! {
/// One escape and its trail.
pub struct EscapeDetail {
    pub escape: EscapeRow,
    pub events: Vec<EscapeEventRow>,
    /// More events exist than listed.
    pub truncated: bool,
    /// The escape's created check (PR 4), when it has one. Additive: absent for an escape with no check.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub check: Option<EscapeCheckRow>,
}
}

wire! {
/// The constants the escape log reads, as stored on this device (or the defaults). Display only: nothing enforces them.
pub struct EscapeThresholds {
    /// The report window, in days.
    pub report_days: u32,
    /// How long after a merge a follow-up fix counts as a signal, in days.
    pub follow_up_days: u32,
    /// Merges a group needs before a rate is shown.
    pub min_merges: u32,
    /// How many times the baseline a group's escape rate must be to be called out.
    pub rate_multiple: u32,
    /// True when a person's values are stored; false when these are the defaults.
    pub stored: bool,
}
}

wire! {
/// The escape as the person saw it when they acted: its state and its gap. An action on an escape that has since changed is refused
/// with `stale`. An action that is already the escape's state is not stale: repeating it changes nothing.
pub struct EscapeSeen {
    pub state: EscapeState,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub gap: Option<EscapeGap>,
}
}

wire! {
/// A person confirms or dismisses one escape (`POST /api/crew/escapes/{id}/confirm` and `/dismiss`). Only a possible escape can
/// move; the same action twice changes nothing.
pub struct EscapeDecide {
    /// The project the escape belongs to; an escape of another project is not found.
    pub project: String,
    pub seen: EscapeSeen,
    /// At most 200 characters, redacted; kept in the audit row only.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub note: Option<String>,
}
}

wire! {
/// A person marks one merge as an escape (`POST /api/crew/escapes/mark`). The mark is the confirm: the escape is confirmed at once.
/// A merge that already has a possible escape is confirmed; one already confirmed is returned unchanged; a dismissed one is refused.
pub struct EscapeMark {
    pub project: String,
    /// The merge, a Review queue item's task id.
    pub merge_task_id: String,
    /// What the person saw of the merge's escape, if it had one; a possible escape that changed since is `stale`.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub seen: Option<EscapeSeen>,
    /// At most 200 characters, redacted. Becomes the escape's reason when the mark creates it.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub note: Option<String>,
}
}

wire! {
/// A person sets the gap of a possible or confirmed escape (`POST /api/crew/escapes/{id}/gap`).
pub struct EscapeGapSet {
    pub project: String,
    pub seen: EscapeSeen,
    pub gap: EscapeGap,
}
}

wire_enum! {
/// Where a created check stands (slice 4k, PR 4). `proposed` is only a proposal: nothing runs until a person accepts. `writing` is a
/// worker writing the test files. `proven` passed the proof (it fails on the escaped merge and passes on the fix or revert) and has
/// gone to the merge queue like any change. `failed` is a worker that stopped, a diff outside the test files, or a proof that did not
/// hold (including a check that passes on both commits). `withdrawn` and `merged` are final.
pub enum EscapeCheckState {
    Proposed,
    Writing,
    Proven,
    Failed,
    Withdrawn,
    Merged,
}
}

wire! {
/// What the proof found: the two commits it ran the new test files on and whether each result was the one that counts. Only a
/// check with both `true` is proven. Commit ids and two booleans: never a test body or command output.
pub struct EscapeCheckProof {
    /// The merge commit that escaped; the check must fail here.
    pub escaped_commit: String,
    /// The revert or fix commit; the check must pass here.
    pub fix_commit: String,
    pub fails_on_escape: bool,
    pub passes_on_fix: bool,
}
}

wire! {
/// The check of one escape. Paths, a short statement of what it asserts, the command that runs it, ids and enums only.
pub struct EscapeCheckRow {
    pub state: EscapeCheckState,
    /// The one new test file the check adds.
    pub path: String,
    /// What the check asserts, at most 300 characters, redacted.
    pub asserts: String,
    /// The command that runs the test, at most 300 characters.
    pub run: String,
    /// The commit the proof must pass on: the revert or fix.
    pub fix_commit: String,
    /// The crew task of the worker writing it, once a person accepted.
    pub task_id: Option<String>,
    /// Absent until the proof ran.
    pub proof: Option<EscapeCheckProof>,
    /// Why it failed, or what is left to do, at most 200 characters.
    pub note: Option<String>,
    /// How many times a person accepted it.
    pub attempts: u32,
    /// One plain sentence that says where the check stands.
    pub line: String,
    #[ts(type = "number")]
    pub proposed_unix: i64,
    #[ts(type = "number")]
    pub updated_unix: i64,
}
}

wire! {
/// The escape and its check as the person saw them: the escape's state and gap and the check's state (absent when it has none). An
/// act on a check that has since changed is refused with `stale`; an act that is already the check's state changes nothing.
pub struct EscapeCheckSeen {
    pub state: EscapeState,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub gap: Option<EscapeGap>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub check: Option<EscapeCheckState>,
}
}

wire! {
/// A person asks Crew to propose a check for a confirmed escape (`POST /api/crew/escapes/{id}/check`). Nothing runs: the proposal
/// waits for the person's accept. A tooling gap has no proposal.
pub struct EscapeCheckPropose {
    pub project: String,
    pub seen: EscapeCheckSeen,
    /// The one new test file to add: a relative path of a test file (a `tests`, `test`, `__tests__`, `spec`, `specs` or `e2e`
    /// folder, or a name such as `test_x`, `x.test.y`, `x_test.y`), never a policy or manifest file.
    pub path: String,
    /// What the check must assert, in plain words, at most 300 characters.
    pub asserts: String,
    /// The command that runs the test, at most 300 characters. The proof runs it on the escaped merge and on the fix.
    pub run: String,
    /// The commit the check must pass on. Absent: the revert or fix the escape recorded; an escape with none needs one.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub fix_commit: Option<String>,
}
}

wire! {
/// A person accepts, retries or withdraws a check (`POST /api/crew/escapes/{id}/check/accept`, `/retry`, `/withdraw`). Only accept
/// starts a worker.
pub struct EscapeCheckAct {
    pub project: String,
    pub seen: EscapeCheckSeen,
}
}

wire! {
/// What a check route answers: the escape as it now stands and its check.
pub struct EscapeCheckAnswer {
    pub escape: EscapeRow,
    pub check: EscapeCheckRow,
}
}
