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
fn acknowledging_a_waiting_review_clears_the_wait_and_keeps_the_reason() {
    let dir = TempDir::new();
    let store = store(&dir);
    let settings = json!({});
    let crew = store
        .create_background_crew(&new_crew("Settled", &settings), "user")
        .unwrap();
    store
        .set_background_enabled(&crew.id, true, "user")
        .unwrap();
    let run = store.start_background_run(&crew.id).unwrap();
    let waiting = store.add_background_spend(&run.id, 10, 0).unwrap();
    assert!(waiting.review_waiting);
    assert_eq!(store.background_standing(&crew.id).unwrap().1, 1);
    let reason = waiting.review_reason.clone();
    assert!(reason.is_some(), "a waiting review says why it waits");

    let settled = store
        .acknowledge_background_review(&run.id, "user")
        .unwrap();
    assert!(!settled.review_waiting);
    assert!(
        settled.updated_at >= waiting.updated_at,
        "the write is stamped"
    );
    assert_eq!(
        settled.review_reason.as_deref(),
        reason.as_deref(),
        "the run keeps why it waited"
    );
    assert_eq!(
        (
            settled.state.as_str(),
            settled.stop_reason.as_deref(),
            settled.turns_spent,
            settled.minutes_spent,
            settled.budget_turns,
            settled.budget_minutes,
            settled.started_at.as_str(),
            settled.ended_at.clone(),
        ),
        (
            waiting.state.as_str(),
            waiting.stop_reason.as_deref(),
            waiting.turns_spent,
            waiting.minutes_spent,
            waiting.budget_turns,
            waiting.budget_minutes,
            waiting.started_at.as_str(),
            waiting.ended_at.clone(),
        ),
        "nothing but the wait and the stamp changes"
    );
    assert_eq!(
        store.background_standing(&crew.id).unwrap().1,
        0,
        "the crew has nothing waiting"
    );

    // The audit row is the person's, names the crew, and records what was settled before it is cleared.
    let audits = store.list_audit(None, 50).unwrap();
    let row = audits
        .iter()
        .find(|row| row.action == "crew.background.run.acknowledge")
        .expect("the acknowledgement is audited");
    assert_eq!(
        (row.actor.as_str(), row.source.as_str(), row.target.as_str()),
        ("user", "user", crew.id.as_str())
    );
    assert_eq!(row.detail["run"], run.id);
    assert_eq!(row.detail["state"], "stopped");
    assert_eq!(row.detail["stop_reason"], "budget");
    assert_eq!(row.detail["review_reason"], reason.unwrap());

    // A second acknowledgement has nothing to settle: plain refusal, nothing written.
    assert!(matches!(
        store.acknowledge_background_review(&run.id, "user"),
        Err(crate::error::CoreError::Invalid(_))
    ));
    assert_eq!(
        store
            .list_audit(None, 100)
            .unwrap()
            .iter()
            .filter(|row| row.action == "crew.background.run.acknowledge")
            .count(),
        1,
        "a refused acknowledgement writes no row"
    );
}

#[test]
fn acknowledging_a_review_that_is_not_waiting_is_refused_plainly_and_writes_nothing() {
    let dir = TempDir::new();
    let store = store(&dir);
    let settings = json!({});
    let crew = store
        .create_background_crew(&new_crew("Refused", &settings), "user")
        .unwrap();
    store
        .set_background_enabled(&crew.id, true, "user")
        .unwrap();
    let refused = |error: crate::error::CoreError| {
        assert!(
            matches!(&error, crate::error::CoreError::Invalid(why)
                if why == "this run is not waiting for the person's review"),
            "{error}"
        );
    };
    // A running run is not waiting.
    let running = store.start_background_run(&crew.id).unwrap();
    refused(
        store
            .acknowledge_background_review(&running.id, "user")
            .unwrap_err(),
    );
    // A run that stopped for an engine error left nothing to review.
    let stopped = store
        .end_background_run(&running.id, RunEnd::Stopped("engine_error"))
        .unwrap();
    assert!(!stopped.review_waiting);
    refused(
        store
            .acknowledge_background_review(&stopped.id, "user")
            .unwrap_err(),
    );
    // A run that is not there is not found, not a refusal.
    assert!(matches!(
        store.acknowledge_background_review("nothing", "user"),
        Err(crate::error::CoreError::NotFound(_))
    ));
    // Nothing was written: no acknowledgement row, and the run is as it was.
    assert!(
        store
            .list_audit(None, 100)
            .unwrap()
            .iter()
            .all(|row| row.action != "crew.background.run.acknowledge")
    );
    let (rows, total) = store.background_runs_page(&crew.id, 10, 0).unwrap();
    assert_eq!(total, 1);
    assert_eq!(
        (rows[0].state.as_str(), rows[0].review_waiting),
        ("stopped", false)
    );
}

#[test]
fn an_acknowledgement_needs_its_audit_row() {
    let dir = TempDir::new();
    let store = store(&dir);
    let settings = json!({});
    let crew = store
        .create_background_crew(&new_crew("Audited", &settings), "user")
        .unwrap();
    store
        .set_background_enabled(&crew.id, true, "user")
        .unwrap();
    let run = store.start_background_run(&crew.id).unwrap();
    let waiting = store.add_background_spend(&run.id, 10, 0).unwrap();
    assert!(waiting.review_waiting);
    store
        .writer()
        .execute_batch(
            "CREATE TRIGGER refuse_acknowledge_audit BEFORE INSERT ON audit_events
             WHEN NEW.action = 'crew.background.run.acknowledge' BEGIN SELECT RAISE(ABORT, 'no audit'); END;",
        )
        .unwrap();
    assert!(
        store
            .acknowledge_background_review(&run.id, "user")
            .is_err()
    );
    let (rows, _) = store.background_runs_page(&crew.id, 10, 0).unwrap();
    assert!(
        rows[0].review_waiting,
        "no effect without the audit row in the same transaction"
    );
}
