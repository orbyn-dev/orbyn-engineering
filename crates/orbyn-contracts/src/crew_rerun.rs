//! A person's re-run of a Crew task's checks (Crew Preview). Local only; an agent cannot send it. The answer is counts and
//! outcomes: it never holds a check's command, its output or what it proves, because that is the answer key (SPEC.md H1).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire! {
/// The optional body of `POST /api/crew/tasks/{id}/rerun-checks`. The body may be empty.
#[serde(deny_unknown_fields)]
pub struct CrewTaskRerun {
    /// Why, in the person's words: 1 to 200 characters, redacted before it is kept. Blank means none.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub reason: Option<String>,
}
}

wire_enum! {
/// What the verifier decided about this run of the checks. `none` when it decided nothing, such as when every check
/// is one this version cannot run, or when a check that would not be fair was met first (a protected file changed).
pub enum CrewRerunGate {
    Pass,
    Fail,
    None,
}
}

wire! {
/// The answer to a re-run. No command, output, excerpt or claim of any check is in it, and no Evidence id: a re-run stores no
/// Evidence or Gate (its record is its two audit rows), and changes no state.
pub struct CrewTaskRerunDone {
    /// The task's id.
    pub task: String,
    /// The task's state, unchanged.
    pub state: String,
    /// How many checks the key has.
    pub total: u32,
    /// How many ran.
    pub ran: u32,
    /// How many of those passed.
    pub passed: u32,
    /// How many the command guard refused. They were not run, and a refused check keeps the verdict from passing.
    pub refused: u32,
    /// How many are of a kind this version does not run.
    pub skipped: u32,
    pub gate: CrewRerunGate,
    /// The ids of the checks the guard refused, never their commands or the rule. Only for a task that is final; otherwise the count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub refused_checks: Option<Vec<String>>,
}
}
