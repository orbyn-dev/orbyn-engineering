//! Threads, queued messages, mentions, slash commands and message search.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use ts_rs::TS;

use crate::{AutoSettleChoice, Run, RunState, Worktree};

wire! {
pub struct Thread {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub engine: String,
    pub model: Option<String>,
    pub cwd: String,
    pub worktree: Option<Worktree>,
    pub skill_profile: Option<String>,
    pub active_run: Option<Run>,
    pub last_run_state: Option<RunState>,
    pub created_at: String,
    pub updated_at: String,
    /// Always present from the service; optional so older payloads still parse.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lifecycle: Option<ThreadLifecycle>,
    /// Reasoning effort for this thread's turns; `None` is the engine's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub effort: Option<String>,
    /// Choices beyond model and effort, by `ModelOption` id, such as `service_tier`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<String, String>>")]
    pub options: BTreeMap<String, String>,
    /// The goal the engine is working toward, as last reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub goal: Option<ThreadGoal>,
    /// This thread works in a scratch folder rather than a project.
    #[serde(default)]
    pub scratch: bool,
    /// The project was removed from Orbyn. The thread stays, archived, and cannot be
    /// unarchived until the folder is added back. Absent unless true.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(as = "Option<bool>", optional)]
    pub project_removed: bool,
    /// The removed project's name, kept so the archive can still say where it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub removed_project_name: Option<String>,
}
}

wire! {
pub struct ThreadGoal {
    pub objective: String,
    /// `active`, `paused`, `blocked`, `usage_limited`, `budget_limited` or `complete`.
    pub status: String,
    pub tokens_used: Option<u64>,
    pub token_budget: Option<u64>,
    pub seconds_used: Option<u64>,
}
}

wire! {
pub struct ThreadLifecycle {
    pub pinned_at: Option<String>,
    pub archived_at: Option<String>,
    pub settled_at: Option<String>,
    pub snoozed_until: Option<String>,
    pub unread: bool,
    /// `None` follows the settings; `Some` is the thread's own choice.
    pub auto_settle: Option<bool>,
    /// Lower sorts first. Thread activity never changes it.
    pub position: f64,
    pub parent_thread_id: Option<String>,
    /// `fork`, `side` or `subagent` when `parent_thread_id` is set.
    pub relation: Option<String>,
    pub permission_mode: PermissionMode,
    pub plan_mode: bool,
    /// Set when something ended a snooze before its time, until you dismiss it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub woke_at: Option<String>,
    /// The engine stopped on a usage or rate limit and nothing has run since.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub limit: Option<ThreadLimit>,
    /// When you were last in this thread.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub visited_at: Option<String>,
    /// A pull request is being watched for this thread.
    #[serde(default)]
    #[ts(optional, as = "Option<bool>")]
    pub watching: bool,
}
}

wire! {
pub struct ThreadLimit {
    pub message: String,
    pub stopped_at: String,
    /// When the engine says the allowance returns; `None` if it did not say.
    pub resets_at: Option<String>,
    /// When a continuation is scheduled; `None` means it waits for you.
    pub resume_at: Option<String>,
}
}

wire_enum! {
pub enum PermissionMode {
    /// Asks before commands and edits.
    Supervised,
    /// Accepts edits inside the checkout.
    AutoEdits,
    /// Also accepts commands.
    Auto,
    /// Accepts everything policy does not refuse. Writes stay in the checkout.
    FullAccess,
}
}

impl PermissionMode {
    /// Every mode, loosest last.
    pub const ALL: [PermissionMode; 4] = [
        PermissionMode::Supervised,
        PermissionMode::AutoEdits,
        PermissionMode::Auto,
        PermissionMode::FullAccess,
    ];
}

wire! {
pub struct QueuedMessage {
    pub id: String,
    pub thread_id: String,
    pub prompt: String,
    pub attachment_ids: Vec<String>,
    /// Held messages wait for a resume after a stop, failure or restart.
    pub held: bool,
    pub created_at: String,
    /// What this message sets on the thread when it starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<String, String>>")]
    pub options: BTreeMap<String, String>,
}
}

wire! {
pub struct EnqueueMessage {
    pub prompt: String,
    #[serde(default)]
    pub attachment_ids: Vec<String>,
    /// Set on the thread when this message starts, and it stays for later turns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<String, String>>")]
    pub options: BTreeMap<String, String>,
}
}

wire! {
pub struct EditQueuedMessage {
    pub prompt: String,
}
}

wire! {
pub struct SteerRun {
    pub prompt: String,
    /// A running turn keeps its own model; these apply from the next turn on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<String, String>>")]
    pub options: BTreeMap<String, String>,
}
}

wire! {
pub struct Attachment {
    pub id: String,
    pub thread_id: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub created_at: String,
}
}

