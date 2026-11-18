//! A person's acts on the escape log at rest: each transition with its event and audit row, an idempotent repeat, the refused
//! transitions, a stale choice, one project's isolation from another's, a mark, the gap, two acts racing, and the bounds.

use std::sync::Barrier;

use orbyn_contracts::{
    EscapeActor, EscapeEventKind, EscapeGap, EscapeSeen, EscapeSource, EscapeState,
};

use super::crew_escapes_tests::{PROJECT, escapes, open, seed};
use super::*;

const AT: i64 = 5_000;

fn seen(state: EscapeState) -> EscapeSeen {
    EscapeSeen { state, gap: None }
}

/// A store with one reverted merge, so one possible escape.
fn with_possible() -> (crate::testing::TempDir, Store, String, String) {
    let (dir, store) = open();
    let task = seed(&store.writer(), 1, "waiting", None, "alpha", 100);
    store.set_merge_reverted(&task, "feed01").unwrap();
    let id = escapes(&store)[0].id.clone();
    (dir, store, task, id)
}

fn audits(store: &Store) -> Vec<(String, String, String)> {
    store
        .reader()
        .prepare("SELECT action, target, detail FROM audit_events WHERE action LIKE 'crew.escape.%' ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

fn kinds(store: &Store, id: &str) -> Vec<EscapeEventKind> {
    store
        .crew_escape_events(id, 50)
        .unwrap()
        .iter()
        .map(|e| e.kind)
        .collect()
}

#[test]
fn a_person_confirms_a_possible_escape_with_one_event_and_one_audit_row() {
    let (_dir, store, task, id) = with_possible();
    let done = store
        .decide_escape(
            PROJECT,
            &id,
            EscapeDecision::Confirm,
            &seen(EscapeState::Possible),
            Some("broke the login"),
            AT,
        )
        .unwrap();
    assert_eq!(done.state, EscapeState::Confirmed);
    assert_eq!(
        (done.decided_unix, done.decided_by, done.source),
        (Some(AT), Some(EscapeActor::Person), EscapeSource::Revert),
        "the source stays where it was found"
    );
    assert_eq!(store.crew_escape_of_merge(&task).unwrap().unwrap(), done);

    let events = store.crew_escape_events(&id, 10).unwrap();
    assert_eq!(events.len(), 2);
    let last = &events[1];
    assert_eq!(last.kind, EscapeEventKind::Confirmed);
    assert_eq!(
        (last.from_state, last.to_state, last.decided_by, last.at),
        (
            Some(EscapeState::Possible),
            Some(EscapeState::Confirmed),
            EscapeActor::Person,
            AT
        )
    );
    let audit = audits(&store);
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].0, "crew.escape.confirm");
    assert_eq!(audit[0].1, format!("{PROJECT}:{id}"));
    assert!(audit[0].2.contains("broke the login"), "{}", audit[0].2);
    assert!(
        last.cause.starts_with("person:"),
        "the event names the audit row written first: {}",
        last.cause
    );

    // Counted apart, and only now as a confirmed escape.
    let tally = store.crew_escape_tallies(PROJECT, 0).unwrap();
    assert_eq!((tally["alpha"].possible, tally["alpha"].confirmed), (0, 1));
}

#[test]
fn a_person_dismisses_and_the_same_act_twice_writes_nothing_more() {
    let (_dir, store, _task, id) = with_possible();
    let first = store
        .decide_escape(
            PROJECT,
            &id,
            EscapeDecision::Dismiss,
            &seen(EscapeState::Possible),
            None,
            AT,
        )
        .unwrap();
    assert_eq!(first.state, EscapeState::Dismissed);
    let events = store.crew_escape_events(&id, 10).unwrap().len();
    let audit = audits(&store).len();

    // A retry after a lost answer, with the old view or the new one, is not an error and writes nothing.
    for view in [EscapeState::Possible, EscapeState::Dismissed] {
        let again = store
            .decide_escape(
                PROJECT,
                &id,
                EscapeDecision::Dismiss,
                &seen(view),
                None,
                AT + 60,
            )
            .unwrap();
        assert_eq!(again, first);
    }
    assert_eq!(store.crew_escape_events(&id, 10).unwrap().len(), events);
    assert_eq!(audits(&store).len(), audit);
}

