//! Watch on main and the bisect revert, on a fake history in a temporary repository: merge commits made with real git, the crew's
//! merge rows written beside them, and a check that fails once a merge rewrites `src/state.txt`. Nothing leaves the repository.

use std::path::Path;

use serde_json::json;

use super::autopilot_merge::{gitq, main_branch};
use super::autopilot_support::kinds;
use super::harness::{Turn, World, brief, key, world};
use super::review_support::commit_all;
use crate::crew::{MergeTarget, WatchOutcome};
use crate::store::MergeRecord;
use crate::testing::run_git;

pub(super) const NOW: i64 = 1_800_000_000;
const IDENTITY: [&str; 4] = ["-c", "user.name=t", "-c", "user.email=t@example.test"];

fn git_id(repo: &Path, args: &[&str]) {
    let mut all = IDENTITY.to_vec();
    all.extend_from_slice(args);
    run_git(repo, &all);
}

/// One merge on `main`: a side branch with `files`, merged with a merge commit.
fn merge(repo: &Path, main: &str, name: &str, files: &[(&str, &str)]) {
    run_git(repo, &["checkout", "-q", "-b", name, main]);
    for (path, text) in files {
        std::fs::write(repo.join(path), text).unwrap();
    }
    commit_all(repo, name);
    run_git(repo, &["checkout", "-q", main]);
    git_id(
        repo,
        &[
            "merge",
            "--no-ff",
            "-q",
            "-m",
            &format!("merge {name}"),
            name,
        ],
    );
}

pub(super) struct Watched {
    pub(super) w: World,
    pub(super) main: String,
    pub(super) target: MergeTarget,
    pub(super) tasks: Vec<String>,
}

/// A history of four merges, `edits[i]` the files merge `i + 1` writes. The checks of `.orbyn/crew.json` pass while
/// `src/state.txt` says GOOD. Each merge has a task and a row, as the queue would have written them.
async fn history_with(rules: &str, edits: [&[(&'static str, &'static str)]; 4]) -> Watched {
    let w = world(true, std::sync::Arc::new(|_, _| Turn::Hang)).await;
    let main = main_branch(&w.repo).await;
    std::fs::create_dir_all(w.repo.join(".orbyn")).unwrap();
    std::fs::write(w.repo.join(".orbyn/crew.json"), rules).unwrap();
    std::fs::write(w.repo.join("src/state.txt"), "GOOD\n").unwrap();
    commit_all(&w.repo, "state");
    let base = gitq(&w.repo, &["rev-parse", "HEAD"]).await;
    for (i, files) in edits.iter().enumerate() {
        merge(&w.repo, &main, &format!("f{}", i + 1), files);
    }
    let merges: Vec<String> = gitq(
        &w.repo,
        &[
            "rev-list",
            "--first-parent",
            "--reverse",
            &format!("{base}..{main}"),
        ],
    )
    .await
    .lines()
    .map(str::to_owned)
    .collect();
    assert_eq!(merges.len(), 4);
    // The queue moves a branch no one has checked out.
    run_git(&w.repo, &["checkout", "-q", "-b", "parking"]);

    let mut tasks = Vec::new();
    for i in 1..=4 {
        let (answer, error) = w.start(brief(json!({ "files": [{ "path": format!("src/t{i}/**"), "mode": "write" }], "outcome": format!("Task {i}.") })), key()).await;
        assert!(!error, "{answer}");
        tasks.push(answer["task"].as_str().unwrap().to_owned());
    }
    for (i, (task, commit)) in tasks.iter().zip(&merges).enumerate() {
        let before = gitq(&w.repo, &["rev-parse", &format!("{commit}^1")]).await;
        let record = MergeRecord {
            id: 0,
            task_id: task.clone(),
            crew_id: w.task(task).crew_id,
            repo: w.repo.to_string_lossy().into_owned(),
            branch: main.clone(),
            before_commit: before,
            merge_commit: commit.clone(),
            candidate: commit.clone(),
            risk: "medium".into(),
            reasons: json!(["ordinary feature work"]),
            areas: vec!["src/x".into()],
            engine: "alpha".into(),
            mode: "full".into(),
            files: 1,
            lines: 1,
            proof: json!({}),
            not_verified: json!([]),
            merged_unix: NOW + i64::try_from(i).unwrap(),
            due_unix: None,
            review_state: "waiting".into(),
            comment: None,
            watch: "unknown".into(),
            reverted_commit: None,
            needs_person: None,
        };
        w.core.store.add_crew_merge(&record).unwrap();
    }
    let target = MergeTarget {
        repo: w.repo.clone(),
        branch: main.clone(),
        owed: Some(Vec::new()),
    };
    Watched {
        w,
        main,
        target,
        tasks,
    }
}

