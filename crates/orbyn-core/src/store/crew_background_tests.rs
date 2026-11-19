//! Background crews at rest: what the table itself refuses, whatever the caller does, and that a change and its audit row stand or fall
//! together.

use serde_json::json;

use super::*;
use crate::testing::TempDir;

fn store(dir: &TempDir) -> Store {
    Store::open(&dir.path().join("orbyn.sqlite3"), "epoch-a").unwrap()
}

fn new_crew<'a>(name: &'a str, settings: &'a Value) -> NewBackground<'a> {
    NewBackground {
        project_id: "project-1",
        name,
        template: "fix_failing_ci",
        trigger_kind: "ci_failure",
        trigger_settings: settings,
        budget_turns: 10,
        budget_minutes: 30,
        daily_runs: 4,
        daily_turns: 40,
        utc_offset_minutes: 600,
    }
}

#[test]
fn the_table_refuses_a_change_of_budget_template_or_project() {
    let dir = TempDir::new();
    let store = store(&dir);
    let settings = json!({});
    let crew = store
        .create_background_crew(&new_crew("One", &settings), "user")
        .unwrap();
    for column in [
        "budget_turns = 99",
        "budget_minutes = 99",
        "daily_runs = 99",
        "daily_turns = 99",
        "utc_offset_minutes = 99",
        "template = 'flaky_test'",
        "project_id = 'other'",
    ] {
        let result = store.writer().execute(
            &format!("UPDATE crew_background SET {column} WHERE id = ?1"),
            [&crew.id],
        );
        assert!(result.is_err(), "{column}");
    }
    assert_eq!(store.background_crew(&crew.id).unwrap().unwrap(), crew);
}

#[test]
fn the_table_refuses_what_the_core_would_have_refused() {
    let dir = TempDir::new();
    let store = store(&dir);
    let insert = |extra: &str| {
        store.writer().execute(
            &format!(
                "INSERT INTO crew_background (id, project_id, name, template, trigger_kind, trigger_settings,
                 budget_turns, budget_minutes, created_at, updated_at)
                 VALUES ('x', 'p', {extra}, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
            ),
            [],
        )
    };
    let ok = "'n', 'flaky_test', 'schedule', '{}', 5, 5";
    assert!(insert(ok).is_ok());
    store
        .writer()
        .execute("DELETE FROM crew_background", [])
        .unwrap();
    for bad in [
        "'', 'flaky_test', 'schedule', '{}', 5, 5",
        "'n', 'deploy', 'schedule', '{}', 5, 5",
        "'n', 'flaky_test', 'webhook', '{}', 5, 5",
        "'n', 'flaky_test', 'schedule', 'not json', 5, 5",
        "'n', 'flaky_test', 'schedule', '{}', 0, 5",
        "'n', 'flaky_test', 'schedule', '{}', 5, 1441",
    ] {
        assert!(insert(bad).is_err(), "{bad}");
    }
}

#[test]
fn a_failed_audit_row_rolls_the_change_back() {
    let dir = TempDir::new();
    let store = store(&dir);
    let settings = json!({});
    let crew = store
        .create_background_crew(&new_crew("Keep", &settings), "user")
        .unwrap();
    store
        .writer()
        .execute_batch(
            "CREATE TRIGGER refuse_background_audit BEFORE INSERT ON audit_events
             WHEN NEW.action LIKE 'crew.background%' BEGIN SELECT RAISE(ABORT, 'no audit'); END;",
        )
        .unwrap();
    assert!(
        store
            .create_background_crew(&new_crew("Unaudited", &settings), "user")
            .is_err()
    );
    assert!(
        store
            .update_background_crew(&crew.id, Some("Renamed"), None, "user")
            .is_err()
    );
    assert!(
        store
            .set_background_enabled(&crew.id, true, "user")
            .is_err()
    );
    assert!(store.delete_background_crew(&crew.id, "user").is_err());
    let (all, total) = store.background_crews_page(None, 50, 0).unwrap();
    assert_eq!(
        (total, all),
        (1, vec![crew]),
        "no effect without its audit row"
    );
}

