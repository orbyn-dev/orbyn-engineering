//! One real turn through the engine host with the person's own, logged-in `claude`, and one permission ask
//! answered through the approval gate. Ignored: it needs the network, a signed-in `claude`, and a host
//! launcher that can load the Claude Agent SDK. Run it by hand:
//!
//! ```text
//! ORBYN_REAL_HOST=<absolute path of a launcher for orbyn-engine-host> \
//!   cargo test -p orbyn-core --lib agents::driver::host::tests::real -- --ignored --nocapture
//! ```

use std::sync::Arc;
use std::time::Duration;

use orbyn_contracts::{AgUiEvent, PermissionMode};

use super::*;
use crate::agents::driver::claude::ClaudeDriver;
use crate::governance::approvals::{ApprovalGate, RunScope};
use crate::store::{Admission, NewProject, NewThread, Store};

#[tokio::test]
#[ignore = "needs the network, a signed-in claude and a host that can load the Claude Agent SDK"]
async fn a_real_claude_turn_through_the_host_with_one_permission_answered_by_the_gate() {
    let Ok(host) = std::env::var("ORBYN_REAL_HOST") else {
        panic!("set ORBYN_REAL_HOST to the absolute path of the host launcher");
    };
    let dir = TempDir::new();
    let work = dir.path().join("work");
    std::fs::create_dir_all(&work).unwrap();

    // The core's own gate over a real store, with a thread and a running run for the ask to belong to.
    let store = Arc::new(Store::open(&dir.path().join("db"), "e").unwrap());
    let project = store
        .add_project(NewProject {
            path: &work.to_string_lossy(),
            name: "real",
            is_git: false,
            managed: false,
            scratch: false,
        })
        .unwrap();
    store.trust_project(&project.id, "test").unwrap();
    let thread = store
        .create_thread(NewThread {
            project_id: project.id,
            title: "real".into(),
            engine: "claude".into(),
            model: None,
            cwd: work.to_string_lossy().into_owned(),
            worktree: None,
            skill_profile: None,
            parent_thread_id: None,
            relation: None,
            scratch: false,
        })
        .unwrap();
    let run = store
        .admit_run(Admission {
            thread_id: &thread.id,
            prompt: "p",
            actor: "test",
        })
        .unwrap();
    store.mark_starting(&run.id).unwrap();
    store.mark_running(&run.id).unwrap();
    let gate = Arc::new(ApprovalGate::new(store.clone(), Duration::from_secs(120)));
    let scope = RunScope {
        run_id: run.id.clone(),
        thread_id: thread.id.clone(),
        engine: "claude".into(),
        family: "claude".into(),
        write_scope: None,
        mode: PermissionMode::Supervised,
        plan: false,
    };

    let pool = Pool::new(
        "claude",
        HostSpec {
            program: host.into(),
            args: vec!["claude".into()],
            env: vec![("ORBYN_ENGINE_HOST_CLAUDE".into(), "1".into())],
            instance: "claude-default".into(),
        },
        Timings::default(),
        None,
    );
    let driver = HostDriver::new(
        Arc::new(ClaudeDriver::new("claude".into(), Duration::from_secs(120))),
        pool.clone(),
        "claude".into(),
    );
    let mut spec = support::run_spec();
    spec.run_id = run.id.clone();
    spec.thread_id = thread.id.clone();
    spec.cwd = work.clone();
    spec.checkout = work.clone();
    spec.library = work.clone();
    spec.model = Some("haiku".into());
    spec.effort = None;
    spec.mcp_url = None;
    spec.context = String::new();
    spec.user_prompt =
        "Create a file named hello.txt in the current folder containing exactly the word hi, then reply with the word done."
            .into();
    spec.prompt = spec.user_prompt.clone();

    let started = start(&driver, spec)
        .await
        .expect("the host did not start the run");
    let mut events = started.events;
    let mut said = String::new();
    let mut asked = Vec::new();
    let mut outcome = None;
    let mut model = None;
    let read = async {
        while let Some(event) = events.recv().await {
            match event {
                EngineEvent::Permission(request) => {
                    println!(
                        "PERMISSION ask: kind={:?} target={} summary={}",
                        request.kind, request.target, request.summary
                    );
                    asked.push(request.target.clone());
                    let ask = crate::governance::approvals::ActionAsk {
                        kind: request.kind,
                        target: request.target,
                        summary: request.summary,
                        detail: request.detail,
                        paths: request.paths,
                    };
                    let gate_for_ask = gate.clone();
                    let scope_for_ask = scope.clone();
                    let reply = request.reply;
                    tokio::spawn(async move {
                        let verdict = gate_for_ask.request(&scope_for_ask, ask).await;
                        println!("PERMISSION gate verdict: {verdict:?}");
                        let _ = reply.send(verdict);
                    });
                    // The person approves it from the pending list, as the app's button does.
                    let person_store = store.clone();
                    let person_gate = gate.clone();
                    let thread_id = thread.id.clone();
                    tokio::spawn(async move {
                        for _ in 0..600 {
                            if let Ok(mut pending) = person_store.pending_for_thread(&thread_id)
                                && let Some(approval) = pending.pop()
                            {
                                let decided =
                                    person_gate.decide(&approval.id, true, false, "test-person");
                                println!("PERSON decided: ok={}", decided.is_ok());
                                return;
                            }
                            tokio::time::sleep(Duration::from_millis(50)).await;
                        }
                    });
                }
                EngineEvent::Agui(AgUiEvent::TextMessageContent { delta, .. }) => {
                    said.push_str(&delta)
                }
                EngineEvent::Agui(AgUiEvent::Custom { name, value }) if name == "engine.model" => {
                    model = Some(value);
                }
                EngineEvent::Finished(done) => {
                    outcome = Some(done);
                    break;
                }
                _ => {}
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(240), read)
        .await
        .expect("the real run did not end in time");
    println!("OUTCOME: {outcome:?}");
    println!("MODEL FRAME: {model:?}");
    println!("SAID: {said}");
    println!("ASKED: {asked:?}");
    let file = std::fs::read_to_string(work.join("hello.txt")).unwrap_or_default();
    println!("FILE hello.txt: {file:?}");
    let status = pool.status().await;
    println!(
        "HOST: state={:?} version={:?}",
        status.state, status.host_version
    );
    pool.shutdown().await;
    assert_eq!(outcome, Some(EngineOutcome::Completed));
    assert!(!asked.is_empty(), "no permission was asked");
    assert!(file.trim() == "hi", "hello.txt: {file:?}");
}
