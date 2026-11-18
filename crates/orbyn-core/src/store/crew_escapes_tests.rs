//! The escape log at rest: migration 51 on a database with old rows, the revert source (one possible escape, idempotent), the
//! immutable trail, the table checks, and reads that fail closed.

use rusqlite::{Connection, params};

use super::*;
use crate::testing::TempDir;
use orbyn_contracts::{EscapeActor, EscapeEventKind, EscapeSource, EscapeState};

const PROJECT: &str = "p1";

/// One project, its lead thread, and a task with a run and a merge (`review_state` as given), by raw SQL as an old release left them.
fn seed(
    conn: &Connection,
    n: u32,
    review_state: &str,
    reverted: Option<&str>,
    engine: &str,
    at: i64,
) -> String {
    let task = format!("sha256:{n:064x}");
    conn.execute(
        "INSERT OR IGNORE INTO projects (id, name, path, trusted, is_git, created_at, managed, scratch)
         VALUES (?1, 'p', '/p', 0, 1, 't', 0, 0)",
        [PROJECT],
    )
    .unwrap();
    conn.execute(
        "INSERT OR IGNORE INTO threads (id, project_id, title, engine, cwd, created_at, updated_at)
         VALUES ('lead-1', ?1, 'lead', 'fake', '/p', 't', 't')",
        [PROJECT],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO crew_tasks (id, crew_id, run_id, created_at, updated_at) VALUES (?1, 'crew-1', ?2, 't', 't')",
        params![task, format!("run-{n}")],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO crew_runs (task_id, lead_thread_id, member) VALUES (?1, 'lead-1', 'lead-01')",
        [&task],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO crew_merges (task_id, crew_id, repo, branch, before_commit, merge_commit, candidate, risk, reasons, areas,
                                  engine, mode, files, lines, proof, not_verified, merged_unix, review_state, reverted_commit, reviewed_unix)
         VALUES (?1, 'crew-1', '/p', 'main', 'a', 'b', 'c', 'low', '[]', '[\"src/app\"]', ?2, 'full', 1, 1, '{}', '[]', ?3, ?4, ?5, ?6)",
        params![task, engine, at, review_state, reverted, reverted.map(|_| at + 10)],
    )
    .unwrap();
    task
}

fn open() -> (TempDir, Store) {
    let dir = TempDir::new();
    let store = Store::open(&dir.path().join("orbyn.db"), "epoch-a").unwrap();
    (dir, store)
}

fn escapes(store: &Store) -> Vec<EscapeRecord> {
    store.crew_escapes_of_project(PROJECT, 0).unwrap()
}

