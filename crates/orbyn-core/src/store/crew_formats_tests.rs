//! Stored formats: populated upgrades for the migrations that change an existing row, and
//! Crew Protocol objects whose stored body no longer matches its content address.

use std::path::{Path, PathBuf};

use orbyn_contracts::{AnswerKey, Brief, CrewObject};
use rusqlite::{Connection, params};
use serde_json::{Value, json};

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
    let store = Store::open(&dir.path().join("orbyn.db"), "epoch-a").unwrap();
    (dir, store)
}

fn task(store: &Store) -> String {
    store
        .create_crew_task(&brief(), &key(), "run-01")
        .unwrap()
        .id
}

/// A database exactly at `through`, created the way an older install left it.
fn database(dir: &TempDir, through: i64) -> (PathBuf, Connection) {
    let path = dir.path().join("old.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(
        "PRAGMA foreign_keys=ON;
         CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL)",
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

/// Opens the store and asserts it upgraded from `from` all the way to the newest schema.
fn upgraded(path: &Path, from: i64, epoch: &str) -> Store {
    let store = Store::open(path, epoch).unwrap();
    let upgrade = store.upgrade().expect("an old database is upgraded");
    assert_eq!(
        (upgrade.from, upgrade.to),
        (from, migrations::MIGRATIONS.last().unwrap().0)
    );
    store
}

/// Reopens an already upgraded store and asserts nothing is applied a second time.
fn reopened(path: &Path, epoch: &str) -> Store {
    let store = Store::open(path, epoch).unwrap();
    assert!(store.upgrade().is_none(), "a second open applies nothing");
    store
}

fn count(store: &Store, table: &str) -> i64 {
    store
        .reader()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}

fn refused(store: &Store, sql: &str, expected: &str) {
    let error = store.writer().execute(sql, []).unwrap_err();
    let text = error.to_string();
    assert!(
        text.contains(expected),
        "wanted {expected:?} from {sql:?}, got {text}"
    );
}

fn refused_one(store: &Store, sql: &str, arg: &str, expected: &str) {
    let error = store.writer().execute(sql, [arg]).unwrap_err();
    let text = error.to_string();
    assert!(
        text.contains(expected),
        "wanted {expected:?} from {sql:?}, got {text}"
    );
}

#[test]
fn migration_53_keeps_a_scored_lane_and_leaves_unresolved_null() {
    let dir = TempDir::new();
    let (path, conn) = database(&dir, 52);
    conn.execute_batch(
        "INSERT INTO projects (id, name, path, trusted, is_git, created_at)
         VALUES ('p', 'p', '/p', 0, 1, 't');
         INSERT INTO crew_backtest_runs (id, project_id, state, budgets, created_unix)
         VALUES ('bt', 'p', 'running', '{\"engine-a\":500}', 1);
         INSERT INTO crew_backtest_lanes
             (run_id, lane, engine, model, tokens_used, state, escapes_n, caught_n)
         VALUES ('bt', 'correctness', 'engine-a', 'model-a', 42, 'done', 3, 2);",
    )
    .unwrap();
    drop(conn);

    let store = upgraded(&path, 52, "backtest-unresolved");
    let lanes = store.crew_backtest_lanes("bt").unwrap();
    assert_eq!(lanes.len(), 1);
    assert_eq!(
        (
            lanes[0].tokens_used,
            lanes[0].escapes_n,
            lanes[0].caught_n,
            lanes[0].unresolved_n
        ),
        (42, Some(3), Some(2), None),
        "the old lane keeps its counts and was never scored for unresolved escapes"
    );
    refused(
        &store,
        "INSERT INTO crew_backtest_lanes
             (run_id, lane, engine, model, tokens_used, state, unresolved_n)
         VALUES ('bt', 'unresolved', 'engine-a', 'model-a', 0, 'done', -1)",
        "CHECK constraint failed: unresolved_n IS NULL OR unresolved_n >= 0",
    );
    assert_eq!(count(&store, "crew_backtest_lanes"), 1);
    drop(store);

    let again = reopened(&path, "backtest-unresolved-reopen");
    let lanes = again.crew_backtest_lanes("bt").unwrap();
    assert_eq!(lanes.len(), 1);
    assert_eq!(lanes[0].tokens_used, 42);
    assert_eq!(lanes[0].unresolved_n, None);
}

#[test]
fn migration_57_gives_an_old_background_crew_no_daily_budget_and_keeps_its_runs() {
    let dir = TempDir::new();
    let (path, conn) = database(&dir, 56);
    conn.execute_batch(
        "INSERT INTO crew_background
             (id, project_id, name, template, trigger_kind, trigger_settings,
              budget_turns, budget_minutes, enabled, created_at, updated_at)
         VALUES ('crew-1', 'project-1', 'Old', 'fix_failing_ci', 'schedule', '{}',
                 10, 30, 1, 'created', 'updated');
         INSERT INTO crew_background_runs
             (id, background_id, state, stop_reason, turns_spent, minutes_spent,
              budget_turns, budget_minutes, review_waiting, review_reason,
              started_at, updated_at, ended_at)
         VALUES ('run-1', 'crew-1', 'stopped', 'budget', 10, 30,
                 10, 30, 1, 'budget', 'started', 'updated', 'ended');",
    )
    .unwrap();
    drop(conn);

    let store = upgraded(&path, 56, "background-daily");
    let crew = store.background_crew("crew-1").unwrap().unwrap();
    assert_eq!(
        (
            crew.name.as_str(),
            crew.budget_turns,
            crew.budget_minutes,
            crew.enabled
        ),
        ("Old", 10, 30, true)
    );
    assert_eq!(
        (crew.daily_runs, crew.daily_turns, crew.utc_offset_minutes),
        (0, 0, 0),
        "a crew from before the daily budget has none"
    );
    let run = store.background_run("run-1").unwrap().unwrap();
    assert_eq!(
        (
            run.turns_spent,
            run.review_waiting,
            run.stop_reason.as_deref()
        ),
        (10, true, Some("budget"))
    );
    assert_eq!(
        store
            .background_daily_spent("crew-1", "2026-11-01", "claude")
            .unwrap(),
        (0, 0)
    );
    assert_eq!(
        store
            .add_background_daily_turn("crew-1", "run-1", "2026-11-01", "claude")
            .unwrap(),
        (1, 1)
    );
    refused(
        &store,
        "INSERT INTO crew_background
             (id, project_id, name, template, trigger_kind, trigger_settings,
              budget_turns, budget_minutes, enabled, created_at, updated_at,
              daily_runs, daily_turns, utc_offset_minutes)
         VALUES ('bad-runs', 'project-1', 'Bad runs', 'fix_failing_ci', 'schedule', '{}',
                 1, 1, 0, 'c', 'u', -1, 0, 0)",
        "CHECK constraint failed: daily_runs >= 0",
    );
    refused(
        &store,
        "INSERT INTO crew_background
             (id, project_id, name, template, trigger_kind, trigger_settings,
              budget_turns, budget_minutes, enabled, created_at, updated_at,
              daily_runs, daily_turns, utc_offset_minutes)
         VALUES ('bad-zone', 'project-1', 'Bad zone', 'fix_failing_ci', 'schedule', '{}',
                 1, 1, 0, 'c', 'u', 0, 0, 841)",
        "CHECK constraint failed: utc_offset_minutes BETWEEN -840 AND 840",
    );
    assert_eq!(count(&store, "crew_background_daily"), 1);
    drop(store);

    let again = reopened(&path, "background-daily-reopen");
    let crew = again.background_crew("crew-1").unwrap().unwrap();
    assert_eq!((crew.daily_runs, crew.daily_turns), (0, 0));
    assert_eq!(
        again
            .background_daily_spent("crew-1", "2026-11-01", "claude")
            .unwrap(),
        (1, 1)
    );
}

#[test]
fn migration_60_defaults_a_crew_note_to_false_on_old_queues_and_runs() {
    let dir = TempDir::new();
    let (path, conn) = database(&dir, 59);
    conn.execute_batch(
        "INSERT INTO projects (id, name, path, trusted, is_git, created_at)
         VALUES ('p', 'p', '/p', 0, 1, 't');
         INSERT INTO threads (id, project_id, title, engine, cwd, created_at, updated_at)
         VALUES ('thread-1', 'p', 't', 'fake', '/p', 't', 't');
         INSERT INTO queued_messages (id, thread_id, prompt, attachment_ids, created_at)
         VALUES ('q-1', 'thread-1', 'hi', '[]', 't');
         INSERT INTO runs (id, thread_id, engine, state, owner_epoch, started_at)
         VALUES ('run-1', 'thread-1', 'fake', 'completed', 'epoch', 't');",
    )
    .unwrap();
    drop(conn);

    let store = upgraded(&path, 59, "crew-notes");
    assert!(
        !store.run_is_crew_note("run-1").unwrap(),
        "an old run is not a crew note"
    );
    let queued: i64 = store
        .reader()
        .query_row(
            "SELECT crew_note FROM queued_messages WHERE id = 'q-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(queued, 0);
    refused(
        &store,
        "INSERT INTO runs (id, thread_id, engine, state, owner_epoch, started_at, crew_note)
         VALUES ('bad-run', 'thread-1', 'fake', 'completed', 'epoch', 't', NULL)",
        "NOT NULL constraint failed: runs.crew_note",
    );
    refused(
        &store,
        "INSERT INTO queued_messages
             (id, thread_id, prompt, attachment_ids, created_at, crew_note)
         VALUES ('bad-q', 'thread-1', 'hi', '[]', 't', NULL)",
        "NOT NULL constraint failed: queued_messages.crew_note",
    );
    assert_eq!(count(&store, "runs"), 1);
    assert_eq!(count(&store, "queued_messages"), 1);
    drop(store);

    let again = reopened(&path, "crew-notes-reopen");
    assert!(!again.run_is_crew_note("run-1").unwrap());
    let queued: i64 = again
        .reader()
        .query_row(
            "SELECT crew_note FROM queued_messages WHERE id = 'q-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(queued, 0);
    assert_eq!(count(&again, "runs"), 1);
}

#[test]
fn migration_69_leaves_an_old_thread_unimported_and_refuses_another_source() {
    let dir = TempDir::new();
    let (path, conn) = database(&dir, 68);
    conn.execute_batch(
        "INSERT INTO projects (id, name, path, trusted, is_git, created_at)
         VALUES ('p', 'p', '/p', 0, 1, 't');
         INSERT INTO threads (id, project_id, title, engine, cwd, created_at, updated_at)
         VALUES ('thread-1', 'p', 't', 'fake', '/p', 't', 't');",
    )
    .unwrap();
    drop(conn);

    let store = upgraded(&path, 68, "thread-import");
    let thread = store.thread("thread-1").unwrap();
    assert_eq!(
        (thread.source, thread.imported_at),
        (None, None),
        "a thread made here was not imported from a transcript"
    );
    assert_eq!(thread.title, "t");
    refused(
        &store,
        "INSERT INTO threads (id, project_id, title, engine, cwd, created_at, updated_at, source)
         VALUES ('bad-thread', 'p', 't', 'fake', '/p', 't', 't', 'other')",
        "CHECK constraint failed: source IS NULL OR source IN ('orbyn-transcript')",
    );
    store
        .writer()
        .execute(
            "UPDATE threads SET source = 'orbyn-transcript',
                 imported_at = '2026-11-23T10:00:00Z' WHERE id = 'thread-1'",
            [],
        )
        .unwrap();
    let imported = store.thread("thread-1").unwrap();
    assert_eq!(imported.source.as_deref(), Some("orbyn-transcript"));
    assert_eq!(
        imported.imported_at.as_deref(),
        Some("2026-11-23T10:00:00Z")
    );
    drop(store);

    let again = reopened(&path, "thread-import-reopen");
    assert_eq!(
        again.thread("thread-1").unwrap().source.as_deref(),
        Some("orbyn-transcript")
    );
    assert_eq!(count(&again, "threads"), 1);
}

#[test]
fn migration_76_creates_a_manual_check_awaiting_a_person_and_keeps_its_task() {
    let dir = TempDir::new();
    let task = format!("sha256:{}", "1".repeat(64));
    let (path, conn) = database(&dir, 75);
    conn.execute(
        "INSERT INTO crew_tasks (id, crew_id, run_id, created_at, updated_at)
         VALUES (?1, 'crew-1', 'run-1', 'created', 'updated')",
        [&task],
    )
    .unwrap();
    drop(conn);

    let store = upgraded(&path, 75, "manual-checks");
    assert_eq!(store.crew_task(&task).unwrap().unwrap().crew_id, "crew-1");
    store.park_manual_check(&task, "chk-manual", 100).unwrap();
    let held = store
        .crew_manual_check(&task, "chk-manual")
        .unwrap()
        .unwrap();
    assert_eq!(
        (held.state, held.note, held.decided_unix),
        (ManualCheckState::Awaiting, None, None)
    );
    assert_eq!(held.waiting_unix, 100);
    refused_one(
        &store,
        "INSERT INTO crew_manual_checks (task_id, check_id, state, waiting_unix)
         VALUES (?1, 'chk-bad-state', 'maybe', 1)",
        &task,
        "CHECK constraint failed: state IN ('awaiting', 'passed', 'failed')",
    );
    refused_one(
        &store,
        "INSERT INTO crew_manual_checks (task_id, check_id, state, waiting_unix, note)
         VALUES (?1, 'chk-bad-note', 'awaiting', 1, '')",
        &task,
        "CHECK constraint failed: note IS NULL OR length(note) BETWEEN 1 AND 200",
    );
    assert_eq!(count(&store, "crew_manual_checks"), 1);
    drop(store);

    let again = reopened(&path, "manual-checks-reopen");
    assert_eq!(
        again
            .crew_manual_check(&task, "chk-manual")
            .unwrap()
            .unwrap()
            .state,
        ManualCheckState::Awaiting
    );
    assert_eq!(count(&again, "crew_manual_checks"), 1);
    assert_eq!(count(&again, "crew_tasks"), 1);
}

#[test]
fn migration_77_keeps_background_rows_across_reopen_and_lets_a_watch_be_created() {
    let dir = TempDir::new();
    let (path, conn) = database(&dir, 76);
    conn.execute_batch(
        "INSERT INTO crew_background
             (id, project_id, name, template, trigger_kind, trigger_settings,
              budget_turns, budget_minutes, enabled, created_at, updated_at,
              daily_runs, daily_turns, utc_offset_minutes)
         VALUES ('old', 'project-1', 'Old', 'fix_failing_ci', 'schedule', '{}',
                 10, 30, 1, 'created', 'updated', 2, 40, 600);
         INSERT INTO crew_background_runs
             (rowid, id, background_id, state, stop_reason, turns_spent, minutes_spent,
              budget_turns, budget_minutes, review_waiting, review_reason,
              started_at, updated_at, ended_at)
         VALUES (7, 'run-1', 'old', 'stopped', 'budget', 10, 30,
                 10, 30, 1, 'budget', 'started', 'updated', 'ended');
         INSERT INTO crew_background_daily (background_id, run_id, day, engine, turns_spent)
         VALUES ('old', 'run-1', '2026-11-01', 'claude', 10);",
    )
    .unwrap();
    drop(conn);

    let store = upgraded(&path, 76, "watch-upgrade");
    let crew = store.background_crew("old").unwrap().unwrap();
    assert_eq!(
        (
            crew.name.as_str(),
            crew.trigger_kind.as_str(),
            crew.budget_turns,
            crew.daily_runs,
            crew.daily_turns,
            crew.utc_offset_minutes
        ),
        ("Old", "schedule", 10, 2, 40, 600)
    );
    let run = store.background_run("run-1").unwrap().unwrap();
    assert_eq!((run.turns_spent, run.review_waiting), (10, true));
    assert_eq!(
        store
            .background_daily_spent("old", "2026-11-01", "claude")
            .unwrap(),
        (1, 10)
    );
    let rowid: i64 = store
        .reader()
        .query_row(
            "SELECT rowid FROM crew_background_runs WHERE id = 'run-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        rowid, 7,
        "the rebuilt run keeps the rowid the page orders by"
    );

    let watch = store
        .create_background_crew(
            &NewBackground {
                project_id: "project-1",
                name: "Watch",
                template: "fix_failing_ci",
                trigger_kind: "watch",
                trigger_settings: &json!({ "command": "cargo test", "every_hours": 6 }),
                budget_turns: 5,
                budget_minutes: 10,
                daily_runs: 0,
                daily_turns: 0,
                utc_offset_minutes: 0,
            },
            "user",
        )
        .unwrap();
    assert_eq!(watch.trigger_kind, "watch");
    assert!(!watch.enabled, "a new crew is created off");
    refused(
        &store,
        "INSERT INTO crew_background
             (id, project_id, name, template, trigger_kind, trigger_settings,
              budget_turns, budget_minutes, created_at, updated_at)
         VALUES ('bad-kind', 'project-1', 'Bad kind', 'fix_failing_ci', 'webhook', '{}',
                 1, 1, 'c', 'u')",
        "CHECK constraint failed: trigger_kind IN ('ci_failure', 'issue_label', 'schedule', 'dependency_update', 'watch')",
    );
    store
        .writer()
        .execute(
            "INSERT INTO crew_background
                 (id, project_id, name, template, trigger_kind, trigger_settings,
                  budget_turns, budget_minutes, created_at, updated_at)
             VALUES ('defaults', 'project-1', 'Defaults', 'fix_failing_ci', 'schedule', '{}',
                     1, 1, 'c', 'u')",
            [],
        )
        .unwrap();
    let defaults: (i64, i64, i64) = store
        .reader()
        .query_row(
            "SELECT enabled, daily_runs, daily_turns FROM crew_background WHERE id = 'defaults'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(defaults, (0, 0, 0));
    drop(store);

    let again = reopened(&path, "watch-reopen");
    let (crews, total) = again.background_crews_page(None, 10, 0).unwrap();
    assert_eq!(total, 3);
    assert_eq!(crews.len(), 3);
    assert_eq!(
        again.background_runs_page("old", 10, 0).unwrap().1,
        1,
        "the rebuild neither duplicates nor loses a run"
    );
    assert_eq!(
        again
            .background_daily_spent("old", "2026-11-01", "claude")
            .unwrap(),
        (1, 10)
    );
}

#[test]
fn migration_78_keeps_a_dismissal_to_one_row_and_refuses_a_null_since() {
    let dir = TempDir::new();
    let task = format!("sha256:{}", "3".repeat(64));
    let (path, conn) = database(&dir, 77);
    conn.execute(
        "INSERT INTO crew_tasks (id, crew_id, run_id, created_at, updated_at)
         VALUES (?1, 'crew-1', 'run-1', 'created', 'updated')",
        [&task],
    )
    .unwrap();
    drop(conn);

    let store = upgraded(&path, 77, "needs-you");
    assert_eq!(store.crew_task(&task).unwrap().unwrap().crew_id, "crew-1");
    assert!(
        store
            .dismiss_crew_needs_you("task", &task, "since-1", "user")
            .unwrap()
    );
    let held = store
        .crew_needs_you_dismissal("task", &task)
        .unwrap()
        .unwrap();
    assert_eq!(held.since, "since-1");
    assert!(
        !store
            .dismiss_crew_needs_you("task", &task, "since-1", "user")
            .unwrap(),
        "the same item at the same time writes nothing"
    );
    assert!(
        store
            .dismiss_crew_needs_you("task", &task, "since-2", "user")
            .unwrap(),
        "a later wait of the same id shows again and replaces the row"
    );
    assert_eq!(store.crew_needs_you_dismissals().unwrap().len(), 1);
    refused(
        &store,
        "INSERT INTO crew_needs_you_dismissed (kind, id, since, dismissed_unix)
         VALUES ('task', 'other', NULL, 1)",
        "NOT NULL constraint failed: crew_needs_you_dismissed.since",
    );
    assert_eq!(count(&store, "crew_needs_you_dismissed"), 1);
    drop(store);

    let again = reopened(&path, "needs-you-reopen");
    let held = again
        .crew_needs_you_dismissal("task", &task)
        .unwrap()
        .unwrap();
    assert_eq!(held.since, "since-2");
    assert_eq!(count(&again, "crew_needs_you_dismissed"), 1);
    assert_eq!(again.crew_task(&task).unwrap().unwrap().crew_id, "crew-1");
}

#[test]
fn migration_79_gives_an_old_run_no_resume_and_counts_the_one_after_it() {
    let dir = TempDir::new();
    let task = format!("sha256:{}", "4".repeat(64));
    let bad_task = format!("sha256:{}", "5".repeat(64));
    let (path, conn) = database(&dir, 78);
    conn.execute(
        "INSERT INTO crew_tasks (id, crew_id, run_id, state, created_at, updated_at)
         VALUES (?1, 'crew-1', 'run-1', 'running', 'created', 'updated')",
        [&task],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO crew_runs
             (task_id, lead_thread_id, member, worker_thread_id, worktree, base_commit,
              attempts, failures, streak, fingerprint, approach_changed, last_report_run,
              protected, verifying_since, index_flags)
         VALUES (?1, 'lead-1', 'worker-01', 'worker-thread', '/wt', 'abc',
                 2, 1, 1, NULL, 0, NULL, NULL, NULL, NULL)",
        [&task],
    )
    .unwrap();
    drop(conn);

    let store = upgraded(&path, 78, "crew-resume");
    let run = store.crew_run(&task).unwrap().unwrap();
    assert_eq!(
        (
            run.member.as_str(),
            run.worker_thread_id.as_deref(),
            run.attempts,
            run.resumed_at.as_deref(),
            run.resume_count,
            run.downtime_secs
        ),
        ("worker-01", Some("worker-thread"), 2, None, 0, 0),
        "an old run keeps its counters and has not resumed"
    );
    store
        .writer()
        .execute(
            "INSERT INTO crew_tasks (id, crew_id, run_id, state, created_at, updated_at)
             VALUES (?1, 'crew-1', 'run-2', 'briefed', 'created', 'updated')",
            [&bad_task],
        )
        .unwrap();
    refused_one(
        &store,
        "INSERT INTO crew_runs (task_id, lead_thread_id, member, resume_count)
         VALUES (?1, 'lead-1', 'worker-01', -1)",
        &bad_task,
        "CHECK constraint failed: resume_count >= 0",
    );
    store.excuse_crew_downtime(&task, 30).unwrap();
    assert_eq!(store.crew_run(&task).unwrap().unwrap().downtime_secs, 30);
    let ResumeOutcome::Resumed { run, .. } = store.resume_crew_task(&task).unwrap() else {
        panic!("a builder lane takes its one resume");
    };
    assert_eq!(
        (run.resume_count, run.attempts, run.resumed_at.is_some()),
        (1, 3, true)
    );
    assert!(matches!(
        store.resume_crew_task(&task).unwrap(),
        ResumeOutcome::Refused("already_resumed")
    ));
    assert_eq!(store.crew_run(&task).unwrap().unwrap().downtime_secs, 30);
    drop(store);

    let again = reopened(&path, "crew-resume-reopen");
    let run = again.crew_run(&task).unwrap().unwrap();
    assert_eq!(
        (run.resume_count, run.attempts, run.downtime_secs),
        (1, 3, 30)
    );
    assert_eq!(count(&again, "crew_runs"), 1);
}

#[test]
fn migration_86_adds_a_typed_stop_detail_without_a_rebuild() {
    let dir = TempDir::new();
    let old = format!("sha256:{}", "6".repeat(64));
    let fresh = format!("sha256:{}", "7".repeat(64));
    // A database an older install left at 85, with a stopped task an old restart wrote.
    let (path, conn) = database(&dir, 85);
    conn.execute(
        "INSERT INTO crew_tasks (id, crew_id, run_id, state, stop_reason, created_at, updated_at)
         VALUES (?1, 'crew-1', 'run-old', 'stopped', 'engine_error', 'created', 'updated')",
        [&old],
    )
    .unwrap();
    drop(conn);

    let store = upgraded(&path, 85, "crew-stop-detail");
    let row = store.crew_task(&old).unwrap().unwrap();
    assert_eq!(
        (
            row.state.as_str(),
            row.stop_reason.as_deref(),
            row.stop_detail.as_deref()
        ),
        ("stopped", Some("engine_error"), None),
        "an old stopped row survives and has no detail"
    );

    // A new row may take the typed detail.
    store
        .writer()
        .execute(
            "INSERT INTO crew_tasks
                 (id, crew_id, run_id, state, stop_reason, stop_detail, created_at, updated_at)
             VALUES (?1, 'crew-1', 'run-new', 'stopped', 'engine_error', 'worker_stopped_twice',
                     'created', 'updated')",
            [&fresh],
        )
        .unwrap();
    assert_eq!(
        store
            .crew_task(&fresh)
            .unwrap()
            .unwrap()
            .stop_detail
            .as_deref(),
        Some("worker_stopped_twice")
    );

    // Any other value is refused by the column's CHECK, with the exact SQLite error.
    let error = store
        .writer()
        .execute(
            "UPDATE crew_tasks SET stop_detail = 'tired' WHERE id = ?1",
            [&old],
        )
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "CHECK constraint failed: stop_detail IS NULL OR stop_detail IN ('worker_stopped_twice')"
    );
    drop(store);

    let again = reopened(&path, "crew-stop-detail-reopen");
    assert_eq!(again.crew_task(&old).unwrap().unwrap().stop_detail, None);
    assert_eq!(
        again
            .crew_task(&fresh)
            .unwrap()
            .unwrap()
            .stop_detail
            .as_deref(),
        Some("worker_stopped_twice")
    );
}

fn tamper_brief(store: &Store, id: &str) {
    let body: String = store
        .reader()
        .query_row("SELECT body FROM crew_briefs WHERE id = ?1", [id], |row| {
            row.get(0)
        })
        .unwrap();
    let mut value: Value = serde_json::from_str(&body).unwrap();
    value["outcome"] = Value::String("A different outcome, still valid.".into());
    let conn = store.writer();
    conn.execute("DELETE FROM crew_briefs WHERE id = ?1", [id])
        .unwrap();
    conn.execute(
        "INSERT INTO crew_briefs (id, body) VALUES (?1, ?2)",
        params![id, value.to_string()],
    )
    .unwrap();
}

fn tamper_answer_key(store: &Store, task_id: &str) {
    let (key_id, body): (String, String) = store
        .reader()
        .query_row(
            "SELECT id, body FROM crew_answer_keys WHERE task_id = ?1",
            [task_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let mut value: Value = serde_json::from_str(&body).unwrap();
    value["checks"][0]["proves"] = Value::String("A different proof, still valid.".into());
    let conn = store.writer();
    conn.execute("DELETE FROM crew_answer_keys WHERE task_id = ?1", [task_id])
        .unwrap();
    conn.execute(
        "INSERT INTO crew_answer_keys (task_id, id, body) VALUES (?1, ?2, ?3)",
        params![task_id, key_id, value.to_string()],
    )
    .unwrap();
}

#[test]
fn a_crew_brief_that_no_longer_hashes_is_refused_by_both_readers() {
    let (_dir, store) = open();
    let id = task(&store);
    tamper_brief(&store, &id);

    let error = store.crew_brief(&id).unwrap_err();
    assert!(matches!(error, CoreError::Internal(_)), "{error:?}");
    let text = error.to_string();
    assert!(
        text.contains("a stored crew object no longer verifies"),
        "{text}"
    );
    assert!(text.contains("IdMismatch"), "{text}");
    assert!(text.contains(&id), "the error names the object: {text}");

    let error = store
        .crew_briefs_of_tasks(std::slice::from_ref(&id))
        .unwrap_err();
    assert!(matches!(error, CoreError::Internal(_)), "{error:?}");
    let text = error.to_string();
    assert!(
        text.contains("a stored crew object no longer verifies"),
        "{text}"
    );
    assert!(text.contains(&id), "the error names the object: {text}");
}

#[test]
fn an_answer_key_that_no_longer_hashes_is_refused_by_both_readers() {
    let (_dir, store) = open();
    let id = task(&store);
    let key_id = key().id.to_string();
    tamper_answer_key(&store, &id);

    let error = store.crew_answer_key(&id).unwrap_err();
    assert!(matches!(error, CoreError::Internal(_)), "{error:?}");
    let text = error.to_string();
    assert!(
        text.contains("a stored crew object no longer verifies"),
        "{text}"
    );
    assert!(text.contains("IdMismatch"), "{text}");
    assert!(text.contains(&key_id), "the error names the object: {text}");

    let error = store
        .crew_answer_keys_of_tasks(std::slice::from_ref(&id))
        .unwrap_err();
    assert!(matches!(error, CoreError::Internal(_)), "{error:?}");
    let text = error.to_string();
    assert!(
        text.contains("a stored crew object no longer verifies"),
        "{text}"
    );
    assert!(text.contains(&key_id), "the error names the object: {text}");
}

#[test]
fn a_crew_run_with_no_worker_and_an_unknown_member_reads_without_panicking() {
    let (_dir, store) = open();
    let id = task(&store);
    store
        .add_crew_run(&id, "lead-thread", "curious-01")
        .unwrap();

    let run = store.crew_run(&id).unwrap().unwrap();
    assert_eq!(run.member, "curious-01");
    assert_eq!(run.worker_thread_id, None);
    assert_eq!(run.resume_count, 0);
    assert_eq!(store.crew_turns_spent(&id).unwrap(), 0);

    store.move_crew_task(&id, "routed", None).unwrap();
    store.move_crew_task(&id, "running", None).unwrap();
    assert_eq!(
        store
            .crew_tasks_in_flight()
            .unwrap()
            .iter()
            .filter(|(task, _)| task.id == id)
            .count(),
        1
    );
    assert!(matches!(
        store.resume_crew_task(&id).unwrap(),
        ResumeOutcome::Refused("not_resumable")
    ));
    let run = store.crew_run(&id).unwrap().unwrap();
    assert_eq!(
        (run.resume_count, run.resumed_at.is_none()),
        (0, true),
        "an unknown member is refused without a write"
    );
}
