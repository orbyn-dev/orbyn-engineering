//! The watch read serves the bisect's progress while the watch finds the merge that broke main. The outcome is stored;
//! the progress is in memory only, so the read shows it while the watch runs and nothing after it.

use orbyn_contracts::{AutopilotBisect, AutopilotBisectCheck};

use super::autopilot_watch::{NOW, history};
use crate::crew::{Page, WatchOutcome};

fn page() -> Page {
    Page::new(None, None).unwrap()
}

/// The check history: main breaks at the third merge, so the watch runs the repository's checks at the tip, at the last
/// known good commit and then in a bisect. Every state the watch publishes is in memory only.
#[tokio::test]
async fn a_bisect_publishes_the_states_it_walks_while_the_watch_runs() {
    let h = history([
        &[("src/a.txt", "a\n")],
        &[("src/b.txt", "b\n")],
        &[("src/state.txt", "BAD\n")],
        &[("src/c.txt", "c\n")],
    ])
    .await;
    h.w.core.clear_bisect_spy();
    let outcome =
        h.w.core
            .crew_watch_main(&h.target, NOW + 100)
            .await
            .unwrap();
    assert!(
        matches!(outcome, WatchOutcome::Reverted { .. }),
        "{outcome:?}"
    );
    let seen = h.w.core.take_bisect_spy();
    let phases: Vec<&str> = seen.iter().map(|state| state.phase.as_str()).collect();
    for want in [
        "checking_tip",
        "checking_boundary",
        "bisecting",
        "confirming",
    ] {
        assert!(phases.contains(&want), "no {want} among {phases:?}");
    }
    // Every step is a real run: the count only grows, each finished run is one checked commit, and the bound is known
    // once the search starts.
    let mut last = 0;
    for state in &seen {
        assert!(state.tested >= last, "the count went backwards: {seen:?}");
        last = state.tested;
        assert_eq!(
            usize::try_from(state.tested).unwrap(),
            state.checked.len(),
            "{state:?}"
        );
    }
    let bisecting: Vec<&AutopilotBisect> = seen
        .iter()
        .filter(|state| state.phase == "bisecting")
        .collect();
    assert!(
        bisecting
            .iter()
            .any(|state| state.after.is_some() && state.before.is_some()),
        "the window is served while bisecting: {bisecting:?}"
    );
    assert!(
        bisecting.iter().all(|state| state.steps > 0),
        "the bound is known while bisecting: {bisecting:?}"
    );
    // Done: nothing is left behind for the read to serve.
    assert!(h.w.core.autopilot_watch(page()).unwrap().bisect.is_none());
}

/// The read serves exactly the state the watch published, and none once the watch is done.
#[tokio::test]
async fn the_watch_read_serves_the_progress_and_ends_with_none() {
    let h = history([
        &[("src/a.txt", "a\n")],
        &[("src/b.txt", "b\n")],
        &[("src/state.txt", "BAD\n")],
        &[("src/c.txt", "c\n")],
    ])
    .await;
    let sample = AutopilotBisect {
        phase: "bisecting".to_owned(),
        tested: 2,
        steps: 5,
        after: Some("a".to_owned()),
        before: Some("b".to_owned()),
        testing: Some("b".to_owned()),
        checked: vec![
            AutopilotBisectCheck {
                commit: "a".to_owned(),
                passes: true,
            },
            AutopilotBisectCheck {
                commit: "b".to_owned(),
                passes: false,
            },
        ],
    };
    *h.w.core.crew_bisect.lock().unwrap() = Some(sample.clone());
    assert_eq!(
        h.w.core.autopilot_watch(page()).unwrap().bisect,
        Some(sample)
    );
    *h.w.core.crew_bisect.lock().unwrap() = None;
    assert!(h.w.core.autopilot_watch(page()).unwrap().bisect.is_none());
}
