//! Closing a lead thread's Crew work (Crew Preview): the live tasks a close would cancel, and what a close did. Both routes
//! are local: the read is a plain read, the close runs the person's cancel for each task.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire! {
/// The live Crew tasks a lead thread leads, as the confirm before closing or archiving it lists them. A task in a final state
/// is not one, and a task another thread leads is never one.
pub struct CrewLeadLive {
    /// The tasks a close would cancel, oldest first.
    pub tasks: Vec<CrewLiveTask>,
}
}

wire! {
/// One live task of a lead thread: its id, the name its Flow card shows, and the state it is in.
pub struct CrewLiveTask {
    /// The task's id.
    pub id: String,
    /// The brief's outcome, scrubbed as the Flow board's card name is.
    pub name: String,
    /// The task's state, as stored.
    pub state: String,
}
}

wire! {
/// The answer to closing a lead thread: the ids of the tasks it cancelled and the ids of those it could not stop. The thread
/// closes whatever it says; a failed task keeps its state and its worker may still run.
pub struct CrewLeadClosed {
    /// The tasks this close cancelled, in the order they were tried.
    pub closed: Vec<String>,
    /// The tasks this close could not stop, in the order they were tried. The thread still closes.
    pub failed: Vec<String>,
}
}
