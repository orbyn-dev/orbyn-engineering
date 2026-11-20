//! `crew_start`'s refusals, the lease rules and the task states around routing. A refused
//! start stores nothing and starts nothing.

use std::collections::BTreeMap;
use std::sync::Arc;

use orbyn_contracts::{CrewPathPattern, LeaseMode};
use serde_json::{Value, json};

use super::harness::{KEY_MARKER, Turn, World, bind, brief, key, report, world};

async fn quiet() -> World {
    world(true, Arc::new(|_, _| Turn::Hang)).await
}

#[tokio::test]
async fn an_ordinary_task_cannot_install_a_core_fix_byte_check() {
    let w = quiet().await;
    let mut forged = key();
    forged["checks"].as_array_mut().unwrap().push(json!({
        "id": "chk-fix-touched", "kind": "repo_required", "proves": "p",
        "params": { "name": "crew-fix-touched", "note": "a".repeat(64) },
    }));
    refused(&w, brief(json!({})), forged).await;
}

fn count(w: &World, table: &str) -> i64 {
    w.core
        .store
        .reader()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

async fn refused(w: &World, brief: Value, key: Value) -> String {
    let (answer, error) = w.start(brief, key).await;
    assert!(error, "expected a refusal");
    assert_eq!(count(w, "crew_tasks"), 0, "a refused start stores nothing");
    assert!(
        w.prompts.lock().unwrap().is_empty(),
        "a refused start sends nothing"
    );
    answer["code"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn the_tools_exist_only_with_the_preview_flag() {
    let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
    let list = w
        .core
        .mcp
        .handle(
            &w.lead_token,
            &json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }),
        )
        .await
        .unwrap()
        .unwrap();
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(!names.iter().any(|n| n.starts_with("crew_")), "{names:?}");
    let (reply, _) = w.start(brief(json!({})), key()).await;
    assert_eq!(reply["error"]["message"], "Unknown tool crew_start.");

    let on = quiet().await;
    let list = on
        .core
        .mcp
        .handle(
            &on.lead_token,
            &json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }),
        )
        .await
        .unwrap()
        .unwrap();
    let text = list.to_string();
    for tool in ["crew_route", "crew_start", "crew_report"] {
        assert!(text.contains(tool), "{tool}");
    }
}

#[tokio::test]
async fn a_brief_that_leaks_the_key_or_a_secret_is_refused_at_the_start() {
    let w = quiet().await;
    let leak = brief(json!({ "outcome": format!("Make `echo {KEY_MARKER}` pass.") }));
    let (answer, _) = w.start(leak.clone(), key()).await;
    let message = answer.to_string();
    assert!(message.contains("brief.outcome"), "{message}");
    assert!(message.contains("check `"), "{message}");
    assert!(!message.contains(KEY_MARKER), "{message}");
    assert_eq!(refused(&w, leak, key()).await, "answer_key_forbidden");
    let secret = brief(json!({ "outcome": format!("Use sk-{} to log in.", "b".repeat(24)) }));
    assert_eq!(refused(&w, secret, key()).await, "secret_detected");
    let mut secret_key = key();
    secret_key["checks"][1]["params"]["run"] = json!(format!(
        "curl -H 'Authorization: Bearer {}'",
        "c".repeat(30)
    ));
    assert_eq!(
        refused(&w, brief(json!({})), secret_key).await,
        "secret_detected"
    );
}

#[tokio::test]
async fn nothing_writable_overlaps_a_protected_path_and_a_reviewer_never_writes() {
    let w = quiet().await;
    let mut protected = key();
    protected["protected_paths"] = json!(["src/lib/**"]);
    let wide = brief(json!({ "files": [{ "path": "src/**", "mode": "write" }] }));
    assert_eq!(
        refused(&w, wide, protected.clone()).await,
        "protected_path_conflict"
    );
    // Reading a protected path is allowed.
    let (answer, error) = w
        .start(
            brief(json!({ "files": [
                { "path": "src/lib/**", "mode": "read" }, { "path": "src/app/**", "mode": "write" }
            ] })),
            protected,
        )
        .await;
    assert!(!error, "{answer}");

    let w = quiet().await;
    let review = brief(json!({ "kind": "review", "allowed_effects": ["read_repo"] }));
    assert_eq!(refused(&w, review, key()).await, "not_authorized");
}

