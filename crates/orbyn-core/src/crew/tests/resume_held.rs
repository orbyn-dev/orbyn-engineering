//! `crew_resume` on fake engines (issue #438): the lead resumes a task held in `needs_person` after its worker reported.
//! The state goes back to `running` and the same worker is re-admitted with its next turn; a task that is not held, one
//! that never started, one waiting on a person's check, another thread and a worker's token are refused and leave it alone.

use orbyn_contracts::CreateThread;
use serde_json::{Value, json};

use super::harness::{Turn, World, bind, brief, key, report, world};
use super::review_support::{arc, built};

/// A key with no check this version can run: the first report leaves the task waiting for a person.
fn undecided_key() -> Value {
    json!({
        "checks": [
            { "id": "chk-probe", "kind": "preview",
              "params": { "url": "http://127.0.0.1:9/health" },
              "proves": "The page answers." }
        ],
        "protected_paths": []
    })
}

/// A key whose second check only a person can answer.
fn manual_key() -> Value {
    json!({
        "protected_paths": [],
        "checks": [
            { "id": "chk-scope", "kind": "file_scope", "params": { "only": ["src/**"] },
              "proves": "Only leased paths changed." },
            { "id": "chk-manual", "kind": "manual", "params": { "name": "The page renders" },
              "proves": "A person looked at the page." }
        ]
    })
}

/// The worker's turns: the first reports, so the task can be held; every later one hangs, so a resumed task stays running.
fn held_script() -> super::harness::Script {
    arc(|turn, _| {
        if turn == 0 {
            Turn::Report(report("done"))
        } else {
            Turn::Hang
        }
    })
}

/// Starts the builder on `alpha` with the undecided key and waits until the task holds in `needs_person`. Returns the task
/// and its worker's thread.
async fn held(w: &World) -> (String, String) {
    let (answer, error) = w
        .start(
            brief(json!({ "engine": { "id": "alpha", "assigned_by": "person" } })),
            undecided_key(),
        )
        .await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    w.until_state(&id, "needs_person").await;
    let worker = w.worker(&id);
    (id, worker)
}

async fn resume(w: &World, token: &str, task: &str) -> (Value, bool) {
    w.call(token, "crew_resume", json!({ "task": task })).await
}

#[tokio::test]
async fn a_lead_resumes_a_held_task_and_its_worker_is_told() {
    let w = world(true, held_script()).await;
    let (id, worker) = held(&w).await;
    assert_eq!(
        w.sent_to(&worker).len(),
        1,
        "the brief is the only turn so far"
    );

    let (answer, error) = resume(&w, &w.lead_token, &id).await;
    assert!(!error, "{answer}");
    assert_eq!(answer["task"].as_str(), Some(id.as_str()));
    assert_eq!(answer["run"].as_str(), Some(w.task(&id).run_id.as_str()));
    assert_eq!(answer["state"], "running");
    assert_eq!(answer["worker"].as_str(), Some(worker.as_str()));

    // The task is running again, and the same worker is re-admitted with its next turn on its own thread.
    assert_eq!(w.task(&id).state, "running");
    w.until("the resumed turn starts", |w| w.sent_to(&worker).len() == 2)
        .await;
    w.until("the resumed run", |w| {
        w.core.store.thread(&worker).unwrap().active_run.is_some()
    })
    .await;
    let prompts = w.sent_to(&worker);
    assert!(prompts[1].contains("resumed"), "{}", prompts[1]);
    assert!(prompts[1].contains("crew_report"), "{}", prompts[1]);

    // The call is the audited action every Crew tool gets; the state and the worker's result are the record.
    let calls = w
        .core
        .store
        .list_audit(None, 500)
        .unwrap()
        .into_iter()
        .filter(|row| row.action == "tool.call" && row.target == "crew_resume")
        .count();
    assert_eq!(calls, 1, "one audited call");
}

#[tokio::test]
async fn a_task_that_is_not_held_is_refused_and_nothing_changes() {
    let w = world(true, std::sync::Arc::new(|_, _| Turn::Hang)).await;
    let (answer, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    let worker = w.worker(&id);
    w.until("the worker's run", |w| {
        w.core.store.thread(&worker).unwrap().active_run.is_some()
    })
    .await;
    let sent = w.sent_to(&worker).len();

    let (reply, refused) = resume(&w, &w.lead_token, &id).await;
    assert!(refused, "{reply}");
    assert_eq!(reply["code"], "invalid_transition");
    assert_eq!(w.task(&id).state, "running", "the task is as it was");
    assert_eq!(w.sent_to(&worker).len(), sent, "no new turn");
    assert!(w.core.store.list_queue(&worker).unwrap().is_empty());

    // A task that already ended has nothing left to resume.
    w.core.crew_cancel_task(&id, None, "user").await.unwrap();
    let (reply, refused) = resume(&w, &w.lead_token, &id).await;
    assert!(refused, "{reply}");
    assert_eq!(reply["code"], "run_ended");
    assert_eq!(w.task(&id).state, "cancelled");
}

#[tokio::test]
async fn a_worker_token_cannot_resume_its_task() {
    let w = world(true, held_script()).await;
    let (id, worker) = held(&w).await;
    let token = bind(&w.core, &worker, "run-worker");

    let (reply, refused) = resume(&w, &token, &id).await;
    assert!(refused, "{reply}");
    assert_eq!(reply["code"], "not_authorized");
    assert_eq!(w.task(&id).state, "needs_person", "the task is as it was");
    assert_eq!(w.sent_to(&worker).len(), 1);
}

#[tokio::test]
async fn another_lead_thread_cannot_resume_the_task() {
    let w = world(true, held_script()).await;
    let (id, _) = held(&w).await;
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
        })
        .await
        .unwrap()
        .id;
    let token = bind(&w.core, &other, "run-other");

    let (reply, refused) = resume(&w, &token, &id).await;
    assert!(refused, "{reply}");
    assert_eq!(reply["code"], "invalid_object");
    assert!(
        reply["message"]
            .as_str()
            .unwrap()
            .contains("no such task in this crew"),
        "{reply}"
    );
    assert_eq!(w.task(&id).state, "needs_person");
}

