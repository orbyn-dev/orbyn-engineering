//! Audit wave 1: the edges the neighbouring crew tests leave open. The time budget at exactly its limit and past
//! it, the daily cap kept per engine rather than per crew, the resume prompt's silence, and two recoveries racing.

use std::sync::Arc;

use orbyn_contracts as wire;
use orbyn_contracts::{AuditEvent, CreateThread};
use serde_json::{Value, json};

use super::harness::{KEY_MARKER, Script, Turn, World, brief, key, report, world, world_with};
use crate::crew::background_daily::local_day_and_reset;
use crate::crew::verify::{budget_left, budget_left_after_downtime};
use crate::error::CoreError;

const OFFSET: i32 = 600;

/// The budget's exact edge: the second the allowance runs out, and the second before; a zero-minute budget; and
/// the restart's off-time credit, which may return the whole budget but never add to it.
#[test]
fn a_budget_at_its_limit_has_nothing_left_and_downtime_never_adds_time() {
    let start = 1_700_000_000_i64;
    let created = crate::util::format_unix(start);
    let cases: [(&str, i64, u64, u64); 4] = [
        ("the exact limit", 30 * 60, 30, 0),
        ("one second before the limit", 30 * 60 - 1, 30, 1),
        ("past the limit", 30 * 60 + 1, 30, 0),
        ("a zero-minute budget", 0, 0, 0),
    ];
    for (what, elapsed, minutes, want) in cases {
        assert_eq!(
            budget_left(&created, minutes, start + elapsed).as_secs(),
            want,
            "budget_left, {what}"
        );
    }
    let after: [(&str, i64, i64, u64, u64); 5] = [
        ("the exact limit after the restart", 30 * 60, 0, 30, 0),
        ("one second before the limit", 30 * 60 - 1, 0, 30, 1),
        ("past the limit", 30 * 60 + 1, 0, 30, 0),
        (
            "off time equal to what was spent",
            10 * 60,
            10 * 60,
            30,
            30 * 60,
        ),
        (
            "off time larger than what was spent",
            60,
            10 * 60,
            30,
            30 * 60,
        ),
    ];
    for (what, last, downtime, minutes, want) in after {
        assert_eq!(
            budget_left_after_downtime(&created, minutes, start + last, downtime).as_secs(),
            want,
            "budget_left_after_downtime, {what}"
        );
    }
    // A zero-minute budget has nothing to return, whatever the off time.
    assert_eq!(
        budget_left_after_downtime(&created, 0, start + 600, 600).as_secs(),
        0
    );
}

fn project(w: &World) -> String {
    w.core.store.thread(&w.lead).unwrap().project_id
}

/// A stored background crew with one turn a day, on: the cap `background_daily` uses.
async fn crew_daily(w: &World, name: &str, runs: u32, turns: u32) -> String {
    let body: wire::BackgroundCrewCreate = serde_json::from_value(json!({
        "project_id": project(w), "name": name, "template": "fix_failing_ci",
        "trigger": { "kind": "ci_failure", "branch": "main" },
        "budget": { "turns": 20, "minutes": 30 },
        "daily": { "runs": runs, "turns": turns },
        "utc_offset_minutes": OFFSET,
    }))
    .unwrap();
    let crew = w.core.background_create(&body).await.unwrap();
    w.core.background_set_enabled(&crew.id, true).unwrap();
    crew.id
}

/// A worker that changes a file it may change and says so, as a real engine does.
fn good() -> Script {
    Arc::new(|n, root| {
        std::fs::write(root.join(format!("src/fix-{n}.txt")), "fixed\n").unwrap();
        Turn::Say(report("done"))
    })
}

fn run_of(w: &World, crew: &str) -> wire::BackgroundRun {
    let page = crate::crew::Page::new(None, None).unwrap();
    w.core.background_runs(crew, page).unwrap().runs.remove(0)
}

async fn until_run(w: &World, crew: &str, state: wire::BackgroundRunState) -> wire::BackgroundRun {
    w.until("the run reaches its state", |w| {
        run_of(w, crew).state == state
    })
    .await;
    run_of(w, crew)
}

