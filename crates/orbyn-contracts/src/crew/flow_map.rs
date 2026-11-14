//! The Crew Map, message labels and board facts over HTTP (Preview). Every type is a view the core builds; none can hold a bus
//! message's text, a fact's value, a thread id, a path on this machine, the answer key, a repro or evidence text (H1). Messages are
//! shown by author, addressee, kind, time and delivery, with a label the core wrote for the kind. Free text is redacted and capped.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire_enum! {
pub enum FlowRole {
    Lead,
    Worker,
}
}

wire_enum! {
/// How long a running worker has been silent: quiet after 5 minutes, may be stuck after 15.
pub enum FlowQuiet {
    Active,
    Quiet,
    MayBeStuck,
}
}

wire_enum! {
/// What the engine registry knows of an engine, without reading its models.
pub enum FlowEngineStatus {
    Ready,
    NotInstalled,
    SignedOut,
    SignInUnconfirmed,
    Broken,
}
}

wire_enum! {
/// Whether the engine reports its own subagents. Depth is one: a worker's parent is the lead.
pub enum FlowSubagents {
    Reported,
    NotReported,
}
}

wire_enum! {
/// How a message reached a recipient. An engine that cannot be steered mid-turn is never `live`. `tool_result` is a held message that
/// went out with the recipient's next bus tool result.
pub enum FlowDeliveryMode {
    Live,
    NextTurn,
    ToolResult,
    Held,
    Failed,
}
}

wire! {
pub struct FlowNode {
    pub member: String,
    pub role: FlowRole,
    /// The work behind the node; absent for the lead.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub task: Option<String>,
    pub engine: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub model: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub effort: Option<String>,
    /// The task's state; absent for the lead.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub state: Option<String>,
    pub quiet: FlowQuiet,
    pub engine_status: FlowEngineStatus,
    pub subagents: FlowSubagents,
    /// The brief's outcome, in place of a live step.
    pub outcome: String,
    /// The files the worker may change, as patterns, at most three.
    pub lease: Vec<String>,
    pub read_only: bool,
    /// Messages waiting for this member's next turn.
    pub unread: u32,
    /// The lead, for a worker.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub started_by: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub created_at: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub updated_at: Option<String>,
}
}

wire! {
/// Who has written to whom, by kind: a count and how the newest one was delivered.
pub struct FlowEdge {
    pub from: String,
    pub to: String,
    pub kind: String,
    pub label: String,
    pub count: u32,
    /// How the newest message was delivered, only when every recipient's newest delivery agrees.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub delivery: Option<FlowDeliveryMode>,
}
}

wire! {
pub struct FlowEngineRow {
    pub engine: String,
    pub status: FlowEngineStatus,
}
}

wire! {
pub struct FlowMap {
    pub crew: String,
    pub nodes: Vec<FlowNode>,
    pub edges: Vec<FlowEdge>,
    pub engines: Vec<FlowEngineRow>,
    /// A list was cut at its cap, or the crew's newest-message or newest-delivery window cut older traffic, so an edge's count and a
    /// node's unread cover recent traffic only.
    pub truncated: bool,
}
}

wire! {
/// One bus message by its labels only. The text is never in it.
pub struct FlowMessage {
    pub id: String,
    pub at: String,
    pub from: String,
    pub to: String,
    pub kind: String,
    /// What the core calls a message of this kind, such as "a question".
    pub label: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub in_reply_to: Option<String>,
    /// Only when every recipient's newest delivery says the same; a message to `all` that went differently has none.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub delivery: Option<FlowDeliveryMode>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub delivered_at: Option<String>,
}
}

wire! {
pub struct FlowMessageList {
    pub messages: Vec<FlowMessage>,
    pub total: u32,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub next_offset: Option<u32>,
}
}

wire! {
/// One board fact by its labels only. The value is never in it.
pub struct FlowFact {
    pub key: String,
    pub class: String,
    pub author: String,
    pub rev: u32,
    pub at: String,
    /// The fact names evidence.
    pub verified: bool,
}
}

wire! {
pub struct FlowFactList {
    pub facts: Vec<FlowFact>,
    pub total: u32,
    pub truncated: bool,
}
}
