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
