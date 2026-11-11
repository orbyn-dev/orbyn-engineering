//! Crew tables: verified objects in, the answer key kept apart, immutable rows, and the
//! migration that creates them.

use crate::crew::lifecycle::Next;
use orbyn_contracts::{AnswerKey, Brief, CrewObject};
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

fn open() -> (TempDir, Store) {
    let dir = TempDir::new();
    let store = Store::open(&dir.path().join("orbyn.sqlite3"), "epoch-a").unwrap();
    (dir, store)
}

fn task(store: &Store) -> String {
    store
        .create_crew_task(&brief(), &key(), "run-01")
        .unwrap()
        .id
}

fn columns(store: &Store, table: &str) -> Vec<String> {
    store
        .reader()
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn a_task_keeps_its_brief_key_and_every_object_the_run_produced() {
    let (_dir, store) = open();
    let id = task(&store);
    let row = store.crew_task(&id).unwrap().unwrap();
    assert_eq!(
        (
            row.state.as_str(),
            row.run_id.as_str(),
            row.crew_id.as_str()
        ),
        ("briefed", "run-01", "crew-01")
    );
    assert_eq!(store.crew_brief(&id).unwrap(), Some(brief()));
    assert_eq!(store.crew_answer_key(&id).unwrap(), Some(key()));

    let added = [
        example!("message.valid.question"),
        example!("lease.valid.granted"),
        example!("evidence.valid.test-pass"),
        example!("gate.valid.checks-by-code"),
        example!("result.valid.passed"),
    ];
    for text in added {
        store.add_crew_object(&id, &object(text)).unwrap();
        // Adding the same object again changes nothing.
        store.add_crew_object(&id, &object(text)).unwrap();
    }
    assert_eq!(store.crew_messages(&id).unwrap().len(), 1);
    assert_eq!(store.crew_leases(&id).unwrap().len(), 1);
    assert_eq!(store.crew_evidence(&id).unwrap().len(), 1);
    assert_eq!(store.crew_gates(&id).unwrap().len(), 1);
    let results = store.crew_results(&id).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].r#final);
    // Creating the task again returns it untouched.
    assert_eq!(
        store.create_crew_task(&brief(), &key(), "run-01").unwrap(),
        row
    );
}

/// H1: the brief a worker reads comes from a table with no place for the key.
#[test]
fn the_answer_key_lives_in_its_own_table() {
    let (_dir, store) = open();
    let id = task(&store);
    for text in [
        example!("message.valid.question"),
        example!("result.valid.passed"),
    ] {
        store.add_crew_object(&id, &object(text)).unwrap();
    }
    assert_eq!(columns(&store, "crew_briefs"), ["id", "body"]);
    let key = key();
    let secrets = [
        key.id.as_str().to_owned(),
        "protected_paths".to_owned(),
        "cargo test -p crew-demo".to_owned(),
    ];
    for table in [
        "crew_tasks",
        "crew_briefs",
        "crew_messages",
        "crew_leases",
        "crew_evidence",
        "crew_gates",
        "crew_results",
    ] {
        let rows: Vec<String> = {
            let conn = store.reader();
            let mut statement = conn.prepare(&format!("SELECT * FROM {table}")).unwrap();
            let width = statement.column_count();
            statement
                .query_map([], |row| {
                    (0..width)
                        .map(|i| Ok(format!("{:?}", row.get_ref(i)?)))
                        .collect::<rusqlite::Result<String>>()
                })
                .unwrap()
                .map(Result::unwrap)
                .collect()
        };
        for row in rows {
            for (number, secret) in secrets.iter().enumerate() {
                assert!(
                    !row.contains(secret.as_str()),
                    "{table} holds secret number {number}"
                );
            }
        }
    }
    let wire = serde_json::to_value(store.crew_brief(&id).unwrap().unwrap()).unwrap();
    assert!(wire.get("checks").is_none() && wire.get("protected_paths").is_none());
    assert!(columns(&store, "crew_answer_keys").contains(&"body".to_owned()));
}

