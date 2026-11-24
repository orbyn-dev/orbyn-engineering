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
    // Later migrations may follow; 74 must come straight after 73.
    let at = versions.iter().position(|version| *version == 74).unwrap();
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

/// The list row names how a run ended badly, so the runs list can tell a cancelled run from a failed one.
#[test]
fn the_runs_list_carries_the_error_kind_of_a_run_that_ended_badly() {
    use super::code_runs::{CodeRunEnd, NewCodeRun};
    use super::{NewProject, NewThread};
    let dir = TempDir::new();
    let store = Store::open(&dir.path().join("db"), "epoch-a").unwrap();
    let project = store
        .add_project(NewProject {
            path: "/repo",
            name: "repo",
            is_git: true,
            managed: false,
            scratch: false,
        })
        .unwrap();
    let thread = store
        .create_thread(NewThread {
            project_id: project.id,
            title: "t".into(),
            engine: "fake".into(),
            model: None,
            cwd: "/repo".into(),
            worktree: None,
            skill_profile: None,
            parent_thread_id: None,
            relation: None,
            scratch: false,
        })
        .unwrap()
        .id;
    let begin = |run_id: &str, at: &str| {
        store
            .code_run_begin(&NewCodeRun {
                run_id: run_id.to_owned(),
                thread_id: thread.clone(),
                script: "return 1".to_owned(),
                script_sha256: "sha".to_owned(),
                seed_start_ms: 0,
                seed: 1,
                started_at: at.to_owned(),
                crew_worker: false,
            })
            .unwrap();
    };
    let end = |run_id: &str, status: &'static str, error: Option<&str>| {
        store
            .code_run_end(
                run_id,
                &CodeRunEnd {
                    status,
                    ended_at: "2026-11-23T10:00:00Z".to_owned(),
                    error: error.map(str::to_owned),
                    result: None,
                    logs: Vec::new(),
                    logs_total: 0,
                    truncated: false,
                },
            )
            .unwrap();
    };
    begin("a-cancelled", "2026-11-23T09:00:01Z");
    end(
        "a-cancelled",
        "error",
        Some(r#"{"kind":"cancelled","message":"The run was cancelled."}"#),
    );
    begin("b-failed", "2026-11-23T09:00:02Z");
    end(
        "b-failed",
        "error",
        Some(r#"{"kind":"runtime","message":"boom","line":1}"#),
    );
    begin("c-unreadable", "2026-11-23T09:00:03Z");
    end("c-unreadable", "error", Some("not json"));
    begin("d-done", "2026-11-23T09:00:04Z");
    end("d-done", "completed", None);

    let rows = store.code_runs_of(&thread, 10).unwrap();
    let kind = |id: &str| {
        rows.iter()
            .find(|row| row.run_id == id)
            .unwrap()
            .error_kind
            .clone()
    };
    assert_eq!(kind("a-cancelled").as_deref(), Some("cancelled"));
    assert_eq!(kind("b-failed").as_deref(), Some("runtime"));
    assert_eq!(
        kind("c-unreadable"),
        None,
        "an error row with no kind is null"
    );
    assert_eq!(kind("d-done"), None, "a run that did not fail has none");
}
