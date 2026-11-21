//! Migration 60: every thread saved with plan mode on comes out with it off.

use rusqlite::Connection;

use super::*;
use crate::testing::TempDir;

fn plan_modes(store: &Store) -> Vec<i64> {
    let reader = store.reader();
    let mut statement = reader
        .prepare("SELECT plan_mode FROM threads ORDER BY id")
        .unwrap();
    statement
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap()
}

#[test]
fn migration_60_follows_59_and_clears_plan_mode_on_every_thread() {
    let versions: Vec<i64> = migrations::MIGRATIONS.iter().map(|(v, _)| *v).collect();
    assert!(versions.windows(2).all(|w| w[1] == w[0] + 1), "contiguous");
    assert!(versions.contains(&60));

    let dir = TempDir::new();
    let path = dir.path().join("orbyn.db");
    let store = Store::open(&path, "epoch-a").unwrap();
    let project = store
        .add_project(NewProject {
            path: "/repo",
            name: "p",
            is_git: true,
            managed: false,
            scratch: false,
        })
        .unwrap()
        .id;
    for title in ["a", "b", "c"] {
        store
            .create_thread(NewThread {
                project_id: project.clone(),
                title: title.into(),
                engine: "fake".into(),
                model: None,
                cwd: "/repo".into(),
                worktree: None,
                skill_profile: None,
                parent_thread_id: None,
                relation: None,
                scratch: false,
            })
            .unwrap();
    }
    store
        .writer()
        .execute("UPDATE threads SET plan_mode = 1 WHERE title <> 'c'", [])
        .unwrap();
    assert_eq!(plan_modes(&store).iter().sum::<i64>(), 2);
    drop(store);

    // The migration's own statement, on a database written before it.
    let (_, sql) = migrations::MIGRATIONS
        .iter()
        .find(|(v, _)| *v == 60)
        .unwrap();
    Connection::open(&path).unwrap().execute_batch(sql).unwrap();
    let store = Store::open(&path, "epoch-b").unwrap();
    assert_eq!(plan_modes(&store), vec![0, 0, 0]);
}
