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
