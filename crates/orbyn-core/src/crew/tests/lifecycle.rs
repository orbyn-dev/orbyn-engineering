//! The lifecycle table and the loop guard, as pure functions, checked against the spec's own
//! table so the two cannot drift apart.

use crate::crew::lifecycle::{Guard, Next, STATES, allowed, is_final, next_states};

const SPEC: &str = include_str!("../../../../../docs/crew/protocol/SPEC.md");

/// The transitions of SPEC.md section 6, read from its table. A row can name more than one From
/// state (the final one lists every final state), so each row keeps every state it names.
fn spec_transitions() -> Vec<(Vec<String>, Vec<String>)> {
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
            // The cells are the table's columns, in the order its `| From | To |` header above sets:
            // the From state is the first cell after the leading delimiter and the To cell the
            // second. That is the SPEC table's own schema, not a fixture order.
            let names = |cell: &str| -> Vec<String> {
                cell.split(',')
                    .map(|n| n.trim().trim_matches('`').to_owned())
                    .collect()
            };
            (names(cells[1]), names(cells[2]))
        })
        .collect()
}

#[test]
fn the_table_is_the_specs() {
    let spec = spec_transitions();
    assert_eq!(
        spec.len(),
        8,
        "every transition row of SPEC.md section 6: {spec:?}"
    );
    for (from, to) in &spec {
        // The final row names every final state in one cell, so every one of them is checked, not
        // just the first: each state it lists must have no transition out.
        for from in from {
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
    }
    // The table's last row is the one that ends a run: its From cell names exactly the final states,
    // so the multi-state row above is read whole rather than by its first place.
    let (final_from, final_to) = spec.last().expect("the final transition row");
    assert_eq!(
        final_from.len(),
        3,
        "the final row names every final state: {final_from:?}"
    );
    assert_eq!(
        final_to.len(),
        1,
        "the final row ends every run: {final_to:?}"
    );
    assert_eq!(
        final_to.first().map(String::as_str),
        Some("nothing"),
        "the final row ends every run"
    );
    let mut final_from: Vec<&str> = final_from.iter().map(String::as_str).collect();
    final_from.sort_unstable();
    assert_eq!(
        final_from,
        ["cancelled", "passed", "stopped"],
        "the final row names the final states"
    );
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
