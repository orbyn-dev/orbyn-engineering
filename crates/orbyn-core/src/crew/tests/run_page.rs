//! The Run page read (Crew Preview) on fake engines: a finished task shows its prompt, the checks of its latest
//! verification round, what it published and what was not verified; a running task shows the rows but no check text until
//! it is final; a finished task never names a running sibling's key; a repro is never a check; a long brief is cut and says
//! a value was hidden; the answer key's id never reaches it.

use std::path::Path;

use orbyn_contracts::{
    CrewId, CrewMember, CrewObject, CrewTimestamp, CrewToken, GateStage, object_id,
};
use serde_json::{Value, json};

use super::harness::{KEY_MARKER, Turn, brief, key, world};
use super::review_support::{arc, commit_all, crew, finding, repro_for, reviewed};
use crate::crew::Stamper;
use crate::error::CoreError;

/// The builder writes a file under `src/` and reports, naming what no check covered.
fn built_noting(root: &Path) -> Turn {
    std::fs::write(root.join("src/lib.txt"), "fn parse_duration() {}\n").unwrap();
    Turn::Report(json!({ "summary": "Added it.", "not_verified": ["Windows paths."] }))
}

/// One fake row as the page reads it: a later verification round's Evidence, built from a real one.
fn add_evidence(
    store: &crate::store::Store,
    task: &str,
    base: &Value,
    check: &str,
    command: &str,
    commit: &str,
    proves: &str,
) -> String {
    let mut value = base.clone();
    value["id"] = json!("");
    value["check"] = json!(check);
    value["command"] = json!(command);
    value["commit"] = json!(commit);
    value["proves"] = json!(proves);
    value["does_not_prove"] = json!("Anything else.");
    value["outcome"] = json!("pass");
    value["exit_code"] = json!(0);
    value["duration_ms"] = json!(1);
    value["id"] = json!(object_id(&value).unwrap());
    store
        .add_crew_object(task, &CrewObject::parse(&value).unwrap())
        .unwrap();
    value["id"].as_str().unwrap().to_owned()
}

/// A later checks gate citing exactly `evidence`, built from a real one.
fn add_gate(
    store: &crate::store::Store,
    task: &str,
    base: &Value,
    evidence: Vec<String>,
    reason: &str,
) {
    let mut value = base.clone();
    value["id"] = json!("");
    value["evidence"] = json!(evidence);
    value["created_at"] = json!("2027-01-01T00:00:00Z");
    value["reason"] = json!(reason);
    value["id"] = json!(object_id(&value).unwrap());
    store
        .add_crew_object(task, &CrewObject::parse(&value).unwrap())
        .unwrap();
}

/// The result that round wrote, built from a real one.
fn add_result(
    store: &crate::store::Store,
    task: &str,
    base: &Value,
    checks: Value,
    commit: &str,
    note: &str,
) {
    let mut value = base.clone();
    value["id"] = json!("");
    value["checks"] = checks;
    value["commit"] = json!(commit);
    value["not_verified"] = json!([note]);
    value["created_at"] = json!("2027-01-01T00:00:01Z");
    value["summary"] = json!("The newer round passed.");
    value["id"] = json!(object_id(&value).unwrap());
    store
        .add_crew_object(task, &CrewObject::parse(&value).unwrap())
        .unwrap();
}

fn failing_key() -> Value {
    json!({
        "checks": [{ "id": "chk-fail", "kind": "command",
            "params": { "run": "git grep --no-index -q NEVERHERE -- src/lib.txt" },
            "proves": "The marker is absent." }],
        "protected_paths": ["tests/**"]
    })
}

