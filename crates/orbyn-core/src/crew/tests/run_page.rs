//! The Run page read (Crew Preview) on fake engines: a finished task shows its prompt, the checks the verifier ran, what it
//! published and what was not verified; a long brief is cut and says a value was hidden; the answer key's id and the paths
//! on this machine never reach it.

use orbyn_contracts::{CrewId, CrewMember, CrewTimestamp, CrewToken};
use serde_json::json;

use super::harness::{KEY_MARKER, Turn, brief, key, world};
use super::review_support::arc;
use crate::crew::Stamper;

/// The builder writes a file under `src/` and reports, naming what no check covered.
fn built_noting(root: &std::path::Path) -> Turn {
    std::fs::write(root.join("src/lib.txt"), "fn parse_duration() {}\n").unwrap();
    Turn::Report(json!({ "summary": "Added it.", "not_verified": ["Windows paths."] }))
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
    assert!(prompt.truncated, "a 3000-character brief is cut");
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