#[test]
fn migration_51_follows_50_and_leaves_older_tables_alone() {
    let versions: Vec<i64> = migrations::MIGRATIONS.iter().map(|(v, _)| *v).collect();
    assert!(versions.windows(2).all(|w| w[1] == w[0] + 1));
    assert!(versions.contains(&51));

    let dir = TempDir::new();
    let path = dir.path().join("old.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(
        "PRAGMA foreign_keys=ON; CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL)",
    )
    .unwrap();
    for (version, sql) in migrations::MIGRATIONS.iter().filter(|(v, _)| *v <= 50) {
        conn.execute_batch(sql).unwrap();
        conn.execute(
            "INSERT INTO schema_migrations VALUES (?1,'fixture')",
            [version],
        )
        .unwrap();
    }
    let reverted = seed(&conn, 1, "reverted", Some("feed01"), "alpha", 100);
    let waiting = seed(&conn, 2, "waiting", None, "alpha", 200);
    let looks_good = seed(&conn, 3, "looks_good", None, "beta", 300);
    let before: Vec<(String, String, Option<String>)> = conn
        .prepare("SELECT task_id, review_state, reverted_commit FROM crew_merges ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    drop(conn);

    let store = Store::open(&path, "escapes-upgrade").unwrap();
    let upgrade = store.upgrade().expect("an upgrade");
    assert_eq!(
        (upgrade.from, upgrade.to),
        (50, migrations::MIGRATIONS.last().unwrap().0)
    );
    // The older tables are as they were.
    let after: Vec<(String, String, Option<String>)> = store
        .reader()
        .prepare("SELECT task_id, review_state, reverted_commit FROM crew_merges ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(store.project(PROJECT).unwrap().name, "p");

    // Exactly one possible escape: the reverted merge, from the `revert` source, with no decision.
    let log = escapes(&store);
    assert_eq!(log.len(), 1);
    let one = &log[0];
    assert_eq!(one.merge_task_id, reverted);
    assert_eq!(
        (one.source, one.state),
        (EscapeSource::Revert, EscapeState::Possible)
    );
    assert_eq!(
        (one.decided_unix, one.decided_by, one.gap, one.gap_by),
        (None, None, None, None)
    );
    assert_eq!(
        (one.engine.as_str(), one.found_unix, one.merged_unix),
        ("alpha", 110, 100)
    );
    assert_eq!(one.areas, vec!["src/app".to_owned()]);
    assert_eq!(one.reason.as_deref(), Some("The merge was reverted."));
    let events = store.crew_escape_events(&one.id, 10).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, EscapeEventKind::Detected);
    assert_eq!(events[0].cause, "revert:feed01");
    assert!(store.crew_escape_of_merge(&waiting).unwrap().is_none());
    assert!(store.crew_escape_of_merge(&looks_good).unwrap().is_none());

    // Reopening applies nothing and adds nothing.
    drop(store);
    let reopened = Store::open(&path, "escapes-reopen").unwrap();
    assert!(reopened.upgrade().is_none());
    assert_eq!(escapes(&reopened), log);
}

#[test]
fn a_revert_makes_one_possible_escape_and_repeating_it_makes_no_second() {
    let (_dir, store) = open();
    let task = seed(&store.writer(), 1, "waiting", None, "alpha", 100);
    let other = seed(&store.writer(), 2, "looks_good", None, "alpha", 100);
    assert!(escapes(&store).is_empty(), "a merge alone is no escape");

    store.set_merge_reverted(&task, "feed01").unwrap();
    let first = escapes(&store);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].merge_task_id, task);
    assert_eq!(
        (first[0].source, first[0].state),
        (EscapeSource::Revert, EscapeState::Possible)
    );
    assert!(store.crew_escape_of_merge(&other).unwrap().is_none());

    // Reverting again, even with another commit, adds neither an escape nor an event.
    store.set_merge_reverted(&task, "feed02").unwrap();
    store.set_merge_reverted(&task, "feed01").unwrap();
    assert_eq!(escapes(&store), first);
    assert_eq!(store.crew_escape_events(&first[0].id, 10).unwrap().len(), 1);

    // Another revert is another escape; the counts stay apart.
    store.set_merge_reverted(&other, "feed03").unwrap();
    assert_eq!(escapes(&store).len(), 2);
    let tallies = store.crew_escape_tallies(PROJECT, 0).unwrap();
    assert_eq!(
        tallies["alpha"],
        EscapeTally {
            merges: 2,
            possible: 2,
            confirmed: 0,
            dismissed: 0
        }
    );
}

#[test]
fn a_dismissed_escape_is_not_offered_again_by_a_later_revert_write() {
    let (_dir, store) = open();
    let task = seed(&store.writer(), 1, "waiting", None, "alpha", 100);
    store.set_merge_reverted(&task, "feed01").unwrap();
    store
        .writer()
        .execute(
            "UPDATE crew_escapes SET state = 'dismissed', decided_unix = 5, decided_by = 'person'",
            [],
        )
        .unwrap();
    store.set_merge_reverted(&task, "feed02").unwrap();
    let log = escapes(&store);
    assert_eq!(log.len(), 1);
    assert_eq!(
        (log[0].state, log[0].decided_by),
        (EscapeState::Dismissed, Some(EscapeActor::Person))
    );
}