#[test]
fn a_run_is_refused_the_second_running_row_by_the_table_too() {
    let dir = TempDir::new();
    let store = store(&dir);
    let settings = json!({});
    let crew = store
        .create_background_crew(&new_crew("Run", &settings), "user")
        .unwrap();
    store
        .set_background_enabled(&crew.id, true, "user")
        .unwrap();
    store.start_background_run(&crew.id).unwrap();
    let second = store.writer().execute(
        "INSERT INTO crew_background_runs (id, background_id, state, budget_turns, budget_minutes, started_at, updated_at)
         VALUES ('r2', ?1, 'running', 1, 1, 'a', 'a')",
        [&crew.id],
    );
    assert!(second.is_err(), "one running run per crew");
    let bad_stop = store.writer().execute(
        "INSERT INTO crew_background_runs (id, background_id, state, budget_turns, budget_minutes, started_at, updated_at, ended_at)
         VALUES ('r3', ?1, 'stopped', 1, 1, 'a', 'a', 'b')",
        [&crew.id],
    );
    assert!(bad_stop.is_err(), "a stopped run names its reason");
}

#[test]
fn a_run_keeps_the_budget_it_was_given() {
    let dir = TempDir::new();
    let store = store(&dir);
    let settings = json!({});
    let crew = store
        .create_background_crew(&new_crew("Fixed", &settings), "user")
        .unwrap();
    store
        .set_background_enabled(&crew.id, true, "user")
        .unwrap();
    let run = store.start_background_run(&crew.id).unwrap();
    for column in ["budget_turns = 999", "budget_minutes = 999"] {
        let result = store.writer().execute(
            &format!("UPDATE crew_background_runs SET {column} WHERE id = ?1"),
            [&run.id],
        );
        assert!(result.is_err(), "{column}");
    }
    // What a run spends still changes.
    let spent = store.add_background_spend(&run.id, 1, 1).unwrap();
    assert_eq!((spent.budget_turns, spent.turns_spent), (10, 1));
}

#[test]
fn a_spend_needs_its_audit_row_and_a_budget_stop_needs_both() {
    let dir = TempDir::new();
    let store = store(&dir);
    let settings = json!({});
    let crew = store
        .create_background_crew(&new_crew("Spend", &settings), "user")
        .unwrap();
    store
        .set_background_enabled(&crew.id, true, "user")
        .unwrap();
    let run = store.start_background_run(&crew.id).unwrap();
    let unchanged = |store: &Store| {
        let (rows, _) = store.background_runs_page(&crew.id, 10, 0).unwrap();
        assert_eq!(
            (
                rows[0].state.as_str(),
                rows[0].turns_spent,
                rows[0].minutes_spent
            ),
            ("running", 0, 0)
        );
    };
    // Without its own audit row a spend changes nothing.
    store
        .writer()
        .execute_batch(
            "CREATE TRIGGER refuse_spend_audit BEFORE INSERT ON audit_events
             WHEN NEW.action = 'crew.background.run.spend' BEGIN SELECT RAISE(ABORT, 'no audit'); END;",
        )
        .unwrap();
    assert!(store.add_background_spend(&run.id, 2, 2).is_err());
    unchanged(&store);
    store
        .writer()
        .execute_batch("DROP TRIGGER refuse_spend_audit")
        .unwrap();
    // Reaching the budget needs the stop row as well: without it the spend is rolled back too.
    store
        .writer()
        .execute_batch(
            "CREATE TRIGGER refuse_stop_audit BEFORE INSERT ON audit_events
             WHEN NEW.action = 'crew.background.run.stop' BEGIN SELECT RAISE(ABORT, 'no audit'); END;",
        )
        .unwrap();
    assert!(store.add_background_spend(&run.id, 10, 0).is_err());
    unchanged(&store);
    store
        .writer()
        .execute_batch("DROP TRIGGER refuse_stop_audit")
        .unwrap();
    let stopped = store.add_background_spend(&run.id, 10, 0).unwrap();
    assert_eq!(
        (
            stopped.state.as_str(),
            stopped.turns_spent,
            stopped.review_waiting
        ),
        ("stopped", 10, true)
    );
}
#[test]
fn the_daily_ledger_counts_per_run_day_and_engine() {
    let dir = TempDir::new();
    let store = store(&dir);
    let settings = json!({});
    let crew = store
        .create_background_crew(&new_crew("Daily", &settings), "user")
        .unwrap();
    store
        .set_background_enabled(&crew.id, true, "user")
        .unwrap();
    let day = "2026-11-19";

    // Two turns of one run: one row, two turns, one run.
    let first = store.start_background_run(&crew.id).unwrap();
    assert_eq!(
        store
            .add_background_daily_turn(&crew.id, &first.id, day, "alpha")
            .unwrap(),
        (1, 1)
    );
    assert_eq!(
        store
            .add_background_daily_turn(&crew.id, &first.id, day, "alpha")
            .unwrap(),
        (1, 2)
    );
    // A second run the same day: two runs, three turns.
    store
        .end_background_run(&first.id, RunEnd::NeedsReview("done"))
        .unwrap();
    let second = store.start_background_run(&crew.id).unwrap();
    store
        .add_background_daily_turn(&crew.id, &second.id, day, "alpha")
        .unwrap();
    assert_eq!(
        store
            .background_daily_spent(&crew.id, day, "alpha")
            .unwrap(),
        (2, 3)
    );
    // Another engine's count is its own, and so is another day's.
    assert_eq!(
        store.background_daily_spent(&crew.id, day, "beta").unwrap(),
        (0, 0)
    );
    store
        .add_background_daily_turn(&crew.id, &second.id, day, "beta")
        .unwrap();
    assert_eq!(
        store.background_daily_spent(&crew.id, day, "beta").unwrap(),
        (1, 1)
    );
    assert_eq!(
        store
            .background_daily_spent(&crew.id, "2026-11-20", "alpha")
            .unwrap(),
        (0, 0)
    );
    // Every change stood with its audit row: three spends so far were refused without it.
    let rows: Vec<String> = store
        .list_audit(None, 200)
        .unwrap()
        .into_iter()
        .filter(|row| row.action == "crew.background.daily.spend")
        .map(|row| row.action)
        .collect();
    assert_eq!(rows.len(), 4, "one audit row per counted turn");
}

