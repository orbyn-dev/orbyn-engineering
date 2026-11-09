//! Runs, their lifecycle, the AG-UI event stream and thread setup.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use ts_rs::TS;

use crate::PermissionMode;

wire! {
/// One request Orbyn makes without being asked.
pub struct NetworkRequest {
    pub purpose: String,
    pub host: String,
    pub cadence: String,
    /// What turns it off.
    pub control: String,
    /// The key that does, in `/api/settings` for `background_update_checks` and in
    /// preferences otherwise. Empty when nothing here controls it.
    pub setting: Option<String>,
    /// Happening now. False for one that runs only when asked.
    pub enabled: bool,
}
}

wire_enum! {
pub enum RunState {
    Admitted,
    Starting,
    Running,
    AwaitingApproval,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}
}

impl RunState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Interrupted
        )
    }
}

wire! {
/// Retry the saved workspace preparation and input of one failed run.
pub struct RetryPreparation {
    pub run_id: String,
}
}

wire_enum! {
/// Where a worktree's setup action stands. `stopped` means it ended without an
/// exit code: cancelled, or cut off by a service restart.
pub enum SetupPhase {
    Preparing,
    Running,
    Finished,
    Stopped,
}
}

wire! {
/// The setup action a new worktree runs, and where it stands.
pub struct ThreadSetup {
    pub name: String,
    /// The first turn waits for this action to exit successfully.
    pub blocking: bool,
    pub phase: SetupPhase,
    pub exit_code: Option<i32>,
    /// The terminal that shows its output, once it has started.
    pub terminal_id: Option<String>,
    /// Only a setup this service is preparing or running can be cancelled.
    pub cancellable: bool,
}
}

wire! {
pub struct Run {
    pub id: String,
    pub thread_id: String,
    pub state: RunState,
    pub error: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
}
}

/// AG-UI 1.0 event subset. Field names match `spec/1.0/schema.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(export, export_to = "../../../web/src/generated/")]
pub enum AgUiEvent {
    #[serde(rename_all = "camelCase")]
    RunStarted { thread_id: String, run_id: String },
    #[serde(rename_all = "camelCase")]
    RunFinished {
        thread_id: String,
        run_id: String,
        /// `{type: "success" | "cancelled"}` or `{type: "interrupt", interrupts}`.
        #[serde(skip_serializing_if = "Option::is_none")]
        outcome: Option<Value>,
    },
    #[serde(rename_all = "camelCase")]
    RunError {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    TextMessageStart { message_id: String, role: String },
    #[serde(rename_all = "camelCase")]
    TextMessageContent { message_id: String, delta: String },
    #[serde(rename_all = "camelCase")]
    TextMessageEnd { message_id: String },
    #[serde(rename_all = "camelCase")]
    ReasoningMessageStart { message_id: String, role: String },
    #[serde(rename_all = "camelCase")]
    ReasoningMessageContent { message_id: String, delta: String },
    #[serde(rename_all = "camelCase")]
    ReasoningMessageEnd { message_id: String },
    #[serde(rename_all = "camelCase")]
    ToolCallStart {
        tool_call_id: String,
        tool_call_name: String,
    },
    #[serde(rename_all = "camelCase")]
    ToolCallArgs { tool_call_id: String, delta: String },
    #[serde(rename_all = "camelCase")]
    ToolCallEnd { tool_call_id: String },
    #[serde(rename_all = "camelCase")]
    ToolCallResult {
        message_id: String,
        tool_call_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        role: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    ActivitySnapshot {
        message_id: String,
        activity_type: String,
        content: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        replace: Option<bool>,
    },
    #[serde(rename_all = "camelCase")]
    Custom { name: String, value: Value },
    #[serde(rename_all = "camelCase")]
    Raw {
        event: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        source: Option<String>,
    },
}

wire! {
pub struct ThreadEvent {
    pub seq: i64,
    pub thread_id: String,
    pub run_id: Option<String>,
    pub ts: String,
    pub event: AgUiEvent,
}
}

wire! {
pub struct StartRun {
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<String>>")]
    pub attachment_ids: Vec<String>,
    /// Sets the thread's mode for this turn and the ones after it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub permission_mode: Option<PermissionMode>,
    /// Sets the thread's model for this turn and the ones after it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
    /// Sets the thread's effort for this turn and the ones after it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub effort: Option<String>,
    /// Merged into the thread's options for this turn and the ones after it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<String, String>>")]
    pub options: BTreeMap<String, String>,
}
}

wire_enum! {
pub enum SteerState {
    /// Handed to the running turn.
    Delivered,
    /// This engine takes no mid-turn input, so the message waits for the next turn.
    Queued,
}
}

wire! {
pub struct SteerResult {
    pub state: SteerState,
}
}

wire! {
/// The script a Claude Code workflow runs, read for display only.
pub struct WorkflowScript {
    /// Where the CLI keeps the script, when it reported one.
    pub path: Option<String>,
    pub contents: String,
    /// The script was longer than the display cap and was cut.
    pub truncated: bool,
}
}

wire! {
/// A workflow stop the running engine was asked for; its end arrives as an event.
pub struct WorkflowStop {
    pub requested: bool,
}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_use_ag_ui_names() {
        let event = AgUiEvent::TextMessageContent {
            message_id: "m".into(),
            delta: "hi".into(),
        };
        assert_eq!(
            serde_json::to_value(event).unwrap(),
            serde_json::json!({"type":"TEXT_MESSAGE_CONTENT","messageId":"m","delta":"hi"})
        );
    }

    #[test]
    fn terminal_states_are_closed() {
        assert!(RunState::Interrupted.is_terminal());
        assert!(!RunState::AwaitingApproval.is_terminal());
    }
}