const RULES: &str = r#"{"checks":[{"run":"git grep --no-index -q GOOD -- src/state.txt"}]}"#;

pub(super) async fn history(edits: [&[(&'static str, &'static str)]; 4]) -> Watched {
    history_with(RULES, edits).await
}

impl Watched {
    async fn tip(&self) -> String {
        gitq(&self.w.repo, &["rev-parse", &self.main]).await
    }

    async fn state(&self) -> String {
        gitq(
            &self.w.repo,
            &["show", &format!("{}:src/state.txt", self.main)],
        )
        .await
    }

    fn row(&self, i: usize) -> MergeRecord {
        self.w
            .core
            .store
            .crew_merge(&self.tasks[i])
            .unwrap()
            .unwrap()
    }
}

/// Slice 4f's last "done when": main breaks at the third merge, and the watch bisects to that merge and reverts it.
#[tokio::test]
async fn a_broken_main_bisects_to_the_right_merge_and_reverts_it() {
    let h = history([
        &[("src/a.txt", "a\n")],
        &[("src/b.txt", "b\n")],
        &[("src/state.txt", "BAD\n")],
        &[("src/c.txt", "c\n")],
    ])
    .await;
    assert_eq!(h.state().await, "BAD");
    let before = h.tip().await;
    let outcome =
        h.w.core
            .crew_watch_main(&h.target, NOW + 100)
            .await
            .unwrap();
    let WatchOutcome::Reverted {
        task_id,
        revert_commit,
    } = outcome
    else {
        panic!("{outcome:?}")
    };
    assert_eq!(task_id, h.tasks[2], "the third merge broke it");
    assert_eq!(h.tip().await, revert_commit);
    assert_eq!(h.state().await, "GOOD");
    // One new commit on the first-parent line, the revert; the other merges stay.
    assert_eq!(
        gitq(
            &h.w.repo,
            &[
                "rev-list",
                "--first-parent",
                "--count",
                &format!("{before}..{}", h.main)
            ]
        )
        .await,
        "1"
    );
    assert_eq!(
        gitq(&h.w.repo, &["show", &format!("{}:src/c.txt", h.main)]).await,
        "c"
    );
    assert_eq!(
        gitq(&h.w.repo, &["show", &format!("{}:src/a.txt", h.main)]).await,
        "a"
    );
    // The rows: the cause is reverted, the merges before it are green, the one after waits for the next watch.
    let culprit = h.row(2);
    assert_eq!(
        (
            culprit.review_state.as_str(),
            culprit.reverted_commit.as_deref()
        ),
        ("reverted", Some(revert_commit.as_str()))
    );
    assert_eq!(
        (
            h.row(0).watch.as_str(),
            h.row(1).watch.as_str(),
            h.row(3).watch.as_str()
        ),
        ("green", "green", "unknown")
    );
    // The person is told, and it is in the ledger.
    let told = kinds(&h.w, &h.tasks[2]);
    assert!(
        told.contains(&"reverted".to_owned()) && told.contains(&"person_told".to_owned()),
        "{told:?}"
    );
    // And the next watch finds main healthy.
    let next =
        h.w.core
            .crew_watch_main(&h.target, NOW + 200)
            .await
            .unwrap();
    assert_eq!(next, WatchOutcome::Healthy { cleared: 1 });
    assert_eq!(h.row(3).watch, "green");
}

