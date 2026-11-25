//! `crew_resume` on fake engines (issue #438): the lead resumes a task held in `needs_person` after its worker reported.
//! The state goes back to `running` and the same worker is re-admitted with its next turn; a task that is not held, one
//! that never started, one waiting on a person's check, a review lane, a merge correction, a busy worker, one past its
//! minutes and a prompt that cannot be queued are refused (or put back) and leave the task alone, and a task held across a
//! restart is credited its off time so the minutes it resumes with are right. Another thread and a worker's token are refused.

use orbyn_contracts::{CreateThread, CrewError, ErrorCode, PermissionMode};
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

/// Puts a task's stored times where a test needs them, as a long wait or a restart would leave them.
fn set_times(w: &World, task: &str, created: i64, updated: i64) {
    w.core
        .store
        .reader()
        .execute(
            "UPDATE crew_tasks SET created_at = ?2, updated_at = ?3 WHERE id = ?1",
            rusqlite::params![
                task,
                crate::util::format_unix(created),
                crate::util::format_unix(updated)
            ],
        )
        .unwrap();
}

/// The lead's binding, for the seam the tool leaves open: `crew_resume_with` takes the binding, so a test can drive the one
/// effect after the state move (the worker's prompt).
fn lead_binding(w: &World) -> crate::tools::mcp::McpBinding {
    crate::tools::mcp::McpBinding {
        scope: crate::governance::approvals::RunScope {
            run_id: "run-seam".into(),
            thread_id: w.lead.clone(),
            engine: "alpha".into(),
            family: "alpha".into(),
            write_scope: None,
            mode: PermissionMode::Supervised,
            plan: false,
        },
        permission_prompt: false,
        skills: Vec::new(),
    }
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

#[tokio::test]
async fn a_restart_credits_a_held_tasks_off_time_so_the_resume_keeps_its_minutes() {
    let w = world(true, held_script()).await;
    let (id, _) = held(&w).await;
    // A task held two hours ago, before the app was off: its thirty minutes have not run out, and the app off time must not
    // be charged to them. Without the credit the resume below would be refused (see the past-minutes test).
    let now = crate::util::unix_now();
    let held_at = now - 2 * 60 * 60;
    set_times(&w, &id, held_at, held_at);
    w.core.recover_crew_tasks().await;

    let run = w.core.store.crew_run(&id).unwrap().unwrap();
    assert!(
        run.downtime_secs >= 2 * 60 * 60 - 5,
        "the off time is credited: {}",
        run.downtime_secs
    );
    let credited = w
        .core
        .store
        .list_audit(None, 500)
        .unwrap()
        .into_iter()
        .any(|row| row.action == "crew.task.downtime_excused" && row.target == id);
    assert!(credited, "the credit has its audit row");

    let (answer, error) = resume(&w, &w.lead_token, &id).await;
    assert!(!error, "{answer}");
    assert_eq!(answer["state"], "running");
}

#[tokio::test]
async fn a_task_past_its_minutes_is_not_resumed() {
    let w = world(true, held_script()).await;
    let (id, worker) = held(&w).await;
    // Two hours on a thirty-minute brief with no restart between: the wait counts, so there is no budget left to resume into.
    let now = crate::util::unix_now();
    let held_at = now - 2 * 60 * 60;
    set_times(&w, &id, held_at, held_at);
    let sent = w.sent_to(&worker).len();

    let (reply, refused) = resume(&w, &w.lead_token, &id).await;
    assert!(refused, "{reply}");
    assert_eq!(reply["code"], "invalid_object");
    assert!(
        reply["message"].as_str().unwrap().contains("time budget"),
        "{reply}"
    );
    assert_eq!(w.task(&id).state, "needs_person", "the task is as it was");
    assert_eq!(w.sent_to(&worker).len(), sent, "no new turn");
}

#[tokio::test]
async fn a_resumed_task_gets_its_minutes_timer_back() {
    let w = world(true, held_script()).await;
    let (id, _) = held(&w).await;
    // Ten seconds of the thirty-minute brief are left: the timer the resume arms must be the one that ends the task.
    let now = crate::util::unix_now();
    let held_at = now - (30 * 60 - 10);
    set_times(&w, &id, held_at, held_at);

    let (answer, error) = resume(&w, &w.lead_token, &id).await;
    assert!(!error, "{answer}");
    assert_eq!(answer["state"], "running");
    w.until_state(&id, "stopped").await;
    let task = w.task(&id);
    assert_eq!(task.stop_reason.as_deref(), Some("budget"));
}

#[tokio::test]
async fn a_merge_correction_is_not_resumed() {
    let w = world(true, held_script()).await;
    let (id, worker) = held(&w).await;
    // The stored shape of a corrective task: the correction owns it, and its owner takes it on without a worker turn.
    let db = rusqlite::Connection::open(w._dir.path().join("data/orbyn.sqlite3")).unwrap();
    db.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
    let body = json!({
        "parent_task": "parent-task",
        "parent_commit": "a".repeat(40),
        "task_id": id,
        "run_id": "run-correction",
        "repo": "C:/repo",
        "remote": "origin",
        "branch": "main",
        "base": "b".repeat(40),
        "candidate": "c".repeat(40),
        "tree": "d".repeat(40),
        "checkout": "C:/repo",
    })
    .to_string();
    db.execute(
        "INSERT INTO crew_forge_corrections(parent_task,task_id,body) VALUES(?1,?2,?3)",
        rusqlite::params!["parent-task", id, body],
    )
    .unwrap();
    let sent = w.sent_to(&worker).len();

    let (reply, refused) = resume(&w, &w.lead_token, &id).await;
    assert!(refused, "{reply}");
    assert_eq!(reply["code"], "invalid_object");
    assert!(
        reply["message"]
            .as_str()
            .unwrap()
            .contains("merge correction"),
        "{reply}"
    );
    assert_eq!(w.task(&id).state, "needs_person", "the task is as it was");
    assert_eq!(w.sent_to(&worker).len(), sent, "no new turn");
}

#[tokio::test]
async fn a_busy_worker_is_not_resumed() {
    let w = world(true, held_script()).await;
    let (id, worker) = held(&w).await;
    // A prompt of its own puts the worker mid-turn: the resume must not queue a second one behind it.
    w.core.runs.enqueue(&worker, "later", &[]).unwrap();
    w.until("the worker's own turn", |w| {
        w.core.store.thread(&worker).unwrap().active_run.is_some()
    })
    .await;
    let sent = w.sent_to(&worker).len();

    let (reply, refused) = resume(&w, &w.lead_token, &id).await;
    assert!(refused, "{reply}");
    assert_eq!(reply["code"], "invalid_object");
    assert!(
        reply["message"].as_str().unwrap().contains("still running"),
        "{reply}"
    );
    assert_eq!(w.task(&id).state, "needs_person", "the task is as it was");
    assert_eq!(w.sent_to(&worker).len(), sent, "no new turn");
}

#[tokio::test]
async fn a_prompt_that_cannot_be_queued_puts_the_task_back() {
    let w = world(true, held_script()).await;
    let (id, worker) = held(&w).await;
    let binding = lead_binding(&w);
    let args = json!({ "task": id });

    let refused = w.core.crew_resume_with(&binding, &args, |_, _| {
        Err(CrewError::new(
            ErrorCode::InvalidObject,
            "the test sender failed",
        ))
    });
    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(w.task(&id).state, "needs_person", "the state is put back");
    assert_eq!(w.sent_to(&worker).len(), 1, "no turn started");
    assert!(w.core.store.list_queue(&worker).unwrap().is_empty());

    // The rollback leaves a task the same call can resume: it did not move it halfway.
    let (answer, error) = resume(&w, &w.lead_token, &id).await;
    assert!(!error, "{answer}");
    assert_eq!(answer["state"], "running");
}

#[tokio::test]
async fn a_held_task_with_nothing_left_at_the_budget_boundary_is_not_resumed() {
    let w = world(true, held_script()).await;
    let (id, worker) = held(&w).await;
    // The task has spent exactly its thirty minutes: the resume reads zero left and must not arm a timer
    // with nothing on it. The positive side of the same boundary is a_resumed_task_gets_its_minutes_timer_back.
    let now = crate::util::unix_now();
    let started = now - 30 * 60;
    set_times(&w, &id, started, started);

    let (reply, refused) = resume(&w, &w.lead_token, &id).await;
    assert!(refused, "{reply}");
    assert_eq!(reply["code"], "invalid_object");
    assert!(
        reply["message"]
            .as_str()
            .unwrap()
            .contains("time budget"),
        "{reply}"
    );
    assert_eq!(w.task(&id).state, "needs_person", "the task is as it was");
    assert_eq!(w.sent_to(&worker).len(), 1, "only the brief was sent");
    assert!(w.core.store.list_queue(&worker).unwrap().is_empty());
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
