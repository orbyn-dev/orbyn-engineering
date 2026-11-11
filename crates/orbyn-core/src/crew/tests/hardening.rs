//! What a hostile or careless worker must not manage: starting work of its own, editing the
//! checks, changing files while they run, and reading the key. Fake engine, real checks.

use std::sync::Arc;

use serde_json::json;

use super::harness::{Turn, bind, brief, key, report, world};

fn threads(w: &super::harness::World) -> usize {
    w.core.store.list_threads(None, true).unwrap().len()
}

/// H3: a worker is refused every tool that makes a thread or a schedule, or reads another
/// thread, so it cannot make an unmarked thread that would count as a lead.
#[tokio::test]
async fn a_worker_cannot_make_a_lead_of_its_own() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (started, _) = w.start(brief(json!({})), key()).await;
    let id = started["task"].as_str().unwrap().to_owned();
    let token = bind(&w.core, &w.worker(&id), "run-w");
    let before = threads(&w);
    for (tool, args) in [
        ("orbyn_thread_launch", json!({ "prompt": "go" })),
        (
            "orbyn_threads_create",
            json!({ "threads": [{ "prompt": "go" }] }),
        ),
        ("orbyn_delegate_task", json!({ "prompt": "go" })),
        (
            "orbyn_schedule_task",
            json!({ "prompt": "go", "name": "x" }),
        ),
        ("orbyn_thread_read", json!({ "thread_id": w.lead })),
        ("orbyn_thread_search", json!({ "query": "check" })),
        (
            "orbyn_thread_send",
            json!({ "thread_id": w.lead, "prompt": "hi" }),
        ),
    ] {
        let (text, error) = w.raw(&token, tool, args).await;
        assert!(
            error && text.contains("crew worker cannot use"),
            "{tool}: {text}"
        );
    }
    assert_eq!(threads(&w), before, "no thread was made");
    // The lead is not a worker, so its own tools are untouched.
    let (_, error) = w.raw(&w.lead_token, "orbyn_thread_list", json!({})).await;
    assert!(!error);
}

/// N7, H9, H11: a worker that edits a file the checks use gets no checks run, and is not told
/// which file.
#[tokio::test]
async fn a_worker_that_edits_the_checks_runs_none_and_is_not_told_which() {
    let w = world(
        true,
        Arc::new(|n, root| {
            if n == 0 {
                std::fs::create_dir_all(root.join("tests")).unwrap();
                std::fs::write(root.join("tests/hack.txt"), "always pass\n").unwrap();
            }
            Turn::Report(report("done"))
        }),
    )
    .await;
    let only_a_command = json!({
        "checks": [{ "id": "chk-run", "kind": "command", "params": { "run": "exit 0" },
                     "proves": "It exits 0." }],
        "protected_paths": ["tests/**"]
    });
    let (answer, _) = w.start(brief(json!({})), only_a_command).await;
    let id = answer["task"].as_str().unwrap().to_owned();
    w.until_state(&id, "stopped").await;
    assert!(
        w.core.store.crew_evidence(&id).unwrap().is_empty(),
        "no check ran on the worker's tampered checkout"
    );
    let prompts = w.sent_to(&w.worker(&id));
    assert!(prompts[1].contains("may not modify"));
    for prompt in &prompts {
        assert!(
            !prompt.contains("hack.txt") && !prompt.contains("tests/"),
            "a path was named"
        );
    }
    assert_eq!(w.task(&id).stop_reason.as_deref(), Some("repeated_failure"));
}

