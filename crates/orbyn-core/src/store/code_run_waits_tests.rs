//! Migration 74 and the wait total it adds to Code Mode runs (docs/code-mode.md, "Run record").

use super::Store;
use super::migrations;
use crate::testing::TempDir;
use rusqlite::{Connection, params};

fn apply(conn: &Connection, last: i64) {
    for (_, sql) in migrations::MIGRATIONS
        .iter()
        .filter(|(version, _)| *version <= last)
    {
        conn.execute_batch(sql).unwrap();
    }
}

#[test]
fn migration_74_follows_73_and_leaves_an_old_run_without_a_wait() {
    let versions: Vec<i64> = migrations::MIGRATIONS
        .iter()
        .map(|(version, _)| *version)
        .collect();
    let at = versions.iter().position(|v| *v == 74).unwrap();
    assert_eq!(versions[at - 1], 73);
    let (_, sql) = migrations::MIGRATIONS
        .iter()
        .find(|(version, _)| *version == 74)
        .unwrap();
    assert!(sql.contains("ALTER TABLE code_runs ADD COLUMN waited_ms INTEGER"));
    assert!(sql.contains("ALTER TABLE approvals ADD COLUMN created_at_ms INTEGER"));

    let conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "foreign_keys", "OFF").unwrap();
    apply(&conn, 73);
    conn.execute(
        "INSERT INTO code_runs (run_id, thread_id, status, script, script_sha256,
            seed_start_ms, seed, started_at)
         VALUES (?1, ?2, 'running', 'return 1', 'sha', 0, 1, '2026-11-21T10:00:00Z')",
        params!["old", "thread"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO approvals (id, thread_id, run_id, engine, kind, target, summary, detail,
            state, created_at)
         VALUES ('old-approval', 'thread', 'run', 'fake', 'tool', 't', 's', '{}', 'pending',
            '2026-11-21T10:00:00Z')",
        [],
    )
    .unwrap();
    conn.execute_batch(sql).unwrap();

    let waited: Option<i64> = conn
        .query_row(
            "SELECT waited_ms FROM code_runs WHERE run_id = 'old'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(waited, None, "an old run keeps no number");
    let created: Option<i64> = conn
        .query_row(
            "SELECT created_at_ms FROM approvals WHERE id = 'old-approval'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(created, None, "an old approval has no start");
    let columns: Vec<String> = conn
        .prepare("PRAGMA table_info(approvals)")
        .unwrap()
        .query_map([], |row| row.get(1))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(columns.iter().any(|name| name == "created_at_ms"));
}

#[test]
fn migration_74_applies_through_store_open_and_reopening_is_a_no_op() {
    let dir = TempDir::new();
    let path = dir.path().join("db");
    {
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "foreign_keys", "OFF").unwrap();
        apply(&conn, 73);
        conn.execute_batch(
            "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL)",
        )
        .unwrap();
        for (version, _) in migrations::MIGRATIONS
            .iter()
            .filter(|(version, _)| *version <= 73)
        {
            conn.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, 'fixture')",
                [version],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO code_runs (run_id, thread_id, status, script, script_sha256,
                seed_start_ms, seed, started_at)
             VALUES ('old', 'thread', 'running', 'return 1', 'sha', 0, 1, '2026-11-21T10:00:00Z')",
            [],
        )
        .unwrap();
    }

    let store = Store::open(&path, "epoch-a").unwrap();
    let applied: i64 = store
        .reader()
        .query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version = 74",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(applied, 1, "Store::open applies migration 74");
    assert_eq!(store.code_run("old").unwrap().unwrap().waited_ms, None);

    let before: i64 = store
        .reader()
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .unwrap();
    drop(store);
    let reopened = Store::open(&path, "epoch-b").unwrap();
    let after: i64 = reopened
        .reader()
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(
        before, after,
        "reopening an already migrated store writes no migration"
    );
}
