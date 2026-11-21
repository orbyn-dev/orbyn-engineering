//! The merge queue's conflict forecast, on a real repository: a dry-run merge of each queued candidate onto the branch says
//! whether that merge would conflict now. The fixtures branch, commit and advance the branch with real git; nothing is pushed.

use std::sync::Arc;

use serde_json::json;

use super::autopilot_merge::{gitq, main_branch};
use super::harness::{Turn, World, brief, key, world};
use super::review_support::commit_all;
use crate::crew::Page;
use crate::store::MergeEntry;
use crate::testing::run_git;

const NOW: i64 = 1_800_000_000;

fn page() -> Page {
    Page::new(None, None).unwrap()
}

/// A world with its first commit on the branch, and the branch's name. The worker never runs a turn.
async fn world_with_base() -> (World, String) {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let main = main_branch(&w.repo).await;
    std::fs::write(w.repo.join("shared.txt"), "base\n").unwrap();
    commit_all(&w.repo, "base");
    (w, main)
}

/// A candidate branch from `main`'s tip that writes `files`, then back on `main`. Its commit is the candidate.
async fn candidate(w: &World, main: &str, name: &str, files: &[(&str, &str)]) -> String {
    run_git(&w.repo, &["checkout", "-q", "-b", name, main]);
    for (path, text) in files {
        std::fs::write(w.repo.join(path), text).unwrap();
    }
    commit_all(&w.repo, name);
    let commit = gitq(&w.repo, &["rev-parse", "HEAD"]).await;
    run_git(&w.repo, &["checkout", "-q", main]);
    commit
}

/// A running task the queue can name: its id and its crew's, as a real crew start makes them.
async fn started(w: &World, n: usize) -> (String, String) {
    let files = json!([{ "path": format!("src/fixture-{n}/**"), "mode": "write" }]);
    let (answer, error) = w
        .start(
            brief(json!({ "files": files, "outcome": format!("Task {n}.") })),
            key(),
        )
        .await;
    assert!(!error, "{answer}");
    let task = answer["task"].as_str().unwrap().to_owned();
    let crew = w.task(&task).crew_id;
    (task, crew)
}

fn entry(w: &World, main: &str, task: &str, crew: &str, candidate: &str, age: i64) {
    w.core
        .store
        .add_merge_entry(&MergeEntry {
            task_id: task.to_owned(),
            crew_id: crew.to_owned(),
            repo: w.repo.to_string_lossy().into_owned(),
            branch: main.to_owned(),
            candidate: candidate.to_owned(),
            state: "queued".to_owned(),
            risk: "low".to_owned(),
            reasons: json!(["ordinary feature work"]),
            areas: vec!["shared".to_owned()],
            samples: Vec::new(),
            engine: "alpha".to_owned(),
            files: 1,
            lines: 1,
            mode: "full".to_owned(),
            note: None,
            released: false,
            queued_unix: NOW + age,
        })
        .unwrap();
}

/// Two candidates that each rewrite the file the branch also rewrote: merging either now would conflict.
#[tokio::test]
async fn queued_candidates_that_would_conflict_are_forecast() {
    let (w, main) = world_with_base().await;
    let first = candidate(&w, &main, "work-1", &[("shared.txt", "first\n")]).await;
    let second = candidate(&w, &main, "work-2", &[("shared.txt", "second\n")]).await;
    std::fs::write(w.repo.join("shared.txt"), "branch\n").unwrap();
    commit_all(&w.repo, "the branch moved on");
    let (first_task, first_crew) = started(&w, 1).await;
    let (second_task, second_crew) = started(&w, 2).await;
    entry(&w, &main, &first_task, &first_crew, &first, 1);
    entry(&w, &main, &second_task, &second_crew, &second, 2);

    let queue = w.core.autopilot_merge_queue(page()).await.unwrap();
    assert!(
        queue.forecast.conflict_forecast,
        "both would conflict: {:?}",
        queue.forecast
    );
    assert_eq!(queue.forecast.checked, 2, "{:?}", queue.forecast);
    let tasks: Vec<&str> = queue
        .forecast
        .conflicts
        .iter()
        .map(|c| c.task_id.as_str())
        .collect();
    assert_eq!(tasks, [first_task.as_str(), second_task.as_str()]);
    assert_eq!(queue.forecast.conflicts[0].paths, ["shared.txt"]);
}

/// Two candidates that touch a file the branch did not: merging each now is clean, so the forecast says so.
#[tokio::test]
async fn clean_queued_candidates_are_not_forecast_as_conflicting() {
    let (w, main) = world_with_base().await;
    let first = candidate(&w, &main, "work-1", &[("one.txt", "one\n")]).await;
    let second = candidate(&w, &main, "work-2", &[("two.txt", "two\n")]).await;
    std::fs::write(w.repo.join("shared.txt"), "branch\n").unwrap();
    commit_all(&w.repo, "the branch moved on");
    let (first_task, first_crew) = started(&w, 1).await;
    let (second_task, second_crew) = started(&w, 2).await;
    entry(&w, &main, &first_task, &first_crew, &first, 1);
    entry(&w, &main, &second_task, &second_crew, &second, 2);

    let queue = w.core.autopilot_merge_queue(page()).await.unwrap();
    assert_eq!(queue.forecast.checked, 2, "{:?}", queue.forecast);
    assert!(!queue.forecast.conflict_forecast, "{:?}", queue.forecast);
    assert!(
        queue.forecast.conflicts.is_empty(),
        "{:?}",
        queue.forecast.conflicts
    );
}

/// A repository that configures a custom merge driver is not dry-run: the driver would run a program of the repository's choosing
/// on a read, so its candidates are left out of the forecast, never guessed at.
#[tokio::test]
async fn a_repository_with_a_merge_driver_is_not_dry_run() {
    let (w, main) = world_with_base().await;
    let first = candidate(&w, &main, "work-1", &[("shared.txt", "first\n")]).await;
    std::fs::write(w.repo.join("shared.txt"), "branch\n").unwrap();
    commit_all(&w.repo, "the branch moved on");
    run_git(&w.repo, &["config", "merge.custom.driver", "true"]);
    let (task, crew) = started(&w, 1).await;
    entry(&w, &main, &task, &crew, &first, 1);

    let queue = w.core.autopilot_merge_queue(page()).await.unwrap();
    assert_eq!(queue.forecast.checked, 0, "{:?}", queue.forecast);
    assert!(!queue.forecast.conflict_forecast, "{:?}", queue.forecast);
}
