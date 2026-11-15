//! The ten cells of a task's line, derived and never stored: a pure function of what `Facts` read. Only a confirmed
//! finding that still blocks turns a review cell red; a model's review never passes or fails a stage by itself.

use orbyn_contracts::{CheckOutcome, Evidence, FlowCell, FlowCellState, FlowStage, Gate};
use serde_json::Value;

use super::facts::Facts;
use crate::crew::lifecycle::is_final;

/// Only the review's own security lane is the Security review; every other lane, a free-form one named `secrets`
/// included, is the Code review.
pub(super) fn is_security(lane: &str) -> bool {
    lane == crate::crew::SECURITY_LANE
}

fn cell(stage: FlowStage, state: FlowCellState) -> FlowCell {
    FlowCell {
        stage,
        state,
        passed: None,
        of: None,
        lanes: None,
    }
}

/// The lanes of the review: `(lane, round, state of that round's run)` for each lane's latest round.
pub(super) fn latest_rounds(review: &Value) -> Vec<(String, u32, String, String)> {
    review["lanes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|lane| {
            let last = lane["rounds"].as_array()?.last()?;
            Some((
                lane["lane"].as_str()?.to_owned(),
                u32::try_from(last["round"].as_u64()?).ok()?,
                last["state"].as_str()?.to_owned(),
                last["task"].as_str()?.to_owned(),
            ))
        })
        .collect()
}

/// The state the build is in now: the latest fix when a finding sent one, else the task.
pub(super) fn build_state(facts: &Facts) -> &str {
    facts
        .fixes
        .last()
        .map_or(facts.task.state.as_str(), |(_, state)| state.as_str())
}

fn build(facts: &Facts) -> FlowCellState {
    let fixing = !facts.fixes.is_empty();
    match build_state(facts) {
        "briefed" | "routed" if fixing => FlowCellState::Running,
        "briefed" | "routed" => FlowCellState::Pending,
        "running" | "changed_approach" => FlowCellState::Running,
        "verifying" | "passed" => FlowCellState::Done,
        "failed" | "stopped" => FlowCellState::Failed,
        "cancelled" => FlowCellState::Skipped,
        "needs_person" => FlowCellState::NeedsYou,
        _ => FlowCellState::Pending,
    }
}

/// One row of evidence as the rule reads it: its id, and whether it passed or broke.
struct Row<'a> {
    id: &'a str,
    pass: bool,
    /// Failed or errored; a skipped check is neither a pass nor a break.
    broke: bool,
}

fn rows(evidence: &[Evidence]) -> Vec<Row<'_>> {
    evidence
        .iter()
        .map(|e| Row {
            id: e.id.as_str(),
            pass: e.outcome == CheckOutcome::Pass,
            broke: matches!(e.outcome, CheckOutcome::Fail | CheckOutcome::Error),
        })
        .collect()
}

/// The rows of the final verification, once there is one: the ones the newest checks gate named. Nothing else is counted.
///
/// A count is shown only in `passed`. `passed` is reached only when the final verification decided pass, and that verification
/// wrote a gate naming its rows, so the newest gate is the right one. In every other state the store cannot say which attempt a
/// row belongs to: nothing dates a state entry or tags a row with its attempt, a retry keeps its run and its commit, and a
/// verification that stops early (a protected file changed, a hidden file found) or is undecided writes no gate and leaves rows
/// no gate names. Counting there would show an earlier attempt's result, so `failed`, `verifying`, `needs_person`, `stopped` and
/// `cancelled` have none, until the store keeps a record per verification attempt.
fn current_batch<'a, 'b>(
    state: &str,
    rows: &'b [Row<'a>],
    newest: Option<&[&str]>,
) -> Vec<&'b Row<'a>> {
    match (state, newest) {
        ("passed", Some(ids)) => rows.iter().filter(|r| ids.contains(&r.id)).collect(),
        _ => Vec::new(),
    }
}

/// The evidence ids the newest checks gate named; `gates` holds only checks gates about the task's own brief.
fn newest_verification(gates: &[Gate]) -> Option<Vec<&str>> {
    gates
        .last()
        .map(|g| g.evidence.iter().map(|h| h.as_str()).collect())
}

