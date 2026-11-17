//! The reviewer backtest at rest: migration 49 on a database with old rows, the table checks, a run with its lanes
//! and results, saving a lane twice, and what a removed project takes with it.

use std::collections::BTreeMap;

use rusqlite::{Connection, params};

use super::*;
use crate::testing::TempDir;

fn open() -> (TempDir, Store, String) {
    let dir = TempDir::new();
    let store = Store::open(&dir.path().join("orbyn.db"), "epoch-a").unwrap();
    let project = store
        .add_project(NewProject {
            path: "/repo/a",
            name: "a",
            is_git: true,
            managed: false,
            scratch: false,
        })
        .unwrap()
        .id;
    (dir, store, project)
}

fn budgets() -> BTreeMap<String, u64> {
    BTreeMap::from([("engine-a".to_owned(), 500), ("engine-b".to_owned(), 900)])
}

fn lanes() -> Vec<(String, String, String)> {
    vec![
        ("correctness".into(), "engine-a".into(), "model-a".into()),
        ("correctness".into(), "engine-b".into(), "model-b".into()),
    ]
}

fn create(store: &Store, id: &str, project: &str) {
    store
        .crew_backtest_create(
            id,
            project,
            BacktestState::Estimated,
            &budgets(),
            &lanes(),
            100,
        )
        .unwrap();
}

fn lane(engine: &str, tokens: u64, state: LaneState) -> BacktestLaneRow {
    BacktestLaneRow {
        lane: "correctness".into(),
        engine: engine.into(),
        model: format!("model-{}", &engine[7..]),
        tokens_used: tokens,
        state,
        escapes_n: None,
        caught_n: None,
    }
}

fn result(engine: &str, pr: u64, fp: &str, outcome: ResultOutcome) -> BacktestResultRow {
    BacktestResultRow {
        lane: "correctness".into(),
        engine: engine.into(),
        pr,
        finding_fp: fp.into(),
        outcome,
    }
}