/// The checks run when the worker's turn has ended, so the worker cannot change files while
/// they run, and what it does after reporting is what gets checked.
#[tokio::test]
async fn the_checks_wait_for_the_workers_turn_to_end() {
    let release = Arc::new(tokio::sync::Notify::new());
    let held = release.clone();
    let w = world(
        true,
        Arc::new(move |n, root| {
            if n == 0 {
                std::fs::write(root.join("src/lib.txt"), "ok\n").unwrap();
                Turn::Linger(report("done"), held.clone())
            } else {
                std::fs::remove_dir_all(root.join("docs")).unwrap();
                Turn::Report(report("fixed"))
            }
        }),
    )
    .await;
    let (answer, _) = w.start(brief(json!({})), key()).await;
    let id = answer["task"].as_str().unwrap().to_owned();
    w.until_state(&id, "verifying").await;
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    assert_eq!(w.task(&id).state, "verifying");
    assert!(
        w.core.store.crew_evidence(&id).unwrap().is_empty(),
        "nothing was checked while the turn was open"
    );
    // The worker goes on working after its report, then its turn ends.
    std::fs::create_dir_all(w.repo.join("docs")).unwrap();
    std::fs::write(w.repo.join("docs/late.txt"), "late\n").unwrap();
    release.notify_one();
    w.until_state(&id, "passed").await;
    // The late file was seen by the first check, which failed, so the worker was told.
    let prompts = w.sent_to(&w.worker(&id));
    assert!(prompts[1].contains("docs/late.txt"), "{}", prompts[1]);
}

/// A task whose time budget ends is stopped even in the middle of a turn.
#[tokio::test]
async fn the_budget_ends_a_task_in_the_middle_of_a_turn() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (answer, _) = w.start(brief(json!({})), key()).await;
    let id = answer["task"].as_str().unwrap().to_owned();
    w.core.crew_budget_ended(&id);
    let task = w.task(&id);
    assert_eq!(
        (task.state.as_str(), task.stop_reason.as_deref()),
        ("stopped", Some("budget"))
    );
    let result = w.core.store.crew_results(&id).unwrap().pop().unwrap();
    assert!(result.r#final);
    // Once it has ended, a late timer changes nothing.
    w.core.crew_budget_ended(&id);
    assert_eq!(w.core.store.crew_results(&id).unwrap().len(), 1);
}

/// H1: a check that prints its own command line fails, and the command, its marker and the key's
/// id appear nowhere the worker can read: its prompts, the tool results, its events, the
/// objects it is shown. (Unix only: the check asks `ps` for its own command line.)
#[cfg(unix)]
#[tokio::test]
async fn a_failing_check_that_prints_its_own_command_leaks_nothing_to_the_worker() {
    const MARKER: &str = "orbyn-leak-91";
    let command = format!("ps -ww -o args= -p $$ ; exit 3 # {MARKER}");
    let w = world(true, Arc::new(|_, _| Turn::Report(report("done")))).await;
    let key = json!({
        "checks": [{ "id": "chk-self", "kind": "command", "params": { "run": command },
                     "proves": "It exits 0." }],
        "protected_paths": ["tests/**"]
    });
    let (answer, error) = w.start(brief(json!({})), key).await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    w.until_state(&id, "stopped").await;

    let key_id = w.core.store.crew_answer_key(&id).unwrap().unwrap().id;
    let worker = w.worker(&id);
    let prompts = w.sent_to(&worker);
    // The check really did print its command, and the worker was told only that it failed.
    assert!(prompts[1].contains("exited with 3") && prompts[1].contains("[check]"));
    let member = w.core.store.crew_run(&id).unwrap().unwrap().member;
    let readable = [
        prompts.join("\n"),
        w.answers.lock().unwrap().join("\n"),
        format!(
            "{:?}",
            w.core.store.events_after(&worker, 0, 10_000).unwrap()
        ),
        json!(w.core.store.crew_worker_objects(&id, &member).unwrap()).to_string(),
    ];
    for (place, text) in ["prompts", "tool results", "events", "stored messages"]
        .iter()
        .zip(&readable)
    {
        assert!(
            !text.contains(MARKER),
            "the marker reached the worker in its {place}"
        );
        assert!(
            !text.contains(&command),
            "the command reached the worker in its {place}"
        );
        assert!(
            !text.contains(key_id.as_str()),
            "the key id reached the worker in its {place}"
        );
    }
}

