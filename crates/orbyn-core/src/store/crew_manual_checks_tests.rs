//! The check a person makes at rest (migration 76): the wait a `manual` check parks, the person's decision on it, and the audit row that
//! comes with it. No engine and no checkout: the stored shapes only.

use rusqlite::params;

use super::*;
use crate::testing::TempDir;

fn open() -> (TempDir, Store) {
    let dir = TempDir::new();
    let store = Store::open(&dir.path().join("orbyn.db"), "epoch-a").unwrap();
    (dir, store)
}

fn add_task(store: &Store, n: u32) -> String {
    let id = format!("sha256:{n:064x}");
    store
        .writer()
        .execute(
            "INSERT INTO crew_tasks (id, crew_id, run_id, created_at, updated_at) VALUES (?1, 'crew-1', ?2, 't', 't')",
            params![id, format!("run-{n}")],
        )
        .unwrap();
    id
}

fn audit_rows(store: &Store, action: &str) -> i64 {
    store
        .reader()
        .query_row(
            "SELECT COUNT(*) FROM audit_events WHERE action = ?1",
            [action],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn a_manual_check_waits_until_a_person_decides_it() {
    let (_dir, store) = open();
    let task = add_task(&store, 1);
    store.park_manual_check(&task, "chk-manual", 100).unwrap();
    // Parking the same check again keeps the row it has: a later verification does not restart the wait.
    store.park_manual_check(&task, "chk-manual", 160).unwrap();
    let held = store
        .crew_manual_check(&task, "chk-manual")
        .unwrap()
        .unwrap();
    assert_eq!(held.state, ManualCheckState::Awaiting);
    assert_eq!(held.waiting_unix, 100);
    assert_eq!(
        store.crew_manual_checks_waiting(&task).unwrap(),
        ["chk-manual"]
    );
    // A person passes it, with a note.
    assert!(
        store
            .decide_manual_check(&task, "chk-manual", true, Some("Looked at the page."), 200)
            .unwrap()
    );
    let held = store
        .crew_manual_check(&task, "chk-manual")
        .unwrap()
        .unwrap();
    assert_eq!(held.state, ManualCheckState::Passed);
    assert_eq!(held.note.as_deref(), Some("Looked at the page."));
    assert_eq!(held.decided_unix, Some(200));
    assert!(store.crew_manual_checks_waiting(&task).unwrap().is_empty());
    assert_eq!(audit_rows(&store, "crew.task.decide_manual_check"), 1);
}

#[test]
fn the_same_decision_again_writes_nothing_and_the_opposite_is_refused() {
    let (_dir, store) = open();
    let task = add_task(&store, 2);
    store.park_manual_check(&task, "chk-manual", 100).unwrap();
    assert!(
        store
            .decide_manual_check(&task, "chk-manual", false, None, 200)
            .unwrap()
    );
    // A retry of the same denial after a lost answer is not an error, and writes no second audit row.
    assert!(
        !store
            .decide_manual_check(&task, "chk-manual", false, None, 210)
            .unwrap()
    );
    assert_eq!(audit_rows(&store, "crew.task.decide_manual_check"), 1);
    // The opposite decision on a decided check is refused in plain words.
    let refused = store
        .decide_manual_check(&task, "chk-manual", true, None, 220)
        .unwrap_err();
    assert!(
        refused.public_message().contains("already denied"),
        "{}",
        refused.public_message()
    );
    assert_eq!(audit_rows(&store, "crew.task.decide_manual_check"), 1);
    let held = store
        .crew_manual_check(&task, "chk-manual")
        .unwrap()
        .unwrap();
    assert_eq!(
        (held.state, held.decided_unix),
        (ManualCheckState::Failed, Some(200))
    );
}

#[test]
fn a_decision_for_a_check_the_task_never_parked_is_not_found() {
    let (_dir, store) = open();
    let task = add_task(&store, 3);
    store.park_manual_check(&task, "chk-manual", 100).unwrap();
    let refused = store
        .decide_manual_check(&task, "chk-other", true, None, 200)
        .unwrap_err();
    assert!(matches!(refused, CoreError::NotFound(_)), "{refused:?}");
    // Another task's wait is not this task's.
    let other = add_task(&store, 4);
    let refused = store
        .decide_manual_check(&other, "chk-manual", true, None, 200)
        .unwrap_err();
    assert!(matches!(refused, CoreError::NotFound(_)), "{refused:?}");
    assert_eq!(
        store.crew_manual_checks_waiting(&task).unwrap(),
        ["chk-manual"]
    );
}

#[test]
fn two_checks_of_one_task_wait_on_their_own_and_a_persons_note_is_bounded() {
    let (_dir, store) = open();
    let task = add_task(&store, 5);
    store.park_manual_check(&task, "chk-first", 100).unwrap();
    store.park_manual_check(&task, "chk-second", 101).unwrap();
    assert_eq!(
        store.crew_manual_checks_waiting(&task).unwrap(),
        ["chk-first", "chk-second"]
    );
    let long: String = "x".repeat(MANUAL_CHECK_NOTE_MAX + 50);
    store
        .decide_manual_check(&task, "chk-first", true, Some(&long), 200)
        .unwrap();
    let held = store
        .crew_manual_check(&task, "chk-first")
        .unwrap()
        .unwrap();
    assert_eq!(
        held.note.map(|note| note.chars().count()),
        Some(MANUAL_CHECK_NOTE_MAX)
    );
    // The second check still waits, and the task has one decision and one wait.
    assert_eq!(
        store.crew_manual_checks_waiting(&task).unwrap(),
        ["chk-second"]
    );
    let all = store.crew_manual_checks(&task).unwrap();
    assert_eq!(
        all.iter().map(|held| held.state).collect::<Vec<_>>(),
        [ManualCheckState::Passed, ManualCheckState::Awaiting]
    );
}
