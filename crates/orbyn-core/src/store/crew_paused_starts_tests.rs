//! Refused starts at rest: one row per trigger, the cap, and migration 54 on an older database.

use rusqlite::Connection;

use super::*;
use crate::crew::capacity::PausedKind;
use crate::testing::TempDir;

fn project(store: &Store, path: &str) -> String {
    store
        .add_project(NewProject {
            path,
            name: "p",
            is_git: true,
            managed: false,
            scratch: false,
        })
        .unwrap()
        .id
}

fn open() -> (TempDir, Store, String) {
    let dir = TempDir::new();
    let store = Store::open(&dir.path().join("orbyn.db"), "epoch-a").unwrap();
    let project = project(&store, "/repo/a");
    (dir, store, project)
}

#[test]
fn a_repeat_of_one_trigger_updates_its_row_and_a_new_trigger_adds_one() {
    let (_dir, store, p) = open();
    store
        .record_paused_start(&p, PausedKind::ReviewFix, "fp1", 9, 8, 100)
        .unwrap();
    store
        .record_paused_start(&p, PausedKind::ReviewFix, "fp1", 12, 8, 200)
        .unwrap();
    store
        .record_paused_start(&p, PausedKind::CommentFix, "fp1", 12, 8, 150)
        .unwrap();
    let rows = store.crew_paused_starts(&p).unwrap();
    assert_eq!(rows.len(), 2);
    let review = rows
        .iter()
        .find(|r| r.kind == PausedKind::ReviewFix)
        .unwrap();
    assert_eq!(
        (
            review.waiting,
            review.first_seen_unix,
            review.last_seen_unix,
            review.started
        ),
        (12, 100, 200, false)
    );
    assert_eq!(rows[0].kind, PausedKind::ReviewFix, "newest sighting first");
}

#[test]
fn started_marks_only_a_held_row_and_a_new_refusal_holds_it_again() {
    let (_dir, store, p) = open();
    store
        .mark_paused_start_started(&p, PausedKind::ReviewFix, "none", 50)
        .unwrap();
    assert!(store.crew_paused_starts(&p).unwrap().is_empty());
    store
        .record_paused_start(&p, PausedKind::ReviewFix, "fp", 9, 8, 100)
        .unwrap();
    store
        .mark_paused_start_started(&p, PausedKind::ReviewFix, "fp", 300)
        .unwrap();
    let row = store.crew_paused_starts(&p).unwrap().remove(0);
    assert_eq!((row.started, row.started_unix), (true, Some(300)));
    store
        .record_paused_start(&p, PausedKind::ReviewFix, "fp", 10, 8, 400)
        .unwrap();
    let row = store.crew_paused_starts(&p).unwrap().remove(0);
    assert_eq!(
        (row.started, row.started_unix, row.first_seen_unix),
        (false, None, 400)
    );
}

#[test]
fn rows_are_capped_per_project_and_the_oldest_go_first() {
    let (_dir, store, p) = open();
    let other = project(&store, "/repo/b");
    store
        .record_paused_start(&other, PausedKind::CommentFix, "keep", 9, 8, 1)
        .unwrap();
    for n in 0..(PAUSED_STARTS_PER_PROJECT + 5) {
        store
            .record_paused_start(
                &p,
                PausedKind::ReviewFix,
                &format!("k{n}"),
                9,
                8,
                1000 + i64::from(n),
            )
            .unwrap();
    }
    let rows = store.crew_paused_starts(&p).unwrap();
    assert_eq!(rows.len(), PAUSED_STARTS_PER_PROJECT as usize);
    assert!(rows.iter().all(|r| r.key != "k0" && r.key != "k4"));
    assert!(rows.iter().any(|r| r.key == "k5"));
    assert_eq!(store.crew_paused_starts(&other).unwrap().len(), 1);
}

#[test]
fn the_table_refuses_values_outside_its_checks() {
    let (_dir, store, p) = open();
    store
        .record_paused_start(&p, PausedKind::Background, "b", 9, 8, 1)
        .unwrap();
    let conn = store.writer();
    for sql in [
        "UPDATE crew_paused_starts SET kind = 'other'",
        "UPDATE crew_paused_starts SET state = 'gone'",
        "UPDATE crew_paused_starts SET key = ''",
        "UPDATE crew_paused_starts SET waiting = -1",
    ] {
        assert!(conn.execute(sql, []).is_err(), "{sql}");
    }
    drop(conn);
}

#[test]
fn the_cap_drops_started_rows_before_held_ones() {
    let (_dir, store, p) = open();
    for n in 0..PAUSED_STARTS_PER_PROJECT {
        store
            .record_paused_start(
                &p,
                PausedKind::ReviewFix,
                &format!("h{n}"),
                9,
                8,
                1000 + i64::from(n),
            )
            .unwrap();
    }
    // The oldest row has started; a new refusal pushes the project over the cap.
    store
        .mark_paused_start_started(&p, PausedKind::ReviewFix, "h0", 5000)
        .unwrap();
    store
        .record_paused_start(&p, PausedKind::ReviewFix, "new", 9, 8, 6000)
        .unwrap();
    let rows = store.crew_paused_starts(&p).unwrap();
    assert_eq!(rows.len(), PAUSED_STARTS_PER_PROJECT as usize);
    assert!(
        rows.iter().all(|r| r.key != "h0"),
        "the started row went first"
    );
    assert!(rows.iter().any(|r| r.key == "h1" && !r.started));
}

#[test]
fn a_project_delete_removes_its_rows() {
    let (_dir, store, p) = open();
    store
        .record_paused_start(&p, PausedKind::Unknown, "u", 9, 8, 1)
        .unwrap();
    store
        .writer()
        .execute("DELETE FROM projects WHERE id = ?1", [&p])
        .unwrap();
    let left: i64 = store
        .reader()
        .query_row("SELECT COUNT(*) FROM crew_paused_starts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(left, 0);
}

#[test]
fn migration_54_follows_53_and_applies_on_a_database_with_old_rows() {
    let versions: Vec<i64> = migrations::MIGRATIONS.iter().map(|(v, _)| *v).collect();
    assert!(versions.windows(2).all(|w| w[1] == w[0] + 1));
    assert!(versions.contains(&54));
    let dir = TempDir::new();
    let path = dir.path().join("old.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL)").unwrap();
    for (version, sql) in migrations::MIGRATIONS.iter().filter(|(v, _)| *v <= 53) {
        conn.execute_batch(sql).unwrap();
        conn.execute(
            "INSERT INTO schema_migrations VALUES (?1,'fixture')",
            [version],
        )
        .unwrap();
    }
    drop(conn);
    let store = Store::open(&path, "paused-upgrade").unwrap();
    let upgrade = store.upgrade().expect("an upgrade");
    assert_eq!(
        (upgrade.from, upgrade.to),
        (53, migrations::MIGRATIONS.last().unwrap().0)
    );
    let p = project(&store, "/repo/a");
    assert!(store.crew_paused_starts(&p).unwrap().is_empty());
}