#[tokio::test]
async fn a_later_merge_that_depends_on_the_cause_is_listed_and_nothing_is_reverted() {
    // The fourth merge edits the same file, so the third cannot be taken out cleanly.
    let h = history([
        &[("src/a.txt", "a\n")],
        &[("src/b.txt", "b\n")],
        &[("src/state.txt", "BAD\n")],
        &[("src/state.txt", "STILL BAD\n")],
    ])
    .await;
    let before = h.tip().await;
    let outcome =
        h.w.core
            .crew_watch_main(&h.target, NOW + 100)
            .await
            .unwrap();
    let WatchOutcome::NeedsPerson { culprit, .. } = &outcome else {
        panic!("{outcome:?}")
    };
    assert_eq!(culprit.as_deref(), Some(h.tasks[2].as_str()));
    assert_eq!(h.tip().await, before, "the branch moved without certainty");
    let row = h.row(2);
    assert_eq!(row.watch, "broken");
    assert!(
        row.needs_person
            .as_deref()
            .is_some_and(|why| why.contains("same lines")),
        "{:?}",
        row.needs_person
    );
    // It tops the Review queue, even under Full.
    assert_eq!(
        h.w.core.crew_review_queue(NOW + 200).unwrap()[0].task_id,
        h.tasks[2]
    );
    assert!(kinds(&h.w, &h.tasks[2]).contains(&"main_broken".to_owned()));
}

#[tokio::test]
async fn a_break_that_is_not_a_crew_merge_is_never_reverted() {
    let h = history([
        &[("src/a.txt", "a\n")],
        &[("src/b.txt", "b\n")],
        &[("src/c.txt", "c\n")],
        &[("src/d.txt", "d\n")],
    ])
    .await;
    // Someone commits to the branch directly, with a break, on top of the crew's merges.
    run_git(&h.w.repo, &["checkout", "-q", &h.main]);
    std::fs::write(h.w.repo.join("src/state.txt"), "BAD\n").unwrap();
    commit_all(&h.w.repo, "a direct commit");
    run_git(&h.w.repo, &["checkout", "-q", "parking"]);
    let before = h.tip().await;
    let outcome =
        h.w.core
            .crew_watch_main(&h.target, NOW + 100)
            .await
            .unwrap();
    assert!(
        matches!(&outcome, WatchOutcome::NeedsPerson { why, culprit: None, .. } if why.contains("not a merge the crew made")),
        "{outcome:?}"
    );
    assert_eq!(h.tip().await, before);
}

#[tokio::test]
async fn a_healthy_main_clears_the_merges_and_a_repository_with_no_checks_is_not_called_healthy() {
    let h = history([
        &[("src/a.txt", "a\n")],
        &[("src/b.txt", "b\n")],
        &[("src/c.txt", "c\n")],
        &[("src/d.txt", "d\n")],
    ])
    .await;
    let outcome =
        h.w.core
            .crew_watch_main(&h.target, NOW + 100)
            .await
            .unwrap();
    assert_eq!(outcome, WatchOutcome::Healthy { cleared: 4 });
    assert!((0..4).all(|i| h.row(i).watch == "green"));
    assert_eq!(
        h.w.core
            .crew_watch_main(&h.target, NOW + 200)
            .await
            .unwrap(),
        WatchOutcome::NothingToWatch
    );
}

#[tokio::test]
async fn a_repository_with_no_checks_is_not_called_healthy() {
    let h = history_with(
        "{}",
        [
            &[("src/a.txt", "a\n")],
            &[("src/b.txt", "b\n")],
            &[("src/c.txt", "c\n")],
            &[("src/d.txt", "d\n")],
        ],
    )
    .await;
    assert_eq!(
        h.w.core
            .crew_watch_main(&h.target, NOW + 100)
            .await
            .unwrap(),
        WatchOutcome::NoChecks
    );
    assert!((0..4).all(|i| h.row(i).watch == "unknown"));
}
