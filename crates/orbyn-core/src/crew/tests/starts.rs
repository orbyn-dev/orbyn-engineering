//! `crew_start`'s refusals, the lease rules and the task states around routing. A refused
//! start stores nothing and starts nothing.

use std::collections::BTreeMap;
use std::sync::Arc;

use orbyn_contracts::{CheckKind, CheckOutcome, CrewObject, CrewPathPattern, LeaseMode};
use serde_json::{Value, json};

use super::harness::{KEY_MARKER, Turn, World, bind, brief, key, report, world};

async fn quiet() -> World {
    world(true, Arc::new(|_, _| Turn::Hang)).await
}

#[tokio::test]
async fn an_ordinary_launch_keeps_its_persisted_project_and_checkout_binding() {
    use crate::crew::stamp::Stamper;
    use crate::crew::start::{Launch, Links};
    use crate::crew::tool::stamp_now;
    use orbyn_contracts::{CrewId, CrewMember, CrewToken};

    let w = quiet().await;
    let lead = w.core.store.thread(&w.lead).unwrap();
    let alternate = w._dir.path().join("alternate-checkout");
    crate::testing::run_git(
        &w.repo,
        &[
            "worktree",
            "add",
            "--detach",
            &alternate.to_string_lossy(),
            "HEAD",
        ],
    );
    let mut wrong_checkout = lead.clone();
    wrong_checkout.cwd = alternate.to_string_lossy().into_owned();
    let other = w
        .core
        .new_project("other-project", false, None)
        .await
        .unwrap();
    let mut wrong_project = lead.clone();
    wrong_project.project_id = other.id;

    for incoming in [wrong_checkout, wrong_project] {
        let stamper = Stamper {
            crew_id: CrewId::new(lead.id.clone()).unwrap(),
            engine: CrewToken::new("alpha").unwrap(),
            member: CrewMember::new("lead-01").unwrap(),
            now: stamp_now().unwrap(),
        };
        let brief = stamper.brief(&brief(json!({}))).unwrap();
        let key = stamper.answer_key(&key(), &brief.id).unwrap();
        let error = w
            .core
            .launch_crew_task(Launch {
                lead: &incoming,
                now: stamper.now,
                brief,
                key,
                role: None,
                lane: None,
                lane_keeps_lead: false,
                author_engine: None,
                prefix: "worker",
                prompt: None,
                links: Links::default(),
            })
            .await
            .unwrap_err();
        assert!(error.message.contains("binding changed"), "{error:?}");
        assert_eq!(count(&w, "crew_tasks"), 0);
        assert!(w.prompts.lock().unwrap().is_empty());
        assert!(w.crew_tasks.lock().unwrap().is_empty());
    }

    let (answer, error) = w.start(brief(json!({})), key()).await;
    assert!(!error && answer["routed"] == true, "{answer}");
    let task = answer["task"].as_str().unwrap();
    let worker = w.core.store.crew_run(task).unwrap().unwrap();
    assert_eq!(worker.worktree.as_deref(), Some(lead.cwd.as_str()));
    w.core.shutdown().await;
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

/// The one row a read returned, unpacked by pattern so a test never reads a place in the list. A
/// read that returns none or several fails here with the rows attached.
fn one<'a, T: std::fmt::Debug>(rows: &'a [T], what: &str) -> &'a T {
    match rows {
        [row] => row,
        [] => panic!("no {what}: {rows:?}"),
        _ => panic!("{} {what}s where one is expected: {rows:?}", rows.len()),
    }
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
    // The command check is found by its id, so a fixture check inserted before it cannot stand in.
    let mut secret_key = key();
    let checks = secret_key["checks"]
        .as_array_mut()
        .expect("the checks list");
    let command = checks
        .iter_mut()
        .find(|check| check["id"] == "chk-run")
        .expect("the command check");
    command["params"]["run"] = json!(format!(
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
    let mut results = w.core.store.crew_results(id).unwrap();
    assert_eq!(results.len(), 1, "the brief's one result: {results:?}");
    let result = results.pop().expect("the brief's result");
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
    let mut decisions = w.core.store.crew_route_decisions(&id).unwrap();
    assert_eq!(
        decisions.len(),
        1,
        "the start stored one route decision: {decisions:?}"
    );
    let decision = decisions.pop().expect("the route decision");
    assert_eq!(decision.engine.as_str(), "alpha");
    // A task that is running cannot be routed again.
    let refusal = w.core.route_crew_task(&id, &decision).unwrap_err();
    assert_eq!(refusal.code, orbyn_contracts::ErrorCode::InvalidTransition);
    assert_eq!(w.task(&id).state, "running");
    // Nor can a task skip a state, or pass without Evidence (H8).
    let skipped = w
        .core
        .store
        .move_crew_task(&id, "passed", None)
        .unwrap_err();
    assert!(
        matches!(&skipped, crate::error::CoreError::InvalidTransition { from, to } if (from.as_str(), to.as_str()) == ("running", "passed")),
        "{skipped:?}"
    );
    let reasonless = w
        .core
        .store
        .move_crew_task(&id, "stopped", None)
        .unwrap_err();
    assert!(
        matches!(&reasonless, crate::error::CoreError::Invalid(why) if why == "a stopped task names its reason, and no other state has one"),
        "{reasonless:?}"
    );
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

/// The owner's crew-wide budget (2026-11-20): turns per crew, set through the settings API, refusing a new
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

/// The crew budget fails closed: a budget that no read can settle refuses the start instead of
/// reading as "no budget", so a crew whose Settings cannot be read starts nothing rather than
/// spending turns nobody can count. The refusal names the failed read, as the background crews'
/// daily budget does (`refuse_background_daily`).
#[tokio::test]
async fn a_crew_turn_budget_that_cannot_be_read_refuses_a_new_start() {
    let w = quiet().await;
    w.core
        .set_preferences(
            None,
            &BTreeMap::from([("crew_turn_budget".to_owned(), json!(5))]),
        )
        .unwrap();
    // The read of the stored settings is what fails, as under a corrupt settings row or a locked
    // database; the budget itself is still on disk and unread.
    w.core
        .store
        .reader()
        .execute_batch("DROP TABLE settings")
        .unwrap();
    let (refusal, error) = w.start(leased("src/a.txt"), key()).await;
    assert!(error, "the start was not refused: {refusal}");
    assert_eq!(refusal["code"], "not_authorized");
    assert_eq!(
        refusal["message"],
        "The crew's turn budget could not be read, so no new task can start. Try again."
    );
    assert_eq!(count(&w, "crew_tasks"), 0, "a refused start stores nothing");
    assert!(
        w.prompts.lock().unwrap().is_empty(),
        "a refused start sends nothing"
    );
}

/// The budget record itself holds only whole turns, so a value the setting cannot hold is a count
/// nobody can settle: the start is refused rather than read as an absent budget. Nothing is
/// stored or sent, and the refusal is the same one an unreadable settings row gives.
#[tokio::test]
async fn a_crew_turn_budget_that_is_not_a_whole_number_refuses_a_new_start() {
    let w = quiet().await;
    w.core
        .set_preferences(
            None,
            &BTreeMap::from([("crew_turn_budget".to_owned(), json!(5))]),
        )
        .unwrap();
    // What an older or damaged row leaves behind: the setting only ever writes a whole number.
    w.core
        .store
        .reader()
        .execute_batch("UPDATE settings SET value = '\"a few\"' WHERE key LIKE '%crew_turn_budget'")
        .unwrap();
    let (refusal, error) = w.start(leased("src/a.txt"), key()).await;
    assert!(error, "the start was not refused: {refusal}");
    assert_eq!(refusal["code"], "not_authorized");
    assert_eq!(
        refusal["message"],
        "The crew's turn budget could not be read, so no new task can start. Try again."
    );
    assert_eq!(count(&w, "crew_tasks"), 0, "a refused start stores nothing");
    assert!(
        w.prompts.lock().unwrap().is_empty(),
        "a refused start sends nothing"
    );
}

/// A crew whose spend cannot be read fails closed too: "not under budget" is not something a
/// failed count can establish, so the start is refused. The crew's own tasks stay untouched.
#[tokio::test]
async fn a_crew_whose_turns_cannot_be_read_refuses_a_new_start() {
    let w = quiet().await;
    w.core
        .set_preferences(
            None,
            &BTreeMap::from([("crew_turn_budget".to_owned(), json!(5))]),
        )
        .unwrap();
    // The per-task tally reads the approvals it reserved, but only for a run that exists: a crew with no run yet is
    // never asked for them. So one task runs first, and then losing that table loses the count.
    let (answer, error) = w.start(leased("src/first.txt"), key()).await;
    assert!(!error, "{answer}");
    w.until("the first task's turn is spent", |w| {
        w.core.store.crew_turns_spent_in_crew(&w.lead).unwrap() == 1
    })
    .await;
    w.core
        .store
        .reader()
        .execute_batch("DROP TABLE crew_autopilot_events")
        .unwrap();
    let prompts_before = w.prompts.lock().unwrap().len();
    let (refusal, error) = w.start(leased("src/a.txt"), key()).await;
    assert!(error, "the start was not refused: {refusal}");
    assert_eq!(refusal["code"], "not_authorized", "{refusal}");
    assert_eq!(
        refusal["message"],
        "The crew's turns could not be read, so no new task can start. Try again."
    );
    assert_eq!(
        count(&w, "crew_tasks"),
        1,
        "a refused start stores nothing beside the first task"
    );
    assert_eq!(
        w.prompts.lock().unwrap().len(),
        prompts_before,
        "a refused start sends nothing"
    );
}

/// The boundary the budget is read against is "at least the limit", never "over it": a crew one
/// turn under its budget still starts a task, and the start that spends the last turn is the one
/// after which starts are refused.
#[tokio::test]
async fn one_turn_under_the_crew_turn_budget_still_starts_a_task() {
    let w = quiet().await;
    w.core
        .set_preferences(
            None,
            &BTreeMap::from([("crew_turn_budget".to_owned(), json!(3))]),
        )
        .unwrap();
    for path in ["src/a.txt", "src/b.txt"] {
        let (answer, error) = w.start(leased(path), key()).await;
        assert!(!error, "{answer}");
    }
    w.until("the crew's two turns are spent", |w| {
        w.core.store.crew_turns_spent_in_crew(&w.lead).unwrap() == 2
            && w.prompts.lock().unwrap().len() == 2
    })
    .await;
    // Two of the three turns are gone: the crew is one under, so this start runs.
    let (answer, error) = w.start(leased("src/c.txt"), key()).await;
    assert!(
        !error,
        "a start one turn under the budget was refused: {answer}"
    );
    w.until("the crew's third turn is spent", |w| {
        w.core.store.crew_turns_spent_in_crew(&w.lead).unwrap() == 3
    })
    .await;
    let (refusal, error) = w.start(leased("src/d.txt"), key()).await;
    assert!(error, "the start at the limit was not refused: {refusal}");
    assert_eq!(
        refusal["message"],
        "The crew has spent its turn budget, so no new task can start."
    );
    assert_eq!(count(&w, "crew_tasks"), 3, "a refused start stores nothing");
}

/// No budget recorded behaves as documented: a crew made before the budget existed (unset) and a
/// budget of 0 both start tasks without limit, while the turns are still counted for the person.
#[tokio::test]
async fn a_crew_with_no_turn_budget_recorded_starts_every_task() {
    let w = quiet().await;
    for (budget, paths) in [
        (None, ["src/a.txt", "src/b.txt"]),
        (Some(json!(0)), ["src/c.txt", "src/d.txt"]),
    ] {
        if let Some(value) = budget {
            w.core
                .set_preferences(
                    None,
                    &BTreeMap::from([("crew_turn_budget".to_owned(), value)]),
                )
                .unwrap();
        }
        for path in paths {
            let (answer, error) = w.start(leased(path), key()).await;
            assert!(
                !error,
                "a crew with no turn budget refused a start: {answer}"
            );
        }
    }
    w.until("the crew's four turns are counted", |w| {
        w.core.store.crew_turns_spent_in_crew(&w.lead).unwrap() == 4
    })
    .await;
    assert_eq!(
        count(&w, "crew_tasks"),
        4,
        "a crew with no budget starts every task"
    );
}

/// The disk guard (KC4): a start is refused, with the free space in the reason, when the lead's volume has less than the
/// person's floor, and nothing is started. The default floor is 5 GB; faked free space lets this run on any machine.
#[tokio::test]
async fn a_start_under_the_disk_floor_is_refused_with_the_free_space() {
    const GIB: u64 = 1024 * 1024 * 1024;
    let w = quiet().await;
    let cwd = w.core.store.thread(&w.lead).unwrap().cwd;
    let path = std::path::Path::new(&cwd);
    crate::host::disk::set_free_for_test(path, Some(7 * GIB / 2));
    let (refused, error) = w.start(leased("src/a.txt"), key()).await;
    assert!(error, "the start was not refused: {refused}");
    assert_eq!(refused["code"], "disk_low");
    assert_eq!(refused["free_bytes"].as_u64(), Some(7 * GIB / 2));
    assert_eq!(refused["floor_gb"].as_u64(), Some(5));
    assert_eq!(
        refused["message"],
        "Not started: 3.5 GB free, Crew needs 5 GB. Free space and try again."
    );
    assert!(w.prompts.lock().unwrap().is_empty(), "nothing was started");

    crate::host::disk::set_free_for_test(path, Some(640 * 1024 * 1024));
    let (refused, error) = w.start(leased("src/a.txt"), key()).await;
    assert!(error, "the start was not refused: {refused}");
    assert_eq!(refused["code"], "disk_low");
    assert_eq!(refused["free_bytes"].as_u64(), Some(640 * 1024 * 1024));
    assert_eq!(refused["floor_gb"].as_u64(), Some(5));
    let message = refused["message"].as_str().unwrap();
    assert!(message.contains("640 MB free"), "{message}");
    assert!(w.prompts.lock().unwrap().is_empty(), "nothing was started");

    crate::host::disk::set_free_for_test(path, Some(20 * GIB));
    let (answer, error) = w.start(leased("src/a.txt"), key()).await;
    assert!(!error, "{answer}");
    crate::host::disk::set_free_for_test(path, None);
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
            branch_name_seed: None,
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

/// A task whose first prompt is still queued (no `runs` row yet) already counts a turn, so a lead
/// cannot be granted unlimited starts while the queue holds them back.
#[tokio::test]
async fn a_task_whose_first_prompt_is_queued_counts_a_turn() {
    let w = quiet().await;
    w.core
        .set_preferences(
            None,
            &BTreeMap::from([("crew_turn_budget".to_owned(), json!(1))]),
        )
        .unwrap();
    let (answer, error) = w.start(leased("src/a.txt"), key()).await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    let thread = w.worker(&id);
    w.until("the worker was sent its brief", |w| {
        !w.sent_to(&thread).is_empty()
    })
    .await;
    // Put the worker back to where a queued first prompt leaves it: no run, one queued message.
    {
        let conn = w.core.store.reader();
        conn.execute("DELETE FROM runs WHERE thread_id = ?1", [&thread])
            .unwrap();
        conn.execute(
            "INSERT INTO queued_messages (id, thread_id, prompt, attachment_ids, created_at)
             VALUES ('queued-1', ?1, 'brief', '[]', '2026-11-21T00:00:00Z')",
            [&thread],
        )
        .unwrap();
    }
    assert_eq!(w.core.store.crew_turns_spent(&id).unwrap(), 1);
    assert_eq!(w.core.store.crew_turns_spent_in_crew(&w.lead).unwrap(), 1);
    let (refusal, error) = w.start(leased("src/b.txt"), key()).await;
    assert!(error, "a queued start was counted as free: {refusal}");
    assert_eq!(refusal["code"], "not_authorized");
    assert_eq!(count(&w, "crew_tasks"), 1);
}

/// Writes the repository's rules file and commits it, so it is part of the commit a task starts from.
fn crew_json(w: &World, text: &str) {
    std::fs::create_dir_all(w.repo.join(".orbyn")).unwrap();
    std::fs::write(w.repo.join(".orbyn/crew.json"), text).unwrap();
    crate::testing::run_git(&w.repo, &["add", "-f", ".orbyn/crew.json"]);
    super::review_support::commit_all(&w.repo, "repository checks");
}

/// The key a task was started with, as the store holds it.
fn stored_key(w: &World, task: &str) -> orbyn_contracts::AnswerKey {
    w.core.store.crew_answer_key(task).unwrap().unwrap()
}

/// Issue #795: every task that writes inherits the repository's own checks at the commit it starts from, and
/// the task runs them, so a lead does not have to declare one and a worker cannot lower the bar.
#[tokio::test]
async fn a_task_that_writes_inherits_the_repositorys_checks_at_its_start() {
    let w = world(
        true,
        Arc::new(|_, root| {
            std::fs::write(root.join("src/lib.txt"), "fn parse_duration() {}\n").unwrap();
            Turn::Report(report("done"))
        }),
    )
    .await;
    crew_json(&w, r#"{"checks":[{"run":"echo repository-check"}]}"#);
    let (answer, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    let stored = stored_key(&w, &id);
    let inherited: Vec<_> = stored
        .checks
        .iter()
        .filter(|check| matches!(check.kind, CheckKind::RepoRequired))
        .collect();
    assert_eq!(inherited.len(), 1, "{:?}", stored.checks);
    let inherited = one(&inherited, "inherited repository check");
    assert_eq!(inherited.id.as_str(), "chk-repo-required");
    assert_eq!(
        inherited.params["name"], "inherited",
        "the inherited check is not marked as inherited"
    );
    // Nothing the lead declared was dropped, and the key's id is the hash of what now runs.
    assert_eq!(stored.checks.len(), 4, "{:?}", stored.checks);
    assert_eq!(
        stored.id,
        CrewObject::AnswerKey(stored.clone()).computed_id().unwrap()
    );
    // The inherited check runs in the task and records its Evidence.
    w.until_state(&id, "passed").await;
    let evidence = w.core.store.crew_evidence(&id).unwrap();
    let ran = evidence
        .iter()
        .find(|one| one.check.as_str() == "chk-repo-required")
        .unwrap_or_else(|| panic!("the inherited check did not run: {evidence:?}"));
    assert_eq!(ran.outcome, CheckOutcome::Pass);
    assert_eq!(ran.command.as_str(), "repo_required: chk-repo-required");
    assert!(
        ran.excerpt.contains("echo repository-check"),
        "the log does not show the repository's own check: {}",
        ran.excerpt
    );
    w.core.shutdown().await;
}

/// Issue #795: a repository that declares no check, or no rules file at all, leaves the key as the lead sent it.
#[tokio::test]
async fn a_repository_that_declares_no_check_inherits_nothing() {
    for rules in [None, Some(r#"{"checks":[]}"#)] {
        let w = quiet().await;
        if let Some(rules) = rules {
            crew_json(&w, rules);
        }
        let (answer, error) = w.start(brief(json!({})), key()).await;
        assert!(!error, "{answer}");
        let id = answer["task"].as_str().unwrap().to_owned();
        let stored = stored_key(&w, &id);
        assert_eq!(stored.checks.len(), 3, "{rules:?}: {:?}", stored.checks);
        assert!(
            !stored
                .checks
                .iter()
                .any(|check| matches!(check.kind, CheckKind::RepoRequired)),
            "{rules:?}: an empty repository config added a check"
        );
        w.core.shutdown().await;
    }
}

/// Issue #795: a key that already runs the repository's checks keeps its own check rather than gaining a second.
#[tokio::test]
async fn a_key_that_already_runs_the_repositorys_checks_gets_no_second_one() {
    let w = quiet().await;
    crew_json(&w, r#"{"checks":[{"run":"echo repository-check"}]}"#);
    let mut declared = key();
    declared["checks"].as_array_mut().unwrap().push(json!({
        "id": "chk-repo", "kind": "repo_required", "params": {},
        "proves": "The repository's own checks pass.",
    }));
    let (answer, error) = w.start(brief(json!({})), declared).await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    let stored = stored_key(&w, &id);
    let repo_required: Vec<_> = stored
        .checks
        .iter()
        .filter(|check| matches!(check.kind, CheckKind::RepoRequired))
        .collect();
    assert_eq!(repo_required.len(), 1, "{:?}", stored.checks);
    let repo_required = one(&repo_required, "repository check");
    assert_eq!(
        repo_required.id.as_str(),
        "chk-repo",
        "the start replaced the lead's own check"
    );
    w.core.shutdown().await;
}

/// Issue #795: repository rules only add or tighten (docs/crew.md, "Rules"), so a check the brief declares
/// stays beside the inherited one and is never removed to make room for it.
#[tokio::test]
async fn a_briefs_own_check_coexists_with_the_inherited_one() {
    let w = quiet().await;
    crew_json(&w, r#"{"checks":[{"run":"echo repository-check"}]}"#);
    let mut declared = key();
    declared["checks"].as_array_mut().unwrap().push(json!({
        "id": "chk-same-line", "kind": "command",
        "params": { "run": "echo repository-check" },
        "proves": "The same line passes for the brief.",
    }));
    let (answer, error) = w.start(brief(json!({})), declared).await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    let stored = stored_key(&w, &id);
    let ids: Vec<&str> = stored
        .checks
        .iter()
        .map(|check| check.id.as_str())
        .collect();
    assert_eq!(
        ids,
        [
            "chk-scope",
            "chk-run",
            "chk-probe",
            "chk-same-line",
            "chk-repo-required"
        ],
        "{ids:?}"
    );
    w.core.shutdown().await;
}

/// Issue #795: a review writes nothing, so it runs no repository check; the reviewed work's own key carries
/// them. A key that already runs them needs no second one either.
#[tokio::test]
async fn a_review_task_inherits_no_repository_checks() {
    use crate::crew::stamp::Stamper;
    use crate::crew::start::needs_repo_checks;
    use crate::crew::tool::stamp_now;
    use orbyn_contracts::{CrewId, CrewMember, CrewToken};

    let w = quiet().await;
    let lead = w.core.store.thread(&w.lead).unwrap();
    let stamper = Stamper {
        crew_id: CrewId::new(lead.id.clone()).unwrap(),
        engine: CrewToken::new("alpha").unwrap(),
        member: CrewMember::new("lead-01").unwrap(),
        now: stamp_now().unwrap(),
    };
    let writing = stamper.brief(&brief(json!({}))).unwrap();
    let writing_key = stamper.answer_key(&key(), &writing.id).unwrap();
    assert!(needs_repo_checks(&writing, &writing_key));
    let review = stamper
        .brief(&brief(
            json!({ "kind": "review", "files": [], "allowed_effects": ["read_repo"] }),
        ))
        .unwrap();
    let review_key = stamper.answer_key(&key(), &review.id).unwrap();
    assert!(!needs_repo_checks(&review, &review_key));
    let mut declared = key();
    declared["checks"].as_array_mut().unwrap().push(json!({
        "id": "chk-repo", "kind": "repo_required", "params": {},
        "proves": "The repository's own checks pass.",
    }));
    let declared_key = stamper.answer_key(&declared, &writing.id).unwrap();
    assert!(!needs_repo_checks(&writing, &declared_key));
    // A fix key's internal byte comparison has the same kind but runs no repository command: it does not count as coverage.
    let mut fix = key();
    fix["checks"].as_array_mut().unwrap().push(json!({
        "id": "chk-fix-touched", "kind": "repo_required", "params": { "name": "crew-fix-touched" },
        "proves": "The fix touched what it said.",
    }));
    let fix_key = stamper.answer_key(&fix, &writing.id).unwrap();
    assert!(needs_repo_checks(&writing, &fix_key));
    w.core.shutdown().await;
}

/// Issue #795: a rules file that cannot be read refuses the start, rather than starting a task whose bar
/// nobody can read (docs/crew.md: a broken rule file is never an empty rule set).
#[tokio::test]
async fn a_repository_check_file_that_cannot_be_read_refuses_the_start() {
    let w = quiet().await;
    crew_json(&w, "{");
    let (answer, error) = w.start(brief(json!({})), key()).await;
    assert!(error, "{answer}");
    assert_eq!(answer["code"], "invalid_object");
    assert!(
        answer["message"]
            .as_str()
            .unwrap()
            .contains(".orbyn/crew.json"),
        "{answer}"
    );
    assert_eq!(count(&w, "crew_tasks"), 0, "a refused start stores nothing");
    w.core.shutdown().await;
}

/// Issue #795: a check that declares the `when` this version does not read refuses the start, so a condition is
/// never silently dropped and its check never runs unconditionally.
#[tokio::test]
async fn a_repository_that_declares_a_check_condition_refuses_the_start() {
    let w = quiet().await;
    crew_json(
        &w,
        r#"{"checks":[{"run":"echo repository-check","when":{"paths":["src/**"]}}]}"#,
    );
    let (answer, error) = w.start(brief(json!({})), key()).await;
    assert!(error, "{answer}");
    assert!(
        answer["message"]
            .as_str()
            .unwrap()
            .contains("checks[].when"),
        "{answer}"
    );
    assert_eq!(count(&w, "crew_tasks"), 0, "a refused start stores nothing");
    w.core.shutdown().await;
}

/// A created checkout is removed only when the worker's run is confirmed gone: a run still live in
/// it keeps its folder, so a failed start never pulls the files out from under a running worker.
#[tokio::test]
async fn a_checkout_is_only_discarded_once_the_worker_is_confirmed_stopped() {
    let w = quiet().await;
    let (answer, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "{answer}");
    let task = answer["task"].as_str().unwrap().to_owned();
    // The worker's turn hangs, so its run is live and nothing confirms it stopped.
    w.until("the worker's turn to run", |w| {
        !w.core.worker_confirmed_stopped(&task)
    })
    .await;
    // Once its run is gone and its queue is empty, the start may remove the checkout it made.
    let run = w.core.store.crew_run(&task).unwrap().unwrap();
    w.core.stop_crew_worker(Some(&run), "test").await.unwrap();
    w.until("the worker to stop", |w| {
        w.core.worker_confirmed_stopped(&task)
    })
    .await;
    w.core.shutdown().await;
}
