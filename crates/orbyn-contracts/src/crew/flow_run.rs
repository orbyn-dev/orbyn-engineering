//! Crew Run page (Preview): what the Run view and the Run tab read about one work task (`docs/crew-run-page.md`). The page
//! is generated on the person's device from the run's own records: the prompt, the checks the verifier ran, what was not
//! verified and the pull request the task made. A check's command and what it proves are in it, which the Flow read
//! deliberately withholds, because this page is the person's own record of their own run. The answer key's id, its
//! expectations, an output excerpt and a path on this machine are never in it.
//!
//! The forge's own pull request URL and the pull request's current head are not stored for a Crew pull request, so they are
//! not in this read: the publish would have to record the link, which is the remaining work on `docs/crew-run-page.md`.
//! Times are RFC 3339 and a length is counted in characters.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire! {
/// The person's brief as the Run page quotes it: redacted, cut at the page's length, and flagged when a value was hidden.
pub struct FlowRunPrompt {
    pub text: String,
    /// A secret, a key or one of the person's own patterns was hidden in `text`.
    pub hidden: bool,
    /// The brief was longer than one page shows.
    pub truncated: bool,
}
}

wire! {
/// One check the verifier ran for the run: the command as written, how it ended, how long it took, the commit it ran on
/// and what it proves. No output excerpt, no answer key and no path outside the repository.
pub struct FlowRunCheck {
    pub label: String,
    /// `pass`, `fail`, `error` or `skipped`.
    pub outcome: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub exit_code: Option<u32>,
    #[ts(type = "number")]
    pub duration_ms: u64,
    pub proves: String,
    /// The commit the check ran on.
    pub commit: String,
}
}

wire! {
/// The pull request one task made, from its forge intent and its result: the number the forge gave it and what the run
/// published. A task Orbyn opened no pull request for has none.
pub struct FlowRunPull {
    /// The forge's pull request number.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub number: Option<u32>,
    /// The branch the run published.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub branch: Option<String>,
    /// The commit the run published.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub commit: Option<String>,
}
}

wire! {
/// One work task's run as its own page shows it. A read: no write, no process, no file. It is listed in
/// `orbyn_core::crew::changes::READS`, so Autopilot's rule does not treat it as a change.
pub struct FlowRunPage {
    pub task: String,
    pub crew: String,
    /// The task's stored state.
    pub state: String,
    /// The task has ended: `passed`, `stopped` or `cancelled`.
    pub finished: bool,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub prompt: Option<FlowRunPrompt>,
    /// The checks that decided the task's latest round.
    pub checks: Vec<FlowRunCheck>,
    /// What no check covered, so a pass never implies more than was checked.
    pub not_verified: Vec<String>,
    /// The commit the checks ran on.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub commit: Option<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub pull: Option<FlowRunPull>,
    /// A list was cut at its cap.
    pub truncated: bool,
}
}