#[test]
fn a_daily_spend_needs_its_audit_row() {
    let dir = TempDir::new();
    let store = store(&dir);
    let settings = json!({});
    let crew = store
        .create_background_crew(&new_crew("Unaudited daily", &settings), "user")
        .unwrap();
    store
        .set_background_enabled(&crew.id, true, "user")
        .unwrap();
    let run = store.start_background_run(&crew.id).unwrap();
    store
        .writer()
        .execute_batch(
            "CREATE TRIGGER refuse_daily_audit BEFORE INSERT ON audit_events
             WHEN NEW.action = 'crew.background.daily.spend' BEGIN SELECT RAISE(ABORT, 'no audit'); END;",
        )
        .unwrap();
    assert!(
        store
            .add_background_daily_turn(&crew.id, &run.id, "2026-11-19", "alpha")
            .is_err()
    );
    assert_eq!(
        store
            .background_daily_spent(&crew.id, "2026-11-19", "alpha")
            .unwrap(),
        (0, 0),
        "no count without its audit row"
    );
}

#[test]
fn a_daily_stop_ends_the_run_with_the_sentence_it_was_given() {
    let dir = TempDir::new();
    let store = store(&dir);
    let settings = json!({});
    let crew = store
        .create_background_crew(&new_crew("Capped", &settings), "user")
        .unwrap();
    store
        .set_background_enabled(&crew.id, true, "user")
        .unwrap();
    let run = store.start_background_run(&crew.id).unwrap();
    let stopped = store
        .end_background_run_daily(
            &run.id,
            "Engine alpha reached this crew's daily budget: 3 of its 2 turns today. The budget resets at 2026-11-20T00:00:00+11:00.",
            (3, 12),
        )
        .unwrap();
    assert_eq!(
        (
            stopped.state.as_str(),
            stopped.stop_reason.as_deref(),
            stopped.review_waiting,
            stopped.turns_spent,
            stopped.minutes_spent
        ),
        ("stopped", Some("budget"), true, 3, 12),
        "a daily stop waits for the person and shows the overshoot"
    );
    assert_eq!(
        stopped.review_reason.as_deref(),
        Some(
            "Engine alpha reached this crew's daily budget: 3 of its 2 turns today. The budget resets at 2026-11-20T00:00:00+11:00."
        )
    );
    // It is a stop like any other: a second one is refused.
    assert!(
        store
            .end_background_run_daily(&run.id, "again", (3, 12))
            .is_err()
    );
}

#[test]
fn a_daily_refusal_writes_the_audit_row_that_names_the_engine() {
    let dir = TempDir::new();
    let store = store(&dir);
    let settings = json!({});
    let crew = store
        .create_background_crew(&new_crew("Refused", &settings), "user")
        .unwrap();
    store
        .audit_background_daily_refusal(
            &crew.id,
            "codex",
            "2026-11-19",
            "2026-11-20T00:00:00+11:00",
        )
        .unwrap();
    let row = store
        .list_audit(None, 200)
        .unwrap()
        .into_iter()
        .find(|row| row.action == "crew.background.daily.refused")
        .expect("the refusal is audited");
    assert_eq!(row.target, crew.id);
    assert_eq!(row.detail["reason"], json!("daily budget spent: codex"));
    assert_eq!(row.detail["resets_at"], json!("2026-11-20T00:00:00+11:00"));
}