#[test]
fn migration_49_follows_48_and_applies_on_a_database_with_old_rows() {
    let versions: Vec<i64> = migrations::MIGRATIONS.iter().map(|(v, _)| *v).collect();
    assert!(versions.windows(2).all(|w| w[1] == w[0] + 1));
    assert!(versions.contains(&49));

    let dir = TempDir::new();
    let path = dir.path().join("old.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(
        "PRAGMA foreign_keys=ON; CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL)",
    )
    .unwrap();
    for (version, sql) in migrations::MIGRATIONS.iter().filter(|(v, _)| *v <= 48) {
        conn.execute_batch(sql).unwrap();
        conn.execute(
            "INSERT INTO schema_migrations VALUES (?1,'fixture')",
            [version],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO projects (id, name, path, trusted, is_git, created_at, managed, scratch)
         VALUES ('old-project', 'old', '/old', 0, 1, 't', 0, 0)",
        [],
    )
    .unwrap();
    drop(conn);

    let store = Store::open(&path, "backtest-upgrade").unwrap();
    let upgrade = store.upgrade().expect("an upgrade");
    assert_eq!(
        (upgrade.from, upgrade.to),
        (48, migrations::MIGRATIONS.last().unwrap().0)
    );
    // The old project is untouched and has no runs.
    assert_eq!(store.project("old-project").unwrap().name, "old");
    assert!(
        store
            .crew_backtest_runs_of("old-project")
            .unwrap()
            .is_empty()
    );
    create(&store, "run-1", "old-project");
    drop(store);
    let reopened = Store::open(&path, "backtest-reopen").unwrap();
    assert!(reopened.upgrade().is_none());
    assert_eq!(
        reopened.crew_backtest_runs_of("old-project").unwrap().len(),
        1
    );
}

#[test]
fn the_tables_refuse_values_outside_their_checks() {
    let (_dir, store, project) = open();
    create(&store, "run-1", &project);
    let conn = store.writer();
    for state in ["paused", "", "FINISHED"] {
        assert!(
            conn.execute(
                "INSERT INTO crew_backtest_runs (id, project_id, state, budgets, created_unix) VALUES ('r', ?1, ?2, '{}', 1)",
                params![project, state],
            )
            .is_err(),
            "run state {state}"
        );
    }
    assert!(
        conn.execute(
            "INSERT INTO crew_backtest_runs (id, project_id, state, budgets, created_unix) VALUES ('r', ?1, 'running', 'not json', 1)",
            [&project],
        )
        .is_err()
    );
    assert!(
        conn.execute(
            "INSERT INTO crew_backtest_runs (id, project_id, state, budgets, created_unix) VALUES ('r', 'no-such-project', 'running', '{}', 1)",
            [],
        )
        .is_err()
    );
    for sql in [
        "INSERT INTO crew_backtest_lanes (run_id, lane, engine, model, state) VALUES ('run-1', 'l', 'e', 'm', 'paused')",
        "INSERT INTO crew_backtest_lanes (run_id, lane, engine, model, state, tokens_used) VALUES ('run-1', 'l', 'e', 'm', 'done', -1)",
        "INSERT INTO crew_backtest_lanes (run_id, lane, engine, model, state) VALUES ('no-run', 'l', 'e', 'm', 'done')",
        "INSERT INTO crew_backtest_lanes (run_id, lane, engine, model, state) VALUES ('run-1', 'correctness', 'engine-a', 'm', 'done')",
        "INSERT INTO crew_backtest_lanes (run_id, lane, engine, model, state, escapes_n) VALUES ('run-1', 'l', 'e', 'm', 'done', -2)",
        "INSERT INTO crew_backtest_results (run_id, lane, engine, pr, finding_fp, outcome) VALUES ('run-1', 'correctness', 'engine-a', 1, 'fp', 'wrong')",
        "INSERT INTO crew_backtest_results (run_id, lane, engine, pr, finding_fp, outcome) VALUES ('run-1', 'other', 'engine-a', 1, 'fp', 'caught')",
        "INSERT INTO crew_backtest_results (run_id, lane, engine, pr, finding_fp, outcome) VALUES ('run-1', 'correctness', 'engine-a', -1, 'fp', 'caught')",
    ] {
        assert!(conn.execute(sql, []).is_err(), "{sql}");
    }
}

#[test]
fn the_tables_keep_ids_and_counts_only() {
    let (_dir, store, _project) = open();
    let conn = store.reader();
    for (table, allowed) in [
        (
            "crew_backtest_runs",
            vec![
                "id",
                "project_id",
                "state",
                "budgets",
                "stop_reason",
                "created_unix",
                "finished_unix",
            ],
        ),
        (
            "crew_backtest_lanes",
            vec![
                "run_id",
                "lane",
                "engine",
                "model",
                "tokens_used",
                "state",
                "escapes_n",
                "caught_n",
            ],
        ),
        (
            "crew_backtest_results",
            vec!["run_id", "lane", "engine", "pr", "finding_fp", "outcome"],
        ),
    ] {
        let mut statement = conn
            .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
            .unwrap();
        let mut columns: Vec<String> = statement
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|c| c.unwrap())
            .collect();
        columns.sort();
        let mut want: Vec<String> = allowed.iter().map(|c| (*c).to_owned()).collect();
        want.sort();
        assert_eq!(columns, want, "{table}");
    }
}

#[test]
fn a_run_is_created_with_pending_lanes_and_read_back() {
    let (_dir, store, project) = open();
    create(&store, "run-1", &project);
    let run = store.crew_backtest_run("run-1").unwrap();
    assert_eq!(
        run,
        BacktestRunRow {
            id: "run-1".into(),
            project_id: project.clone(),
            state: BacktestState::Estimated,
            budgets: budgets(),
            stop_reason: None,
            created_unix: 100,
            finished_unix: None,
        }
    );
    let stored = store.crew_backtest_lanes("run-1").unwrap();
    assert_eq!(stored.len(), 2);
    assert!(
        stored
            .iter()
            .all(|l| l.state == LaneState::Pending && l.tokens_used == 0)
    );
    assert!(
        stored
            .iter()
            .all(|l| l.escapes_n.is_none() && l.caught_n.is_none())
    );
    assert!(store.crew_backtest_results("run-1").unwrap().is_empty());
    assert_eq!(store.crew_backtest_runs_of(&project).unwrap(), vec![run]);
    assert!(matches!(
        store.crew_backtest_run("missing"),
        Err(CoreError::NotFound(_))
    ));
}

