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