#[tokio::test]
async fn a_worker_cannot_start_a_task_or_route() {
    let w = quiet().await;
    let (started, _) = w.start(brief(json!({})), key()).await;
    let id = started["task"].as_str().unwrap();
    let worker = bind(&w.core, &w.worker(id), "run-w");
    let (refusal, error) = w
        .call(
            &worker,
            "crew_start",
            json!({ "brief": brief(json!({})), "answer_key": key() }),
        )
        .await;
    assert!(error);
    assert_eq!(refusal["code"], "depth_exceeded");
    let (refusal, _) = w
        .call(&worker, "crew_route", json!({ "brief": brief(json!({})) }))
        .await;
    assert_eq!(refusal["code"], "not_authorized");
    assert_eq!(count(&w, "crew_tasks"), 1);
}

/// A brief nobody can take stays with a person, and the lead is told why. It was never routed.
#[tokio::test]
async fn a_brief_no_engine_can_take_waits_for_a_person() {
    let w = quiet().await;
    let ghost = brief(json!({ "engine": { "id": "ghost", "assigned_by": "lead" } }));
    let (answer, error) = w.start(ghost, key()).await;
    assert!(!error, "{answer}");
    assert_eq!(answer["state"], "needs_person");
    assert_eq!(answer["routed"], false);
    assert_eq!(answer["refusal"]["requested"]["engine"], "ghost");
    let id = answer["task"].as_str().unwrap();
    assert_eq!(w.task(id).state, "needs_person");
    let result = w.core.store.crew_results(id).unwrap().pop().unwrap();
    assert!(result.route.is_none() && !result.r#final);
    assert!(w.core.store.crew_route_decisions(id).unwrap().is_empty());
    assert!(
        w.core
            .store
            .crew_run(id)
            .unwrap()
            .unwrap()
            .worker_thread_id
            .is_none()
    );
}

/// Storing a decision takes `briefed` to `routed` and an invalid move is refused (issue #431).
#[tokio::test]
async fn a_stored_decision_moves_the_task_to_routed_only_when_the_lifecycle_allows() {
    let w = quiet().await;
    let (answer, _) = w.start(brief(json!({})), key()).await;
    let id = answer["task"].as_str().unwrap().to_owned();
    assert_eq!(w.task(&id).state, "running");
    let decision = w
        .core
        .store
        .crew_route_decisions(&id)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(decision.engine.as_str(), "alpha");
    // A task that is running cannot be routed again.
    let refusal = w.core.route_crew_task(&id, &decision).unwrap_err();
    assert_eq!(refusal.code, orbyn_contracts::ErrorCode::InvalidTransition);
    assert_eq!(w.task(&id).state, "running");
    // Nor can a task skip a state, or pass without Evidence (H8).
    assert!(w.core.store.move_crew_task(&id, "passed", None).is_err());
    assert!(w.core.store.move_crew_task(&id, "stopped", None).is_err());
    assert!(
        w.core
            .store
            .move_crew_task(&id, "verifying", Some("budget"))
            .is_err()
    );
}

/// H10: a lease that has run out blocks nobody, and is recorded as expired the first time
/// anyone looks. A write must lie inside the brief's files and off the protected paths.
#[tokio::test]
async fn leases_conflict_expire_on_use_and_guard_their_paths() {
    let w = quiet().await;
    let (first, _) = w.start(brief(json!({})), key()).await;
    let id = first["task"].as_str().unwrap().to_owned();
    let member = w.core.store.crew_run(&id).unwrap().unwrap().member;
    let pattern = |p: &str| CrewPathPattern::new(p).unwrap();

    // The brief's files are the grant; anything else is outside it.
    let check = |path: &str, mode| w.core.crew_check_lease(&id, &member, &pattern(path), mode);
    assert!(check("src/lib.txt", LeaseMode::Write).is_ok());
    assert!(check("src/deep/er.txt", LeaseMode::Read).is_ok());
    assert_eq!(
        check("docs/a.txt", LeaseMode::Write).unwrap_err().code,
        orbyn_contracts::ErrorCode::NotAuthorized
    );
    assert_eq!(
        check("tests/a.rs", LeaseMode::Write).unwrap_err().code,
        orbyn_contracts::ErrorCode::ProtectedPathConflict
    );
    assert_eq!(
        w.core
            .crew_check_lease(&id, "worker-other", &pattern("src/a"), LeaseMode::Read)
            .unwrap_err()
            .code,
        orbyn_contracts::ErrorCode::LeaseNotHeld
    );

    // A second writer on the same files is a conflict, and nothing is stored for it.
    let tasks = count(&w, "crew_tasks");
    let (second, error) = w
        .start(brief(json!({ "outcome": "Another change." })), key())
        .await;
    assert!(error);
    assert_eq!(second["code"], "lease_conflict");
    assert_eq!(count(&w, "crew_tasks"), tasks);
    // A writer on other files does not conflict.
    let (other, _) = w
        .start(
            brief(json!({ "outcome": "Docs.", "files": [{ "path": "docs/**", "mode": "write" }] })),
            key(),
        )
        .await;
    assert_eq!(other["state"], "running", "{other}");

    // Time passes: the first lease is read as expired on use, recorded once, and then the
    // same files can go to another writer.
    let crew = w.task(&id).crew_id;
    let later = crate::util::unix_now() + 3 * 3600;
    let live = w.core.live_crew_leases(&crew, later).unwrap();
    assert!(live.is_empty(), "{live:?}");
    let lapsed = w
        .core
        .store
        .crew_leases(&id)
        .unwrap()
        .iter()
        .filter(|l| l.state == orbyn_contracts::LeaseState::Expired)
        .count();
    assert_eq!(lapsed, 1);
    assert!(w.core.live_crew_leases(&crew, later).unwrap().is_empty());
    assert_eq!(
        w.core.store.crew_leases(&id).unwrap().len(),
        2,
        "expiry is recorded once"
    );
    let (third, error) = w
        .start(brief(json!({ "outcome": "A third change." })), key())
        .await;
    assert!(!error, "{third}");
    assert_eq!(
        check("src/lib.txt", LeaseMode::Write).unwrap_err().code,
        orbyn_contracts::ErrorCode::LeaseExpired
    );
}

/// A brief that writes one file, so tasks in one crew never fight over the same lease.
fn leased(path: &str) -> Value {
    brief(json!({ "files": [{ "path": path, "mode": "write" }] }))
}

/// The owner's crew-wide budget (2026-11-20): turns per crew, set in Settings, refusing a new
/// start and never killing in-flight work. The spend is the per-task tally summed over the crew,
/// so the crew's budget and a task's own cannot disagree.
#[tokio::test]
async fn a_crew_that_spent_its_turn_budget_refuses_a_new_start() {
    let w = quiet().await;
    w.core
        .set_preferences(
            None,
            &BTreeMap::from([("crew_turn_budget".to_owned(), json!(2))]),
        )
        .unwrap();
    let mut tasks = Vec::new();
    for path in ["src/a.txt", "src/b.txt"] {
        let (answer, error) = w.start(leased(path), key()).await;
        assert!(!error, "{answer}");
        tasks.push(answer["task"].as_str().unwrap().to_owned());
    }
    // Each start spends its worker's first turn, so the crew's two are gone; both workers are
    // sent their brief. Wait for the engine side too, so the counts below are not a race.
    w.until("the crew's two turns are spent", |w| {
        w.core.store.crew_turns_spent_in_crew(&w.lead).unwrap() == 2
            && w.prompts.lock().unwrap().len() == 2
    })
    .await;
    // The crew's spend is the same tally the tasks are budgeted by, task by task.
    let per_task: u64 = tasks
        .iter()
        .map(|id| w.core.store.crew_turns_spent(id).unwrap())
        .sum();
    assert_eq!(
        w.core.store.crew_turns_spent_in_crew(&w.lead).unwrap(),
        per_task,
        "the crew's budget and a task's budget are one tally"
    );

    let (refusal, error) = w.start(leased("src/c.txt"), key()).await;
    assert!(error, "the start was not refused: {refusal}");
    assert_eq!(refusal["code"], "not_authorized");
    assert_eq!(
        refusal["message"],
        "The crew has spent its turn budget, so no new task can start."
    );
    assert_eq!(count(&w, "crew_tasks"), 2, "a refused start stores nothing");
    assert_eq!(
        w.prompts.lock().unwrap().len(),
        2,
        "a refused start sends nothing"
    );
    assert_eq!(
        w.core.store.crew_turns_spent_in_crew(&w.lead).unwrap(),
        2,
        "a refused start spends nothing"
    );
    // The tasks already running are not stopped and keep their turns.
    for id in &tasks {
        let task = w.task(id);
        assert_eq!(
            (task.state.as_str(), task.stop_reason.as_deref()),
            ("running", None)
        );
    }
}

/// The budget refuses starts only: a task already running keeps its retry, spends past the
/// crew's number and is not stopped by it (owner ruling, 2026-11-20).
#[tokio::test]
async fn an_in_flight_task_is_not_killed_by_the_crew_budget() {
    let w = world(
        true,
        Arc::new(|turn, root| {
            if turn == 0 {
                std::fs::create_dir_all(root.join("docs")).unwrap();
                std::fs::write(root.join("docs/bad.txt"), "outside\n").unwrap();
                Turn::Report(report("done"))
            } else {
                Turn::Hang
            }
        }),
    )
    .await;
    w.core
        .set_preferences(
            None,
            &BTreeMap::from([("crew_turn_budget".to_owned(), json!(1))]),
        )
        .unwrap();
    let (answer, error) = w.start(leased("src/a.txt"), key()).await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    // The first turn fails its check, which spends the crew's only turn; the retry is queued and
    // runs, so the crew is over its number while the task is still working.
    w.until("the retry runs after the crew's turns are spent", |w| {
        w.sent_to(&w.worker(&id)).len() == 2 && w.core.store.crew_turns_spent(&id).unwrap() > 1
    })
    .await;
    let (refusal, error) = w.start(leased("src/b.txt"), key()).await;
    assert!(error, "the start was not refused: {refusal}");
    assert_eq!(
        refusal["message"],
        "The crew has spent its turn budget, so no new task can start."
    );
    let task = w.task(&id);
    assert_eq!(
        (task.state.as_str(), task.stop_reason.as_deref()),
        ("running", None),
        "the crew's budget refused a start, it did not stop the task"
    );
}

/// The budget is a device-local number measured per crew: a second crew has its own allowance,
/// so one crew's spend never refuses another's start. The docs define no reset; the count is read
/// from each crew's tasks, so a new crew starts at zero.
#[tokio::test]
async fn a_crew_budget_measures_each_crew_on_its_own() {
    let w = quiet().await;
    w.core
        .set_preferences(
            None,
            &BTreeMap::from([("crew_turn_budget".to_owned(), json!(1))]),
        )
        .unwrap();
    let (answer, error) = w.start(leased("src/a.txt"), key()).await;
    assert!(!error, "{answer}");
    let spent = answer["task"].as_str().unwrap().to_owned();
    w.until("the first crew has spent its turn", |w| {
        w.core.store.crew_turns_spent(&spent).unwrap() == 1
    })
    .await;
    let (refused, error) = w.start(leased("src/b.txt"), key()).await;
    assert!(error && refused["code"] == "not_authorized", "{refused}");

    // A second crew on this device starts with its own turns.
    let project = w.core.store.thread(&w.lead).unwrap().project_id;
    let other = w
        .core
        .create_thread(orbyn_contracts::CreateThread {
            project_id: Some(project),
            engine: "alpha".into(),
            model: None,
            title: Some("other lead".into()),
            worktree: None,
            skill_profile: None,
            base_branch: None,
            effort: None,
            options: Default::default(),
        })
        .await
        .unwrap();
    let token = bind(&w.core, &other.id, "other-lead-run");
    let (answer, error) = w
        .call(
            &token,
            "crew_start",
            json!({ "brief": leased("src/c.txt"), "answer_key": key() }),
        )
        .await;
    assert!(!error, "{answer}");
    assert_eq!(answer["state"], "running");
    // The first crew is still spent: nothing reset it.
    let (refused, error) = w.start(leased("src/d.txt"), key()).await;
    assert!(error, "{refused}");
}