#[tokio::test]
async fn a_task_whose_worker_never_ran_is_not_resumed() {
    let w = world(true, std::sync::Arc::new(|_, _| Turn::Hang)).await;
    let ghost = brief(json!({ "engine": { "id": "ghost", "assigned_by": "lead" } }));
    let (answer, error) = w.start(ghost, key()).await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    assert_eq!(w.task(&id).state, "needs_person");

    let (reply, refused) = resume(&w, &w.lead_token, &id).await;
    assert!(refused, "{reply}");
    assert_eq!(reply["code"], "invalid_object");
    assert!(
        reply["message"].as_str().unwrap().contains("never started"),
        "{reply}"
    );
    assert_eq!(w.task(&id).state, "needs_person", "the task is as it was");
}

#[tokio::test]
async fn a_task_waiting_on_a_person_is_not_resumed() {
    let w = world(true, arc(|_, root| built(root))).await;
    let (answer, error) = w
        .start(
            brief(json!({ "engine": { "id": "alpha", "assigned_by": "person" } })),
            manual_key(),
        )
        .await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    w.until("the task holds for the person", |w| {
        matches!(
            w.task(&id).state.as_str(),
            "needs_person" | "passed" | "stopped"
        )
    })
    .await;
    assert_eq!(w.task(&id).state, "needs_person");
    let worker = w.worker(&id);
    let sent = w.sent_to(&worker).len();

    let (reply, refused) = resume(&w, &w.lead_token, &id).await;
    assert!(refused, "{reply}");
    assert_eq!(reply["code"], "invalid_object");
    assert!(
        reply["message"]
            .as_str()
            .unwrap()
            .contains("the person answers it"),
        "{reply}"
    );
    assert_eq!(w.task(&id).state, "needs_person");
    assert_eq!(w.sent_to(&worker).len(), sent, "no new turn");
}

#[tokio::test]
async fn a_review_lane_is_not_resumed() {
    use orbyn_contracts::{CrewId, CrewMember, CrewTimestamp, CrewToken};

    let w = world(true, std::sync::Arc::new(|_, _| Turn::Hang)).await;
    // The stored shape of a review lane that reported and then held: KC8's rule keeps a review lane out of a resume, and
    // the lead's resume does too, because the review machinery asks for the lane again with its own prompt.
    let s = crate::crew::Stamper {
        crew_id: CrewId::new(w.lead.clone()).unwrap(),
        engine: CrewToken::new("alpha").unwrap(),
        member: CrewMember::new("lead-01").unwrap(),
        now: CrewTimestamp::new("2026-11-12T09:00:00Z").unwrap(),
    };
    let b = s.brief(&brief(json!({}))).unwrap();
    let key = s.answer_key(&key(), &b.id).unwrap();
    let task = w
        .core
        .store
        .create_crew_task(&b, &key, &crate::util::new_id())
        .unwrap();
    w.core
        .store
        .add_crew_run(&task.id, &w.lead, "reviewer-abc")
        .unwrap();
    for state in ["routed", "running"] {
        w.core.store.move_crew_task(&task.id, state, None).unwrap();
    }
    w.core
        .store
        .begin_crew_verification(&task.id, "engine-run-1", None)
        .unwrap();
    w.core
        .store
        .move_crew_task(&task.id, "needs_person", None)
        .unwrap();

    let (reply, refused) = resume(&w, &w.lead_token, &task.id).await;
    assert!(refused, "{reply}");
    assert_eq!(reply["code"], "invalid_object");
    assert!(
        reply["message"].as_str().unwrap().contains("review lane"),
        "{reply}"
    );
    assert_eq!(w.task(&task.id).state, "needs_person");
}

#[test]
fn the_resume_tool_is_listed_for_the_lead_with_one_task_argument() {
    let spec = crate::crew::tool_specs()
        .into_iter()
        .find(|spec| spec["name"] == "crew_resume")
        .expect("crew_resume is listed");
    assert_eq!(spec["inputSchema"]["required"], json!(["task"]));
    assert_eq!(spec["inputSchema"]["additionalProperties"], json!(false));
    assert_eq!(spec["inputSchema"]["properties"]["task"]["type"], "string");
    assert!(
        spec["description"].as_str().unwrap().contains("lead only"),
        "{spec}"
    );
    assert!(crate::crew::is_tool("crew_resume"));
}