#[test]
fn the_trail_is_immutable_and_unique_per_cause() {
    let (_dir, store) = open();
    let task = seed(&store.writer(), 1, "waiting", None, "alpha", 100);
    store.set_merge_reverted(&task, "feed01").unwrap();
    let id = escapes(&store)[0].id.clone();
    let conn = store.writer();
    let update = conn
        .execute("UPDATE crew_escape_events SET cause = 'x'", [])
        .unwrap_err();
    assert!(update.to_string().contains("immutable"), "{update}");
    let delete = conn
        .execute("DELETE FROM crew_escape_events", [])
        .unwrap_err();
    assert!(delete.to_string().contains("immutable"), "{delete}");
    let insert = |kind: &str, cause: &str| {
        conn.execute(
            "INSERT INTO crew_escape_events (escape_id, project_id, kind, to_state, decided_by, cause, at)
             VALUES (?1, ?2, ?3, 'possible', 'core', ?4, 9)",
            params![id, PROJECT, kind, cause],
        )
    };
    assert!(
        insert("detected", "revert:feed01").is_err(),
        "same escape, kind and cause"
    );
    assert!(insert("bogus", "x").is_err());
    assert!(insert("reclassified", "").is_err());
    assert!(insert("reclassified", &"x".repeat(201)).is_err());
    assert!(insert("reclassified", "person:1").is_ok());
    drop(conn);
    assert_eq!(store.crew_escape_events(&id, 10).unwrap().len(), 2);
}

#[test]
fn the_tables_refuse_values_outside_their_checks() {
    let (_dir, store) = open();
    let task = seed(&store.writer(), 1, "waiting", None, "alpha", 100);
    let conn = store.writer();
    let insert = |source: &str, state: &str, decided: Option<(i64, &str)>, reason: &str| {
        conn.execute(
            "INSERT INTO crew_escapes (id, project_id, merge_task_id, source, state, engine, areas, reason, found_unix, decided_unix, decided_by)
             VALUES ('e', ?1, ?2, ?3, ?4, 'alpha', '[]', ?5, 1, ?6, ?7)",
            params![PROJECT, task, source, state, reason, decided.map(|d| d.0), decided.map(|d| d.1)],
        )
    };
    assert!(
        insert("guess", "possible", None, "").is_err(),
        "an unknown source"
    );
    assert!(
        insert("revert", "likely", None, "").is_err(),
        "an unknown state"
    );
    assert!(
        insert("revert", "possible", Some((2, "person")), "").is_err(),
        "a possible escape has no decision"
    );
    assert!(
        insert("revert", "confirmed", None, "").is_err(),
        "a confirmed escape has a decision"
    );
    assert!(
        insert("revert", "confirmed", Some((2, "core")), "").is_err(),
        "only a person confirms"
    );
    assert!(
        insert("revert", "possible", None, &"x".repeat(201)).is_err(),
        "a reason is at most 200 characters"
    );
    assert!(
        conn.execute("UPDATE crew_escapes SET gap = 'wiring' WHERE 0", [])
            .is_ok(),
        "no row, no check"
    );
    assert!(insert("revert", "possible", None, &"x".repeat(200)).is_ok());
    // One escape per merge, whatever its source.
    assert!(insert("person_marked", "confirmed", Some((2, "person")), "").is_err());
}