#[test]
fn a_decided_escape_cannot_be_decided_the_other_way_and_the_refusal_is_plain() {
    let (_dir, store, _task, id) = with_possible();
    store
        .decide_escape(
            PROJECT,
            &id,
            EscapeDecision::Confirm,
            &seen(EscapeState::Possible),
            None,
            AT,
        )
        .unwrap();
    let kept = store.crew_escape(&id).unwrap().unwrap();
    let refused = store
        .decide_escape(
            PROJECT,
            &id,
            EscapeDecision::Dismiss,
            &seen(EscapeState::Confirmed),
            None,
            AT + 1,
        )
        .unwrap_err();
    assert!(
        matches!(&refused, CoreError::Invalid(m) if m.contains("already confirmed")),
        "{refused:?}"
    );
    assert_eq!(store.crew_escape(&id).unwrap().unwrap(), kept);

    let (_dir, store, _task, id) = with_possible();
    store
        .decide_escape(
            PROJECT,
            &id,
            EscapeDecision::Dismiss,
            &seen(EscapeState::Possible),
            None,
            AT,
        )
        .unwrap();
    let refused = store
        .decide_escape(
            PROJECT,
            &id,
            EscapeDecision::Confirm,
            &seen(EscapeState::Dismissed),
            None,
            AT + 1,
        )
        .unwrap_err();
    assert!(
        matches!(&refused, CoreError::Invalid(m) if m.contains("already dismissed")),
        "{refused:?}"
    );
    assert_eq!(
        kinds(&store, &id),
        [EscapeEventKind::Detected, EscapeEventKind::Dismissed]
    );
}

#[test]
fn a_choice_on_an_escape_that_changed_is_stale_and_writes_nothing() {
    let (_dir, store, _task, id) = with_possible();
    store
        .decide_escape(
            PROJECT,
            &id,
            EscapeDecision::Confirm,
            &seen(EscapeState::Possible),
            None,
            AT,
        )
        .unwrap();
    // Someone else confirmed it; a person who still sees it as possible and chooses to dismiss is told it changed.
    let stale = store
        .decide_escape(
            PROJECT,
            &id,
            EscapeDecision::Dismiss,
            &seen(EscapeState::Possible),
            None,
            AT + 1,
        )
        .unwrap_err();
    assert!(matches!(stale, CoreError::Stale(_)), "{stale:?}");

    // A gap set since the person looked makes the view stale as well.
    let (_dir, store, _task, id) = with_possible();
    store
        .set_escape_gap(
            PROJECT,
            &id,
            &seen(EscapeState::Possible),
            EscapeGap::TestGap,
            AT,
        )
        .unwrap();
    let before = (store.crew_escape(&id).unwrap(), audits(&store).len());
    let stale = store
        .decide_escape(
            PROJECT,
            &id,
            EscapeDecision::Confirm,
            &seen(EscapeState::Possible),
            None,
            AT + 1,
        )
        .unwrap_err();
    assert!(matches!(stale, CoreError::Stale(_)), "{stale:?}");
    assert_eq!(
        (store.crew_escape(&id).unwrap(), audits(&store).len()),
        before
    );
}

#[test]
fn another_projects_escape_and_merge_are_not_found_and_stay_untouched() {
    let (_dir, store, task, id) = with_possible();
    store
        .writer()
        .execute(
            "INSERT INTO projects (id, name, path, trusted, is_git, created_at, managed, scratch) VALUES ('p2', 'q', '/q', 0, 1, 't', 0, 0)",
            [],
        )
        .unwrap();
    let before = store.crew_escape(&id).unwrap();
    let other =
        |r: Result<EscapeRecord>| assert!(matches!(r, Err(CoreError::NotFound(_))), "{r:?}");
    for decision in [EscapeDecision::Confirm, EscapeDecision::Dismiss] {
        other(store.decide_escape("p2", &id, decision, &seen(EscapeState::Possible), None, AT));
    }
    other(store.set_escape_gap(
        "p2",
        &id,
        &seen(EscapeState::Possible),
        EscapeGap::TestGap,
        AT,
    ));
    other(store.mark_escape("p2", &task, None, None, AT));
    assert_eq!(store.crew_escape(&id).unwrap(), before);
    assert!(audits(&store).is_empty());
    assert_eq!(store.crew_escape_tallies("p2", 0).unwrap().len(), 0);
    // An unknown escape and an unknown merge are not found either.
    other(store.decide_escape(
        PROJECT,
        "esc_none",
        EscapeDecision::Confirm,
        &seen(EscapeState::Possible),
        None,
        AT,
    ));
    other(store.mark_escape(PROJECT, "sha256:none", None, None, AT));
}

