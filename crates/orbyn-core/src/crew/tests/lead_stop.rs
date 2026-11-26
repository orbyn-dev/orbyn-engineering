//! The lead's stop: `crew_stop` ends a task the lead thread leads as `stopped` for `lead_stopped`, and nothing a worker
//! or another crew sends can stop a task.

use std::sync::Arc;

use orbyn_contracts::{CreateThread, CrewResult, ResultState, StopReason};
use serde_json::json;

use super::harness::{Turn, World, bind, brief, key, world};

fn last_result(w: &World, id: &str) -> CrewResult {
    w.core
        .store
        .crew_results(id)
        .unwrap()
        .into_iter()
        .last()
        .expect("the task has a result")
}

/// A stored task of this crew that never got a worker: `briefed` as created, or moved on to `routed`. The store's own rows,
/// because `crew_start` routes and launches in one call, so it cannot leave a task at either state for a test.
fn seeded_task(w: &World, state: &str) -> String {
    use orbyn_contracts as wire;
    let stamper = crate::crew::Stamper {
        crew_id: wire::CrewId::new(w.lead.clone()).unwrap(),
        engine: wire::CrewToken::new("alpha").unwrap(),
        member: wire::CrewMember::new("lead-01").unwrap(),
        now: wire::CrewTimestamp::new("2026-11-12T10:00:00Z").unwrap(),
    };
    let brief = stamper
        .brief(&brief(json!({ "outcome": format!("Seed {state}.") })))
        .unwrap();
    let key = stamper.answer_key(&key(), &brief.id).unwrap();
    let task = w
        .core
        .store
        .create_crew_task(&brief, &key, &format!("run-{state}"))
        .unwrap();
    w.core
        .store
        .add_crew_run(&task.id, &w.lead, "worker-01")
        .unwrap();
    if state == "routed" {
        w.core
            .store
            .move_crew_task(&task.id, "routed", None)
            .unwrap();
    }
    task.id
}

#[tokio::test]
async fn the_lead_stops_a_task_it_leads() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (started, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "{started}");
    let id = started["task"].as_str().unwrap().to_owned();

    let (answer, error) = w
        .call(&w.lead_token, "crew_stop", json!({ "task": id }))
        .await;
    assert!(!error, "{answer}");
    assert_eq!(answer["task"], json!(id));
    assert_eq!(answer["state"], json!("stopped"));
    assert_eq!(answer["stop_reason"], json!("lead_stopped"));

    let task = w.task(&id);
    assert_eq!(task.state, "stopped");
    assert_eq!(task.stop_reason.as_deref(), Some("lead_stopped"));
    let result = last_result(&w, &id);
    assert_eq!(
        (result.state, result.stop_reason, result.r#final),
        (ResultState::Stopped, Some(StopReason::LeadStopped), true)
    );
    assert_eq!(result.summary, "The lead stopped the task.");
    // The call is audited before it happens, as every Crew tool call is.
    let audits = w.core.store.list_audit(Some(&w.lead), 100).unwrap();
    assert!(
        audits
            .iter()
            .any(|row| row.action == "tool.call" && row.target == "crew_stop"),
        "the stop call is audited"
    );

    // A task that ended is not stopped twice.
    let (again, refused) = w
        .call(&w.lead_token, "crew_stop", json!({ "task": id }))
        .await;
    assert!(refused, "{again}");
    assert_eq!(again["code"], json!("run_ended"));
    assert_eq!(w.core.store.crew_results(&id).unwrap().len(), 1);
}

#[tokio::test]
async fn the_lead_stops_a_held_task() {
    // A brief no engine can take is held for a person (`needs_person`) and never gets a worker.
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let ghost = brief(json!({ "engine": { "id": "ghost", "assigned_by": "lead" } }));
    let (started, error) = w.start(ghost, key()).await;
    assert!(!error, "{started}");
    let id = started["task"].as_str().unwrap().to_owned();
    assert_eq!(w.task(&id).state, "needs_person");

    let (answer, error) = w
        .call(&w.lead_token, "crew_stop", json!({ "task": id }))
        .await;
    assert!(!error, "{answer}");
    assert_eq!(answer["state"], json!("stopped"));
    assert_eq!(answer["stop_reason"], json!("lead_stopped"));
    let task = w.task(&id);
    assert_eq!(task.state, "stopped");
    assert_eq!(task.stop_reason.as_deref(), Some("lead_stopped"));
    let result = last_result(&w, &id);
    assert_eq!(
        (result.state, result.stop_reason),
        (ResultState::Stopped, Some(StopReason::LeadStopped))
    );
}

/// The lifecycle has no move from `briefed` or `routed` to `stopped`, so a task that never started a worker is refused as
/// an invalid transition (the store's own move would refuse it too, but without the guard it would read as `run_ended`).
#[tokio::test]
async fn a_task_the_lifecycle_cannot_stop_is_refused() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    for state in ["briefed", "routed"] {
        let id = seeded_task(&w, state);
        assert_eq!(w.task(&id).state, state);
        let before = w.task(&id);
        let (answer, refused) = w
            .call(&w.lead_token, "crew_stop", json!({ "task": id }))
            .await;
        assert!(refused, "{state}: {answer}");
        assert_eq!(
            answer["code"],
            json!("invalid_transition"),
            "{state}: {answer}"
        );
        assert_eq!(w.task(&id), before, "{state}: nothing changed");
        assert!(
            w.core.store.crew_results(&id).unwrap().is_empty(),
            "{state}: no result is written"
        );
    }
}

#[tokio::test]
async fn a_worker_cannot_stop_a_task() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (started, _) = w.start(brief(json!({})), key()).await;
    let id = started["task"].as_str().unwrap().to_owned();
    let token = bind(&w.core, &w.worker(&id), "run-w");
    let before = w.task(&id).state;
    let (answer, refused) = w.call(&token, "crew_stop", json!({ "task": id })).await;
    assert!(refused, "{answer}");
    assert_eq!(answer["code"], json!("not_authorized"));
    assert_eq!(answer["message"], json!("only the lead stops a task"));
    assert_eq!(w.task(&id).state, before, "nothing changed");
}

/// A task of another crew is not in this crew to stop, even for a lead thread of its own.
#[tokio::test]
async fn another_crew_cannot_stop_a_task() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (started, _) = w.start(brief(json!({})), key()).await;
    let id = started["task"].as_str().unwrap().to_owned();
    let project = w.core.store.thread(&w.lead).unwrap().project_id;
    let other = w
        .core
        .create_thread(CreateThread {
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
    let token = bind(&w.core, &other.id, "run-other");
    let before = w.task(&id).state;
    let (answer, refused) = w.call(&token, "crew_stop", json!({ "task": id })).await;
    assert!(refused, "{answer}");
    assert_eq!(
        answer["code"],
        json!("invalid_object"),
        "another crew's task is not there to stop"
    );
    assert_eq!(w.task(&id).state, before, "nothing changed");
}

/// The tool is listed for the lead, with the one argument it takes, and hidden from no Crew tool list.
#[test]
fn the_stop_tool_is_listed_with_its_one_argument() {
    let spec = crate::crew::tool_specs()
        .into_iter()
        .find(|tool| tool["name"] == crate::crew::STOP_TOOL)
        .expect("crew_stop is listed");
    assert_eq!(spec["inputSchema"]["required"], json!(["task"]));
    assert_eq!(spec["inputSchema"]["additionalProperties"], json!(false));
    assert!(crate::crew::is_tool("crew_stop"));
}