#[tokio::test]
async fn a_finished_task_shows_its_prompt_its_checks_and_what_was_not_verified() {
    let w = world(true, arc(|_, root| built_noting(root))).await;
    let (answer, error) = w
        .start(
            brief(json!({ "engine": { "id": "alpha", "assigned_by": "person" } })),
            key(),
        )
        .await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    w.until_state(&id, "passed").await;

    let page = w.core.crew_flow_run(&id).unwrap();
    assert_eq!(
        (page.task.as_str(), page.state.as_str()),
        (id.as_str(), "passed")
    );
    assert!(page.finished);
    assert!(!page.truncated);

    // The prompt is the person's own brief.
    let prompt = page.prompt.as_ref().expect("the task has a brief");
    assert_eq!(prompt.text, "Add a parse_duration function.");
    assert!(!prompt.hidden && !prompt.truncated);

    // The command check ran, with its own record: the command as written, its exit code, how long it took and what it proves.
    let command = page
        .checks
        .iter()
        .find(|c| c.label.contains(KEY_MARKER))
        .expect("the command check is on the page");
    assert_eq!(command.label, format!("echo {KEY_MARKER}"));
    assert_eq!(command.outcome, "pass");
    assert_eq!(command.exit_code, Some(0));
    assert_eq!(command.proves, "The command exits 0.");
    assert_eq!(command.commit.chars().count(), 40);

    // What no check covered is the run's own answer, and the proof covers one commit.
    assert!(
        page.not_verified
            .iter()
            .any(|t| t.contains("Windows paths")),
        "{:?}",
        page.not_verified
    );
    assert_eq!(page.commit.as_deref(), Some(command.commit.as_str()));
    // No pull request was made for this task here.
    assert!(page.pull.is_none());

    // The answer key's id never reaches the page, and neither does a path on this machine.
    let body = serde_json::to_string(&page).unwrap();
    let key_id = w.core.store.crew_answer_key(&id).unwrap().unwrap().id;
    assert!(
        !body.contains(key_id.as_str()),
        "the page holds the key's id"
    );
    assert!(
        !body.contains(&w.repo.to_string_lossy().to_string()),
        "the page holds a path on this machine"
    );
}

fn stamper() -> Stamper {
    Stamper {
        crew_id: CrewId::new("crew-1").unwrap(),
        engine: CrewToken::new("claude").unwrap(),
        member: CrewMember::new("lead-01").unwrap(),
        now: CrewTimestamp::new("2026-11-12T10:00:00Z").unwrap(),
    }
}

#[tokio::test]
async fn a_long_prompt_is_cut_and_the_page_says_a_value_was_hidden() {
    let w = world(true, arc(|_, root| built_noting(root))).await;
    // A task that never ran: the read answers from its stored brief alone, and shows no check.
    let secret = "AKIAIOSFODNN7EXAMPLE";
    let outcome = format!("Use {secret} once. {}", "word ".repeat(420));
    let length = outcome.chars().count();
    let stamper = stamper();
    let brief = stamper
        .brief(&json!({
            "kind": "implement",
            "outcome": outcome,
            "inputs": [],
            "files": [{ "path": "src/**", "mode": "write" }],
            "allowed_effects": ["read_repo", "write_leased", "run_checks"],
            "stop_when": ["Done."],
            "budget": { "turns": 20, "minutes": 30 }
        }))
        .unwrap();
    let key = stamper.answer_key(&key(), &brief.id).unwrap();
    let task = w
        .core
        .store
        .create_crew_task(&brief, &key, "run-long")
        .unwrap();

    let page = w.core.crew_flow_run(&task.id).unwrap();
    assert!(!page.finished, "the task never ran");
    let prompt = page.prompt.expect("the task has a brief");
    assert!(
        prompt.truncated,
        "a {length}-character brief is cut at 2,000"
    );
    assert!(
        prompt.hidden,
        "the key-shaped value is hidden, and the page says so"
    );
    assert!(
        prompt.text.chars().count() <= 2000,
        "{}",
        prompt.text.chars().count()
    );
    assert!(!prompt.text.contains(secret), "the secret is not quoted");
    assert!(page.checks.is_empty());
    assert!(page.commit.is_none());
    assert!(page.pull.is_none());
}

