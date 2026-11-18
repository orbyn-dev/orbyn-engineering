//! The scan's writes at rest: candidates are merges with no escape row, a batch is all or nothing against the 5,000-escape and
//! 20,000-event limits, a repeat adds nothing, a dismissed escape is never a candidate, and a scan cannot write a mark or a confirm.

use orbyn_contracts::{EscapeEventKind, EscapeSource, EscapeState};

use super::crew_escapes_tests::{PROJECT, escapes, open, seed};
use super::*;
use crate::store::EscapeDecision;

fn found(task: &str, source: EscapeSource, cause: &str) -> PossibleEscape {
    PossibleEscape {
        project_id: PROJECT.to_owned(),
        merge_task_id: task.to_owned(),
        source,
        cause: cause.to_owned(),
        reason: "Commit abc reverts this merge.".to_owned(),
    }
}

#[test]
fn candidates_are_merges_without_an_escape_newest_first_and_bounded() {
    let (_dir, store) = open();
    let old = seed(&store.writer(), 1, "waiting", None, "alpha", 100);
    let mid = seed(&store.writer(), 2, "waiting", None, "alpha", 200);
    let new = seed(&store.writer(), 3, "waiting", None, "beta", 300);
    let reverted = seed(&store.writer(), 4, "waiting", None, "alpha", 400);
    store.set_merge_reverted(&reverted, "feed01").unwrap();
    let ids = |since, limit| -> Vec<String> {
        store
            .crew_escape_scan_candidates(since, limit)
            .unwrap()
            .into_iter()
            .map(|(m, project)| {
                assert_eq!(project, PROJECT);
                m.task_id
            })
            .collect()
    };
    // The reverted merge has its escape from the schema's trigger, so it is not a candidate.
    assert_eq!(ids(0, 50), vec![new.clone(), mid.clone(), old.clone()]);
    assert_eq!(ids(150, 50), vec![new.clone(), mid.clone()]);
    assert_eq!(ids(0, 2), vec![new, mid]);
    let _ = (old, reverted);
}

#[test]
fn a_batch_adds_possible_rows_only_with_one_detected_event_and_a_repeat_adds_nothing() {
    let (_dir, store) = open();
    let a = seed(&store.writer(), 1, "waiting", None, "alpha", 100);
    let b = seed(&store.writer(), 2, "waiting", None, "alpha", 200);
    let batch = vec![
        found(&a, EscapeSource::Revert, "revert:aaa"),
        found(&b, EscapeSource::FollowUpFix, "follow_up_fix:bbb"),
    ];
    let first = store.record_possible_escapes(PROJECT, &batch, 900).unwrap();
    assert_eq!((first.added, first.already_held), (2, 0));
    let rows = escapes(&store);
    assert_eq!(rows.len(), 2);
    for row in &rows {
        assert_eq!(row.state, EscapeState::Possible);
        assert_eq!((row.decided_unix, row.decided_by), (None, None));
        assert_eq!(row.found_unix, 900);
        assert_eq!(row.engine, "alpha");
        assert_eq!(row.areas, vec!["src/app".to_owned()]);
        let events = store.crew_escape_events(&row.id, 10).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EscapeEventKind::Detected);
        assert_eq!(events[0].decided_by, orbyn_contracts::EscapeActor::Core);
        assert_eq!(events[0].to_state, Some(EscapeState::Possible));
    }
    let again = store.record_possible_escapes(PROJECT, &batch, 950).unwrap();
    assert_eq!((again.added, again.already_held), (0, 2));
    assert_eq!(escapes(&store), rows, "a repeat changes nothing");
    assert!(store.crew_escape_scan_candidates(0, 50).unwrap().is_empty());
}