/// The engine of the run's one task, read from the worker thread the task stored.
fn worker_engine(w: &World, run: &wire::BackgroundRun) -> String {
    let tasks = w.core.store.crew_tasks_in_crew(&run.id).unwrap();
    assert_eq!(tasks.len(), 1, "one task per run");
    let run = w.core.store.crew_run(&tasks[0].0.id).unwrap().unwrap();
    w.core
        .store
        .thread(run.worker_thread_id.as_deref().unwrap())
        .unwrap()
        .engine
}

/// The audit rows of one action, oldest first.
fn audits(w: &World, action: &str) -> Vec<Value> {
    w.core
        .store
        .list_audit(None, 500)
        .unwrap()
        .into_iter()
        .filter(|row| row.action == action)
        .map(|row| row.detail)
        .collect()
}

/// The audit rows of one action about one task.
fn task_audits(w: &World, task: &str, action: &str) -> Vec<AuditEvent> {
    w.core
        .store
        .list_audit(None, 1000)
        .unwrap()
        .into_iter()
        .filter(|row| row.target == task && row.action == action)
        .collect()
}

/// A crew's daily cap is per engine: alpha at its cap must not refuse bravo's start or turn, and bravo's own
/// cap must still refuse bravo.
#[tokio::test]
async fn an_engine_at_its_daily_cap_does_not_refuse_another_engines_run() {
    let w = world_with(true, good(), &[("alpha", false), ("bravo", false)]).await;
    let project = project(&w);
    let choose = |engine: &str| {
        w.core
            .set_preferences(
                Some(&project),
                &[("default_engine".to_owned(), json!(engine))].into(),
            )
            .unwrap();
    };
    choose("alpha");
    let id = crew_daily(&w, "Two engines", 5, 1).await;

    // Alpha takes its one turn of the day and passes: the run ends waiting for a person, and alpha is at its cap.
    let first = w.core.crew_background_run_now(&id, "user").await.unwrap();
    assert_eq!(worker_engine(&w, &first), "alpha");
    let first = until_run(&w, &id, wire::BackgroundRunState::NeedsReview).await;
    assert_eq!(first.turns_spent, 1);

    // Alpha's cap must not refuse bravo: the router now gives bravo the run, whose day is unspent, and its one
    // turn passes.
    choose("bravo");
    let second = w.core.crew_background_run_now(&id, "user").await.unwrap();
    assert_eq!(worker_engine(&w, &second), "bravo");
    let second = until_run(&w, &id, wire::BackgroundRunState::NeedsReview).await;
    assert_eq!(second.turns_spent, 1);

    // Bravo's own one turn is spent now: the next start is refused, naming bravo and nobody else.
    let error = w
        .core
        .crew_background_run_now(&id, "user")
        .await
        .unwrap_err();
    let CoreError::PolicyDenied(why) = &error else {
        panic!("{error}")
    };
    let (_, reset) = local_day_and_reset(OFFSET, crate::util::unix_now()).unwrap();
    assert!(why.starts_with("daily budget spent: bravo"), "{why}");
    assert!(!why.contains("alpha"), "{why}");
    assert!(why.contains(&reset), "{why}");
    let refused = audits(&w, "crew.background.daily.refused");
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(refused[0]["engine"], json!("bravo"), "{refused:?}");
    let page = crate::crew::Page::new(None, None).unwrap();
    assert_eq!(
        w.core.background_runs(&id, page).unwrap().total,
        2,
        "the refused start stored no run"
    );
}

/// An idle thread in the lead's project, as a restart leaves one.
async fn idle_thread(w: &World, title: &str) -> String {
    let project = w.core.store.thread(&w.lead).unwrap().project_id;
    w.core
        .create_thread(CreateThread {
            project_id: Some(project),
            engine: "alpha".into(),
            model: None,
            title: Some(title.to_owned()),
            worktree: None,
            skill_profile: None,
            base_branch: None,
            effort: None,
            options: Default::default(),
        })
        .await
        .unwrap()
        .id
}