#[tokio::test]
async fn a_running_task_shows_its_check_rows_without_text_until_it_is_final() {
    let w = world(
        true,
        arc(|n, root| match n {
            0 => {
                std::fs::write(root.join("src/lib.txt"), "fn parse_duration() {}\n").unwrap();
                Turn::Report(json!({ "summary": "Added it." }))
            }
            _ => Turn::Hang,
        }),
    )
    .await;
    let (answer, error) = w
        .start(
            brief(json!({ "engine": { "id": "alpha", "assigned_by": "person" } })),
            failing_key(),
        )
        .await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    w.until("the failed round and its retry", |w| {
        w.task(&id).state == "running"
            && !w.core.store.crew_evidence(&id).unwrap().is_empty()
            && !w.core.store.crew_results(&id).unwrap().is_empty()
    })
    .await;

    let page = w.core.crew_flow_run(&id).unwrap();
    assert!(!page.finished);
    assert!(!page.checks.is_empty(), "the check rows are shown");
    assert!(
        page.checks
            .iter()
            .all(|check| check.label.is_empty() && check.proves.is_empty()),
        "{:?}",
        page.checks
    );
    assert!(page.not_verified.is_empty());
    let body = serde_json::to_string(&page).unwrap();
    assert!(
        !body.contains("NEVERHERE"),
        "a running task's page held its check text"
    );
    assert!(
        page.checks.iter().all(|check| check.outcome == "fail"),
        "{:?}",
        page.checks
    );

    w.core.crew_cancel_task(&id, None, "test").await.unwrap();
    assert_eq!(w.task(&id).state, "cancelled");
    let page = w.core.crew_flow_run(&id).unwrap();
    assert!(page.finished);
    assert!(
        !page.checks.is_empty(),
        "the last verification's rows stay after the task is final"
    );
    assert!(
        page.checks
            .iter()
            .any(|check| check.label.contains("NEVERHERE")),
        "{:?}",
        page.checks
    );
    assert!(
        page.checks
            .iter()
            .any(|check| check.proves == "The marker is absent."),
        "{:?}",
        page.checks
    );
}

#[tokio::test]
async fn a_finished_task_does_not_name_a_running_siblings_check() {
    let w = crew(arc(|n, root| match n {
        0 => {
            std::fs::write(root.join("src/lib.txt"), "fn parse_duration() {}\n").unwrap();
            Turn::Report(json!({ "summary": "Added it." }))
        }
        _ => Turn::Hang,
    }))
    .await;
    let done = w.built_work().await;
    let (answer, error) = w
        .start(
            brief(
                json!({ "outcome": "Still running.", "engine": { "id": "alpha", "assigned_by": "person" } }),
            ),
            key(),
        )
        .await;
    assert!(!error, "{answer}");
    let running = answer["task"].as_str().unwrap().to_owned();
    w.until_state(&running, "running").await;

    let page = w.core.crew_flow_run(&done).unwrap();
    assert!(page.finished);
    assert!(!page.checks.is_empty());
    let body = serde_json::to_string(&page).unwrap();
    assert!(
        !body.contains(KEY_MARKER),
        "a finished task named a running sibling's key"
    );
}

/// A fix that changes the finding's file, commits it and reports its own note, so the fix round differs from the build's.
fn fixed_with_note(root: &Path) -> Turn {
    let mut text = std::fs::read_to_string(root.join("src/lib.txt")).unwrap_or_default();
    text.push_str("FIXED\n");
    std::fs::write(root.join("src/lib.txt"), text).unwrap();
    commit_all(root, "fix");
    Turn::Report(json!({ "summary": "fixed", "not_verified": ["The fix's own note."] }))
}

