//! A person's cancel of a Crew task (Crew Preview): the stop button. Local only; an agent cannot send it.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire! {
/// The optional body of `POST /api/crew/tasks/{id}/cancel`. The body may be empty.
#[serde(deny_unknown_fields)]
pub struct CrewTaskCancel {
    /// Why, in the person's words: 1 to 200 characters, redacted before it is kept. Blank means none.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub reason: Option<String>,
}
}

wire_enum! {
/// The state a cancel leaves a task in. There is only one.
pub enum CrewCancelState {
    Cancelled,
}
}

wire! {
/// The answer to a cancel. `already` is true when the task was cancelled before this request: nothing was done and nothing was
/// written.
pub struct CrewTaskCancelled {
    /// The task's id.
    pub task: String,
    pub state: CrewCancelState,
    pub already: bool,
}
}

wire! {
/// The answer to `POST /api/crew/tasks/{id}/retry-elsewhere`: the task that could not start is cancelled and its lead is asked to
/// start it again. The body is empty.
pub struct CrewTaskRetried {
    /// The task that could not start, now cancelled.
    pub task: String,
    /// The lead was told to start it again. False when the cancel stands but the lead's queue refused the message.
    pub lead_told: bool,
}
}
