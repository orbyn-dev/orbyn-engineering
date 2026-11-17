//! Review capacity at rest: migration 47 on a database with old rows, the table checks, settings, a task's
//! priority and start, the waiting merges of a project and a recorded sample batch.

use rusqlite::{Connection, params};

use super::*;
use crate::crew::capacity::{CapacitySettings, Priority, Sampling, waiting_count};
use crate::crew::fixtures::merge_record;
use crate::testing::TempDir;

fn open() -> (TempDir, Store) {
    let dir = TempDir::new();
    let store = Store::open(&dir.path().join("orbyn.db"), "epoch-a").unwrap();
    (dir, store)
}

/// A task row with only what the table needs. The id is the task's.
fn insert_task(conn: &Connection, id: &str) {
    conn.execute(
        "INSERT INTO crew_tasks (id, crew_id, run_id, created_at, updated_at) VALUES (?1, 'crew-1', ?2, 't', 't')",
        params![id, format!("run-{id}")],
    )
    .unwrap();
}

fn task_id(n: u32) -> String {
    format!("sha256:{n:064x}")
}

fn add_task(store: &Store, n: u32) -> String {
    let id = task_id(n);
    insert_task(&store.writer(), &id);
    id
}

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

fn lead_thread(store: &Store, project_id: &str) -> String {
    store
        .create_thread(NewThread {
            project_id: project_id.to_owned(),
            title: "lead".into(),
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
        .id
}

#[test]
fn migration_47_follows_46_and_applies_on_a_database_with_old_rows() {
    let versions: Vec<i64> = migrations::MIGRATIONS.iter().map(|(v, _)| *v).collect();
    assert!(versions.windows(2).all(|w| w[1] == w[0] + 1));
    assert!(versions.contains(&47));

    let dir = TempDir::new();
    let path = dir.path().join("old.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL)").unwrap();
    for (version, sql) in migrations::MIGRATIONS.iter().filter(|(v, _)| *v <= 46) {
        conn.execute_batch(sql).unwrap();
        conn.execute(
            "INSERT INTO schema_migrations VALUES (?1,'fixture')",
            [version],
        )
        .unwrap();
    }
    insert_task(&conn, &task_id(1));
    insert_task(&conn, &task_id(2));
    drop(conn);

    let store = Store::open(&path, "capacity-upgrade").unwrap();
    let upgrade = store.upgrade().expect("an upgrade");
    assert_eq!(
        (upgrade.from, upgrade.to),
        (46, migrations::MIGRATIONS.last().unwrap().0)
    );
    for n in [1, 2] {
        // An old task is Medium and was started by a person, so it can never be paused.
        assert_eq!(
            store.crew_task_capacity(&task_id(n)).unwrap(),
            (Priority::Medium, false)
        );
    }
    assert_eq!(
        store.crew_capacity_settings("none").unwrap(),
        CapacitySettings::default()
    );
    drop(store);
    assert!(
        Store::open(&path, "capacity-reopen")
            .unwrap()
            .upgrade()
            .is_none()
    );
}

#[test]
fn the_tables_refuse_values_outside_their_checks() {
    let (_dir, store) = open();
    let id = add_task(&store, 1);
    let p = project(&store, "/repo/a");
    let conn = store.writer();
    for sql in [
        "UPDATE crew_tasks SET priority = 'urgent'",
        "UPDATE crew_tasks SET priority = NULL",
        "UPDATE crew_tasks SET crew_started = 2",
        "UPDATE crew_tasks SET crew_started = NULL",
    ] {
        assert!(conn.execute(sql, []).is_err(), "{sql}");
    }
    for (limit, sampling) in [(0, "off"), (-3, "off"), (8, "half")] {
        assert!(
            conn.execute(
                "INSERT INTO crew_capacity_settings (project_id, limit_n, sampling) VALUES (?1, ?2, ?3)",
                params![p, limit, sampling],
            )
            .is_err(),
            "{limit} {sampling}"
        );
    }
    assert!(
        conn.execute(
            "INSERT INTO crew_review_samples (batch_id, task_id, chosen, seed, chosen_unix) VALUES ('b', ?1, 2, 1, 1)",
            [&id],
        )
        .is_err()
    );
    assert!(
        conn.execute(
            "INSERT INTO crew_review_samples (batch_id, task_id, chosen, seed, chosen_unix) VALUES ('b', 'sha256:nope', 1, 1, 1)",
            [],
        )
        .is_err(),
        "a sample belongs to a task"
    );
}

#[test]
fn settings_default_round_trip_and_update() {
    let (_dir, store) = open();
    let p = project(&store, "/repo/a");
    let other = project(&store, "/repo/b");
    let defaults = store.crew_capacity_settings(&p).unwrap();
    assert_eq!(defaults, CapacitySettings::default());
    assert_eq!((defaults.limit, defaults.sampling), (8, Sampling::Off));

    let saved = CapacitySettings {
        limit: 3,
        sampling: Sampling::OneInFour,
    };
    store.set_crew_capacity_settings(&p, saved).unwrap();
    assert_eq!(store.crew_capacity_settings(&p).unwrap(), saved);
    assert_eq!(
        store.crew_capacity_settings(&other).unwrap(),
        CapacitySettings::default(),
        "settings are per project"
    );

    let updated = CapacitySettings {
        limit: 12,
        sampling: Sampling::Off,
    };
    store.set_crew_capacity_settings(&p, updated).unwrap();
    assert_eq!(store.crew_capacity_settings(&p).unwrap(), updated);
    let rows: i64 = store
        .reader()
        .query_row("SELECT COUNT(*) FROM crew_capacity_settings", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(rows, 1);
}

#[test]
fn a_limit_of_zero_or_an_unknown_project_is_refused_and_changes_nothing() {
    let (_dir, store) = open();
    let p = project(&store, "/repo/a");
    let saved = CapacitySettings {
        limit: 5,
        sampling: Sampling::OneInFour,
    };
    store.set_crew_capacity_settings(&p, saved).unwrap();
    let zero = CapacitySettings {
        limit: 0,
        sampling: Sampling::Off,
    };
    assert!(matches!(
        store.set_crew_capacity_settings(&p, zero),
        Err(CoreError::Invalid(_))
    ));
    assert!(matches!(
        store.set_crew_capacity_settings("missing", saved),
        Err(CoreError::NotFound(_))
    ));
    assert_eq!(store.crew_capacity_settings(&p).unwrap(), saved);
}

#[test]
fn a_person_started_task_keeps_crew_started_off_and_priority_is_settable() {
    let (_dir, store) = open();
    let id = add_task(&store, 1);
    assert_eq!(
        store.crew_task_capacity(&id).unwrap(),
        (Priority::Medium, false)
    );
    store.set_crew_task_priority(&id, Priority::Low).unwrap();
    assert_eq!(
        store.crew_task_capacity(&id).unwrap(),
        (Priority::Low, false),
        "changing the priority leaves a person's start alone"
    );
    store.set_crew_task_crew_started(&id, true).unwrap();
    assert_eq!(
        store.crew_task_capacity(&id).unwrap(),
        (Priority::Low, true)
    );
    store.set_crew_task_crew_started(&id, false).unwrap();
    store.set_crew_task_priority(&id, Priority::High).unwrap();
    assert_eq!(
        store.crew_task_capacity(&id).unwrap(),
        (Priority::High, false)
    );
    assert!(matches!(
        store.crew_task_capacity("sha256:missing"),
        Err(CoreError::NotFound(_))
    ));
    assert!(matches!(
        store.set_crew_task_priority("sha256:missing", Priority::Low),
        Err(CoreError::NotFound(_))
    ));
}

#[test]
fn only_a_projects_waiting_high_and_medium_merges_are_loaded() {
    let (_dir, store) = open();
    let mine = project(&store, "/repo/a");
    let theirs = project(&store, "/repo/b");
    let (mine_lead, their_lead) = (lead_thread(&store, &mine), lead_thread(&store, &theirs));
    let rows = [
        (1, &mine_lead, "high", "waiting"),
        (2, &mine_lead, "medium", "waiting"),
        (3, &mine_lead, "low", "waiting"),
        (4, &mine_lead, "high", "looks_good"),
        (5, &mine_lead, "medium", "reverted"),
        (6, &their_lead, "high", "waiting"),
    ];
    for (n, lead, risk, state) in rows {
        let id = add_task(&store, n);
        store.add_crew_run(&id, lead, "worker-01").unwrap();
        let mut record = merge_record(&id, "/repo/a", "main", "x");
        record.risk = risk.to_owned();
        store.add_crew_merge(&record).unwrap();
        if state != "waiting" {
            store
                .writer()
                .execute(
                    "UPDATE crew_merges SET review_state = ?2 WHERE task_id = ?1",
                    params![id, state],
                )
                .unwrap();
        }
    }
    let waiting = store.crew_waiting_merges_of_project(&mine).unwrap();
    let ids: Vec<_> = waiting.iter().map(|m| m.task_id.clone()).collect();
    assert_eq!(ids, [task_id(1), task_id(2)]);
    assert_eq!(waiting_count(&waiting), 2);
    assert_eq!(
        waiting_count(&store.crew_waiting_merges_of_project(&theirs).unwrap()),
        1
    );
    assert!(
        store
            .crew_waiting_merges_of_project("none")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_sample_batch_round_trips_with_chosen_and_not_chosen_and_never_changes() {
    let (_dir, store) = open();
    let tasks: Vec<SampledTask> = (1..=4)
        .map(|n| SampledTask {
            task_id: add_task(&store, n),
            chosen: n == 3,
        })
        .collect();
    assert_eq!(store.crew_review_sample("batch-1").unwrap(), None);
    let seed = u64::MAX - 6;
    store
        .record_crew_review_sample("batch-1", seed, 1_700_000_000, &tasks)
        .unwrap();
    let batch = store.crew_review_sample("batch-1").unwrap().unwrap();
    assert_eq!(batch.seed, seed, "the whole 64-bit seed comes back");
    assert_eq!(batch.chosen_unix, 1_700_000_000);
    assert_eq!(batch.tasks, tasks);
    assert_eq!(batch.tasks.iter().filter(|t| t.chosen).count(), 1);

    assert!(matches!(
        store.record_crew_review_sample("batch-1", 1, 2, &tasks[..1]),
        Err(CoreError::Invalid(_))
    ));
    assert!(matches!(
        store.record_crew_review_sample("batch-2", 1, 2, &[]),
        Err(CoreError::Invalid(_))
    ));
    let twice = [tasks[0].clone(), tasks[0].clone()];
    assert!(matches!(
        store.record_crew_review_sample("batch-3", 1, 2, &twice),
        Err(CoreError::Invalid(_))
    ));
    assert_eq!(
        store.crew_review_sample("batch-3").unwrap(),
        None,
        "a refused batch leaves no row"
    );
    assert_eq!(store.crew_review_sample("batch-1").unwrap().unwrap(), batch);
}