#[test]
fn an_object_must_verify_and_belong_to_the_task() {
    let (_dir, store) = open();
    let id = task(&store);
    // An id that is not the object's hash.
    let CrewObject::Message(mut message) = object(example!("message.valid.question")) else {
        unreachable!()
    };
    message.body = orbyn_contracts::CrewLongText::new("A different body.").unwrap();
    let error = store
        .add_crew_object(&id, &CrewObject::Message(message))
        .unwrap_err();
    assert!(error.to_string().contains("not its hash"), "{error}");
    // A run that is not the task's.
    let other_run = object(example!("result.valid.stopped"));
    let error = store.add_crew_object(&id, &other_run).unwrap_err();
    assert!(error.to_string().contains("is not part of task"), "{error}");
    // A task that does not exist.
    let error = store
        .add_crew_object(
            "sha256:missing",
            &object(example!("message.valid.question")),
        )
        .unwrap_err();
    assert!(matches!(error, CoreError::NotFound(_)));
    // A type with no table in this slice, and a key that belongs to another brief.
    assert!(
        store
            .add_crew_object(&id, &object(example!("claim.valid.unverified")))
            .is_err()
    );
    let mut wrong = key();
    wrong.brief = orbyn_contracts::CrewHash::new(format!("sha256:{}", "0".repeat(64))).unwrap();
    assert!(store.create_crew_task(&brief(), &wrong, "run-02").is_err());
    assert!(store.crew_messages(&id).unwrap().is_empty());
}

#[test]
fn stored_objects_cannot_be_rewritten() {
    let (_dir, store) = open();
    let id = task(&store);
    store
        .add_crew_object(&id, &object(example!("message.valid.question")))
        .unwrap();
    store
        .add_crew_object(&id, &object(example!("route-decision.valid.explicit")))
        .unwrap();
    let conn = store.writer();
    for table in ["crew_briefs", "crew_messages", "crew_route_decisions"] {
        let refused = conn
            .execute(&format!("UPDATE {table} SET body = '{{}}'"), [])
            .unwrap_err();
        assert!(
            refused.to_string().contains("immutable"),
            "{table}: {refused}"
        );
    }
    let refused = conn
        .execute("UPDATE crew_answer_keys SET body = '{}'", [])
        .unwrap_err();
    assert!(refused.to_string().contains("immutable"));
    // Only a task's state moves, and only to a state the lifecycle names.
    conn.execute(
        "UPDATE crew_tasks SET state = 'routed' WHERE id = ?1",
        [&id],
    )
    .unwrap();
    assert!(
        conn.execute("UPDATE crew_tasks SET state = 'done' WHERE id = ?1", [&id])
            .is_err()
    );
    assert!(
        conn.execute(
            "UPDATE crew_tasks SET stop_reason = 'tired' WHERE id = ?1",
            [&id]
        )
        .is_err()
    );
}

/// A route decision is kept with its task, only for the brief it answers (migration 31).
#[test]
fn a_route_decision_is_kept_with_its_task_and_only_for_its_brief() {
    let (_dir, store) = open();
    let id = task(&store);
    let decision = object(example!("route-decision.valid.explicit"));
    store.add_crew_object(&id, &decision).unwrap();
    store.add_crew_object(&id, &decision).unwrap();
    let kept = store.crew_route_decisions(&id).unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].engine.as_str(), "codex");
    // A decision for another brief, or for another run, is not this task's.
    let CrewObject::RouteDecision(mut other) = decision.clone() else {
        unreachable!()
    };
    other.brief = orbyn_contracts::CrewHash::new(format!("sha256:{}", "1".repeat(64))).unwrap();
    other.id = CrewObject::RouteDecision(other.clone())
        .computed_id()
        .unwrap();
    let error = store
        .add_crew_object(&id, &CrewObject::RouteDecision(other))
        .unwrap_err();
    assert!(error.to_string().contains("is not part of task"), "{error}");
    let stray = object(example!("route-decision.valid.scored"));
    assert!(store.add_crew_object(&id, &stray).is_err());
    assert_eq!(store.crew_route_decisions(&id).unwrap().len(), 1);
}