/// The refusal is an allow list driven from the tool registry, so a tool added later is
/// refused to a worker until someone allows it.
#[tokio::test]
async fn every_orbyn_tool_off_the_allow_list_is_refused_to_a_worker() {
    use crate::crew::workers::{ALLOWED, allowed};
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (started, _) = w.start(brief(json!({})), key()).await;
    let id = started["task"].as_str().unwrap().to_owned();
    let worker = crate::tools::mcp::McpBinding {
        scope: crate::governance::approvals::RunScope {
            run_id: "run-w".into(),
            thread_id: w.worker(&id),
            engine: "alpha".into(),
            family: "alpha".into(),
            write_scope: None,
            mode: orbyn_contracts::PermissionMode::Supervised,
            plan: false,
        },
        permission_prompt: false,
        skills: Vec::new(),
    };
    let lead = crate::tools::mcp::McpBinding {
        scope: crate::governance::approvals::RunScope {
            thread_id: w.lead.clone(),
            ..worker.scope.clone()
        },
        ..worker.clone()
    };
    let names: Vec<String> = crate::tools::mcp_tools::tool_specs()
        .map(|spec| spec["name"].as_str().unwrap().to_owned())
        .collect();
    assert!(names.len() > 40, "the registry is read");
    for name in ALLOWED {
        assert!(
            names.iter().any(|n| n == name),
            "{name} is no longer a tool"
        );
    }
    let mut refused = 0;
    for name in &names {
        // The lead is never limited by this list.
        assert!(
            w.core
                .refuse_for_crew_worker(&lead, name, &json!({}))
                .is_ok(),
            "{name}"
        );
        let result = w.core.refuse_for_crew_worker(
            &worker,
            name,
            &json!({ "thread_id": worker.scope.thread_id }),
        );
        assert_eq!(result.is_ok(), allowed(name), "{name}");
        refused += usize::from(result.is_err());
    }
    assert!(refused > 30, "most tools are refused: {refused}");
    // The tools named in the review are among them.
    for name in [
        "orbyn_pending_requests",
        "orbyn_queue_list",
        "orbyn_thread_configure",
        "orbyn_thread_interrupt",
        "orbyn_task_cancel",
        "orbyn_environment_preferences_update",
        "orbyn_project_update",
        "orbyn_thread_organize",
        "orbyn_thread_update",
    ] {
        assert!(
            w.core
                .refuse_for_crew_worker(&worker, name, &json!({}))
                .is_err(),
            "{name}"
        );
    }
}

/// After the budget timer stops a task, its checks stop too: nothing more is run or written.
#[tokio::test]
async fn a_verification_stops_when_the_task_has_been_stopped() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (answer, _) = w.start(brief(json!({})), key()).await;
    let id = answer["task"].as_str().unwrap().to_owned();
    w.core.crew_budget_ended(&id);
    let run = w.core.store.crew_run(&id).unwrap().unwrap();
    let error = w
        .core
        .crew_verify_live(&w.task(&id), &run)
        .await
        .unwrap_err();
    assert_eq!(error.code, orbyn_contracts::ErrorCode::RunEnded);
    assert!(w.core.store.crew_evidence(&id).unwrap().is_empty());
}

#[test]
fn a_verification_gets_what_is_left_of_the_budget() {
    use crate::crew::verify::budget_left;
    let start = "2026-11-12T09:00:00Z";
    let at = crate::util::parse_time(start).unwrap();
    assert_eq!(budget_left(start, 30, at + 60).as_secs(), 29 * 60);
    assert_eq!(budget_left(start, 30, at + 30 * 60 + 5).as_secs(), 0);
}

