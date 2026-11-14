//! The ten cells of a task's line, derived and never stored: a pure function of what `Facts` read. Only a confirmed
//! finding that still blocks turns a review cell red; a model's review never passes or fails a stage by itself.

use orbyn_contracts::{
    CheckOutcome, Evidence, FlowCell, FlowCellState, FlowStage, Gate, GateStage,
};
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

/// The evidence ids each finished verification named, oldest first: one checks gate is written per verification, so the ids it
/// names are that attempt's own, whatever commit and run they ran under.
fn verifications(gates: &[Gate]) -> Vec<Vec<&str>> {
    gates
        .iter()
        .filter(|g| g.stage == GateStage::Checks)
        .map(|g| g.evidence.iter().map(|h| h.as_str()).collect())
        .collect()
}

/// The rows of the verification a build is in now, or none when the store cannot say which attempt they belong to.
///
/// Nothing in the store dates a state entry or tags a row with its attempt (a retry keeps its run and its commit), so the rule
/// reads what is exact. While `verifying`, the attempt's rows are the ones no finished verification has named yet. In `failed` and
/// `passed`, which are reached only from verifying, they are the ones the newest verification named. `needs_person`, `stopped` and
/// `cancelled` can be reached from verifying or from running, so a count there could be an earlier attempt's: none is shown.
fn current_batch<'a, 'b>(
    state: &str,
    rows: &'b [Row<'a>],
    verified: &[Vec<&str>],
) -> Vec<&'b Row<'a>> {
    match state {
        "verifying" => rows
            .iter()
            .filter(|r| !verified.iter().any(|ids| ids.contains(&r.id)))
            .collect(),
        "failed" | "passed" => verified.last().map_or_else(Vec::new, |ids| {
            rows.iter().filter(|r| ids.contains(&r.id)).collect()
        }),
        _ => Vec::new(),
    }
}

/// Checks passed out of checks run in the current verification, as two counts and nothing else; see `current_batch` for which
/// states have one. A build with no row in its current verification has none, whatever an earlier attempt left.
pub(super) fn check_counts(
    state: &str,
    evidence: &[Evidence],
    gates: &[Gate],
) -> Option<(u32, u32)> {
    let all = rows(evidence);
    let batch = current_batch(state, &all, &verifications(gates));
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
    let batch = current_batch(build_state(facts), &all, &verifications(&facts.build_gates));
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

    fn counts(state: &str, rows: &[Row<'_>], verified: &[Vec<&str>]) -> Option<(usize, usize)> {
        let batch = current_batch(state, rows, verified);
        (!batch.is_empty()).then(|| (batch.iter().filter(|r| r.pass).count(), batch.len()))
    }

    #[test]
    fn a_retry_in_the_same_run_shows_none_until_it_writes_a_row() {
        // Round 1 verified three checks (a, b, c) and failed; the task went back to running and is verifying again.
        let rows = [row("a", true), row("b", true), row("c", false)];
        let verified = [vec!["a", "b", "c"]];
        assert_eq!(counts("verifying", &rows, &verified), None);
        // Its first row appears: only that row counts, though the commit may be the same as round 1's.
        let rows = [
            row("a", true),
            row("b", true),
            row("c", false),
            row("d", true),
        ];
        assert_eq!(counts("verifying", &rows, &verified), Some((1, 1)));
    }

    #[test]
    fn a_finished_retry_counts_its_own_verification_and_not_the_first() {
        let rows = [
            row("a", true),
            row("b", false),
            row("d", true),
            row("e", true),
        ];
        let verified = [vec!["a", "b"], vec!["d", "e"]];
        assert_eq!(counts("passed", &rows, &verified), Some((2, 2)));
        assert_eq!(counts("failed", &rows, &verified), Some((2, 2)));
        // Round 1 alone: the failed state it rests in counts it.
        assert_eq!(counts("failed", &rows[..2], &verified[..1]), Some((1, 2)));
    }

    #[test]
    fn a_state_that_may_have_been_reached_from_running_has_no_count() {
        // Round 1 left rows and a gate; the task went back to running and was stopped or sent to the person before verifying.
        let rows = [row("a", true), row("b", false)];
        let verified = [vec!["a", "b"]];
        for state in ["needs_person", "stopped", "cancelled"] {
            assert_eq!(counts(state, &rows, &verified), None, "{state}");
        }
        for state in ["briefed", "routed", "running", "changed_approach"] {
            assert_eq!(counts(state, &rows, &verified), None, "{state}");
        }
    }

    #[test]
    fn a_finished_verification_with_no_rows_or_no_gate_has_none() {
        assert_eq!(counts("passed", &[], &[]), None);
        assert_eq!(counts("failed", &[row("a", true)], &[]), None);
        assert_eq!(counts("verifying", &[], &[]), None);
    }
}
