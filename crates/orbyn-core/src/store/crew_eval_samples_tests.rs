//! EvalHistory samples at rest (migration 88, KC5 slice 4): one sample per task, the lesson ids it carried and left
//! out, the outcome written once, the project read, the bookkeeping failures that never refuse a start or a move, and
//! the migration that adds the table to an old database.

use orbyn_contracts::{AnswerKey, Brief, CrewObject};
use rusqlite::Connection;
use serde_json::Value;

use super::*;
use crate::testing::TempDir;

macro_rules! example {
    ($name:literal) => {
        include_str!(concat!(
            "../../../../docs/crew/protocol/schemas/examples/",
            $name,
            ".json"
        ))
    };
}

fn object(text: &str) -> CrewObject {
    CrewObject::parse(&serde_json::from_str::<Value>(text).unwrap()).unwrap()
}

fn brief() -> Brief {
    match object(example!("brief.valid.basic")) {
        CrewObject::Brief(brief) => brief,
        _ => unreachable!(),
    }
}

fn key() -> AnswerKey {
    match object(example!("answer-key.valid.basic")) {
        CrewObject::AnswerKey(key) => key,
        _ => unreachable!(),
    }
}

/// The same brief with a new outcome, and a key that belongs to it, so one test can create two tasks.
fn another_brief(outcome: &str) -> (Brief, AnswerKey) {
    let mut brief = brief();
    brief.outcome = outcome.to_owned();
    brief.id = CrewObject::Brief(brief.clone()).computed_id().unwrap();
    let mut key = key();
    key.brief = brief.id.clone();
    key.id = CrewObject::AnswerKey(key.clone()).computed_id().unwrap();
    (brief, key)
}

fn open() -> (TempDir, Store) {
    let dir = TempDir::new();
    let store = Store::open(&dir.path().join("orbyn.db"), "epoch-a").unwrap();
    (dir, store)
}

fn sample(store: &Store, task_id: &str) -> CrewEvalSample {
    store.crew_eval_sample(task_id).unwrap().unwrap()
}

fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn seed_project(store: &Store, project: &str) {
    store
        .writer()
        .execute(
            "INSERT OR IGNORE INTO projects (id, name, path, trusted, is_git, created_at, managed, scratch)
             VALUES (?1, 'p', ?2, 0, 1, 't', 0, 0)",
            rusqlite::params![project, format!("/{project}")],
        )
        .unwrap();
}

fn seed_lead(store: &Store, project: &str, lead: &str) {
    seed_project(store, project);
    store
        .writer()
        .execute(
            "INSERT OR IGNORE INTO threads (id, project_id, title, engine, cwd, created_at, updated_at)
             VALUES (?1, ?2, 'Lead', 'fake', '/p', 't', 't')",
            rusqlite::params![lead, project],
        )
        .unwrap();
}

fn move_to(store: &Store, task_id: &str, states: &[(&str, Option<&str>)]) {
    for (state, reason) in states {
        store.move_crew_task(task_id, state, *reason).unwrap();
    }
}

#[test]
fn a_task_with_lessons_records_one_sample_and_a_repeat_create_changes_nothing() {
    let (_dir, store) = open();
    let used = ids(&["les_a", "les_b"]);
    let left_out = ids(&["les_c"]);
    let task = store
        .create_crew_task_with_lessons(&brief(), &key(), "run-01", &used, &left_out)
        .unwrap();
    let recorded = sample(&store, &task.id);
    assert_eq!(recorded.task_id, task.id);
    assert_eq!(recorded.kind, "implement");
    assert!(recorded.had_lessons);
    assert_eq!(recorded.lesson_ids, used);
    assert_eq!(recorded.left_out_ids, left_out);
    assert_eq!(recorded.outcome, None);
    assert_eq!(recorded.stop_reason, None);
    assert_eq!(recorded.engine, None);
    assert_eq!(recorded.finished_at, None);

    // Creating the same task again is a no-op, even with different lessons: one task, one sample.
    let again = store
        .create_crew_task_with_lessons(&brief(), &key(), "run-01", &ids(&["les_other"]), &[])
        .unwrap();
    assert_eq!(again, task);
    assert_eq!(
        sample(&store, &task.id),
        recorded,
        "the existing task's sample is never rewritten"
    );
}

#[test]
fn a_task_without_lessons_records_had_lessons_false_and_empty_ids() {
    let (_dir, store) = open();
    let task = store.create_crew_task(&brief(), &key(), "run-01").unwrap();
    let recorded = sample(&store, &task.id);
    assert!(!recorded.had_lessons);
    assert!(recorded.lesson_ids.is_empty());
    assert!(recorded.left_out_ids.is_empty());
    assert_eq!(recorded.kind, "implement");
}

