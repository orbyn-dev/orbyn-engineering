//! Approvals, questions, policy rules, audit and computer use.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

wire_enum! {
pub enum ActionKind {
    Command,
    FileChange,
    Tool,
    Computer,
    Browser,
    Network,
}
}

wire_enum! {
pub enum ApprovalState {
    Pending,
    Allowed,
    Denied,
    Expired,
}
}

wire! {
pub struct Approval {
    pub id: String,
    pub thread_id: String,
    pub run_id: String,
    pub kind: ActionKind,
    pub summary: String,
    pub detail: Value,
    pub state: ApprovalState,
    pub decided_by: Option<String>,
    pub decided_at: Option<String>,
    pub created_at: String,
}
}

wire! {
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}
}

wire! {
pub struct Question {
    /// What an answer is keyed by.
    pub id: String,
    pub header: String,
    pub question: String,
    pub options: Vec<QuestionOption>,
    pub multi_select: bool,
    /// A typed answer is allowed alongside the options.
    pub allow_other: bool,
    /// Mask the input. Orbyn passes the answer on and never stores it.
    pub secret: bool,
}
}

wire_enum! {
pub enum QuestionState {
    Pending,
    Answered,
    Dismissed,
    Expired,
}
}

wire! {
/// An engine's question, which holds its run until answered or dismissed.
pub struct QuestionRequest {
    pub id: String,
    pub thread_id: String,
    pub run_id: String,
    pub questions: Vec<Question>,
    pub state: QuestionState,
    /// Keyed by question id. Secret answers read `[hidden]`.
    pub answers: Option<std::collections::BTreeMap<String, Vec<String>>>,
    /// Files a custom answer carried, keyed by question id. They stay with the
    /// question record; an engine's answer schema has no attachment field.
    pub attachments: Option<std::collections::BTreeMap<String, Vec<String>>>,
    pub decided_by: Option<String>,
    pub decided_at: Option<String>,
    pub created_at: String,
}
}

wire! {
pub struct AnswerQuestion {
    pub answers: std::collections::BTreeMap<String, Vec<String>>,
    /// Files already uploaded to this thread, keyed by question id.
    pub attachments: Option<std::collections::BTreeMap<String, Vec<String>>>,
}
}

wire_enum! {
pub enum PolicyEffect {
    Allow,
    Ask,
    Deny,
}
}

wire! {
pub struct PolicyRule {
    pub id: String,
    pub effect: PolicyEffect,
    pub kind: ActionKind,
    /// `*` wildcard glob matched against the action target. A leading `=` makes
    /// the rest of the pattern an exact literal, which is how `remember` stores
    /// a target that itself contains `*`.
    pub pattern: String,
    pub engine: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
}
}

wire! {
pub struct PolicyDecision {
    pub effect: PolicyEffect,
    pub rule_id: Option<String>,
    pub source: String,
    pub reason: String,
}
}

wire! {
pub struct AuditEvent {
    pub id: i64,
    pub ts: String,
    pub actor: String,
    pub action: String,
    pub target: String,
    pub decision: Option<PolicyEffect>,
    pub source: String,
    pub thread_id: Option<String>,
    pub run_id: Option<String>,
    pub detail: Value,
}
}

wire! {
pub struct ComputerStatus {
    pub desktop_control: bool,
    pub desktop_path: Option<String>,
    pub desktop_version: Option<String>,
    pub browser_control: bool,
    pub browser_path: Option<String>,
    pub problem: Option<String>,
}
}

wire! {
pub struct ComputerResult {
    pub ok: bool,
    pub target: String,
    /// The gateway's normalized result for the action.
    pub data: Value,
}
}

wire! {
pub struct DecideApproval {
    pub allow: bool,
    /// Also store an allow rule for this exact target.
    pub remember: bool,
    /// Sent back with a denial, on engines that can hear it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub feedback: Option<String>,
}
}

wire! {
pub struct CreatePolicyRule {
    pub effect: PolicyEffect,
    pub kind: ActionKind,
    pub pattern: String,
    pub engine: Option<String>,
    pub note: Option<String>,
}
}

wire! {
pub struct ComputerAction {
    pub action: String,
    pub target: String,
    pub args: Value,
}
}