#[test]
fn a_dismissed_escape_is_not_a_candidate_and_is_not_offered_again() {
    let (_dir, store) = open();
    let task = seed(&store.writer(), 1, "waiting", None, "alpha", 100);
    let item = found(&task, EscapeSource::FollowUpFix, "follow_up_fix:abc");
    store
        .record_possible_escapes(PROJECT, std::slice::from_ref(&item), 500)
        .unwrap();
    let id = escapes(&store)[0].id.clone();
    store
        .decide_escape(
            PROJECT,
            &id,
            EscapeDecision::Dismiss,
            &orbyn_contracts::EscapeSeen {
                state: EscapeState::Possible,
                gap: None,
            },
            None,
            600,
        )
        .unwrap();
    assert!(store.crew_escape_scan_candidates(0, 50).unwrap().is_empty());
    // Even a scan that found it again, with new evidence, adds nothing and does not undo the dismissal.
    let other = found(&task, EscapeSource::Revert, "revert:new");
    let added = store
        .record_possible_escapes(PROJECT, &[other], 700)
        .unwrap();
    assert_eq!((added.added, added.already_held), (0, 1));
    let row = &escapes(&store)[0];
    assert_eq!(
        (row.state, row.source),
        (EscapeState::Dismissed, EscapeSource::FollowUpFix)
    );
    assert_eq!(store.crew_escape_events(&id, 10).unwrap().len(), 2);
}

#[test]
fn a_scan_never_writes_a_mark_and_never_another_projects_merge() {
    let (_dir, store) = open();
    let task = seed(&store.writer(), 1, "waiting", None, "alpha", 100);
    let mark = found(&task, EscapeSource::PersonMarked, "person:1");
    assert!(store.record_possible_escapes(PROJECT, &[mark], 1).is_err());
    let foreign = PossibleEscape {
        project_id: "other".into(),
        ..found(&task, EscapeSource::Revert, "revert:x")
    };
    assert!(
        store
            .record_possible_escapes(PROJECT, std::slice::from_ref(&foreign), 1)
            .is_err()
    );
    // A merge of another project named under this project's id is skipped by the SQL, not written.
    store
        .record_possible_escapes("other", &[foreign], 1)
        .unwrap();
    assert!(escapes(&store).is_empty());
}

#[test]
fn a_batch_that_would_pass_the_escape_limit_is_refused_whole_and_writes_nothing() {
    let (_dir, store) = open();
    let a = seed(&store.writer(), 1, "waiting", None, "alpha", 100);
    let b = seed(&store.writer(), 2, "waiting", None, "alpha", 200);
    store.fill_escape_log_for_test(PROJECT, 4999, 0).unwrap();
    let batch = vec![
        found(&a, EscapeSource::Revert, "revert:a"),
        found(&b, EscapeSource::Revert, "revert:b"),
    ];
    let refused = store
        .record_possible_escapes(PROJECT, &batch, 5)
        .unwrap_err();
    assert!(
        matches!(&refused, CoreError::Invalid(m) if m.contains("full")),
        "{refused:?}"
    );
    assert!(store.crew_escape_of_merge(&a).unwrap().is_none());
    assert!(store.crew_escape_of_merge(&b).unwrap().is_none());
    assert_eq!(store.crew_escape_room(PROJECT).unwrap().0, 4999);
    // One fits exactly: the limit is 5,000.
    let one = store
        .record_possible_escapes(PROJECT, &batch[..1], 5)
        .unwrap();
    assert_eq!(one.added, 1);
    assert_eq!(store.crew_escape_room(PROJECT).unwrap().0, 5000);
}

#[test]
fn a_batch_that_would_pass_the_event_limit_is_refused_whole_and_writes_nothing() {
    let (_dir, store) = open();
    let a = seed(&store.writer(), 1, "waiting", None, "alpha", 100);
    let b = seed(&store.writer(), 2, "waiting", None, "alpha", 200);
    store.fill_escape_log_for_test(PROJECT, 0, 19_999).unwrap();
    let batch = vec![
        found(&a, EscapeSource::Revert, "revert:a"),
        found(&b, EscapeSource::Revert, "revert:b"),
    ];
    let refused = store
        .record_possible_escapes(PROJECT, &batch, 5)
        .unwrap_err();
    assert!(
        matches!(&refused, CoreError::Invalid(m) if m.contains("full")),
        "{refused:?}"
    );
    assert!(escapes(&store).is_empty());
    assert_eq!(store.crew_escape_room(PROJECT).unwrap(), (0, 19_999));
}