#[test]
fn a_mark_confirms_at_once_and_nothing_else_does() {
    let (_dir, store) = open();
    let task = seed(&store.writer(), 1, "waiting", None, "alpha", 100);
    assert!(escapes(&store).is_empty());

    let marked = store
        .mark_escape(PROJECT, &task, None, Some("failed in production"), AT)
        .unwrap();
    assert_eq!(
        (
            marked.source,
            marked.state,
            marked.decided_by,
            marked.decided_unix
        ),
        (
            EscapeSource::PersonMarked,
            EscapeState::Confirmed,
            Some(EscapeActor::Person),
            Some(AT)
        )
    );
    assert_eq!(
        (
            marked.engine.as_str(),
            marked.found_unix,
            marked.reason.as_deref()
        ),
        ("alpha", AT, Some("failed in production"))
    );
    assert_eq!(marked.areas, vec!["src/app".to_owned()]);
    assert_eq!(
        kinds(&store, &marked.id),
        [EscapeEventKind::Detected, EscapeEventKind::Confirmed]
    );
    assert_eq!(audits(&store).len(), 1);

    // Marking again changes nothing; marking a revert that the person left possible confirms it, keeping its source.
    assert_eq!(
        store
            .mark_escape(PROJECT, &task, None, None, AT + 9)
            .unwrap(),
        marked
    );
    assert_eq!(audits(&store).len(), 1);
    let other = seed(&store.writer(), 2, "waiting", None, "alpha", 200);
    store.set_merge_reverted(&other, "feed02").unwrap();
    let possible = store.crew_escape_of_merge(&other).unwrap().unwrap();
    assert_eq!(possible.state, EscapeState::Possible);
    let confirmed = store
        .mark_escape(
            PROJECT,
            &other,
            Some(&seen(EscapeState::Possible)),
            None,
            AT,
        )
        .unwrap();
    assert_eq!(
        (confirmed.id.as_str(), confirmed.source, confirmed.state),
        (
            possible.id.as_str(),
            EscapeSource::Revert,
            EscapeState::Confirmed
        )
    );
    assert_eq!(
        kinds(&store, &possible.id),
        [EscapeEventKind::Detected, EscapeEventKind::Confirmed]
    );
    assert_eq!(escapes(&store).len(), 2, "one escape per merge");
}

#[test]
fn a_dismissed_escape_is_not_marked_again_and_a_stale_view_is_refused() {
    let (_dir, store, task, id) = with_possible();
    let stale = store
        .mark_escape(
            PROJECT,
            &task,
            Some(&seen(EscapeState::Confirmed)),
            None,
            AT,
        )
        .unwrap_err();
    assert!(matches!(stale, CoreError::Stale(_)), "{stale:?}");
    store
        .decide_escape(
            PROJECT,
            &id,
            EscapeDecision::Dismiss,
            &seen(EscapeState::Possible),
            None,
            AT,
        )
        .unwrap();
    let refused = store
        .mark_escape(PROJECT, &task, None, None, AT + 1)
        .unwrap_err();
    assert!(
        matches!(&refused, CoreError::Invalid(m) if m.contains("dismissed")),
        "{refused:?}"
    );
    assert_eq!(
        store.crew_escape(&id).unwrap().unwrap().state,
        EscapeState::Dismissed
    );
}

#[test]
fn a_person_sets_the_gap_changes_it_and_repeats_it_for_nothing() {
    let (_dir, store, _task, id) = with_possible();
    let set = store
        .set_escape_gap(
            PROJECT,
            &id,
            &seen(EscapeState::Possible),
            EscapeGap::TestGap,
            AT,
        )
        .unwrap();
    assert_eq!(
        (set.gap, set.gap_by, set.state),
        (
            Some(EscapeGap::TestGap),
            Some(EscapeActor::Person),
            EscapeState::Possible
        )
    );
    let audit = audits(&store).len();
    // The same gap again, with the old view, is not an error and writes nothing.
    assert_eq!(
        store
            .set_escape_gap(
                PROJECT,
                &id,
                &seen(EscapeState::Possible),
                EscapeGap::TestGap,
                AT + 1
            )
            .unwrap(),
        set
    );
    assert_eq!(audits(&store).len(), audit);

    // Another gap, and back again: each is its own event, so the unique cause never refuses a second change.
    let view = EscapeSeen {
        state: EscapeState::Possible,
        gap: Some(EscapeGap::TestGap),
    };
    let tooling = store
        .set_escape_gap(PROJECT, &id, &view, EscapeGap::ToolingGap, AT + 2)
        .unwrap();
    assert_eq!(tooling.gap, Some(EscapeGap::ToolingGap));
    let view = EscapeSeen {
        state: EscapeState::Possible,
        gap: Some(EscapeGap::ToolingGap),
    };
    store
        .set_escape_gap(PROJECT, &id, &view, EscapeGap::TestGap, AT + 3)
        .unwrap();
    assert_eq!(
        kinds(&store, &id),
        [
            EscapeEventKind::Detected,
            EscapeEventKind::Reclassified,
            EscapeEventKind::Reclassified,
            EscapeEventKind::Reclassified
        ]
    );
    let stale = store
        .set_escape_gap(
            PROJECT,
            &id,
            &seen(EscapeState::Possible),
            EscapeGap::ToolingGap,
            AT + 4,
        )
        .unwrap_err();
    assert!(matches!(stale, CoreError::Stale(_)), "{stale:?}");

    // A dismissed escape has no gap to set. The state does not move.
    store
        .decide_escape(
            PROJECT,
            &id,
            EscapeDecision::Dismiss,
            &EscapeSeen {
                state: EscapeState::Possible,
                gap: Some(EscapeGap::TestGap),
            },
            None,
            AT + 5,
        )
        .unwrap();
    let refused = store
        .set_escape_gap(
            PROJECT,
            &id,
            &EscapeSeen {
                state: EscapeState::Dismissed,
                gap: Some(EscapeGap::TestGap),
            },
            EscapeGap::ToolingGap,
            AT + 6,
        )
        .unwrap_err();
    assert!(matches!(refused, CoreError::Invalid(_)), "{refused:?}");
}

