//! The lifecycle table and the loop guard, as pure functions, checked against the spec's own
//! table so the two cannot drift apart.

use crate::crew::lifecycle::{Guard, Next, STATES, allowed, is_final, next_states};

const SPEC: &str = include_str!("../../../../../docs/crew/protocol/SPEC.md");

/// The transitions of SPEC.md section 6, read from its table.
fn spec_transitions() -> Vec<(String, Vec<String>)> {
    let table = SPEC
        .split("| From | To |")
        .nth(1)
        .expect("the lifecycle table")
        .lines()
        .skip(2)
        .take_while(|line| line.starts_with('|'));
    table
        .map(|line| {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            let names = |cell: &str| -> Vec<String> {
                cell.split(',')
                    .map(|n| n.trim().trim_matches('`').to_owned())
                    .collect()
            };
            (names(cells[1]).remove(0), names(cells[2]))
        })
        .collect()
}

#[test]
fn the_table_is_the_specs() {
    let spec = spec_transitions();
    assert!(spec.len() >= 7, "{spec:?}");
    for (from, to) in &spec {
        if to.iter().any(|t| t == "nothing") {
            assert!(next_states(from).is_empty(), "{from}");
            continue;
        }
        let mut ours: Vec<&str> = next_states(from).to_vec();
        let mut theirs: Vec<&str> = to.iter().map(String::as_str).collect();
        ours.sort_unstable();
        theirs.sort_unstable();
        assert_eq!(ours, theirs, "from {from}");
    }
    // Every state is known, and only passed, stopped and cancelled end the run.
    for state in STATES {
        assert_eq!(
            is_final(state),
            matches!(state, "passed" | "stopped" | "cancelled"),
            "{state}"
        );
        assert!(
            next_states(state).iter().all(|to| STATES.contains(to)),
            "{state}"
        );
    }
    assert!(!allowed("passed", "running") && !allowed("briefed", "running"));
    assert!(!is_final("failed") && !is_final("needs_person"));
}

/// The sequence the spec gives: one retry per new failure, one change of approach, then stop.
#[test]
fn the_guard_retries_changes_approach_once_and_then_stops() {
    let mut guard = Guard::default();
    let mut step = |fingerprint: &str| {
        let (next_guard, next) = guard.after_failure(fingerprint);
        guard = next_guard;
        next
    };
    assert_eq!(step("a"), Next::Retry);
    assert_eq!(step("a"), Next::ChangeApproach);
    assert_eq!(step("a"), Next::Stop);
}

#[test]
fn a_different_failure_starts_the_count_again() {
    let mut guard = Guard::default();
    let mut step = |fingerprint: &str| {
        let (next_guard, next) = guard.after_failure(fingerprint);
        guard = next_guard;
        (next, guard.streak)
    };
    assert_eq!(step("a"), (Next::Retry, 1));
    assert_eq!(step("b"), (Next::Retry, 1));
    assert_eq!(step("b"), (Next::ChangeApproach, 2));
    // The change is used up: a new failure retries, and its repeat stops.
    assert_eq!(step("c"), (Next::Retry, 1));
    assert_eq!(step("c"), (Next::Stop, 2));
    assert_eq!(Next::Retry.state(), "running");
    assert_eq!(Next::ChangeApproach.state(), "changed_approach");
    assert_eq!(Next::Stop.state(), "stopped");
}