wire! {
/// One engine item as the engine reported it, for the item inspector.
pub struct ThreadItemRecord {
    pub item_id: String,
    pub run_id: Option<String>,
    pub kind: String,
    pub raw: Value,
    pub exit_code: Option<i64>,
    pub updated_at: String,
}
}

wire! {
pub struct ThreadUsage {
    pub total_tokens: Option<u64>,
    pub context_window: Option<u64>,
    /// The engine's own report, which varies by engine.
    pub raw: Value,
}
}

wire! {
pub struct ForkThread {
    /// Fork after this run; absent forks the whole conversation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    /// A side chat: allowed while the parent runs, and kept out of the thread list.
    #[serde(default)]
    pub side: bool,
    /// Forks from just before `run_id` rather than just after it.
    #[serde(default)]
    pub before: bool,
}
}

wire! {
pub struct SwitchEngine {
    pub engine: String,
    /// Absent uses what was last chosen on that engine.
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<String, String>>")]
    pub options: BTreeMap<String, String>,
}
}

wire! {
pub struct Mentions {
    pub skills: Vec<String>,
    /// Paths relative to the checkout, best match first.
    pub files: Vec<String>,
    /// Other threads in this project.
    #[serde(default)]
    pub threads: Vec<ThreadMention>,
}
}

wire! {
pub struct ThreadMention {
    pub id: String,
    pub title: String,
}
}

wire! {
pub struct SlashCommand {
    pub name: String,
    pub description: String,
    /// `orbyn` commands the service or client handles, `engine` commands sent to
    /// the engine as typed, `agent` for a subagent reached by mentioning it, or
    /// `skill`.
    #[serde(default)]
    pub source: String,
}
}

wire! {
/// `PATCH /api/threads/:id`. Absent fields are left alone.
#[derive(Default)]
pub struct UpdateThread {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pinned: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub archived: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub settled: Option<bool>,
    /// RFC 3339 time in the future.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub snoozed_until: Option<String>,
    /// True ends a snooze early.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub wake: Option<bool>,
    /// True records that you are reading the thread now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub visited: Option<bool>,
    /// True clears the note that a snooze ended early.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub dismiss_woke: Option<bool>,
    /// True schedules a continuation for the limit's reset; false cancels it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub resume_at_reset: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub unread: Option<bool>,
    /// `null` clears the thread's own choice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub auto_settle: Option<AutoSettleChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub permission_mode: Option<PermissionMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub plan_mode: Option<bool>,
    /// An empty string goes back to the engine's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
    /// An empty string goes back to the engine's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub effort: Option<String>,
    /// Merged by key; an empty value goes back to the engine's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub options: Option<BTreeMap<String, String>>,
}
}

wire! {
pub struct ReorderThreads {
    /// Thread ids in their new order. Ids left out keep their position.
    pub ids: Vec<String>,
}
}

wire! {
/// A message that holds every word. `seq` is its place in the thread's log, and
/// `message_id` the message clients draw, so a client can open right at it.
pub struct MessageHit {
    pub thread_id: String,
    pub thread_title: String,
    pub project_id: String,
    pub message_id: String,
    pub seq: i64,
    pub role: String,
    pub ts: String,
    /// The text around the first word found.
    pub snippet: String,
}
}

wire! {
pub struct ThreadMessage {
    pub message_id: String,
    pub seq: i64,
    pub role: String,
    pub ts: String,
    pub text: String,
}
}

wire! {
pub struct MessageSearch {
    pub hits: Vec<MessageHit>,
    /// The words to highlight, lower-cased.
    pub words: Vec<String>,
    /// No word was long enough to search for, so nothing was searched.
    pub too_short: bool,
    /// More matched, or too many messages to read them all. Narrow the search.
    pub truncated: bool,
}
}

wire! {
/// `run_id` is what was stopped, and empty when nothing was running.
pub struct ForcedStop {
    pub run_id: Option<String>,
    pub state: Option<RunState>,
}
}

wire! {
pub struct BatchThread {
    pub thread: CreateThread,
    pub prompt: String,
}
}

wire! {
pub struct BatchThreads {
    pub threads: Vec<BatchThread>,
}
}

wire! {
pub struct BatchThreadResult {
    pub thread: Option<Thread>,
    pub error: Option<String>,
}
}

wire! {
/// Moves a thread with no runs yet. No `project_id` gives it a scratch folder.
pub struct MoveThread {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub project_id: Option<String>,
}
}

wire! {
pub struct CreateThread {
    /// Absent starts the thread in its own scratch folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub project_id: Option<String>,
    pub engine: String,
    pub model: Option<String>,
    pub title: Option<String>,
    /// Purpose slug for an isolated worktree; `None` works in the project root.
    pub worktree: Option<String>,
    pub skill_profile: Option<String>,
    /// Where a new worktree starts; absent follows the settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base_branch: Option<String>,
    /// Absent effort and options follow what was last chosen for this model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<String, String>>")]
    pub options: BTreeMap<String, String>,
}
}