#[test]
fn the_key_checks_left_out_ids_are_recorded_as_they_are() {
    let (_dir, store) = open();
    let task = store
        .create_crew_task_with_lessons(
            &brief(),
            &key(),
            "run-01",
            &[],
            &ids(&["les_scope", "les_command"]),
        )
        .unwrap();
    let recorded = sample(&store, &task.id);
    assert!(!recorded.had_lessons, "no lesson reached the brief");
    assert!(recorded.lesson_ids.is_empty());
    assert_eq!(recorded.left_out_ids, ids(&["les_scope", "les_command"]));
}

#[test]
fn each_sampled_state_sets_its_outcome_once() {
    let (_dir, store) = open();

    let passed = store.create_crew_task(&brief(), &key(), "run-01").unwrap();
    move_to(
        &store,
        &passed.id,
        &[("routed", None), ("running", None), ("verifying", None)],
    );
    let evidence = object(example!("evidence.valid.test-pass"));
    store.add_crew_object(&passed.id, &evidence).unwrap();
    store
        .pass_crew_task(&passed.id, &[evidence.id().as_str().to_owned()])
        .unwrap();
    let passed_sample = sample(&store, &passed.id);
    assert_eq!(passed_sample.outcome.as_deref(), Some("passed"));
    assert_eq!(passed_sample.stop_reason, None);

    let stopped = {
        let (brief, key) = another_brief("A stopped task.");
        store.create_crew_task(&brief, &key, "run-02").unwrap()
    };
    move_to(
        &store,
        &stopped.id,
        &[
            ("routed", None),
            ("running", None),
            ("stopped", Some("budget")),
        ],
    );
    let stopped_sample = sample(&store, &stopped.id);
    assert_eq!(stopped_sample.outcome.as_deref(), Some("stopped"));
    assert_eq!(stopped_sample.stop_reason.as_deref(), Some("budget"));

    let cancelled = {
        let (brief, key) = another_brief("A cancelled task.");
        store.create_crew_task(&brief, &key, "run-03").unwrap()
    };
    move_to(&store, &cancelled.id, &[("cancelled", None)]);
    let cancelled_sample = sample(&store, &cancelled.id);
    assert_eq!(cancelled_sample.outcome.as_deref(), Some("cancelled"));
    assert_eq!(cancelled_sample.stop_reason, None);

    let failed = {
        let (brief, key) = another_brief("A failed task.");
        store.create_crew_task(&brief, &key, "run-04").unwrap()
    };
    move_to(
        &store,
        &failed.id,
        &[
            ("routed", None),
            ("running", None),
            ("verifying", None),
            ("failed", None),
        ],
    );
    let failed_sample = sample(&store, &failed.id);
    assert_eq!(failed_sample.outcome.as_deref(), Some("failed"));
    assert_eq!(failed_sample.stop_reason, None);
}

#[test]
fn a_failed_verification_records_only_its_last_sampled_state() {
    let (_dir, store) = open();
    let task = store.create_crew_task(&brief(), &key(), "run-01").unwrap();
    store.add_crew_run(&task.id, "lead-1", "worker-01").unwrap();
    move_to(&store, &task.id, &[("routed", None), ("running", None)]);

    // The first failure retries: the transient `failed` is not the task's outcome.
    store
        .begin_crew_verification(&task.id, "engine-run-1", None)
        .unwrap();
    let (_, next) = store
        .fail_crew_verification(&task.id, "fingerprint-1")
        .unwrap();
    assert_eq!(next, crate::crew::lifecycle::Next::Retry);
    assert_eq!(sample(&store, &task.id).outcome, None);

    // The second failure changes the approach, still no outcome.
    store
        .begin_crew_verification(&task.id, "engine-run-2", None)
        .unwrap();
    let (_, next) = store
        .fail_crew_verification(&task.id, "fingerprint-1")
        .unwrap();
    assert_eq!(next, crate::crew::lifecycle::Next::ChangeApproach);
    assert_eq!(sample(&store, &task.id).outcome, None);

    // The third failure stops it: the sample keeps the stop, never the `failed` it went through.
    store.move_crew_task(&task.id, "running", None).unwrap();
    store
        .begin_crew_verification(&task.id, "engine-run-3", None)
        .unwrap();
    let (moved, next) = store
        .fail_crew_verification(&task.id, "fingerprint-1")
        .unwrap();
    assert_eq!(
        (moved.state.as_str(), next),
        ("stopped", crate::crew::lifecycle::Next::Stop)
    );
    let recorded = sample(&store, &task.id);
    assert_eq!(recorded.outcome.as_deref(), Some("stopped"));
    assert_eq!(recorded.stop_reason.as_deref(), Some("repeated_failure"));
}

