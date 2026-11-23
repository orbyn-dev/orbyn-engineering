//! Background crews at rest: what the table itself refuses, whatever the caller does, and that a change and its audit row stand or fall
//! together.

use rusqlite::Connection;
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
fn migration_59_follows_58_and_adds_the_event_claim_table() {
    let versions: Vec<i64> = migrations::MIGRATIONS.iter().map(|(v, _)| *v).collect();
    assert!(versions.contains(&59));
    // the escape retention owns 58, so this fixture stops at the migration before 59.
    let before = *versions
        .iter()
        .filter(|version| **version < 59)
        .max()
        .unwrap();

    let dir = TempDir::new();
    let path = dir.path().join("old.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL)").unwrap();
    for (version, sql) in migrations::MIGRATIONS.iter().filter(|(v, _)| *v < 59) {
        conn.execute_batch(sql).unwrap();
        conn.execute(
            "INSERT INTO schema_migrations VALUES (?1,'fixture')",
            [version],
        )
        .unwrap();
    }
    // A crew written before this migration survives it.
    conn.execute(
        "INSERT INTO crew_background (id, project_id, name, template, trigger_kind, trigger_settings,
         budget_turns, budget_minutes, created_at, updated_at)
         VALUES ('crew-1', 'p', 'Hourly', 'fix_failing_ci', 'schedule', '{}', 10, 30, 't', 't')",
        [],
    )
    .unwrap();
    drop(conn);
    let store = Store::open(&path, "event-upgrade").unwrap();
    let upgrade = store.upgrade().expect("an upgrade");
    assert_eq!(
        (upgrade.from, upgrade.to),
        (before, migrations::MIGRATIONS.last().unwrap().0)
    );
    assert_eq!(
        store.background_crew("crew-1").unwrap().unwrap().name,
        "Hourly"
    );
    // The table is empty on an upgraded database and claims first-write-wins from then on.
    let claims: i64 = store
        .reader()
        .query_row("SELECT COUNT(*) FROM crew_background_events", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(claims, 0);
    assert!(store.claim_background_event("schedule:crew-1:1").unwrap());
    assert!(!store.claim_background_event("schedule:crew-1:1").unwrap());
}

/// A `CoreConfig` for a test that opens a `Core` on `dir`: no network, no engine lookup, engine programs that do not exist.
fn recovery_config(dir: &TempDir) -> crate::CoreConfig {
    let mut config = crate::CoreConfig::new(dir.path().to_path_buf(), "http://127.0.0.1:1".into());
    config.engines.lookup_latest = false;
    config.engines.known_engine_ids = Some(Vec::new());
    config.engines.claude_program = dir
        .path()
        .join("absent/claude")
        .to_string_lossy()
        .into_owned();
    config.engines.codex_program = dir
        .path()
        .join("absent/codex")
        .to_string_lossy()
        .into_owned();
    config
}

/// What a startup must do to a running run that has this task: the task's state afterwards and whether the run ends.
struct Case {
    state: &'static str,
    worker: bool,
    task_after: &'static str,
    run_ends: bool,
}