#[test]
fn unreadable_rows_fail_the_whole_read_closed() {
    let (_dir, store) = open();
    let a = seed(&store.writer(), 1, "waiting", None, "alpha", 100);
    let b = seed(&store.writer(), 2, "waiting", None, "alpha", 200);
    store.set_merge_reverted(&a, "feed01").unwrap();
    store.set_merge_reverted(&b, "feed02").unwrap();
    assert_eq!(escapes(&store).len(), 2);

    // One unknown state word, as a damaged table or a newer release might leave.
    store
        .writer()
        .execute_batch(
            "PRAGMA ignore_check_constraints = ON;
             UPDATE crew_escapes SET state = 'bogus' WHERE merge_task_id = (SELECT task_id FROM crew_merges ORDER BY id LIMIT 1);
             PRAGMA ignore_check_constraints = OFF;",
        )
        .unwrap();
    let unreadable = |r: Result<()>| assert!(matches!(r, Err(CoreError::Unreadable(_))), "{r:?}");
    unreadable(store.crew_escapes_of_project(PROJECT, 0).map(|_| ()));
    unreadable(store.crew_escape_page(PROJECT, None, 50, 0).map(|_| ()));
    unreadable(store.crew_escape_of_merge(&a).map(|_| ()));
    unreadable(store.crew_escape_tallies(PROJECT, 0).map(|_| ()));
    // The sound row is still readable on its own, and says nothing of the other.
    assert!(store.crew_escape_of_merge(&b).unwrap().is_some());

    // A damaged areas value (valid JSON, wrong shape) is just as unreadable.
    store
        .writer()
        .execute_batch(
            "PRAGMA ignore_check_constraints = ON;
             UPDATE crew_escapes SET state = 'possible', areas = '{\"a\":1}';
             PRAGMA ignore_check_constraints = OFF;",
        )
        .unwrap();
    unreadable(store.crew_escapes_of_project(PROJECT, 0).map(|_| ()));
    store
        .writer()
        .execute_batch("ALTER TABLE crew_escapes RENAME TO gone")
        .unwrap();
    unreadable(store.crew_escape("x").map(|_| ()));
}

#[test]
fn pages_filter_by_state_and_windows_cut_by_merge_time() {
    let (_dir, store) = open();
    let tasks: Vec<String> = (1..=3)
        .map(|n| {
            seed(
                &store.writer(),
                n,
                "waiting",
                None,
                if n == 3 { "beta" } else { "alpha" },
                100 * i64::from(n),
            )
        })
        .collect();
    for (n, task) in tasks.iter().enumerate() {
        store
            .set_merge_reverted(task, &format!("feed0{n}"))
            .unwrap();
    }
    store
        .writer()
        .execute(
            "UPDATE crew_escapes SET state = 'dismissed', decided_unix = 5, decided_by = 'person' WHERE merge_task_id = ?1",
            [&tasks[0]],
        )
        .unwrap();
    let (all, total) = store.crew_escape_page(PROJECT, None, 2, 0).unwrap();
    assert_eq!((all.len(), total), (2, 3));
    let (rest, _) = store.crew_escape_page(PROJECT, None, 2, 2).unwrap();
    assert_eq!(rest.len(), 1);
    let (dismissed, total) = store
        .crew_escape_page(PROJECT, Some(EscapeState::Dismissed), 50, 0)
        .unwrap();
    assert_eq!((dismissed.len(), total), (1, 1));
    assert_eq!(dismissed[0].merge_task_id, tasks[0]);
    let (confirmed, total) = store
        .crew_escape_page(PROJECT, Some(EscapeState::Confirmed), 50, 0)
        .unwrap();
    assert!(
        confirmed.is_empty() && total == 0,
        "a revert is never confirmed by itself"
    );
    assert_eq!(
        store.crew_escapes_of_project(PROJECT, 150).unwrap().len(),
        2
    );
    let tallies = store.crew_escape_tallies(PROJECT, 150).unwrap();
    assert_eq!(
        tallies["alpha"],
        EscapeTally {
            merges: 1,
            possible: 1,
            confirmed: 0,
            dismissed: 0
        }
    );
    assert_eq!(
        tallies["beta"],
        EscapeTally {
            merges: 1,
            possible: 1,
            confirmed: 0,
            dismissed: 0
        }
    );
}

#[test]
fn a_log_over_the_cap_is_an_error_not_a_shortened_list() {
    let (_dir, store) = open();
    for n in 1..=3 {
        let task = seed(
            &store.writer(),
            n,
            "waiting",
            None,
            "alpha",
            100 * i64::from(n),
        );
        store.set_merge_reverted(&task, "feed").unwrap();
    }
    assert_eq!(
        store
            .escapes_of_project_capped(PROJECT, 0, 3)
            .unwrap()
            .len(),
        3
    );
    assert!(matches!(
        store.escapes_of_project_capped(PROJECT, 0, 2),
        Err(CoreError::Unreadable(_))
    ));
    assert_eq!(
        store
            .escapes_of_project_capped(PROJECT, 150, 2)
            .unwrap()
            .len(),
        2
    );
}
