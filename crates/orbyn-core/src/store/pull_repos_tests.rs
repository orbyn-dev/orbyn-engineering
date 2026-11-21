//! Migration 70: a pull request link is kept per repository, and every older link stays on the thread's own project.

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

#[test]
fn links_made_before_the_migration_stay_on_the_threads_own_project() {
    let dir = TempDir::new();
    let path = dir.path().join("old.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(
        "PRAGMA foreign_keys=OFF;
         CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL)",
    )
    .unwrap();
    for (version, sql) in migrations::MIGRATIONS.iter().filter(|(v, _)| *v < 70) {
        conn.execute_batch(sql).unwrap();
        conn.execute(
            "INSERT INTO schema_migrations VALUES (?1,'fixture')",
            [version],
        )
        .unwrap();
    }
    insert_minimal(&conn, "threads", &[("id", "t1"), ("project_id", "proj-a")]);
    insert_minimal(&conn, "threads", &[("id", "t2"), ("project_id", "proj-b")]);
    for (thread, number, title) in [
        ("t1", 12, "First"),
        ("t1", 13, "Second"),
        ("t2", 12, "Other"),
    ] {
        conn.execute(
            "INSERT INTO thread_pull_requests (thread_id, number, url, title, state, head, base, author)
             VALUES (?1, ?2, 'u', ?3, 'open', 'h', 'b', NULL)",
            rusqlite::params![thread, number, title],
        )
        .unwrap();
    }
    drop(conn);

    let store = Store::open(&path, "epoch").unwrap();
    assert_eq!(
        store.upgrade().expect("an upgrade").to,
        migrations::MIGRATIONS.last().unwrap().0
    );
    let first = store.linked_pulls("t1", "proj-a").unwrap();
    assert_eq!(
        first
            .iter()
            .map(|p| (p.number, p.title.as_str()))
            .collect::<Vec<_>>(),
        [(12, "First"), (13, "Second")]
    );
    assert_eq!(store.linked_pulls("t2", "proj-b").unwrap().len(), 1);
    assert!(store.linked_pulls("t1", "proj-b").unwrap().is_empty());
    let threads: Vec<_> = store
        .threads_linked_to_pull("proj-a", 12)
        .unwrap()
        .into_iter()
        .map(|t| t.id)
        .collect();
    assert_eq!(threads, ["t1"]);
}

#[test]
fn two_repositories_of_one_thread_can_both_have_the_same_number() {
    let dir = TempDir::new();
    let store = Store::open(&dir.path().join("db"), "epoch").unwrap();
    let project = store
        .add_project(NewProject {
            path: "/r",
            name: "r",
            is_git: true,
            managed: false,
            scratch: false,
        })
        .unwrap();
    let thread = store
        .create_thread(NewThread {
            project_id: project.id.clone(),
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
        .unwrap();
    store
        .link_pull(&thread.id, &project.id, &pull(5, "Primary"))
        .unwrap();
    store
        .link_pull(&thread.id, "repo-b", &pull(5, "Second repo"))
        .unwrap();
    assert_eq!(
        store.linked_pulls(&thread.id, &project.id).unwrap()[0].title,
        "Primary"
    );
    assert_eq!(
        store.linked_pulls(&thread.id, "repo-b").unwrap()[0].title,
        "Second repo"
    );
    // The project's pull request #5 finds the thread through its own repository only.
    assert_eq!(
        store.threads_linked_to_pull(&project.id, 5).unwrap().len(),
        1
    );
    store.unlink_pull(&thread.id, "repo-b", 5).unwrap();
    assert!(store.linked_pulls(&thread.id, "repo-b").unwrap().is_empty());
    assert_eq!(
        store.linked_pulls(&thread.id, &project.id).unwrap().len(),
        1
    );
    // Removing a repository from the thread takes its links and no one else's.
    store
        .link_pull(&thread.id, "repo-b", &pull(6, "Again"))
        .unwrap();
    store.thread_repo_delete(&thread.id, "repo-b").unwrap();
    assert!(store.linked_pulls(&thread.id, "repo-b").unwrap().is_empty());
    assert_eq!(
        store.linked_pulls(&thread.id, &project.id).unwrap().len(),
        1
    );
}