/// A restart leaves tasks `running` or `verifying` with nothing driving them. The next start
/// verifies the one that was being checked and stops the one whose worker is gone.
#[tokio::test]
async fn a_restart_picks_up_tasks_left_running_or_verifying() {
    use crate::crew::tests::fake::stamper;
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let worker_thread = |title: &str| {
        let core = w.core.clone();
        let (project, title) = (
            w.core.store.thread(&w.lead).unwrap().project_id,
            title.to_owned(),
        );
        async move {
            core.create_thread(orbyn_contracts::CreateThread {
                project_id: Some(project),
                engine: "alpha".into(),
                model: None,
                title: Some(title),
                worktree: None,
                skill_profile: None,
                base_branch: None,
                effort: None,
                options: Default::default(),
            })
            .await
            .unwrap()
        }
    };
    let mut made = Vec::new();
    for (n, state) in [(1, "verifying"), (2, "running")] {
        let s = stamper();
        let brief = s
            .brief(&super::harness::brief(
                json!({ "outcome": format!("Task {n}."), "files": [] }),
            ))
            .unwrap();
        let key = s.answer_key(&key(), &brief.id).unwrap();
        let task = w
            .core
            .store
            .create_crew_task(&brief, &key, &format!("run-{n}"))
            .unwrap();
        w.core
            .store
            .add_crew_run(&task.id, &w.lead, &format!("worker-r{n}"))
            .unwrap();
        let thread = worker_thread(&format!("w{n}")).await;
        let base = {
            let out = std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&w.repo)
                .output()
                .unwrap();
            String::from_utf8(out.stdout).unwrap().trim().to_owned()
        };
        w.core
            .store
            .set_crew_worker(&task.id, &thread.id, w.repo.to_str().unwrap(), Some(&base))
            .unwrap();
        let record = crate::crew::protect::snapshot(&w.repo, &key.protected_paths);
        w.core
            .store
            .set_crew_protected(&task.id, &serde_json::to_string(&record).unwrap())
            .unwrap();
        for step in ["routed", "running"] {
            w.core.store.move_crew_task(&task.id, step, None).unwrap();
        }
        if state == "verifying" {
            w.core
                .store
                .begin_crew_verification(&task.id, "gone", None)
                .unwrap();
        }
        made.push(task.id);
    }
    w.core.recover_crew_tasks().await;
    w.until_state(&made[0], "passed").await;
    w.until_state(&made[1], "stopped").await;
    assert_eq!(
        w.task(&made[1]).stop_reason.as_deref(),
        Some("engine_error")
    );
}

/// An attachment tool takes any thread of the project; a worker may use only its own.
#[tokio::test]
async fn a_worker_may_attach_only_to_its_own_thread() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (started, _) = w.start(brief(json!({})), key()).await;
    let id = started["task"].as_str().unwrap().to_owned();
    let own = w.worker(&id);
    let token = bind(&w.core, &own, "run-w");
    for tool in ["orbyn_attachment_add", "orbyn_attachment_discard"] {
        let (text, error) = w
            .raw(
                &token,
                tool,
                json!({ "thread_id": w.lead, "path": "src/base.txt", "attachment_id": "x" }),
            )
            .await;
        assert!(
            error && text.contains("only on its own thread"),
            "{tool}: {text}"
        );
        let (text, _) = w
            .raw(
                &token,
                tool,
                json!({ "thread_id": own, "path": "src/base.txt", "attachment_id": "x" }),
            )
            .await;
        assert!(!text.contains("only on its own thread"), "{tool}: {text}");
    }
    // Without a thread it is refused as well.
    let (text, error) = w
        .raw(
            &token,
            "orbyn_attachment_add",
            json!({ "path": "src/base.txt" }),
        )
        .await;
    assert!(error && text.contains("only on its own thread"), "{text}");
}

/// Two verifications of one task at once are one: the second finds the claim taken, so the
/// checks do not run twice in a checkout and the loop guard counts the failure once.
#[tokio::test]
async fn two_verifications_at_once_are_one() {
    use crate::crew::tests::fake::stamper;
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let s = stamper();
    let brief = s
        .brief(&super::harness::brief(json!({ "files": [] })))
        .unwrap();
    let failing = json!({
        "checks": [{ "id": "chk-run", "kind": "command", "params": { "run": "exit 3" },
                     "proves": "It exits 0." }],
        "protected_paths": []
    });
    let key = s.answer_key(&failing, &brief.id).unwrap();
    let task = w
        .core
        .store
        .create_crew_task(&brief, &key, "run-claim")
        .unwrap();
    w.core
        .store
        .add_crew_run(&task.id, &w.lead, "worker-claim")
        .unwrap();
    let thread = w.core.store.thread(&w.lead).unwrap();
    w.core
        .store
        .set_crew_worker(&task.id, &w.lead, &thread.cwd, None)
        .unwrap();
    for step in ["routed", "running"] {
        w.core.store.move_crew_task(&task.id, step, None).unwrap();
    }
    w.core
        .store
        .begin_crew_verification(&task.id, "run-x", None)
        .unwrap();
    assert!(w.core.store.claim_crew_verification(&task.id).unwrap());
    assert!(!w.core.store.claim_crew_verification(&task.id).unwrap());
    // A claim from a dead process is cleared at the next start.
    w.core.store.clear_crew_claims().unwrap();
    let (a, b) = tokio::join!(
        w.core.verify_crew_task(&task.id),
        w.core.verify_crew_task(&task.id)
    );
    let _ = (a, b);
    let run = w.core.store.crew_run(&task.id).unwrap().unwrap();
    assert_eq!(run.failures, 1, "one failure, counted once");
    assert_eq!(w.core.store.crew_evidence(&task.id).unwrap().len(), 1);
}

