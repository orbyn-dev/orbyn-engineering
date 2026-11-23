//! Whether a Crew worker owns live work now (KC3 PR2): one pure function the Taskboard, the Map and the
//! dashboard all ask, so no two surfaces can disagree about who is Working.
//!
//! It is deliberately not `quiet_of(...) == FlowQuiet::Active`. Quiet answers silence, not ownership, and it
//! disagrees with this module's truth table in both directions: a passed, stopped or idle worker reads
//! `Active` there, and a worker that has been silent for five minutes (or fifteen) reads `Quiet` or
//! `MayBeStuck` while it still owns the work it was given. The truth-table test below is the specification;
//! `quiet_of` stays the separate word it always was.
//!
//! "A worker" is a task with a run: a build, a reviewer lane's round, or a fix of a review finding. A task that
//! has not started yet owns nothing, a task waiting on the person is not working, and a finished task is done;
//! a finished task is working again only while its latest fix has not finished. A background run in flight is
//! working too (a live turn), and an idle or waiting background crew is not.

/// Why a worker owns live work now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Working {
    /// A live turn: a build running, a reviewer lane's round running, or a background run in flight.
    Running,
    /// The verifier is running the task's checks.
    Verifying,
    /// A retry is queued after a failed check: `failed` goes back to `running`, and `changed_approach` is the
    /// once-in-a-life retry with another approach. The worker owns the retry before it starts again.
    Retry,
    /// The task itself has ended and a fix of a review finding has not finished: it was queued, is running, or
    /// is being verified.
    Fix,
}

impl Working {
    /// The wire spelling every read sends.
    pub(crate) fn wire(self) -> orbyn_contracts::FlowWorking {
        match self {
            Self::Running => orbyn_contracts::FlowWorking::Running,
            Self::Verifying => orbyn_contracts::FlowWorking::Verifying,
            Self::Retry => orbyn_contracts::FlowWorking::Retry,
            Self::Fix => orbyn_contracts::FlowWorking::Fix,
        }
    }
}

/// The one answer as the wire's word, or `None` when the worker owns no live work.
pub(crate) fn flow_working(word: Option<Working>) -> Option<orbyn_contracts::FlowWorking> {
    word.map(Working::wire)
}

/// The task states in which a fix has not finished, so the worker of a finished task is still working.
fn fix_in_flight(state: &str) -> bool {
    matches!(
        state,
        "briefed" | "routed" | "running" | "verifying" | "failed" | "changed_approach"
    )
}

/// Whether `state` — a task's state, a background run's state, or a crew's own state — means a worker owns live
/// work, and why. `fix_state` is the state of the task's latest fix, when it has one; it is read only when the
/// task itself has not ended, because a fix outlives the state of the work it fixes.
pub(crate) fn working(state: &str, fix_state: Option<&str>) -> Option<Working> {
    match state {
        "running" => return Some(Working::Running),
        "verifying" => return Some(Working::Verifying),
        // The verifier failed and the lifecycle has queued the retry; the worker owns it now.
        "failed" | "changed_approach" => return Some(Working::Retry),
        _ => {}
    }
    fix_state
        .filter(|state| fix_in_flight(state))
        .map(|_| Working::Fix)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one truth table: every row a surface can be asked about, and the answer. A row here is a
    /// behaviour, not an implementation: `quiet_of` must not be used to answer any of them, and
    /// `crew::flow::map`'s `quiet_is_not_working` test pins that disagreement.
    #[test]
    fn a_worker_is_working_only_while_it_owns_live_work() {
        // (state, latest fix state, expected, the row's own words)
        let rows: &[(&str, Option<&str>, Option<Working>, &str)] = &[
            ("briefed", None, None, "queued; no worker has started"),
            ("routed", None, None, "routed; the worker has not started"),
            ("running", None, Some(Working::Running), "a build running"),
            (
                "changed_approach",
                None,
                Some(Working::Retry),
                "the approach change is a queued retry",
            ),
            (
                "failed",
                None,
                Some(Working::Retry),
                "a retry is queued after a failed check",
            ),
            (
                "verifying",
                None,
                Some(Working::Verifying),
                "checks running",
            ),
            (
                "needs_person",
                None,
                None,
                "waiting on the person; it owns nothing it may run",
            ),
            (
                "passed",
                None,
                None,
                "passed and merged as far as work goes",
            ),
            ("stopped", None, None, "stopped"),
            ("cancelled", None, None, "cancelled"),
            (
                "passed",
                Some("briefed"),
                Some(Working::Fix),
                "a fix of a review finding is queued",
            ),
            (
                "passed",
                Some("routed"),
                Some(Working::Fix),
                "a fix is routed",
            ),
            (
                "passed",
                Some("running"),
                Some(Working::Fix),
                "a fix is running on a task that already passed",
            ),
            (
                "passed",
                Some("verifying"),
                Some(Working::Fix),
                "a fix is being verified",
            ),
            (
                "passed",
                Some("failed"),
                Some(Working::Fix),
                "a fix retries",
            ),
            (
                "stopped",
                Some("running"),
                Some(Working::Fix),
                "a fix still runs after the work stopped",
            ),
            (
                "passed",
                Some("passed"),
                None,
                "the fix finished; nothing is left",
            ),
            (
                "passed",
                Some("stopped"),
                None,
                "the fix stopped; nothing is left",
            ),
            ("passed", Some("cancelled"), None, "the fix was cancelled"),
            (
                "passed",
                Some("not a task state"),
                None,
                "an unreadable fix state owns nothing",
            ),
            // A reviewer lane's round is a task of its own; its state is the state.
            ("running", None, Some(Working::Running), "a lane in flight"),
            // A background run and a background crew carry their own words through the same function.
            (
                "running",
                None,
                Some(Working::Running),
                "a background run in flight",
            ),
            (
                "needs_review",
                None,
                None,
                "a background run waiting for review",
            ),
            ("idle", None, None, "an idle background crew"),
        ];
        for (state, fix, expected, row) in rows {
            assert_eq!(working(state, *fix), *expected, "{row} ({state})");
        }
    }
}
