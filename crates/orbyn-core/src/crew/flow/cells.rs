//! The ten cells of a task's line, derived and never stored: a pure function of what `Facts` read. Only a confirmed
//! finding that still blocks turns a review cell red; a model's review never passes or fails a stage by itself.

use orbyn_contracts::{CheckOutcome, Evidence, FlowCell, FlowCellState, FlowStage};
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

/// One row of evidence as the rule reads it: the run it was written under, the commit it ran on, and whether it passed.
struct Row<'a> {
    run: &'a str,
    commit: &'a str,
    pass: bool,
    /// Failed or errored; a skipped check is neither a pass nor a break.
    broke: bool,
}

fn rows(evidence: &[Evidence]) -> Vec<Row<'_>> {
    evidence
        .iter()
        .map(|e| Row {
            run: e.run_id.as_str(),
            commit: e.commit.as_str(),
            pass: e.outcome == CheckOutcome::Pass,
            broke: matches!(e.outcome, CheckOutcome::Fail | CheckOutcome::Error),
        })
        .collect()
}

/// The current verification: the rows written under the build's own run, on the commit the newest of them ran on. A row of an
/// earlier round was written under another run and is never counted.
fn current_batch<'a, 'b>(rows: &'b [Row<'a>], run_id: &str) -> Vec<&'b Row<'a>> {
    let mine: Vec<&Row<'a>> = rows.iter().filter(|r| r.run == run_id).collect();
    let Some(last) = mine.last() else {
        return Vec::new();
    };
    mine.iter()
        .filter(|r| r.commit == last.commit)
        .copied()
        .collect()
}

/// The states a task is in once it has reached verifying or gone on from it. `failed` is one: it is where a failed verification
/// rests until the task is tried again, and its count is the most useful one.
pub(super) fn reached_verifying(state: &str) -> bool {
    matches!(
        state,
        "verifying" | "failed" | "needs_person" | "passed" | "stopped" | "cancelled"
    )
}

fn counts_of(state: &str, run_id: &str, rows: &[Row<'_>]) -> Option<(u32, u32)> {
    if !reached_verifying(state) {
        return None;
    }
    let batch = current_batch(rows, run_id);
    if batch.is_empty() {
        return None;
    }
    let passed = batch.iter().filter(|r| r.pass).count();
    Some((
        u32::try_from(passed).ok()?,
        u32::try_from(batch.len()).ok()?,
    ))
}

/// Checks passed out of checks run in the current verification, as two counts and nothing else. Only a build that has reached
/// verifying (`verifying`, `failed`, `needs_person`, `passed`, `stopped`, `cancelled`) has one; a queued, routed or running build
/// has none, and neither has one whose current run has written no row yet, even when an earlier round left evidence.
pub(super) fn check_counts(state: &str, run_id: &str, evidence: &[Evidence]) -> Option<(u32, u32)> {
    counts_of(state, run_id, &rows(evidence))
}

fn checks(facts: &Facts) -> FlowCell {
    // The current verification: the build's own run, on the commit its newest row ran on. An earlier round's rows are not it.
    let all = rows(&facts.evidence);
    let batch = current_batch(&all, &facts.run_id);
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

    const GATED: [&str; 6] = [
        "verifying",
        "failed",
        "needs_person",
        "passed",
        "stopped",
        "cancelled",
    ];

    fn row<'a>(run: &'a str, commit: &'a str, pass: bool) -> Row<'a> {
        Row {
            run,
            commit,
            pass,
            broke: !pass,
        }
    }

    #[test]
    fn only_a_task_that_reached_verifying_has_a_check_count() {
        let rows = [row("r1", "c1", true), row("r1", "c1", false)];
        for state in ["briefed", "routed", "running", "changed_approach"] {
            assert!(!reached_verifying(state), "{state}");
            assert_eq!(counts_of(state, "r1", &rows), None, "{state}");
        }
        for state in GATED {
            assert!(reached_verifying(state), "{state}");
            assert!(crate::crew::lifecycle::STATES.contains(&state), "{state}");
            assert_eq!(counts_of(state, "r1", &rows), Some((1, 2)), "{state}");
            assert_eq!(counts_of(state, "r1", &[]), None, "{state}: none ran");
        }
    }

    #[test]
    fn a_later_round_shows_none_until_its_own_run_writes_a_row() {
        // Round 1 ran under r1; round 2 is a new run, r2, that has written nothing yet.
        let first = [
            row("r1", "c1", true),
            row("r1", "c1", true),
            row("r1", "c1", false),
        ];
        for state in GATED {
            assert_eq!(counts_of(state, "r2", &first), None, "{state} in round 2");
        }
        assert_eq!(counts_of("verifying", "r1", &first), Some((2, 3)));
    }

    #[test]
    fn a_later_round_counts_only_its_own_rows() {
        let both = [
            row("r1", "c1", true),
            row("r1", "c1", true),
            row("r1", "c1", true),
            row("r2", "c2", true),
            row("r2", "c2", false),
        ];
        for state in GATED {
            assert_eq!(counts_of(state, "r2", &both), Some((1, 2)), "{state}");
        }
        // The earlier round is still its own, and a foreign run's newest row never decides the commit.
        assert_eq!(counts_of("passed", "r1", &both), Some((3, 3)));
        let after = [
            row("r1", "c1", true),
            row("r1", "c1", false),
            row("r2", "c9", true),
        ];
        assert_eq!(counts_of("failed", "r1", &after), Some((1, 2)));
    }

    #[test]
    fn only_the_newest_commit_of_the_current_run_is_counted() {
        let tried_twice = [
            row("r1", "c1", false),
            row("r1", "c1", false),
            row("r1", "c2", true),
        ];
        assert_eq!(counts_of("passed", "r1", &tried_twice), Some((1, 1)));
    }

    #[test]
    fn running_in_round_two_shows_none_even_with_current_rows() {
        let rows = [row("r2", "c2", true)];
        assert_eq!(counts_of("running", "r2", &rows), None);
        assert_eq!(counts_of("needs_person", "r2", &rows), Some((1, 1)));
    }
}