#[test]
fn core_startup_recovers_interrupted_background_starts_once() {
    use crate::Core;
    let dir = TempDir::new();
    let settings = json!({});
    let cases = [
        Case {
            state: "briefed",
            worker: false,
            task_after: "needs_person",
            run_ends: true,
        },
        Case {
            state: "routed",
            worker: false,
            task_after: "needs_person",
            run_ends: true,
        },
        Case {
            state: "routed",
            worker: true,
            task_after: "needs_person",
            run_ends: true,
        },
        Case {
            state: "running",
            worker: false,
            task_after: "needs_person",
            run_ends: true,
        },
        Case {
            state: "running",
            worker: true,
            task_after: "running",
            run_ends: false,
        },
        Case {
            state: "verifying",
            worker: false,
            task_after: "needs_person",
            run_ends: true,
        },
        Case {
            state: "verifying",
            worker: true,
            task_after: "verifying",
            run_ends: false,
        },
        Case {
            state: "changed_approach",
            worker: true,
            task_after: "changed_approach",
            run_ends: false,
        },
        Case {
            state: "changed_approach",
            worker: false,
            task_after: "changed_approach",
            run_ends: true,
        },
        Case {
            state: "needs_person",
            worker: false,
            task_after: "needs_person",
            run_ends: true,
        },
        Case {
            state: "failed",
            worker: false,
            task_after: "failed",
            run_ends: true,
        },
        Case {
            state: "stopped",
            worker: false,
            task_after: "stopped",
            run_ends: true,
        },
        Case {
            state: "passed",
            worker: false,
            task_after: "passed",
            run_ends: true,
        },
        Case {
            state: "cancelled",
            worker: false,
            task_after: "cancelled",
            run_ends: true,
        },
    ];
    let (crew_id, interrupted, lead, runs, mixed) = {
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
                project_id: project.id.clone(),
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
        let run = store
            .start_background_run_as(&crew.id, &lead.id, "user")
            .unwrap();
        let interrupted = store.add_background_spend(&run.id, 2, 3).unwrap();
        let mut runs = Vec::new();
        for (n, case) in cases.iter().enumerate() {
            let crew = store
                .create_background_crew(&new_crew(&format!("Task {n}"), &settings), "user")
                .unwrap();
            store
                .set_background_enabled(&crew.id, true, "user")
                .unwrap();
            let run = store.start_background_run(&crew.id).unwrap();
            let task = format!("sha256:{n:064x}");
            store.writer().execute(
                "INSERT INTO crew_tasks (id, crew_id, run_id, state, stop_reason, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, 'now', 'now')",
                params![task, run.id, format!("task-run-{n}"), case.state, (case.state == "stopped").then_some("engine_error")],
            ).unwrap();
            store.add_crew_run(&task, &run.id, "worker").unwrap();
            if case.worker {
                let worker = store
                    .create_thread(NewThread {
                        project_id: project.id.clone(),
                        title: format!("Worker {n}"),
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
                store
                    .set_crew_worker(&task, &worker.id, "worktree", None)
                    .unwrap();
            }
            runs.push((run, store.crew_task(&task).unwrap().unwrap()));
        }
        for end in [
            RunEnd::Stopped("disabled"),
            RunEnd::NeedsReview("Already waiting"),
        ] {
            let crew = store
                .create_background_crew(
                    &new_crew(&format!("Terminal {}", runs.len()), &settings),
                    "user",
                )
                .unwrap();
            store
                .set_background_enabled(&crew.id, true, "user")
                .unwrap();
            let run = store.start_background_run(&crew.id).unwrap();
            let ended = store.end_background_run(&run.id, end).unwrap();
            let task = format!("sha256:{:064x}", 100 + runs.len());
            store.writer().execute(
                "INSERT INTO crew_tasks (id, crew_id, run_id, state, created_at, updated_at) VALUES (?1, ?2, ?3, 'needs_person', 'now', 'now')",
                params![task, ended.id, format!("terminal-{}", runs.len())],
            ).unwrap();
            runs.push((ended, store.crew_task(&task).unwrap().unwrap()));
        }
        // One run with a task in flight with a worker beside a task already waiting: the live task keeps the whole run.
        let mixed = {
            let crew = store
                .create_background_crew(&new_crew("Mixed", &settings), "user")
                .unwrap();
            store
                .set_background_enabled(&crew.id, true, "user")
                .unwrap();
            let run = store.start_background_run(&crew.id).unwrap();
            let live = format!("sha256:{:064x}", 200);
            let waiting = format!("sha256:{:064x}", 201);
            for (task, state, n) in [(&live, "running", "a"), (&waiting, "needs_person", "b")] {
                store.writer().execute(
                    "INSERT INTO crew_tasks (id, crew_id, run_id, state, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'now', 'now')",
                    params![task, run.id, format!("mixed-{n}"), state],
                ).unwrap();
                store.add_crew_run(task, &run.id, "worker").unwrap();
            }
            let worker = store
                .create_thread(NewThread {
                    project_id: project.id.clone(),
                    title: "Mixed worker".into(),
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
            store
                .set_crew_worker(&live, &worker.id, "worktree", None)
                .unwrap();
            (run, live, waiting)
        };
        (crew.id, interrupted, lead, runs, mixed)
    };
    let snapshot = |core: &Core| -> Vec<(String, String, String, Option<String>, usize)> {
        let mut all = Vec::new();
        for (run, task) in &runs {
            let run = core.store.background_run(&run.id).unwrap().unwrap();
            let task = core.store.crew_task(&task.id).unwrap().unwrap();
            let results = core.store.crew_results(&task.id).unwrap().len();
            all.push((
                run.state,
                task.state,
                run.updated_at,
                run.review_reason,
                results,
            ));
        }
        all
    };
    let count = |core: &Core, sql: &str| -> i64 {
        core.store
            .reader()
            .query_row(sql, [], |row| row.get(0))
            .unwrap()
    };
    // No Store guard, Core, Arc or engine task from the previous owner survives into startup.
    let config = recovery_config(&dir);
    let after_first = {
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
        assert_eq!(core.store.thread(&lead.id).unwrap(), lead);
        for (case, (run, task)) in cases.iter().zip(&runs) {
            let now = core.store.background_run(&run.id).unwrap().unwrap();
            let label = format!("{} worker={}", case.state, case.worker);
            assert_eq!(
                core.store.crew_task(&task.id).unwrap().unwrap().state,
                case.task_after,
                "{label}"
            );
            if case.run_ends {
                assert_eq!(now.state, "needs_review", "{label}");
                assert!(now.review_waiting && now.ended_at.is_some(), "{label}");
                let reason = now.review_reason.as_deref().unwrap();
                let held = matches!(case.state, "briefed" | "routed" | "running" | "verifying");
                assert_eq!(
                    reason.contains("Its task waits for a person"),
                    held,
                    "{label}: {reason}"
                );
                assert_eq!(
                    reason.contains("The run's task ended"),
                    matches!(case.state, "stopped" | "passed" | "cancelled"),
                    "{label}: {reason}"
                );
                assert_eq!(
                    reason.contains("left as it was"),
                    matches!(case.state, "needs_person" | "failed" | "changed_approach"),
                    "{label}: {reason}"
                );
                assert_eq!(
                    (now.turns_spent, now.minutes_spent, now.budget_turns),
                    (run.turns_spent, run.minutes_spent, run.budget_turns),
                    "{label}"
                );
            } else {
                assert_eq!(&now, run, "{label}: left to Crew task recovery");
            }
            // A task held for a person shows why: exactly one NeedsPerson result, and no other task gets one.
            let results = core.store.crew_results(&task.id).unwrap();
            if case.run_ends && case.task_after == "needs_person" {
                assert_eq!(results.len(), 1, "{label}: {results:?}");
                assert_eq!(results[0].state, orbyn_contracts::ResultState::NeedsPerson);
            } else {
                assert!(results.is_empty(), "{label}: {results:?}");
            }
        }
        // A task in flight with a worker keeps its whole run, whatever else the run holds.
        assert_eq!(
            core.store.background_run(&mixed.0.id).unwrap().unwrap(),
            mixed.0
        );
        assert_eq!(
            core.store.crew_task(&mixed.1).unwrap().unwrap().state,
            "running"
        );
        assert_eq!(
            core.store.crew_task(&mixed.2).unwrap().unwrap().state,
            "needs_person"
        );
        assert!(core.store.crew_results(&mixed.2).unwrap().is_empty());
        // Terminal runs, and their tasks, are unchanged.
        for (run, task) in &runs[cases.len()..] {
            assert_eq!(
                core.store.background_run(&run.id).unwrap().as_ref(),
                Some(run)
            );
            assert_eq!(core.store.crew_task(&task.id).unwrap().as_ref(), Some(task));
        }
        // Recovery starts no new run and no engine work.
        assert_eq!(
            count(&core, "SELECT COUNT(*) FROM crew_background_runs"),
            i64::try_from(cases.len()).unwrap() + 4
        );
        assert_eq!(
            count(&core, "SELECT COUNT(*) FROM crew_tasks"),
            i64::try_from(cases.len()).unwrap() + 4
        );
        assert_eq!(count(&core, "SELECT COUNT(*) FROM runs"), 0);
        let audit: i64 = core.store.reader().query_row(
            "SELECT COUNT(*) FROM audit_events WHERE action = 'crew.background.run.recover' AND json_extract(detail, '$.run') = ?1 AND json_extract(detail, '$.why') IS NOT NULL",
            [&interrupted.id], |row| row.get(0),
        ).unwrap();
        assert_eq!(audit, 1);
        let listed: String = core.store.reader().query_row(
            "SELECT json_extract(detail, '$.tasks') FROM audit_events WHERE action = 'crew.background.run.recover' AND json_extract(detail, '$.run') = ?1",
            [&runs[0].0.id], |row| row.get(0),
        ).unwrap();
        assert!(
            listed.contains("briefed") && listed.contains(&runs[0].1.id),
            "{listed}"
        );
        snapshot(&core)
    };
    let core = Core::open(config).unwrap();
    assert_eq!(
        snapshot(&core),
        after_first,
        "a second open changes nothing"
    );
    assert_eq!(
        count(
            &core,
            "SELECT COUNT(*) FROM audit_events WHERE action = 'crew.background.run.recover'"
        ),
        1 + i64::try_from(cases.iter().filter(|case| case.run_ends).count()).unwrap()
    );
    assert!(
        core.store.start_background_run(&crew_id).is_ok(),
        "recovery releases the active-run slot"
    );
}

#[test]
fn core_startup_does_not_hide_a_failed_background_recovery_audit() {
    use crate::Core;
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
    let config = recovery_config(&dir);
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

#[test]
fn a_refused_start_with_a_task_is_retained_atomically_and_retryable() {
    let dir = TempDir::new();
    let store = store(&dir);
    let settings = json!({});
    let crew = store
        .create_background_crew(&new_crew("Refusal", &settings), "user")
        .unwrap();
    store
        .set_background_enabled(&crew.id, true, "user")
        .unwrap();
    let run = store.start_background_run(&crew.id).unwrap();
    store.writer().execute(
        "INSERT INTO crew_tasks (id, crew_id, run_id, created_at, updated_at) VALUES ('sha256:refused', ?1, 'refused', 'now', 'now'), ('sha256:other', 'other-crew', 'other', 'now', 'now')",
        [&run.id],
    ).unwrap();
    store.writer().execute_batch(
        "CREATE TRIGGER refuse_review_audit BEFORE INSERT ON audit_events WHEN NEW.action = 'crew.background.run.review' BEGIN SELECT RAISE(ABORT, 'no audit'); END;",
    ).unwrap();
    assert!(
        store
            .unwind_background_run(&run.id, "launch refused")
            .is_err()
    );
    assert_eq!(store.background_run(&run.id).unwrap().unwrap(), run);
    assert_eq!(
        store.crew_task("sha256:refused").unwrap().unwrap().state,
        "briefed"
    );
    store
        .writer()
        .execute_batch("DROP TRIGGER refuse_review_audit")
        .unwrap();
    store
        .unwind_background_run(&run.id, "launch refused")
        .unwrap();
    let ended = store.background_run(&run.id).unwrap().unwrap();
    assert_eq!(ended.state, "needs_review");
    assert_eq!(ended.review_reason.as_deref(), Some("launch refused"));
    assert_eq!(
        store.crew_task("sha256:refused").unwrap().unwrap().state,
        "needs_person"
    );
    assert_eq!(
        store.crew_task("sha256:other").unwrap().unwrap().state,
        "briefed"
    );
    store.unwind_background_run(&run.id, "retry").unwrap();
    assert_eq!(store.background_run(&run.id).unwrap().unwrap(), ended);
    assert!(store.start_background_run(&crew.id).is_ok());
}
#[test]
fn the_table_keeps_the_daily_budget_and_the_persons_clock_within_their_bounds() {
    let dir = TempDir::new();
    let store = store(&dir);
    // A crew made by the core is in bounds; the hand-made row past the widest real offset is
    // refused by the table, exactly as the contract checks it.
    let settings = json!({});
    let crew = store
        .create_background_crew(&new_crew("Clock", &settings), "user")
        .unwrap();
    assert_eq!(crew.utc_offset_minutes, 600);
    for offset in [841, -841] {
        let result = store.writer().execute(
            "INSERT INTO crew_background (id, project_id, name, template, trigger_kind, trigger_settings,
             budget_turns, budget_minutes, utc_offset_minutes, created_at, updated_at)
             VALUES ('clock', 'p', 'n', 'flaky_test', 'schedule', '{}', 5, 5, ?1, 'a', 'a')",
            [offset],
        );
        assert!(result.is_err(), "utc_offset_minutes {offset}");
    }
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