#[test]
fn deleting_a_task_removes_its_objects() {
    let (_dir, store) = open();
    let id = task(&store);
    store
        .add_crew_object(&id, &object(example!("lease.valid.granted")))
        .unwrap();
    store
        .add_crew_object(&id, &object(example!("route-decision.valid.explicit")))
        .unwrap();
    store
        .writer()
        .execute("DELETE FROM crew_tasks WHERE id = ?1", [&id])
        .unwrap();
    for table in [
        "crew_briefs",
        "crew_answer_keys",
        "crew_leases",
        "crew_route_decisions",
    ] {
        let left: i64 = store
            .reader()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0, "{table}");
    }
}

#[test]
fn a_database_without_crew_tables_gets_them_in_30_to_35() {
    let dir = TempDir::new();
    let path = dir.path().join("at29.db");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(
        "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL)",
    )
    .unwrap();
    for (version, sql) in super::migrations::MIGRATIONS
        .iter()
        .filter(|(version, _)| *version <= 29)
    {
        conn.execute_batch(sql).unwrap();
        conn.execute(
            "INSERT INTO schema_migrations VALUES (?1, 'fixture')",
            [version],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('kept', 'yes')",
        [],
    )
    .unwrap();
    let tables = |conn: &rusqlite::Connection| -> Vec<String> {
        conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name LIKE 'crew_%' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    assert!(tables(&conn).is_empty());
    drop(conn);

    let store = Store::open(&path, "a").unwrap();
    assert_eq!(
        store.upgrade(),
        Some(orbyn_contracts::DatabaseUpgrade {
            from: 29,
            to: super::migrations::MIGRATIONS.last().unwrap().0,
        })
    );
    assert_eq!(
        tables(&store.reader()),
        [
            "crew_answer_keys",
            "crew_briefs",
            "crew_bus_events",
            "crew_deliveries",
            "crew_evidence",
            "crew_facts",
            "crew_gates",
            "crew_leases",
            "crew_messages",
            "crew_reports",
            "crew_results",
            "crew_route_decisions",
            "crew_runs",
            "crew_tasks"
        ]
    );
    let kept: String = store
        .reader()
        .query_row("SELECT value FROM settings WHERE key = 'kept'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(kept, "yes", "existing data survives the migration");
    // The tables work on the upgraded database, and the next start does nothing.
    let id = store
        .create_crew_task(&brief(), &key(), "run-01")
        .unwrap()
        .id;
    drop(store);
    let again = Store::open(&path, "b").unwrap();
    assert!(again.upgrade().is_none());
    assert!(again.crew_brief(&id).unwrap().is_some());
}

#[test]
fn migration_30_is_the_crew_migration_and_numbering_has_no_gap() {
    let versions: Vec<i64> = super::migrations::MIGRATIONS
        .iter()
        .map(|(v, _)| *v)
        .collect();
    assert!(versions.windows(2).all(|w| w[1] == w[0] + 1));
    assert!(versions.contains(&30) && versions.contains(&31) && versions.contains(&32));
    let sql = super::migrations::MIGRATIONS
        .iter()
        .find(|(v, _)| *v == 30)
        .unwrap()
        .1;
    assert!(sql.contains("CREATE TABLE crew_answer_keys"));
}

#[test]
fn creating_a_task_again_needs_the_same_key_and_run() {
    let (_dir, store) = open();
    let id = task(&store);
    // A corrected key (a different id) is refused, and the stored one stays.
    let mut corrected = key();
    corrected
        .protected_paths
        .push(orbyn_contracts::CrewPathPattern::new("extra/**").unwrap());
    corrected.id = CrewObject::AnswerKey(corrected.clone())
        .computed_id()
        .unwrap();
    let error = store
        .create_crew_task(&brief(), &corrected, "run-01")
        .unwrap_err();
    assert!(
        error.to_string().contains("different answer key"),
        "{error}"
    );
    assert_eq!(store.crew_answer_key(&id).unwrap(), Some(key()));
    // The same key under another run is refused too.
    let error = store
        .create_crew_task(&brief(), &key(), "run-02")
        .unwrap_err();
    assert!(error.to_string().contains("different run"), "{error}");
    assert_eq!(store.crew_task(&id).unwrap().unwrap().run_id, "run-01");
}

#[test]
fn a_worker_reads_the_brief_and_messages_but_nothing_with_the_checks() {
    let (_dir, store) = open();
    let id = task(&store);
    for text in [
        example!("message.valid.question"),
        example!("message.valid.blocker-to-lead"),
        example!("evidence.valid.test-pass"),
        example!("gate.valid.checks-by-code"),
        example!("result.valid.passed"),
    ] {
        store.add_crew_object(&id, &object(text)).unwrap();
    }
    let evidence = store.crew_evidence(&id).unwrap();
    assert!(
        evidence[0].command.as_str().contains("cargo test"),
        "the stored evidence names the command"
    );
    let shown = store.crew_worker_objects(&id, "worker-02").unwrap();
    let kinds: Vec<_> = shown
        .iter()
        .map(|o| CrewObject::from(o.clone()).type_name())
        .collect();
    assert_eq!(
        kinds,
        ["brief", "message"],
        "no evidence, gate, result or message to the lead"
    );
    let wire = serde_json::to_string(&shown).unwrap();
    for (label, secret) in [
        ("check command", evidence[0].command.as_str()),
        ("answer key id", key().id.as_str()),
        ("protected paths field", "protected_paths"),
        ("check id", "chk-tests"),
    ] {
        assert!(!wire.contains(secret), "a worker was shown the {label}");
    }
}

/// A message restated with another recipient, under its own content address.
fn message_to(to: &str, body: &str) -> CrewObject {
    let CrewObject::Message(mut message) = object(example!("message.valid.question")) else {
        unreachable!()
    };
    message.to = orbyn_contracts::CrewAddressee::new(to).unwrap();
    message.body = orbyn_contracts::CrewLongText::new(body).unwrap();
    message.wait_seconds = None;
    message.id = CrewObject::Message(message.clone()).computed_id().unwrap();
    CrewObject::Message(message)
}

#[test]
fn a_worker_sees_only_messages_to_all_or_to_itself() {
    let (_dir, store) = open();
    let id = task(&store);
    for (to, body) in [
        ("worker-01", "for A"),
        ("worker-02", "for B"),
        ("lead", "for the lead"),
        ("all", "for everyone"),
    ] {
        store.add_crew_object(&id, &message_to(to, body)).unwrap();
    }
    let bodies = |reader: &str| -> Vec<String> {
        store
            .crew_worker_objects(&id, reader)
            .unwrap()
            .into_iter()
            .filter_map(|o| match o {
                orbyn_contracts::WorkerObject::Message(m) => Some(m.body.to_string()),
                _ => None,
            })
            .collect()
    };
    // The example's author is worker-01, so its own broadcast is not shown back to it.
    assert_eq!(bodies("worker-01"), ["for A"]);
    assert_eq!(bodies("worker-02"), ["for B", "for everyone"]);
    assert_eq!(bodies("worker-03"), ["for everyone"]);
}

/// Migration 32: a task has a run row, and the task moves only along the lifecycle (H13).
#[test]
fn a_task_moves_only_along_the_lifecycle_and_a_pass_needs_evidence() {
    let (_dir, store) = open();
    let id = task(&store);
    store.add_crew_run(&id, "lead-thread", "worker-1").unwrap();
    let run = store.crew_run(&id).unwrap().unwrap();
    assert_eq!((run.attempts, run.worker_thread_id), (0, None));

    let state = |store: &Store| store.crew_task(&id).unwrap().unwrap().state;
    let refused = |to: &str, reason: Option<&str>| store.move_crew_task(&id, to, reason).is_err();
    // `briefed` goes to `routed`, and not straight to `running`.
    assert!(refused("running", None) && refused("verifying", None));
    assert_eq!(
        store.move_crew_task(&id, "routed", None).unwrap().state,
        "routed"
    );
    store
        .set_crew_worker(&id, "worker-thread", "/work", Some("abc"))
        .unwrap();
    assert_eq!(
        store.move_crew_task(&id, "running", None).unwrap().state,
        "running"
    );
    assert_eq!(
        store
            .crew_task_for_worker("worker-thread")
            .unwrap()
            .unwrap()
            .0
            .id,
        id
    );
    // Only `stopped` names a reason, and nothing passes without Evidence (H8).
    assert!(refused("stopped", None) && refused("needs_person", Some("budget")));
    assert!(refused("passed", None));
    assert_eq!(state(&store), "running");

    // A report counts the attempt; a failure goes through `failed` to what the guard names.
    let attempt = |store: &Store, n: u32| {
        store
            .begin_crew_verification(&id, &format!("engine-run-{n}"), None)
            .unwrap();
        assert_eq!(state(store), "verifying");
    };
    attempt(&store, 1);
    let (moved, next) = store.fail_crew_verification(&id, "f1").unwrap();
    assert_eq!((moved.state.as_str(), next), ("running", Next::Retry));
    attempt(&store, 2);
    let (moved, next) = store.fail_crew_verification(&id, "f1").unwrap();
    assert_eq!(
        (moved.state.as_str(), next),
        ("changed_approach", Next::ChangeApproach)
    );
    // Only the core resumes the worker after a change of approach.
    assert!(
        store
            .begin_crew_verification(&id, "engine-run-x", None)
            .is_err()
    );
    store.move_crew_task(&id, "running", None).unwrap();
    attempt(&store, 3);
    let (moved, next) = store.fail_crew_verification(&id, "f1").unwrap();
    assert_eq!((moved.state.as_str(), next), ("stopped", Next::Stop));
    let ended = store.crew_task(&id).unwrap().unwrap();
    assert_eq!(ended.stop_reason.as_deref(), Some("repeated_failure"));
    let run = store.crew_run(&id).unwrap().unwrap();
    assert_eq!(
        (run.failures, run.guard.streak, run.guard.approach_changed),
        (3, 3, true)
    );
    // A final task goes nowhere.
    for to in ["running", "passed", "cancelled", "needs_person"] {
        assert!(refused(to, None), "{to}");
    }
    let again = store.create_crew_task(&brief(), &key(), "run-02");
    assert!(again.is_err(), "the same brief is one task with one run");
}

/// H8: a pass names the Evidence this attempt made, and each id must be the task's.
#[test]
fn a_pass_needs_the_attempts_own_evidence() {
    let (_dir, store) = open();
    let id = task(&store);
    store.add_crew_run(&id, "lead-thread", "worker-1").unwrap();
    store.move_crew_task(&id, "routed", None).unwrap();
    store.move_crew_task(&id, "running", None).unwrap();
    store
        .begin_crew_verification(&id, "engine-run-1", None)
        .unwrap();
    let evidence = object(example!("evidence.valid.test-pass"));
    store.add_crew_object(&id, &evidence).unwrap();
    // Evidence from nowhere, or none, is not enough, and neither is any move to `passed`.
    assert!(store.pass_crew_task(&id, &[]).is_err());
    assert!(
        store
            .pass_crew_task(&id, &[format!("sha256:{}", "1".repeat(64))])
            .is_err()
    );
    assert!(store.move_crew_task(&id, "passed", None).is_err());
    assert_eq!(store.crew_task(&id).unwrap().unwrap().state, "verifying");
    let ids = [evidence.id().as_str().to_owned()];
    assert_eq!(store.pass_crew_task(&id, &ids).unwrap().state, "passed");
}