/// Checks passed out of checks run in the final verification, as two counts and nothing else; only a `passed` build has them.
pub(super) fn check_counts(
    state: &str,
    evidence: &[Evidence],
    gates: &[Gate],
) -> Option<(u32, u32)> {
    let all = rows(evidence);
    let newest = newest_verification(gates);
    let batch = current_batch(state, &all, newest.as_deref());
    if batch.is_empty() {
        return None;
    }
    let passed = batch.iter().filter(|r| r.pass).count();
    Some((
        u32::try_from(passed).ok()?,
        u32::try_from(batch.len()).ok()?,
    ))
}

fn checks(facts: &Facts) -> FlowCell {
    // The current verification of the build now under way; an earlier attempt's rows are not it.
    let all = rows(&facts.build_evidence);
    let newest = newest_verification(&facts.build_gates);
    let batch = current_batch(build_state(facts), &all, newest.as_deref());
    if batch.is_empty() {
        let state = if build_state(facts) == "verifying" {
            FlowCellState::Running
        } else {
            FlowCellState::Pending
        };
        return cell(FlowStage::Checks, state);
    }
    let passed = batch.iter().filter(|r| r.pass).count();
    let broken = batch.iter().any(|r| r.broke);
    let state = if broken {
        FlowCellState::Failed
    } else if build_state(facts) == "verifying" {
        FlowCellState::Running
    } else {
        FlowCellState::Done
    };
    FlowCell {
        passed: u32::try_from(passed).ok(),
        of: u32::try_from(batch.len()).ok(),
        ..cell(FlowStage::Checks, state)
    }
}

fn blocks_in(review: &Value, security: bool) -> bool {
    review["findings"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|f| {
            f["blocks"] == true
                && f["lane"]
                    .as_str()
                    .is_some_and(|l| is_security(l) == security)
        })
}

fn lanes(facts: &Facts, security: bool) -> FlowCell {
    let stage = if security {
        FlowStage::Security
    } else {
        FlowStage::Code
    };
    let mine: Vec<_> = latest_rounds(&facts.review)
        .into_iter()
        .filter(|(lane, ..)| is_security(lane) == security)
        .collect();
    let count = u32::try_from(mine.len()).ok();
    let state = if mine.is_empty() {
        FlowCellState::Pending
    } else if mine.iter().any(|(_, _, s, _)| s == "needs_person") {
        FlowCellState::NeedsYou
    } else if mine
        .iter()
        .any(|(_, _, s, _)| s == "stopped" || s == "cancelled")
    {
        FlowCellState::Failed
    } else if mine.iter().any(|(_, _, s, _)| !is_final(s)) {
        FlowCellState::Running
    } else if blocks_in(&facts.review, security) {
        FlowCellState::Failed
    } else {
        FlowCellState::Done
    };
    FlowCell {
        lanes: count,
        ..cell(stage, state)
    }
}

fn finding(facts: &Facts) -> FlowCellState {
    let findings = facts.review["findings"].as_array();
    let any =
        |key: &str, value: &str| findings.is_some_and(|all| all.iter().any(|f| f[key] == value));
    let rounds = latest_rounds(&facts.review);
    if facts.review["blocked"] == true
        || blocks_in(&facts.review, false)
        || blocks_in(&facts.review, true)
    {
        FlowCellState::Failed
    } else if any("check", "pending") {
        FlowCellState::Running
    } else if rounds.is_empty() || rounds.iter().any(|(_, _, s, _)| !is_final(s)) {
        FlowCellState::Pending
    } else {
        FlowCellState::Done
    }
}

fn merge(facts: &Facts) -> FlowCellState {
    match facts.entry.as_ref().map(|e| e.state.as_str()) {
        Some("queued" | "merging") => FlowCellState::Running,
        Some("waiting") => FlowCellState::NeedsYou,
        Some("merged") => FlowCellState::Done,
        Some("rejected") => FlowCellState::Failed,
        Some(_) => FlowCellState::Pending,
        None if facts.record.is_some() => FlowCellState::Done,
        None => FlowCellState::Pending,
    }
}

fn main_branch(facts: &Facts) -> FlowCellState {
    match facts.record.as_ref().map(|r| r.watch.as_str()) {
        Some("green") => FlowCellState::Done,
        Some("broken") => FlowCellState::Failed,
        Some(_) => FlowCellState::Running,
        None => FlowCellState::Pending,
    }
}

