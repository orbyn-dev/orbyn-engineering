//! The Flow task's artifacts read (Crew Preview): the evidence rows of the build under way, the artifacts those rows name,
//! and the change the checks read. The rows carry no command, output, `proves` text or Evidence id, and a task with no
//! checkout still answers.

use orbyn_contracts::CheckOutcome;
use serde_json::{Value, json};

use super::harness::{Turn, world};
use super::review_support::{arc, built, commit_all};

/// A key with one command check, and the command and `proves` text it must never give away.
fn artifact_key() -> (Value, String, String) {
    let command = format!("echo artifact-read-{}", std::process::id());
    let proves = "The artifact read names this check and nothing more.".to_owned();
    let key = json!({
        "checks": [{
            "id": "chk-artifact",
            "kind": "command",
            "params": { "run": command.clone() },
            "proves": proves.clone(),
        }],
        "protected_paths": []
    });
    (key, command, proves)
}

#[tokio::test]
async fn the_read_serves_the_checks_the_artifacts_and_the_change() {
    let w = world(true, arc(|_, root| built(root))).await;
    let subject = w.built_work().await;
    let run = w.core.store.crew_run(&subject).unwrap().unwrap();
    // The builder leaves its file untracked, so commit it: the diff reads the files git tracks.
    commit_all(&w.repo, "the work");

    let view = w.core.crew_flow_task_artifacts(&subject).await.unwrap();
    assert_eq!(view.task, subject);
    assert_eq!(view.crew, w.task(&subject).crew_id);
    assert!(!view.checks.is_empty(), "the build's checks are rows");
    assert!(!view.truncated);
    for row in &view.checks {
        assert_eq!(row.task, subject);
        assert_eq!(row.outcome, CheckOutcome::Pass);
        assert_eq!(row.exit_code, Some(0));
        assert_eq!(row.commit.as_str(), run.base_commit.as_deref().unwrap());
        assert!(
            row.kind.is_some(),
            "the task's own key names the kind of every check"
        );
        assert!(
            row.artifacts.is_empty(),
            "nothing stores an artifact yet, so no row names one"
        );
    }

    let diff = view.diff.expect("the change the checks read");
    assert_eq!(diff.base, run.base_commit.unwrap());
    assert_eq!(view.diff_absent, None);
    assert!(diff.files_total >= 1, "the builder added a file");
    assert!(diff.added >= 1);
    let added = diff
        .files
        .iter()
        .find(|file| file.path == "src/lib.txt")
        .expect("the file the builder wrote is in the diff");
    assert_eq!(added.status, "added");
    assert!(!diff.truncated);
}

#[tokio::test]
async fn the_rows_never_carry_the_command_the_proves_text_or_an_evidence_id() {
    let (key, command, proves) = artifact_key();
    let w = world(true, arc(|_, root| built(root))).await;
    let subject = w.built_with(key).await;

    let view = w.core.crew_flow_task_artifacts(&subject).await.unwrap();
    assert_eq!(view.checks.len(), 1);
    assert_eq!(
        view.checks[0].kind,
        Some(orbyn_contracts::CheckKind::Command)
    );
    let text = serde_json::to_string(&view).unwrap();
    assert!(!text.contains(&command), "the command is part of the key");
    assert!(!text.contains(&proves), "what a check proves is the key's");
    assert!(!text.contains("\"proves\""));
    assert!(!text.contains("excerpt"));
    assert!(!text.contains("\"command\""));
    for row in w.core.store.crew_evidence(&subject).unwrap() {
        assert!(
            !text.contains(row.id.as_str()),
            "no Evidence id leaves the core"
        );
    }
}

#[tokio::test]
async fn a_task_with_no_checkout_still_answers_and_says_why_there_is_no_diff() {
    let w = world(true, arc(|_, root| built(root))).await;
    let subject = w.built_work().await;
    std::fs::remove_dir_all(&w.repo).unwrap();

    let view = w.core.crew_flow_task_artifacts(&subject).await.unwrap();
    assert!(!view.checks.is_empty(), "the rows come from the store");
    assert!(view.diff.is_none());
    assert_eq!(view.diff_absent.as_deref(), Some("no_checkout"));
}

#[tokio::test]
async fn with_the_flag_off_the_read_is_not_found() {
    let w = world(false, arc(|_, _| Turn::Silent)).await;
    assert!(matches!(
        w.core.crew_flow_task_artifacts("sha256:abc").await,
        Err(crate::error::CoreError::NotFound(_))
    ));
}

#[tokio::test]
async fn a_task_id_that_is_not_one_is_refused_and_an_unknown_task_is_not_found() {
    let w = world(true, arc(|_, root| built(root))).await;
    let subject = w.built_work().await;
    assert!(matches!(
        w.core.crew_flow_task_artifacts("../etc").await,
        Err(crate::error::CoreError::Invalid(_))
    ));
    assert!(matches!(
        w.core.crew_flow_task_artifacts("sha256:abc").await,
        Err(crate::error::CoreError::NotFound(_))
    ));
    assert!(w.core.crew_flow_task_artifacts(&subject).await.is_ok());
}