#[test]
fn a_run_needs_a_known_project_and_a_token_budget_for_every_engine() {
    let (_dir, store, project) = open();
    assert!(matches!(
        store.crew_backtest_create("r", "nope", BacktestState::Running, &budgets(), &lanes(), 1),
        Err(CoreError::NotFound(_))
    ));
    for bad in [
        BTreeMap::new(),
        BTreeMap::from([("engine-a".to_owned(), 0)]),
        BTreeMap::from([("engine-a".to_owned(), u64::MAX)]),
    ] {
        assert!(
            store
                .crew_backtest_create("r", &project, BacktestState::Running, &bad, &lanes(), 1)
                .is_err()
        );
    }
    create(&store, "run-1", &project);
    assert!(
        store
            .crew_backtest_create(
                "run-1",
                &project,
                BacktestState::Running,
                &budgets(),
                &lanes(),
                2
            )
            .is_err()
    );
    // The refused attempts left nothing behind.
    assert_eq!(store.crew_backtest_runs_of(&project).unwrap().len(), 1);
}

#[test]
fn lane_progress_and_results_are_saved_together_and_saving_twice_changes_nothing() {
    let (_dir, store, project) = open();
    create(&store, "run-1", &project);
    let mut a = lane("engine-a", 300, LaneState::Stopped);
    a.escapes_n = Some(4);
    a.caught_n = Some(2);
    let results = [
        result("engine-a", 1, "sha256:f1", ResultOutcome::Caught),
        result("engine-a", 2, "sha256:f2", ResultOutcome::NoEscape),
    ];
    store
        .crew_backtest_save_lane("run-1", &a, &results)
        .unwrap();
    store
        .crew_backtest_save_lane("run-1", &a, &results)
        .unwrap();

    let lanes = store.crew_backtest_lanes("run-1").unwrap();
    assert_eq!(lanes[0], a);
    assert_eq!(lanes[1].state, LaneState::Pending);
    assert_eq!(
        store.crew_backtest_results("run-1").unwrap(),
        results.to_vec()
    );

    // A later save replaces a result with the same fingerprint.
    store
        .crew_backtest_save_lane(
            "run-1",
            &a,
            &[result("engine-a", 1, "sha256:f1", ResultOutcome::Unmatched)],
        )
        .unwrap();
    assert_eq!(
        store.crew_backtest_results("run-1").unwrap()[0].outcome,
        ResultOutcome::Unmatched
    );
    assert_eq!(store.crew_backtest_results("run-1").unwrap().len(), 2);
}

#[test]
fn a_save_for_an_unknown_lane_or_with_another_lanes_result_changes_nothing() {
    let (_dir, store, project) = open();
    create(&store, "run-1", &project);
    assert!(matches!(
        store.crew_backtest_save_lane("run-1", &lane("engine-z", 1, LaneState::Done), &[]),
        Err(CoreError::NotFound(_))
    ));
    let before = store.crew_backtest_lanes("run-1").unwrap();
    let mixed = [
        result("engine-a", 1, "sha256:f1", ResultOutcome::Caught),
        result("engine-b", 1, "sha256:f2", ResultOutcome::Caught),
    ];
    assert!(
        store
            .crew_backtest_save_lane("run-1", &lane("engine-a", 50, LaneState::Done), &mixed)
            .is_err()
    );
    assert_eq!(store.crew_backtest_lanes("run-1").unwrap(), before);
    assert!(store.crew_backtest_results("run-1").unwrap().is_empty());
}

