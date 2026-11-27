//! Migration 90: a repository's worktree records the branch it was cut from. A row written before the column keeps
//! no base rather than a guessed one, and a write through the current store reads its base back.

use rusqlite::Connection;

use super::*;
use crate::store::thread_repos::ThreadRepoRecord;
use crate::testing::TempDir;

/// A database exactly at `through`, created the way an older install left it.
fn database(dir: &TempDir, through: i64) -> (std::path::PathBuf, Connection) {
    let path = dir.path().join("old.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(
        "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL)",
    )
    .unwrap();
    for (version, sql) in migrations::MIGRATIONS
        .iter()
        .filter(|(version, _)| *version <= through)
    {
        conn.execute_batch(sql).unwrap();
        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, 'fixture')",
            [version],
        )
        .unwrap();
    }
    (path, conn)
}

fn record(thread_id: &str, repo_id: &str, position: u32, base: Option<&str>) -> ThreadRepoRecord {
    ThreadRepoRecord {
        thread_id: thread_id.to_owned(),
        repo_id: repo_id.to_owned(),
        position,
        name: repo_id.to_owned(),
        branch: "orbyn/1234".into(),
        base_branch: base.map(str::to_owned),
        worktree_path: format!("/threads/{thread_id}/{repo_id}"),
    }
}

#[test]
fn a_row_stored_before_the_column_keeps_no_base_and_new_rows_read_theirs_back() {
    let dir = TempDir::new();
    let (path, conn) = database(&dir, 89);
    conn.execute_batch(
        "INSERT INTO projects (id, name, path, is_git, created_at) VALUES ('p1', 'p', '/repo', 1, 'first');
         INSERT INTO threads (id, project_id, title, engine, cwd, created_at, updated_at)
             VALUES ('t1', 'p1', 'thread', 'fake', '/threads/t1', 'first', 'first');
         INSERT INTO thread_repos (thread_id, repo_id, position, name, branch, worktree_path, created_at)
             VALUES ('t1', 'p1', 0, 'main', 'orbyn/1234', '/threads/t1/main', 'first');",
    )
    .unwrap();
    drop(conn);

    let store = Store::open(&path, "epoch").unwrap();
    let upgrade = store.upgrade().expect("an old database is upgraded");
    assert_eq!(upgrade.to, migrations::MIGRATIONS.last().unwrap().0);
    let rows = store.thread_repos("t1").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].base_branch, None,
        "the base was never stored, so it is absent rather than guessed"
    );
    // A row written by the current store records its base and reads it back; a written null stays a real absence.
    store
        .thread_repo_insert(&record("t1", "repo-a", 1, Some("release")), "second")
        .unwrap();
    store
        .thread_repo_insert(&record("t1", "repo-b", 2, None), "second")
        .unwrap();
    let rows = store.thread_repos("t1").unwrap();
    assert_eq!(rows[0].base_branch, None);
    assert_eq!(rows[1].base_branch.as_deref(), Some("release"));
    assert_eq!(rows[2].base_branch, None);
}
