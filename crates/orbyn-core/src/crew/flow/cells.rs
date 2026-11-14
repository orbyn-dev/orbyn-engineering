//! The ten cells of a task's line, derived and never stored: a pure function of what `Facts` read. Only a confirmed
//! finding that still blocks turns a review cell red; a model's review never passes or fails a stage by itself.

use orbyn_contracts::{CheckOutcome, FlowCell, FlowCellState, FlowStage};
use serde_json::Value;

use super::facts::Facts;
use crate::crew::lifecycle::is_final;

/// The lanes that count as the Security review; every other lane is the Code review.
const SECURITY_LANES: [&str; 3] = ["security", "secrets", "keys"];

pub(super) fn is_security(lane: &str) -> bool {
    SECURITY_LANES.contains(&lane)
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

fn checks(facts: &Facts) -> FlowCell {
    // The latest verification: the evidence on the commit the newest row ran on.
    let Some(last) = facts.evidence.last() else {
        let state = if build_state(facts) == "verifying" {
            FlowCellState::Running
        } else {
            FlowCellState::Pending
        };
        return cell(FlowStage::Checks, state);
    };
    let batch: Vec<_> = facts
        .evidence
        .iter()
        .filter(|e| e.commit == last.commit)
        .collect();
    let passed = batch
        .iter()
        .filter(|e| e.outcome == CheckOutcome::Pass)
        .count();
    let broken = batch
        .iter()
        .any(|e| matches!(e.outcome, CheckOutcome::Fail | CheckOutcome::Error));
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