#[test]
fn a_second_final_move_never_overwrites_the_first_outcome() {
    let (_dir, store) = open();
    let task = store.create_crew_task(&brief(), &key(), "run-01").unwrap();
    move_to(&store, &task.id, &[("cancelled", None)]);
    let first = sample(&store, &task.id);
    assert_eq!(first.outcome.as_deref(), Some("cancelled"));

    // A re-run moves the task out of the final state; its next end must not replace the first outcome.
    store
        .writer()
        .execute(
            "UPDATE crew_tasks SET state = 'running' WHERE id = ?1",
            [&task.id],
        )
        .unwrap();
    move_to(&store, &task.id, &[("stopped", Some("budget"))]);
    let second = sample(&store, &task.id);
    assert_eq!(second.outcome.as_deref(), Some("cancelled"));
    assert_eq!(second.stop_reason, None);
    assert_eq!(second.finished_at, first.finished_at);
}

#[test]
fn the_latest_route_decision_names_the_samples_engine() {
    let (_dir, store) = open();
    let task = store.create_crew_task(&brief(), &key(), "run-01").unwrap();
    store
        .writer()
        .execute(
            "INSERT INTO crew_route_decisions (id, task_id, engine, assigned_by, body)
             VALUES (?1, ?2, 'codex', 'person', '{}'), (?3, ?2, 'claude', 'person', '{}')",
            rusqlite::params![
                format!("sha256:{}", "1".repeat(64)),
                task.id,
                format!("sha256:{}", "2".repeat(64)),
            ],
        )
        .unwrap();
    move_to(&store, &task.id, &[("cancelled", None)]);
    assert_eq!(sample(&store, &task.id).engine.as_deref(), Some("claude"));
}