#[tokio::test]
async fn a_fix_round_supplies_its_own_checks_commit_and_not_verified_lines() {
    let w = crew(arc(|n, root| match n {
        0 => {
            std::fs::write(root.join("src/lib.txt"), "fn parse_duration() {}\n").unwrap();
            Turn::Report(json!({
                "summary": "Added it.",
                "not_verified": ["The original round's note."]
            }))
        }
        1 => reviewed(vec![finding("src/lib.txt", 1, "It is wrong.")]),
        2 => fixed_with_note(root),
        _ => Turn::Hang,
    }))
    .await;
    let subject = w.built_work().await;
    let lanes = w.review(&subject, &["correctness"]).await;
    w.until_state(&lanes[0], "passed").await;
    let found = w.first_finding(&subject);
    let (answer, error) = w
        .lead(
            "crew_check_finding",
            json!({ "finding": found, "repro": repro_for("repro-page", "src/lib.txt") }),
        )
        .await;
    assert!(!error, "{answer}");
    assert_eq!(answer["fix"]["started"], true, "{answer}");
    let fix = w.core.store.crew_fixes_of(&subject).unwrap()[0]
        .fix_task_id
        .clone();
    w.until("the fix round and its result", |w| {
        w.task(&fix).state == "passed" && !w.core.store.crew_results(&fix).unwrap().is_empty()
    })
    .await;

    let subject_result = w.core.store.crew_results(&subject).unwrap().pop().unwrap();
    let fix_result = w.core.store.crew_results(&fix).unwrap().pop().unwrap();
    let page = w.core.crew_flow_run(&subject).unwrap();
    assert!(page.finished);
    assert!(
        page.checks
            .iter()
            .any(|check| check.label == format!("echo {KEY_MARKER}")),
        "{:?}",
        page.checks
    );
    assert!(
        page.checks
            .iter()
            .all(|check| !check.label.contains("git grep")),
        "a repro became a check: {:?}",
        page.checks
    );
    assert!(
        page.checks
            .iter()
            .any(|check| check.label.contains("chk-fix-touched")),
        "the page did not take the fix round's checks: {:?}",
        page.checks
    );
    assert_eq!(
        page.commit.as_deref(),
        fix_result.commit.as_ref().map(|commit| commit.as_str()),
        "the page's commit is the fix round's"
    );
    assert_ne!(
        page.commit.as_deref(),
        subject_result.commit.as_ref().map(|commit| commit.as_str()),
        "the page did not take the original round's commit"
    );
    assert!(
        page.not_verified
            .iter()
            .any(|line| line.contains("The fix's own note.")),
        "{:?}",
        page.not_verified
    );
    assert!(
        !page
            .not_verified
            .iter()
            .any(|line| line.contains("The original round's note.")),
        "the page took the original round's not-verified lines: {:?}",
        page.not_verified
    );
    let body = serde_json::to_string(&page).unwrap();
    assert!(
        !body.contains("git grep --no-index -q FIXED"),
        "a repro command reached the page"
    );
    // A fix is a round of another task, not a work task with a Run page of its own.
    assert!(matches!(
        w.core.crew_flow_run(&fix),
        Err(CoreError::NotFound(_))
    ));
}