/// `computer_*` could drive the app window that shows the lead's thread, so a worker gets none
/// of it. The lead is not affected, and the permission and skill tools are the worker's own.
#[tokio::test]
async fn a_worker_cannot_drive_the_computer() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (started, _) = w.start(brief(json!({})), key()).await;
    let id = started["task"].as_str().unwrap().to_owned();
    let token = bind(&w.core, &w.worker(&id), "run-w");
    for tool in ["computer_screenshot", "computer_click", "computer_type"] {
        let (text, error) = w.raw(&token, tool, json!({})).await;
        assert!(
            error && text.contains("crew worker cannot use"),
            "{tool}: {text}"
        );
        let (text, _) = w.raw(&w.lead_token, tool, json!({})).await;
        assert!(!text.contains("crew worker cannot use"), "{tool}: {text}");
    }
    let (text, _) = w
        .raw(&token, "skill_search", json!({ "query": "how" }))
        .await;
    assert!(!text.contains("crew worker cannot use"), "{text}");
}

/// A key with protected paths and no usable record of them from the start is not checked at
/// all: the task waits for a person, and no check runs.
#[tokio::test]
async fn protected_paths_without_a_start_record_fail_closed() {
    use crate::crew::tests::fake::stamper;
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let s = stamper();
    for (n, record) in [(1, None), (2, Some("not json"))] {
        let brief = s
            .brief(&super::harness::brief(
                json!({ "outcome": format!("Task {n}."), "files": [] }),
            ))
            .unwrap();
        let key = s.answer_key(&super::harness::key(), &brief.id).unwrap();
        let task = w
            .core
            .store
            .create_crew_task(&brief, &key, &format!("run-closed-{n}"))
            .unwrap();
        w.core
            .store
            .add_crew_run(&task.id, &w.lead, &format!("worker-closed{n}"))
            .unwrap();
        let cwd = w.core.store.thread(&w.lead).unwrap().cwd;
        w.core
            .store
            .set_crew_worker(&task.id, &format!("t{n}"), &cwd, None)
            .unwrap();
        if let Some(text) = record {
            w.core.store.set_crew_protected(&task.id, text).unwrap();
        }
        for step in ["routed", "running"] {
            w.core.store.move_crew_task(&task.id, step, None).unwrap();
        }
        w.core
            .store
            .begin_crew_verification(&task.id, "run-x", None)
            .unwrap();
        w.core.verify_crew_task(&task.id).await;
        assert_eq!(w.task(&task.id).state, "needs_person");
        assert!(
            w.core.store.crew_evidence(&task.id).unwrap().is_empty(),
            "no check ran"
        );
    }
}

/// Claims left by a process that died are cleared as the core opens, before any run can end.
#[tokio::test]
async fn a_dead_claim_is_cleared_when_the_core_opens() {
    use crate::crew::tests::fake::stamper;
    let dir = crate::testing::TempDir::new();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let id = {
        let store = crate::store::Store::open(&data.join("orbyn.sqlite3"), "old").unwrap();
        let s = stamper();
        let brief = s
            .brief(&super::harness::brief(json!({ "files": [] })))
            .unwrap();
        let key = s.answer_key(&super::harness::key(), &brief.id).unwrap();
        let task = store.create_crew_task(&brief, &key, "run-dead").unwrap();
        store.add_crew_run(&task.id, "lead", "worker-dead").unwrap();
        for step in ["routed", "running"] {
            store.move_crew_task(&task.id, step, None).unwrap();
        }
        store
            .begin_crew_verification(&task.id, "run-x", None)
            .unwrap();
        assert!(store.claim_crew_verification(&task.id).unwrap());
        task.id
    };
    let core =
        crate::Core::open(crate::CoreConfig::new(data, "http://127.0.0.1:1".into())).unwrap();
    assert!(
        core.store.claim_crew_verification(&id).unwrap(),
        "the claim was cleared"
    );
}