fn you(facts: &Facts) -> FlowCellState {
    let Some(record) = &facts.record else {
        return FlowCellState::Pending;
    };
    if record.needs_person.is_some() {
        return FlowCellState::NeedsYou;
    }
    match record.review_state.as_str() {
        "waiting" => FlowCellState::ReviewWaiting,
        "reverted" => FlowCellState::Failed,
        _ => FlowCellState::Done,
    }
}

/// The ten cells, in the order of `FlowStage`.
pub(super) fn cells(facts: &Facts) -> Vec<FlowCell> {
    let route = if facts.route.is_some() {
        FlowCellState::Done
    } else if facts.task.state == "needs_person" {
        FlowCellState::NeedsYou
    } else {
        FlowCellState::Pending
    };
    vec![
        cell(FlowStage::Brief, FlowCellState::Done),
        cell(FlowStage::Route, route),
        cell(FlowStage::Build, build(facts)),
        checks(facts),
        lanes(facts, false),
        lanes(facts, true),
        cell(FlowStage::Finding, finding(facts)),
        cell(FlowStage::Merge, merge(facts)),
        cell(FlowStage::Main, main_branch(facts)),
        cell(FlowStage::You, you(facts)),
    ]
}

/// The round the task is in: the build and each fix that answered a finding, or the highest review round.
pub(super) fn round(facts: &Facts) -> u32 {
    let fixed = u32::try_from(facts.fixes.len())
        .unwrap_or(u32::MAX)
        .saturating_add(1);
    let reviewed = latest_rounds(&facts.review)
        .iter()
        .map(|(_, round, ..)| *round)
        .max()
        .unwrap_or(1);
    fixed.max(reviewed)
}

pub(super) fn in_filter(
    cells: &[FlowCell],
    state: &str,
    filter: orbyn_contracts::FlowFilter,
) -> bool {
    use orbyn_contracts::FlowFilter as F;
    let some = |f: &dyn Fn(&FlowCell) -> bool| cells.iter().any(f);
    match filter {
        F::All => true,
        F::NeedsYou => some(&|c| {
            matches!(
                c.state,
                FlowCellState::NeedsYou | FlowCellState::ReviewWaiting
            )
        }),
        F::Reviewing => some(&|c| {
            matches!(
                c.stage,
                FlowStage::Code | FlowStage::Security | FlowStage::Finding
            ) && c.state == FlowCellState::Running
        }),
        F::Failed => state == "stopped" || some(&|c| c.state == FlowCellState::Failed),
        F::Passed => state == "passed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, pass: bool) -> Row<'_> {
        Row {
            id,
            pass,
            broke: !pass,
        }
    }

    fn counts(state: &str, rows: &[Row<'_>], newest: Option<&[&str]>) -> Option<(usize, usize)> {
        let batch = current_batch(state, rows, newest);
        (!batch.is_empty()).then(|| (batch.iter().filter(|r| r.pass).count(), batch.len()))
    }

    #[test]
    fn passed_shows_the_newest_gates_counts_after_an_earlier_failed_attempt() {
        // Attempt 1 ran a, b and failed (gate g1); the retry ran d, e, f and passed (gate g2).
        let rows = [
            row("a", true),
            row("b", false),
            row("d", true),
            row("e", true),
            row("f", true),
        ];
        assert_eq!(
            counts("passed", &rows, Some(&["d", "e", "f"])),
            Some((3, 3))
        );
    }

    #[test]
    fn no_other_state_shows_a_count() {
        let rows = [row("a", true), row("b", false)];
        let newest: Option<&[&str]> = Some(&["a", "b"]);
        for state in [
            "failed",
            "verifying",
            "needs_person",
            "stopped",
            "cancelled",
            "briefed",
            "routed",
            "running",
            "changed_approach",
        ] {
            assert_eq!(counts(state, &rows, newest), None, "{state}");
        }
    }

    #[test]
    fn a_failure_after_a_gate_that_wrote_no_gate_shows_none() {
        // Attempt 1 wrote gate g1; the retry stopped early on a protected file: no row, no gate, state failed.
        let rows = [row("a", true), row("b", false)];
        assert_eq!(counts("failed", &rows, Some(&["a", "b"])), None);
    }

    #[test]
    fn a_passed_task_with_no_gate_or_no_rows_has_none() {
        assert_eq!(counts("passed", &[row("a", true)], None), None);
        assert_eq!(counts("passed", &[], Some(&["a"])), None);
    }
}