#[test]
fn a_run_changes_state_and_records_why_it_stopped() {
    let (_dir, store, project) = open();
    create(&store, "run-1", &project);
    store
        .crew_backtest_set_state(
            "run-1",
            BacktestState::Stopped,
            Some("token budget spent: engine-a"),
            Some(900),
        )
        .unwrap();
    let run = store.crew_backtest_run("run-1").unwrap();
    assert_eq!(run.state, BacktestState::Stopped);
    assert_eq!(
        run.stop_reason.as_deref(),
        Some("token budget spent: engine-a")
    );
    assert_eq!(run.finished_unix, Some(900));
    assert!(matches!(
        store.crew_backtest_set_state("missing", BacktestState::Failed, None, None),
        Err(CoreError::NotFound(_))
    ));
}

#[test]
fn runs_come_newest_first_and_a_removed_project_takes_its_runs_with_it() {
    let (_dir, store, project) = open();
    create(&store, "run-1", &project);
    store
        .crew_backtest_create(
            "run-2",
            &project,
            BacktestState::Running,
            &budgets(),
            &lanes(),
            200,
        )
        .unwrap();
    let ids: Vec<String> = store
        .crew_backtest_runs_of(&project)
        .unwrap()
        .into_iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(ids, vec!["run-2", "run-1"]);

    store
        .crew_backtest_save_lane(
            "run-1",
            &lane("engine-a", 1, LaneState::Done),
            &[result("engine-a", 1, "sha256:f1", ResultOutcome::Caught)],
        )
        .unwrap();
    store
        .writer()
        .execute("DELETE FROM projects WHERE id = ?1", [&project])
        .unwrap();
    for table in [
        "crew_backtest_runs",
        "crew_backtest_lanes",
        "crew_backtest_results",
    ] {
        let count: i64 = store
            .reader()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "{table}");
    }
}

#[test]
fn interrupted_runs_are_failed_with_their_open_lanes_and_finished_ones_are_left() {
    let (_dir, store, project) = open();
    create(&store, "estimated", &project);
    store
        .crew_backtest_create(
            "running",
            &project,
            BacktestState::Running,
            &budgets(),
            &lanes(),
            200,
        )
        .unwrap();
    store
        .crew_backtest_save_lane("running", &lane("engine-a", 5, LaneState::Done), &[])
        .unwrap();
    store
        .crew_backtest_create(
            "finished",
            &project,
            BacktestState::Running,
            &budgets(),
            &lanes(),
            300,
        )
        .unwrap();
    store
        .crew_backtest_set_state("finished", BacktestState::Finished, None, Some(310))
        .unwrap();

    assert_eq!(store.crew_backtest_recover_interrupted(999).unwrap(), 1);
    let run = store.crew_backtest_run("running").unwrap();
    assert_eq!(run.state, BacktestState::Failed);
    assert_eq!(run.stop_reason.as_deref(), Some("interrupted"));
    assert_eq!(run.finished_unix, Some(999));
    let states: Vec<LaneState> = store
        .crew_backtest_lanes("running")
        .unwrap()
        .iter()
        .map(|l| l.state)
        .collect();
    // The lane that was done stays done; the one that never ran is failed.
    assert_eq!(states, vec![LaneState::Done, LaneState::Failed]);
    assert_eq!(
        store.crew_backtest_run("estimated").unwrap().state,
        BacktestState::Estimated
    );
    assert_eq!(
        store.crew_backtest_run("finished").unwrap().state,
        BacktestState::Finished
    );
    // A second call finds nothing.
    assert_eq!(store.crew_backtest_recover_interrupted(1000).unwrap(), 0);
}

#[test]
fn a_budget_above_the_maximum_is_refused_and_a_not_measured_outcome_is_stored() {
    let (_dir, store, project) = open();
    let over = BTreeMap::from([("engine-a".to_owned(), MOST_BUDGET_TOKENS + 1)]);
    assert!(
        store
            .crew_backtest_create("r", &project, BacktestState::Running, &over, &lanes(), 1)
            .is_err()
    );
    create(&store, "run-1", &project);
    store
        .crew_backtest_save_lane(
            "run-1",
            &lane("engine-a", 1, LaneState::Done),
            &[result(
                "engine-a",
                1,
                "sha256:f1",
                ResultOutcome::NotMeasured,
            )],
        )
        .unwrap();
    assert_eq!(
        store.crew_backtest_results("run-1").unwrap()[0].outcome,
        ResultOutcome::NotMeasured
    );
}