#[test]
fn two_people_racing_on_one_escape_leave_exactly_one_decision() {
    for _ in 0..8 {
        let (_dir, store, _task, id) = with_possible();
        let gate = Barrier::new(2);
        let results: Vec<(EscapeDecision, Result<EscapeRecord>)> = std::thread::scope(|scope| {
            let run = |decision| {
                let (store, gate, id) = (&store, &gate, &id);
                scope.spawn(move || {
                    gate.wait();
                    (
                        decision,
                        store.decide_escape(
                            PROJECT,
                            id,
                            decision,
                            &seen(EscapeState::Possible),
                            None,
                            AT,
                        ),
                    )
                })
            };
            let handles = [run(EscapeDecision::Confirm), run(EscapeDecision::Dismiss)];
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let wins: Vec<_> = results.iter().filter(|(_, r)| r.is_ok()).collect();
        assert_eq!(wins.len(), 1, "{results:?}");
        let loser = results.iter().find(|(_, r)| r.is_err()).unwrap();
        assert!(
            matches!(&loser.1, Err(CoreError::Stale(_) | CoreError::Invalid(_))),
            "{loser:?}"
        );
        let state = store.crew_escape(&id).unwrap().unwrap().state;
        assert_eq!(Some(state), wins[0].1.as_ref().ok().map(|r| r.state));
        let decided: Vec<_> = kinds(&store, &id)
            .into_iter()
            .filter(|k| *k != EscapeEventKind::Detected)
            .collect();
        assert_eq!(decided.len(), 1);
        assert_eq!(audits(&store).len(), 1);
    }
}

#[test]
fn a_full_log_refuses_more_and_never_drops_a_row() {
    let (_dir, store, task, id) = with_possible();
    // 20,000 audit rows for the project fill the event limit; the escape still stands as it was and the act is refused.
    store
        .writer()
        .execute(
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 20000)
             INSERT INTO crew_escape_events (escape_id, project_id, kind, decided_by, cause, at)
             SELECT 'ghost', ?1, 'detected', 'core', 'c' || i, 1 FROM n",
            [PROJECT],
        )
        .unwrap();
    let refused = store
        .decide_escape(
            PROJECT,
            &id,
            EscapeDecision::Confirm,
            &seen(EscapeState::Possible),
            None,
            AT,
        )
        .unwrap_err();
    assert!(
        matches!(&refused, CoreError::Invalid(m) if m.contains("full")),
        "{refused:?}"
    );
    assert_eq!(
        store.crew_escape(&id).unwrap().unwrap().state,
        EscapeState::Possible
    );
    assert!(
        audits(&store).is_empty(),
        "a refused act leaves no audit row"
    );
    let _ = task;

    // 5,000 escapes fill the escape limit: a mark is refused.
    let (_dir, store) = open();
    let task = seed(&store.writer(), 1, "waiting", None, "alpha", 100);
    {
        let conn = store.writer();
        conn.execute_batch(
            "PRAGMA foreign_keys = OFF;
             WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 5000)
             INSERT INTO crew_escapes (id, project_id, merge_task_id, source, state, engine, areas, found_unix)
             SELECT 'g' || i, 'p1', 'ghost' || i, 'revert', 'possible', 'alpha', '[]', 1 FROM n;
             PRAGMA foreign_keys = ON;",
        )
        .unwrap();
    }
    let refused = store
        .mark_escape(PROJECT, &task, None, None, AT)
        .unwrap_err();
    assert!(
        matches!(&refused, CoreError::Invalid(m) if m.contains("full")),
        "{refused:?}"
    );
    assert!(store.crew_escape_of_merge(&task).unwrap().is_none());
}