/// A `running` builder task with a worker and nothing active: what a restart leaves. `(task, worker thread)`.
async fn dead_task(w: &World, member: &str, minutes: u32) -> (String, String) {
    let stamper = super::fake::stamper();
    let draft = brief(json!({
        "outcome": "Resume after a restart.",
        "files": [],
        "budget": { "turns": 20, "minutes": minutes },
    }));
    let brief = stamper.brief(&draft).unwrap();
    let answer_key = stamper.answer_key(&key(), &brief.id).unwrap();
    let task = w
        .core
        .store
        .create_crew_task(&brief, &answer_key, &crate::util::new_id())
        .unwrap();
    w.core
        .store
        .add_crew_run(&task.id, &w.lead, member)
        .unwrap();
    let thread = idle_thread(w, "worker").await;
    let cwd = w.core.store.thread(&w.lead).unwrap().cwd;
    w.core
        .store
        .set_crew_worker(&task.id, &thread, &cwd, None)
        .unwrap();
    for step in ["routed", "running"] {
        w.core.store.move_crew_task(&task.id, step, None).unwrap();
    }
    (task.id, thread)
}

/// After a restart resumes a builder, the text handed to its worker holds no check command, no protected path
/// and exactly one resume sentence.
#[tokio::test]
async fn a_resume_prompt_holds_no_check_command_no_protected_path_and_one_resume() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (id, worker) = dead_task(&w, "worker-resume", 30).await;
    w.core.recover_crew_tasks().await;
    w.until("the resume prompt starts", |w| {
        !w.sent_to(&worker).is_empty()
    })
    .await;

    let prompts = w.sent_to(&worker);
    assert_eq!(prompts.len(), 1, "one resume prompt");
    let prompt = &prompts[0];

    let stored = w.core.store.crew_answer_key(&id).unwrap().unwrap();
    let needles = crate::crew::guard::check_needles(&stored.checks);
    assert!(
        needles
            .iter()
            .any(|needle| needle == &format!("echo {KEY_MARKER}")),
        "the key's command is one of its needles: {needles:?}"
    );
    assert!(
        crate::crew::guard::refuse_leak(&json!({ "prompt": prompt }), &needles).is_ok(),
        "the resume prompt repeats an answer-key check command: {prompt}"
    );

    assert!(
        !stored.protected_paths.is_empty(),
        "the fixture key protects a path"
    );
    for protected in &stored.protected_paths {
        let (path, _) = protected.parts();
        assert!(
            !prompt.contains(path) && !prompt.contains(protected.as_str()),
            "the resume prompt names the protected path {path}: {prompt}"
        );
    }

    let sentence = "Your previous turn was lost when Orbyn restarted.";
    assert_eq!(
        prompt.matches(sentence).count(),
        1,
        "exactly one resume sentence: {prompt}"
    );
}

/// Two resumes of one task racing in the store (as two recoveries would, with an idle worker for both): exactly one
/// takes the resume, the other is refused `already_resumed`, and one `crew.task.resumed` row and one extra attempt result.
#[tokio::test]
async fn two_resumes_of_one_task_racing_take_exactly_one() {
    use crate::store::ResumeOutcome;
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (id, _) = dead_task(&w, "worker-race", 30).await;

    let attempts: Vec<_> = (0..2)
        .map(|_| {
            let (core, id) = (w.core.clone(), id.clone());
            tokio::task::spawn_blocking(move || core.store.resume_crew_task(&id).unwrap())
        })
        .collect();
    let mut resumed = 0;
    let mut refused = Vec::new();
    for attempt in attempts {
        match attempt.await.unwrap() {
            ResumeOutcome::Resumed { .. } => resumed += 1,
            ResumeOutcome::Refused(why) => refused.push(why),
            ResumeOutcome::NotInFlight => panic!("the task was still running"),
        }
    }
    assert_eq!(resumed, 1, "exactly one resume is taken");
    assert_eq!(refused, ["already_resumed"]);
    assert_eq!(task_audits(&w, &id, "crew.task.resumed").len(), 1);
    let run = w.core.store.crew_run(&id).unwrap().unwrap();
    assert_eq!((run.resume_count, run.attempts), (1, 1));
}