#[tokio::test]
async fn a_page_shows_only_the_newest_verification_round() {
    let w = world(true, arc(|_, root| built_noting(root))).await;
    let (answer, error) = w
        .start(
            brief(json!({ "engine": { "id": "alpha", "assigned_by": "person" } })),
            key(),
        )
        .await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    w.until_state(&id, "passed").await;

    let base_evidence =
        CrewObject::Evidence(w.core.store.crew_evidence(&id).unwrap().remove(0)).to_value();
    let base_gate = CrewObject::Gate(
        w.core
            .store
            .crew_gates(&id)
            .unwrap()
            .into_iter()
            .find(|gate| gate.stage == GateStage::Checks)
            .unwrap(),
    )
    .to_value();
    let base_result =
        CrewObject::Result(w.core.store.crew_results(&id).unwrap().remove(0)).to_value();

    let commit = "e".repeat(40);
    let evidence = add_evidence(
        &w.core.store,
        &id,
        &base_evidence,
        "chk-new",
        "echo newer-round",
        &commit,
        "The newer round proves it.",
    );
    add_gate(
        &w.core.store,
        &id,
        &base_gate,
        vec![evidence.clone()],
        "The newer round passed.",
    );
    add_result(
        &w.core.store,
        &id,
        &base_result,
        json!([{ "check": "chk-new", "evidence": evidence, "outcome": "pass" }]),
        &commit,
        "The newer round's note.",
    );

    let page = w.core.crew_flow_run(&id).unwrap();
    assert_eq!(page.checks.len(), 1, "{:?}", page.checks);
    assert_eq!(page.checks[0].label, "echo newer-round");
    assert_eq!(page.checks[0].proves, "The newer round proves it.");
    assert_eq!(page.commit.as_deref(), Some(commit.as_str()));
    assert_eq!(
        page.not_verified,
        vec!["The newer round's note.".to_owned()]
    );
    assert!(!page.truncated);
}

#[tokio::test]
async fn a_cap_keeps_the_newest_checks() {
    let w = world(true, arc(|_, root| built_noting(root))).await;
    let (answer, error) = w
        .start(
            brief(json!({ "engine": { "id": "alpha", "assigned_by": "person" } })),
            key(),
        )
        .await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    w.until_state(&id, "passed").await;

    let base_evidence =
        CrewObject::Evidence(w.core.store.crew_evidence(&id).unwrap().remove(0)).to_value();
    let base_gate = CrewObject::Gate(
        w.core
            .store
            .crew_gates(&id)
            .unwrap()
            .into_iter()
            .find(|gate| gate.stage == GateStage::Checks)
            .unwrap(),
    )
    .to_value();
    let base_result =
        CrewObject::Result(w.core.store.crew_results(&id).unwrap().remove(0)).to_value();

    let mut evidence = Vec::new();
    let mut checks = Vec::new();
    for n in 0..51 {
        let evidence_id = add_evidence(
            &w.core.store,
            &id,
            &base_evidence,
            &format!("chk-{n}"),
            &format!("check {n}"),
            &format!("{n:040x}"),
            "It proves.",
        );
        checks.push(
            json!({ "check": format!("chk-{n}"), "evidence": evidence_id.clone(), "outcome": "pass" }),
        );
        evidence.push(evidence_id);
    }
    add_gate(
        &w.core.store,
        &id,
        &base_gate,
        evidence,
        "The capped round passed.",
    );
    add_result(
        &w.core.store,
        &id,
        &base_result,
        json!(checks),
        &"e".repeat(40),
        "The capped round's note.",
    );

    let page = w.core.crew_flow_run(&id).unwrap();
    assert!(page.truncated);
    assert_eq!(page.checks.len(), 50);
    assert_eq!(page.checks[0].label, "check 1");
    assert_eq!(page.checks[49].label, "check 50");
}

#[tokio::test]
async fn a_long_check_command_cut_at_its_cap_says_the_page_was_truncated() {
    let w = world(true, arc(|_, root| built_noting(root))).await;
    let command = format!("echo {}", "a".repeat(995));
    let key = json!({
        "checks": [{ "id": "chk-long", "kind": "command", "params": { "run": command },
            "proves": "It prints." }],
        "protected_paths": []
    });
    let (answer, error) = w
        .start(
            brief(json!({ "engine": { "id": "alpha", "assigned_by": "person" } })),
            key,
        )
        .await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    w.until_state(&id, "passed").await;

    let page = w.core.crew_flow_run(&id).unwrap();
    assert_eq!(page.checks.len(), 1);
    assert!(
        page.truncated,
        "a 1,000-character command was cut and the page did not say so"
    );
    assert!(
        page.checks[0].label.chars().count() <= 200,
        "{}",
        page.checks[0].label.chars().count()
    );
}
