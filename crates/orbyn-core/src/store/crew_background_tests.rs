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
fn core_startup_recovers_taskless_background_starts_once_and_preserves_task_backed_runs() {
    use crate::{Core, CoreConfig};
    let dir = TempDir::new();
    let settings = json!({});
    let (crew_id, interrupted, lead, preserved, modes) = {
        let store = store(&dir);
        let crew = store
            .create_background_crew(&new_crew("Interrupted", &settings), "user")
            .unwrap();
        store
            .set_background_enabled(&crew.id, true, "user")
            .unwrap();
        let project = store
            .add_project(NewProject {
                path: dir.path().to_str().unwrap(),
                name: "Project",
                is_git: false,
                managed: false,
                scratch: false,
            })
            .unwrap();
        let lead = store
            .create_thread(NewThread {
                project_id: project.id,
                title: "Interrupted background lead".into(),
                engine: "alpha".into(),
                model: None,
                cwd: dir.path().to_string_lossy().into_owned(),
                worktree: None,
                skill_profile: None,
                parent_thread_id: None,
                relation: None,
                scratch: false,
            })
            .unwrap();
        let run = store.start_background_run_as(&crew.id, &lead.id).unwrap();
        let interrupted = store.add_background_spend(&run.id, 2, 3).unwrap();
        store
            .set_crew_autopilot_mode(&format!("crew:{}", run.id), "off")
            .unwrap();
        let mut preserved = Vec::new();
        for (n, state) in [
            "briefed",
            "routed",
            "running",
            "needs_person",
            "stopped",
            "passed",
            "cancelled",
        ]
        .into_iter()
        .enumerate()
        {
            let crew = store
                .create_background_crew(&new_crew(&format!("Task {state}"), &settings), "user")
                .unwrap();
            store
                .set_background_enabled(&crew.id, true, "user")
                .unwrap();
            let run = store.start_background_run(&crew.id).unwrap();
            let task = format!("sha256:{n:064x}");
            store.writer().execute(
                "INSERT INTO crew_tasks (id, crew_id, run_id, state, stop_reason, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, 'now', 'now')",
                params![task, run.id, format!("task-run-{n}"), state, (state == "stopped").then_some("engine_error")],
            ).unwrap();
            store.add_crew_run(&task, &run.id, "worker").unwrap();
            preserved.push((run, Some(store.crew_task(&task).unwrap().unwrap())));
        }
        for end in [
            RunEnd::Stopped("disabled"),
            RunEnd::NeedsReview("Already waiting"),
        ] {
            let crew = store
                .create_background_crew(
                    &new_crew(&format!("Terminal {}", preserved.len()), &settings),
                    "user",
                )
                .unwrap();
            store
                .set_background_enabled(&crew.id, true, "user")
                .unwrap();
            let run = store.start_background_run(&crew.id).unwrap();
            preserved.push((store.end_background_run(&run.id, end).unwrap(), None));
        }
        (
            crew.id,
            interrupted,
            lead,
            preserved,
            store.crew_autopilot_modes_all(100).unwrap(),
        )
    };
    // No Store guard, Core, Arc or engine task from the previous owner survives into startup.
    let mut config = CoreConfig::new(dir.path().to_path_buf(), "http://127.0.0.1:1".into());
    config.engines.lookup_latest = false;
    config.engines.known_engine_ids = Some(Vec::new());
    config.engines.claude_program = dir
        .path()
        .join("absent/claude.exe")
        .to_string_lossy()
        .into_owned();
    config.engines.codex_program = dir
        .path()
        .join("absent/codex.exe")
        .to_string_lossy()
        .into_owned();
    let recovered = {
        let core = Core::open(config.clone()).unwrap();
        let recovered = core.store.background_run(&interrupted.id).unwrap().unwrap();
        assert_eq!(recovered.state, "needs_review");
        assert!(recovered.review_waiting && recovered.ended_at.is_some());
        assert!(
            recovered
                .review_reason
                .as_deref()
                .unwrap()
                .contains("interrupted before its task was created")
        );
        assert_eq!(
            (
                recovered.turns_spent,
                recovered.minutes_spent,
                recovered.budget_turns,
                recovered.budget_minutes
            ),
            (2, 3, 10, 30)
        );
        assert_eq!(core.store.crew_autopilot_modes_all(100).unwrap(), modes);
        assert_eq!(core.store.thread(&lead.id).unwrap(), lead);
        for (run, task) in &preserved {
            assert_eq!(
                core.store.background_run(&run.id).unwrap().as_ref(),
                Some(run)
            );
            if let Some(task) = task {
                assert_eq!(core.store.crew_task(&task.id).unwrap().as_ref(), Some(task));
            }
        }
        recovered
    };
    let core = Core::open(config).unwrap();
    assert_eq!(
        core.store.background_run(&interrupted.id).unwrap().unwrap(),
        recovered
    );
    let audit: i64 = core.store.reader().query_row(
        "SELECT COUNT(*) FROM audit_events WHERE action = 'crew.background.run.recover' AND json_extract(detail, '$.run') = ?1 AND json_extract(detail, '$.why') IS NOT NULL",
        [&interrupted.id], |row| row.get(0),
    ).unwrap();
    assert_eq!(audit, 1);
    assert!(
        core.store.start_background_run(&crew_id).is_ok(),
        "recovery releases the active-run slot"
    );
}

#[test]
fn core_startup_does_not_hide_a_failed_background_recovery_audit() {
    use crate::{Core, CoreConfig};
    let dir = TempDir::new();
    let settings = json!({});
    let run = {
        let store = store(&dir);
        let crew = store
            .create_background_crew(&new_crew("Audit failure", &settings), "user")
            .unwrap();
        store
            .set_background_enabled(&crew.id, true, "user")
            .unwrap();
        let run = store.start_background_run(&crew.id).unwrap();
        store.writer().execute_batch(
            "CREATE TRIGGER refuse_recovery BEFORE INSERT ON audit_events WHEN NEW.action = 'crew.background.run.recover' BEGIN SELECT RAISE(ABORT, 'recovery audit unavailable'); END;",
        ).unwrap();
        run
    };
    let mut config = CoreConfig::new(dir.path().to_path_buf(), "http://127.0.0.1:1".into());
    config.engines.lookup_latest = false;
    config.engines.known_engine_ids = Some(Vec::new());
    config.engines.claude_program = dir
        .path()
        .join("absent/claude.exe")
        .to_string_lossy()
        .into_owned();
    config.engines.codex_program = dir
        .path()
        .join("absent/codex.exe")
        .to_string_lossy()
        .into_owned();
    match Core::open(config.clone()) {
        Ok(_) => panic!("startup must report the failed recovery transaction"),
        Err(error) => assert!(
            error.to_string().contains("recovery audit unavailable"),
            "{error}"
        ),
    }
    {
        let store = store(&dir);
        assert_eq!(store.background_run(&run.id).unwrap().unwrap(), run);
        store
            .writer()
            .execute_batch("DROP TRIGGER refuse_recovery")
            .unwrap();
    }
    let core = Core::open(config).unwrap();
    assert_eq!(
        core.store.background_run(&run.id).unwrap().unwrap().state,
        "needs_review"
    );
}
