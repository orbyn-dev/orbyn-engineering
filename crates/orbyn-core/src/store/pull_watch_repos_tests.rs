//! Migration 92: a pull request watch is kept per repository, and every older watch stays on the thread's own
//! project.

use rusqlite::Connection;

use super::*;
use crate::testing::TempDir;
use orbyn_contracts::PullRequest;

/// A row for `table` with every required column filled, so a fixture does not depend on the table's full shape.
fn insert_minimal(conn: &Connection, table: &str, set: &[(&str, &str)]) {
    let mut names = Vec::new();
    let mut values: Vec<String> = Vec::new();
    let mut statement = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap();
    let columns: Vec<(String, String, bool, bool)> = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)? != 0,
                row.get::<_, Option<String>>(4)?.is_some(),
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    for (name, kind, not_null, has_default) in columns {
        if let Some((_, value)) = set.iter().find(|(key, _)| *key == name) {
            names.push(name);
            values.push(format!("'{value}'"));
        } else if not_null && !has_default {
            names.push(name);
            values.push(if kind.contains("INT") { "0" } else { "''" }.into());
        }
    }
    conn.execute(
        &format!(
            "INSERT INTO {table} ({}) VALUES ({})",
            names.join(","),
            values.join(",")
        ),
        [],
    )
    .unwrap();
}

fn pull(number: u64, title: &str) -> PullRequest {
    PullRequest {
        number,
        title: title.into(),
        url: format!("https://example.test/pull/{number}"),
        state: "open".into(),
        head: "feat".into(),
        base: "main".into(),
        author: None,
        draft: false,
        updated_at: None,
        labels: Vec::new(),
        review_state: None,
        checks_state: None,
        checks: None,
    }
}

fn project_and_thread(store: &Store) -> (String, String) {
    let project = store
        .add_project(NewProject {
            path: "/r",
            name: "r",
            is_git: true,
            managed: false,
            scratch: false,
        })
        .unwrap()
        .id;
    let thread = store
        .create_thread(NewThread {
            project_id: project.clone(),
            title: "t".into(),
            engine: "fake".into(),
            model: None,
            cwd: "/r".into(),
            worktree: None,
            skill_profile: None,
            parent_thread_id: None,
            relation: None,
            scratch: false,
        })
        .unwrap()
        .id;
    (project, thread)
}

#[test]
fn watches_made_before_the_migration_stay_on_the_threads_own_project() {
    let dir = TempDir::new();
    let path = dir.path().join("old.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(
        "PRAGMA foreign_keys=OFF;
         CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL)",
    )
    .unwrap();
    for (version, sql) in migrations::MIGRATIONS.iter().filter(|(v, _)| *v < 92) {
        conn.execute_batch(sql).unwrap();
        conn.execute(
            "INSERT INTO schema_migrations VALUES (?1,'fixture')",
            [version],
        )
        .unwrap();
    }
    insert_minimal(&conn, "threads", &[("id", "t1"), ("project_id", "proj-a")]);
    conn.execute(
        "INSERT INTO pull_watches (thread_id, number, snapshot, comment_wakes, failures, created_at)
         VALUES ('t1', 12, '{}', 2, 0, 'fixture')",
        [],
    )
    .unwrap();
    drop(conn);

    let store = Store::open(&path, "epoch").unwrap();
    assert_eq!(
        store.upgrade().expect("an upgrade").to,
        migrations::MIGRATIONS.last().unwrap().0
    );
    let watches = store.pull_watches(Some("t1")).unwrap();
    assert_eq!(watches.len(), 1);
    assert_eq!(watches[0].repo_id, "proj-a");
    assert_eq!(watches[0].number, 12);
    assert_eq!(
        watches[0].comment_wakes, 2,
        "what the last pass saw is kept"
    );
    // The same number can now be watched in another repository of the thread.
    store.watch_pull("t1", "repo-b", 12).unwrap();
    assert_eq!(store.pull_watches(Some("t1")).unwrap().len(), 2);
}

#[test]
fn two_repositories_watch_their_own_number_and_unlinking_drops_one() {
    let dir = TempDir::new();
    let store = Store::open(&dir.path().join("db"), "epoch").unwrap();
    let (project, thread) = project_and_thread(&store);
    let primary = store.watch_pull(&thread, &project, 12).unwrap();
    assert_eq!(primary.repo_id, project);
    let second = store.watch_pull(&thread, "repo-b", 12).unwrap();
    assert_eq!(second.repo_id, "repo-b");
    // Both watches are kept, told apart by repository.
    let rows = store.pull_watches(Some(&thread)).unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| (row.repo_id.as_str(), row.number))
            .collect::<Vec<_>>(),
        [(project.as_str(), 12), ("repo-b", 12)]
    );
    // Watching the same pull request again keeps one row, and what a pass saw goes to its own repository's row.
    store.watch_pull(&thread, &project, 12).unwrap();
    store
        .update_watch(&thread, "repo-b", 12, WatchUpdate::failed(3))
        .unwrap();
    let rows = store.pull_watches(Some(&thread)).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows.iter()
            .find(|row| row.repo_id == "repo-b")
            .unwrap()
            .failures,
        3
    );
    assert_eq!(
        rows.iter()
            .find(|row| row.repo_id == project)
            .unwrap()
            .failures,
        0
    );
    store.unwatch_pull(&thread, "repo-b", 12).unwrap();
    assert_eq!(
        store
            .pull_watches(Some(&thread))
            .unwrap()
            .iter()
            .map(|row| row.repo_id.as_str())
            .collect::<Vec<_>>(),
        [project.as_str()]
    );
    // Unlinking a link drops the watch on that repository's pull request and no other.
    store.watch_pull(&thread, "repo-b", 12).unwrap();
    store.watch_pull(&thread, "repo-b", 13).unwrap();
    store
        .link_pull(&thread, "repo-b", &pull(12, "Second"))
        .unwrap();
    store.unlink_pull(&thread, "repo-b", 12).unwrap();
    assert_eq!(
        store
            .pull_watches(Some(&thread))
            .unwrap()
            .iter()
            .map(|row| row.number)
            .collect::<Vec<_>>(),
        [12, 13]
    );
    // Removing a repository from the thread takes its watches too.
    store.thread_repo_delete(&thread, "repo-b").unwrap();
    assert_eq!(
        store
            .pull_watches(Some(&thread))
            .unwrap()
            .iter()
            .map(|row| row.repo_id.as_str())
            .collect::<Vec<_>>(),
        [project.as_str()]
    );
}