#[test]
fn a_project_read_fills_the_project_and_reads_newest_first() {
    let (_dir, store) = open();
    seed_lead(&store, "p1", "lead-1");
    let (first_brief, first_key) = another_brief("The first task.");
    let first = store
        .create_crew_task(&first_brief, &first_key, "run-01")
        .unwrap();
    store
        .add_crew_run(&first.id, "lead-1", "worker-01")
        .unwrap();
    move_to(&store, &first.id, &[("cancelled", None)]);

    let (second_brief, second_key) = another_brief("The second task.");
    let second = store
        .create_crew_task(&second_brief, &second_key, "run-02")
        .unwrap();
    store
        .add_crew_run(&second.id, "lead-1", "worker-02")
        .unwrap();
    move_to(&store, &second.id, &[("cancelled", None)]);

    // Fixed times make the order the read must give, not the order the clock happened to write.
    store
        .writer()
        .execute(
            "UPDATE crew_eval_samples SET created_at = '2026-01-01T00:00:00Z' WHERE task_id = ?1",
            [&first.id],
        )
        .unwrap();
    store
        .writer()
        .execute(
            "UPDATE crew_eval_samples SET created_at = '2026-01-02T00:00:00Z' WHERE task_id = ?1",
            [&second.id],
        )
        .unwrap();

    let rows = store.crew_eval_samples("p1").unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| row.task_id.as_str())
            .collect::<Vec<_>>(),
        [second.id.as_str(), first.id.as_str()]
    );
    assert_eq!(rows[0].project_id.as_deref(), Some("p1"));
    assert_eq!(rows[1].project_id.as_deref(), Some("p1"));
    assert!(
        store
            .crew_eval_samples("no-such-project")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn deleting_a_lesson_later_leaves_the_samples_ids_as_they_were() {
    let (_dir, store) = open();
    seed_project(&store, "p1");
    let first = store
        .write_crew_lesson(NewCrewLesson {
            scope: "repo".to_owned(),
            project_id: Some("p1".to_owned()),
            source: "person".to_owned(),
            check_kind: None,
            path_class: None,
            text: "Prefer one commit per change.".to_owned(),
            fingerprint: None,
            evidence: Vec::new(),
            edited_by_person: true,
            names_path: false,
        })
        .unwrap()
        .unwrap();
    let second = store
        .write_crew_lesson(NewCrewLesson {
            scope: "repo".to_owned(),
            project_id: Some("p1".to_owned()),
            source: "person".to_owned(),
            check_kind: None,
            path_class: None,
            text: "Run the focused tests first.".to_owned(),
            fingerprint: None,
            evidence: Vec::new(),
            edited_by_person: true,
            names_path: false,
        })
        .unwrap()
        .unwrap();
    let task = store
        .create_crew_task_with_lessons(
            &brief(),
            &key(),
            "run-01",
            &[first.id.clone(), second.id.clone()],
            &[],
        )
        .unwrap();
    let recorded = sample(&store, &task.id);
    store.delete_crew_lesson(&first.id, 1).unwrap();
    store.purge_deleted_lessons("9999-12-31T23:59:59Z").unwrap();
    assert_eq!(store.crew_lesson(&first.id).unwrap(), None);
    assert_eq!(sample(&store, &task.id), recorded);
    assert_eq!(
        sample(&store, &task.id).lesson_ids,
        ids(&[first.id.as_str(), second.id.as_str()])
    );
}

#[test]
fn a_bookkeeping_failure_never_refuses_creation_or_a_state_change() {
    let (_dir, store) = open();
    store
        .writer()
        .execute_batch(
            "CREATE TRIGGER refuse_sample_start BEFORE INSERT ON crew_eval_samples
             BEGIN SELECT RAISE(ABORT, 'sample refused'); END;",
        )
        .unwrap();
    let task = store
        .create_crew_task_with_lessons(&brief(), &key(), "run-01", &ids(&["les_a"]), &[])
        .unwrap();
    assert_eq!(task.id, brief().id.as_str());
    assert!(store.crew_task(&task.id).unwrap().is_some());
    assert_eq!(store.crew_eval_sample(&task.id).unwrap(), None);
    store
        .writer()
        .execute_batch("DROP TRIGGER refuse_sample_start")
        .unwrap();

    let (second_brief, second_key) = another_brief("A task whose sample cannot finish.");
    let second = store
        .create_crew_task(&second_brief, &second_key, "run-02")
        .unwrap();
    store
        .writer()
        .execute_batch(
            "CREATE TRIGGER refuse_sample_finish BEFORE UPDATE ON crew_eval_samples
             BEGIN SELECT RAISE(ABORT, 'sample refused'); END;",
        )
        .unwrap();
    move_to(&store, &second.id, &[("cancelled", None)]);
    assert_eq!(
        store.crew_task(&second.id).unwrap().unwrap().state,
        "cancelled"
    );
    let recorded = sample(&store, &second.id);
    assert_eq!(recorded.outcome, None, "the sample rolls back alone");
    assert_eq!(recorded.finished_at, None);
}

#[test]
fn migration_88_adds_the_samples_table_to_an_old_database() {
    let versions: Vec<i64> = migrations::MIGRATIONS.iter().map(|(v, _)| *v).collect();
    assert!(versions.windows(2).all(|w| w[1] == w[0] + 1));
    assert!(versions.contains(&88));
    // This test proves migration 88 applies to an at-87 database; the latest version moves with every new
    // migration, so read it from the list rather than pinning a number that the next migration would break.
    let latest = migrations::MIGRATIONS.last().unwrap().0;
    assert!(latest >= 88);

    let dir = TempDir::new();
    let path = dir.path().join("at87.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(
        "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL)",
    )
    .unwrap();
    for (version, sql) in migrations::MIGRATIONS
        .iter()
        .filter(|(version, _)| *version <= 87)
    {
        conn.execute_batch(sql).unwrap();
        conn.execute(
            "INSERT INTO schema_migrations VALUES (?1, 'fixture')",
            [version],
        )
        .unwrap();
    }
    let old_id = format!("sha256:{}", "a".repeat(64));
    conn.execute(
        "INSERT INTO crew_tasks (id, crew_id, run_id, created_at, updated_at)
         VALUES (?1, 'crew-1', 'run-old', 't', 't')",
        [&old_id],
    )
    .unwrap();
    let has_samples: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'crew_eval_samples'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(has_samples, 0, "the old database has no sample table");
    drop(conn);

    let store = Store::open(&path, "a").unwrap();
    assert_eq!(
        store.upgrade(),
        Some(orbyn_contracts::DatabaseUpgrade {
            from: 87,
            to: migrations::MIGRATIONS.last().unwrap().0,
        })
    );
    assert_eq!(store.crew_task(&old_id).unwrap().unwrap().id, old_id);
    let new = store.create_crew_task(&brief(), &key(), "run-01").unwrap();
    assert_eq!(sample(&store, &new.id).kind, "implement");
    assert_eq!(store.crew_eval_sample(&old_id).unwrap(), None);
}
