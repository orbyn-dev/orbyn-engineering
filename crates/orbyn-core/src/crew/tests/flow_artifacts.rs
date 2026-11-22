//! The Flow task's artifacts read (Crew Preview): the evidence rows of the build under way, the artifacts those rows name,
//! and the change the checks read. The rows carry no command, output, `proves` text or Evidence id, and a task with no
//! checkout still answers.

use orbyn_contracts::{CheckKind, CheckOutcome};
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
    assert!(
        view.checks
            .iter()
            .any(|row| row.outcome == CheckOutcome::Pass),
        "a passed build has a check that passed"
    );
    for row in &view.checks {
        assert_eq!(row.task, subject);
        assert!(
            matches!(
                row.outcome,
                CheckOutcome::Pass | CheckOutcome::Skipped | CheckOutcome::Fail
            ),
            "a row is one of the outcomes"
        );
        if row.outcome == CheckOutcome::Pass && row.kind == Some(CheckKind::Command) {
            assert_eq!(
                row.exit_code,
                Some(0),
                "a command check that passed exited 0"
            );
        }
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
    assert!(
        !text.contains("\"command\":"),
        "no row carries the command it ran; a kind may be named command"
    );
    for row in w.core.store.crew_evidence(&subject).unwrap() {
        assert!(
            !text.contains(row.id.as_str()),
            "no Evidence id leaves the core"
        );
    }
}

#[tokio::test]
async fn a_task_with_no_run_answers_no_checks_and_no_checkout() {
    // A task stored with its brief and key and no run at all: nothing has been checked, so there is no checkout to read.
    let w = world(true, arc(|_, _| Turn::Silent)).await;
    let stamper = crate::crew::Stamper {
        crew_id: orbyn_contracts::CrewId::new("crew-1").unwrap(),
        engine: orbyn_contracts::CrewToken::new("claude").unwrap(),
        member: orbyn_contracts::CrewMember::new("lead-01").unwrap(),
        now: orbyn_contracts::CrewTimestamp::new("2026-11-12T10:00:00Z").unwrap(),
    };
    let brief = stamper
        .brief(&json!({
            "kind": "implement",
            "outcome": "Nothing has run.",
            "inputs": [],
            "files": [{ "path": "src/**", "mode": "write" }],
            "allowed_effects": ["read_repo", "write_leased", "run_checks"],
            "stop_when": ["Done."],
            "budget": { "turns": 20, "minutes": 30 }
        }))
        .unwrap();
    let key = stamper
        .answer_key(
            &json!({
                "checks": [{ "id": "chk-run", "kind": "command", "params": { "run": "true" }, "proves": "It runs." }],
                "protected_paths": ["tests/**"]
            }),
            &brief.id,
        )
        .unwrap();
    let task = w
        .core
        .store
        .create_crew_task(&brief, &key, "run-none")
        .unwrap();

    let view = w.core.crew_flow_task_artifacts(&task.id).await.unwrap();
    assert!(view.checks.is_empty(), "no check ran");
    assert!(view.diff.is_none());
    assert_eq!(view.diff_absent.as_deref(), Some("no_checkout"));
}

#[tokio::test]
async fn the_diff_lists_the_cap_of_a_larger_change_and_leaves_untracked_files_out() {
    let w = world(true, arc(|_, root| built(root))).await;
    let subject = w.built_work().await;
    // 205 tracked files, more than the 200 the list holds, and one more file left untracked.
    let bulk = w.repo.join("src").join("bulk");
    std::fs::create_dir_all(&bulk).unwrap();
    for n in 0..205u32 {
        std::fs::write(bulk.join(format!("{n:03}.txt")), "bulk\n").unwrap();
    }
    commit_all(&w.repo, "the bulk of the change");
    std::fs::write(w.repo.join("src").join("untracked.txt"), "not added\n").unwrap();

    let view = w.core.crew_flow_task_artifacts(&subject).await.unwrap();
    let diff = view.diff.expect("the change the checks read");
    assert_eq!(diff.files.len(), 200, "the list is cut at its cap");
    assert!(diff.truncated, "the cut is said");
    assert!(diff.files_total > 200, "the total counts every file");
    assert_eq!(
        diff.files_total as usize, 206,
        "205 files and the builder's"
    );
    assert!(
        !diff
            .files
            .iter()
            .any(|file| file.path.ends_with("untracked.txt")),
        "a file the task left untracked is not one git tracks, so it is not listed"
    );
}

#[tokio::test]
async fn a_build_with_more_checks_than_the_cap_shows_the_cap() {
    // 66 checks: two more than the 64 rows the read holds.
    let checks: Vec<Value> = (0..66u32)
        .map(|n| {
            json!({
                "id": format!("chk-{n:02}"),
                "kind": "command",
                "params": { "run": format!("echo check-{n}") },
                "proves": format!("Check {n} runs.")
            })
        })
        .collect();
    let (mut key, _, _) = artifact_key();
    key["checks"] = Value::Array(checks);
    let w = world(true, arc(|_, root| built(root))).await;
    let subject = w.built_with(key).await;

    let view = w.core.crew_flow_task_artifacts(&subject).await.unwrap();
    assert_eq!(view.checks.len(), 64, "the rows are cut at their cap");
    assert!(view.truncated, "the cut is said");
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
