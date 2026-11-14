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
/// The Gate this run of the checks added: what the verifier decided. `none` when it decided nothing, such as when every check
/// is one this version cannot run, or when a check that would not be fair was met first (a protected file changed).
pub enum CrewRerunGate {
    Pass,
    Fail,
    None,
}
}

wire! {
/// A check the command guard would not let run, for a task that has ended. The id is the check's, never its command.
pub struct CrewRerunRefused {
    pub check: String,
    /// The rule that refused it: `N1` to `N10`, `blast` or `unreadable`.
    pub rule: String,
}
}

wire! {
/// The answer to a re-run. No command, output, excerpt or claim of any check is in it. The task's state is as it was: a re-run adds
/// Evidence and a Gate and changes no state.
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
    /// How many the command guard refused. They were not run, and a refused check keeps the Gate from passing.
    pub refused: u32,
    /// How many are of a kind this version does not run.
    pub skipped: u32,
    pub gate: CrewRerunGate,
    /// The Evidence this run added.
    pub evidence_ids: Vec<String>,
    /// Which checks were refused. Only for a task that is final; a task that is not final gets the count only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub refused_checks: Option<Vec<CrewRerunRefused>>,
}
}
