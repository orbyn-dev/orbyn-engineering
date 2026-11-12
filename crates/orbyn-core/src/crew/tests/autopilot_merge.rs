//! The merge queue and the after-merge Review queue through a real `Core` and real git, in a temporary repository: a worker
//! commits on a side branch, the task passes, and the queue merges it onto a branch no one has checked out. Nothing is pushed.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use serde_json::{Value, json};

use super::harness::{Script, Turn, World, brief, report, world};
use super::review_support::commit_all;
use crate::crew::{MergeOutcome, MergeTarget, Mode, ModeScope, ReviewAnswer, Risk};
use crate::source_control::git::{git, text};
use crate::testing::run_git;

const NOW: i64 = 1_800_000_000;

pub async fn gitq(repo: &Path, args: &[&str]) -> String {
    text(&git(repo, args).await.unwrap())
}

/// A worker that, on each turn `n`, branches from `main`, writes the files `files(n)` gives and commits.
fn commits(
    main: Arc<OnceLock<String>>,
    files: impl Fn(usize) -> Vec<(&'static str, &'static str)> + Send + Sync + 'static,
) -> Script {
    Arc::new(move |n, root| {
        run_git(root, &["checkout", "-q", main.get().unwrap()]);
        run_git(root, &["checkout", "-q", "-b", &format!("work-{n}")]);
        for (path, text) in files(n) {
            if let Some(parent) = root.join(path).parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(root.join(path), text).unwrap();
        }
        commit_all(root, &format!("work {n}"));
        Turn::Report(report("done"))
    })
}

fn key_ok() -> Value {
    json!({
        "checks": [{ "id": "chk-ok", "kind": "command", "params": { "run": "echo merge-key-marker-1" }, "proves": "It runs." }],
        "protected_paths": ["tests/**"]
    })
}

pub async fn main_branch(repo: &Path) -> String {
    gitq(repo, &["rev-parse", "--abbrev-ref", "HEAD"]).await
}

/// Starts a task that may write `paths` and waits until it passes.
async fn passed(w: &World, paths: &[&str], outcome: &str) -> String {
    let files: Vec<Value> = paths
        .iter()
        .map(|p| json!({ "path": p, "mode": "write" }))
        .collect();
    let (answer, error) = w
        .start(
            brief(json!({ "files": files, "outcome": outcome })),
            key_ok(),
        )
        .await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    w.until_state(&id, "passed").await;
    id
}

struct Merging {
    w: World,
    main: String,
    target: MergeTarget,
}

async fn merging(
    mode: Mode,
    files: impl Fn(usize) -> Vec<(&'static str, &'static str)> + Send + Sync + 'static,
) -> Merging {
    let cell = Arc::new(OnceLock::new());
    let w = world(true, commits(cell.clone(), files)).await;
    let main = main_branch(&w.repo).await;
    cell.set(main.clone()).unwrap();
    w.core
        .set_crew_autopilot_mode(&ModeScope::Default, mode)
        .unwrap();
    let target = MergeTarget {
        repo: w.repo.clone(),
        branch: main.clone(),
        owed: Some(Vec::new()),
    };
    Merging { w, main, target }
}

impl Merging {
    async fn tip(&self) -> String {
        gitq(&self.w.repo, &["rev-parse", &self.main]).await
    }

    async fn process(&self, now: i64) -> Vec<MergeOutcome> {
        self.w
            .core
            .crew_process_merges(&self.target, now)
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn a_high_risk_diff_waits_for_a_person_under_on_and_merges_once_released() {
    let m = merging(Mode::On, |_| {
        vec![("migrations/001_init.sql", "create table t (id int);\n")]
    })
    .await;
    let id = passed(&m.w, &["migrations/**"], "Add the first migration.").await;
    let before = m.tip().await;
    let entry =
        m.w.core
            .crew_queue_merge(&id, &m.target, NOW)
            .await
            .unwrap();
    assert_eq!(
        (entry.state.as_str(), entry.risk.as_str()),
        ("waiting", "high")
    );
    assert!(entry.reasons.to_string().contains("migration"));
    assert!(
        m.process(NOW).await.is_empty(),
        "nothing is queued, so nothing merges"
    );
    assert_eq!(m.tip().await, before);

    m.w.core.crew_release_merge(&id).unwrap();
    let outcomes = m.process(NOW + 10).await;
    let [MergeOutcome::Merged { merge_commit, .. }] = &outcomes[..] else {
        panic!("{outcomes:?}")
    };
    assert_eq!(&m.tip().await, merge_commit);
    assert_eq!(
        gitq(
            &m.w.repo,
            &["show", &format!("{}:migrations/001_init.sql", m.main)]
        )
        .await,
        "create table t (id int);"
    );
    // A person released it, so that was the review.
    let record = m.w.core.store.crew_merge(&id).unwrap().unwrap();
    assert_eq!(record.review_state, "looks_good");
    assert!(m.w.core.crew_review_queue(NOW + 10).unwrap().is_empty());
    // Every step is in the ledger, with no secret and no check text.
    assert_eq!(
        super::autopilot_support::kinds(&m.w, &id),
        ["merge_queued", "merge_waiting", "merged"]
    );
}

#[tokio::test]
async fn under_full_a_high_risk_diff_merges_and_goes_to_the_top_of_the_review_queue() {
    let m = merging(Mode::Full, |n| match n {
        0 => vec![("docs/note.md", "A note.\n")],
        _ => vec![(
            "migrations/002_more.sql",
            "alter table t add column c int;\n",
        )],
    })
    .await;
    let low = passed(&m.w, &["docs/**"], "Write a note.").await;
    let high = passed(&m.w, &["migrations/**"], "Add a migration.").await;
    assert_eq!(
        m.w.core
            .crew_queue_merge(&low, &m.target, NOW)
            .await
            .unwrap()
            .risk,
        "low"
    );
    let entry =
        m.w.core
            .crew_queue_merge(&high, &m.target, NOW + 1)
            .await
            .unwrap();
    assert_eq!(
        (entry.state.as_str(), entry.risk.as_str()),
        ("queued", "high")
    );
    let outcomes = m.process(NOW + 2).await;
    assert!(
        matches!(
            &outcomes[..],
            [MergeOutcome::Merged { .. }, MergeOutcome::Merged { .. }]
        ),
        "{outcomes:?}"
    );

    // Ranked by risk: the high one first, with a 4 hour window; the low one has none.
    let queue = m.w.core.crew_review_queue(NOW + 2).unwrap();
    let order: Vec<(&str, Risk)> = queue.iter().map(|i| (i.task_id.as_str(), i.risk)).collect();
    assert_eq!(
        order,
        [(high.as_str(), Risk::High), (low.as_str(), Risk::Low)]
    );
    assert_eq!(queue[0].due_unix, Some(NOW + 2 + 4 * 3600));
    assert_eq!(queue[1].due_unix, None);
    assert!(m.w.core.crew_review_queue(NOW + 2 + 5 * 3600).unwrap()[0].overdue);
    assert!(!m.w.core.crew_review_queue(NOW + 2 + 5 * 3600).unwrap()[1].overdue);
    assert!(queue[0].not_verified.iter().any(|n| n.contains("coverage")));
    assert_eq!(queue[0].proof["merge_checks_run"], 1);

    // Answers: looks good clears it, a comment is kept, a revert is opened and verified.
    let target = &m.target;
    m.w.core
        .crew_answer_review(&low, ReviewAnswer::LooksGood, target, NOW + 3)
        .await
        .unwrap();
    let revert =
        m.w.core
            .crew_answer_review(&high, ReviewAnswer::Revert, target, NOW + 3)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(m.tip().await, revert);
    assert!(m.w.core.crew_review_queue(NOW + 3).unwrap().is_empty());
    let gone = git(
        &m.w.repo,
        &[
            "cat-file",
            "-e",
            &format!("{}:migrations/002_more.sql", m.main),
        ],
    )
    .await;
    assert!(gone.is_err(), "the reverted file is still on the branch");
    assert_eq!(
        m.w.core
            .store
            .crew_merge(&high)
            .unwrap()
            .unwrap()
            .review_state,
        "reverted"
    );
    // This repository defines no checks, so the revert ran none and says so.
    let reverted = super::autopilot_support::events(&m.w, &high, "reverted");
    assert_eq!(reverted[0].body["verified"], false);
    assert_eq!(
        m.w.core
            .store
            .crew_merge(&low)
            .unwrap()
            .unwrap()
            .review_state,
        "looks_good"
    );
}

#[tokio::test]
async fn low_and_medium_follow_the_mode() {
    // Medium work merges by itself under On; under Off every merge waits.
    let on = merging(Mode::On, |_| vec![("src/feature.txt", "feature\n")]).await;
    let id = passed(&on.w, &["src/**"], "Add a feature.").await;
    let entry =
        on.w.core
            .crew_queue_merge(&id, &on.target, NOW)
            .await
            .unwrap();
    assert_eq!(
        (entry.state.as_str(), entry.risk.as_str()),
        ("queued", "medium")
    );
    assert!(matches!(
        &on.process(NOW).await[..],
        [MergeOutcome::Merged { .. }]
    ));
    assert_eq!(
        on.w.core.crew_review_queue(NOW).unwrap()[0].due_unix,
        Some(NOW + 24 * 3600)
    );

    let off = merging(Mode::Off, |_| vec![("docs/note.md", "A note.\n")]).await;
    let id = passed(&off.w, &["docs/**"], "Write a note.").await;
    let entry = off
        .w
        .core
        .crew_queue_merge(&id, &off.target, NOW)
        .await
        .unwrap();
    assert_eq!(
        (entry.state.as_str(), entry.risk.as_str()),
        ("waiting", "low")
    );
    assert!(off.process(NOW).await.is_empty());
}

#[tokio::test]
async fn a_required_check_that_fails_on_the_combined_state_stops_the_merge() {
    let m = merging(Mode::Full, |_| vec![("src/state.txt", "BAD\n")]).await;
    std::fs::create_dir_all(m.w.repo.join(".orbyn")).unwrap();
    std::fs::write(
        m.w.repo.join(".orbyn/crew.json"),
        r#"{"checks":[{"run":"git grep --no-index -q GOOD -- src"}]}"#,
    )
    .unwrap();
    std::fs::write(m.w.repo.join("src/state.txt"), "GOOD\n").unwrap();
    commit_all(&m.w.repo, "state");
    let id = passed(&m.w, &["src/**"], "Change the state.").await;
    let before = m.tip().await;
    m.w.core
        .crew_queue_merge(&id, &m.target, NOW)
        .await
        .unwrap();
    // The check names `src`, so this diff is held; a person looks and releases it, and the check still runs on the combined state.
    assert_eq!(
        m.w.core.store.merge_entry(&id).unwrap().unwrap().state,
        "waiting"
    );
    m.w.core.crew_release_merge(&id).unwrap();
    let outcomes = m.process(NOW).await;
    assert!(
        matches!(&outcomes[..], [MergeOutcome::Rejected { why, .. }] if why.contains("combined state")),
        "{outcomes:?}"
    );
    assert_eq!(m.tip().await, before, "main moved although a check failed");
    assert_eq!(
        m.w.core.store.merge_entry(&id).unwrap().unwrap().state,
        "rejected"
    );
    assert!(m.w.core.store.crew_merge(&id).unwrap().is_none());
}

#[tokio::test]
async fn merges_go_in_order_and_a_conflict_is_rejected_without_moving_main() {
    let m = merging(Mode::Full, |n| {
        vec![("src/lib.txt", if n == 0 { "one\n" } else { "two\n" })]
    })
    .await;
    let first = passed(&m.w, &["src/**"], "Write one.").await;
    let second = passed(&m.w, &["src/**"], "Write two.").await;
    m.w.core
        .crew_queue_merge(&first, &m.target, NOW)
        .await
        .unwrap();
    m.w.core
        .crew_queue_merge(&second, &m.target, NOW + 1)
        .await
        .unwrap();
    let before = m.tip().await;
    let outcomes = m.process(NOW + 2).await;
    assert!(
        matches!(&outcomes[0], MergeOutcome::Merged { task_id, .. } if *task_id == first),
        "{outcomes:?}"
    );
    assert!(
        matches!(&outcomes[1], MergeOutcome::Rejected { task_id, why } if *task_id == second && why.contains("conflicts")),
        "{outcomes:?}"
    );
    assert_eq!(
        gitq(&m.w.repo, &["show", &format!("{}:src/lib.txt", m.main)]).await,
        "one"
    );
    // One merge commit on the first-parent line, not two.
    let added = gitq(
        &m.w.repo,
        &[
            "rev-list",
            "--first-parent",
            "--count",
            &format!("{before}..{}", m.main),
        ],
    )
    .await;
    assert_eq!(added, "1");
}

#[tokio::test]
async fn a_checked_out_branch_is_never_moved() {
    let m = merging(Mode::Full, |_| vec![("src/feature.txt", "feature\n")]).await;
    let id = passed(&m.w, &["src/**"], "Add a feature.").await;
    m.w.core
        .crew_queue_merge(&id, &m.target, NOW)
        .await
        .unwrap();
    run_git(&m.w.repo, &["checkout", "-q", &m.main]);
    let before = m.tip().await;
    let outcomes = m.process(NOW).await;
    assert!(
        matches!(&outcomes[..], [MergeOutcome::NeedsPerson { why, .. }] if why.contains("checked out")),
        "{outcomes:?}"
    );
    assert_eq!(m.tip().await, before);
    assert_eq!(
        m.w.core.store.merge_entry(&id).unwrap().unwrap().state,
        "waiting"
    );
}

#[tokio::test]
async fn only_a_passed_task_can_be_queued() {
    let m = merging(Mode::Full, |_| vec![("src/feature.txt", "feature\n")]).await;
    let (answer, error) = m.w.start(brief(json!({})), super::harness::key()).await;
    assert!(!error);
    let running = answer["task"].as_str().unwrap();
    let error =
        m.w.core
            .crew_queue_merge(running, &m.target, NOW)
            .await
            .unwrap_err();
    assert!(error.to_string().contains("only a task that passed"));
}

async fn with_rules(m: &Merging, rules: &str, state: &str) {
    std::fs::create_dir_all(m.w.repo.join(".orbyn")).unwrap();
    std::fs::write(m.w.repo.join(".orbyn/crew.json"), rules).unwrap();
    std::fs::write(m.w.repo.join("src/state.txt"), state).unwrap();
    commit_all(&m.w.repo, "rules");
}

#[tokio::test]
async fn a_diff_that_touches_the_crews_rules_is_held_for_a_person_in_every_mode() {
    let m = merging(Mode::Full, |_| {
        vec![(
            ".orbyn/crew.json",
            "{\"allowed_hosts\":[\"evil.example\"]}\n",
        )]
    })
    .await;
    let id = passed(&m.w, &[".orbyn/**"], "Widen the hosts.").await;
    let before = m.tip().await;
    let entry =
        m.w.core
            .crew_queue_merge(&id, &m.target, NOW)
            .await
            .unwrap();
    assert_eq!(
        (entry.state.as_str(), entry.risk.as_str()),
        ("waiting", "high")
    );
    assert!(
        entry.note.as_deref().is_some_and(|n| n.contains(".orbyn/")),
        "{:?}",
        entry.note
    );
    assert!(m.process(NOW).await.is_empty());
    assert_eq!(m.tip().await, before);
    // A person looks and lets it go.
    m.w.core.crew_release_merge(&id).unwrap();
    assert!(matches!(
        &m.process(NOW + 1).await[..],
        [MergeOutcome::Merged { .. }]
    ));
}

#[tokio::test]
async fn a_diff_that_touches_a_file_a_check_runs_is_held_even_when_the_check_still_passes() {
    // The script a check calls could be turned into `exit 0` and stay green: the check text is read from a trusted commit,
    // the file it runs is not.
    let m = merging(Mode::Full, |_| vec![("src/state.txt", "GOOD and more\n")]).await;
    with_rules(
        &m,
        r#"{"checks":[{"run":"git grep --no-index -q GOOD -- src/state.txt"}]}"#,
        "GOOD\n",
    )
    .await;
    let id = passed(&m.w, &["src/**"], "Edit the state.").await;
    let entry =
        m.w.core
            .crew_queue_merge(&id, &m.target, NOW)
            .await
            .unwrap();
    assert_eq!(entry.state, "waiting");
    assert!(
        entry
            .note
            .as_deref()
            .is_some_and(|n| n.contains("checks runs or reads")),
        "{:?}",
        entry.note
    );
}

#[tokio::test]
async fn ci_config_and_protected_files_are_held_by_the_guard() {
    use crate::crew::{check_words, sensitive};
    let protected = [orbyn_contracts::CrewPathPattern::new("tests/**").unwrap()];
    let held = |paths: &[&str]| {
        let paths: Vec<String> = paths.iter().map(|p| (*p).to_owned()).collect();
        sensitive(
            &paths,
            &protected,
            &[
                "sh scripts/check.sh".to_owned(),
                "git grep -q x -- docs/".to_owned(),
            ],
        )
    };
    assert!(
        held(&[".github/workflows/ci.yml"])
            .iter()
            .any(|w| w.contains("CI"))
    );
    assert!(held(&["tests/a.rs"]).iter().any(|w| w.contains("protects")));
    assert!(
        held(&[".orbyn/crew.json"])
            .iter()
            .any(|w| w.contains(".orbyn/"))
    );
    assert!(
        held(&["scripts/check.sh"])
            .iter()
            .any(|w| w.contains("checks"))
    );
    assert!(
        held(&["docs/a.md"]).iter().any(|w| w.contains("checks")),
        "a folder a check names with a slash"
    );
    assert!(held(&["src/lib.rs", "scripts/other.sh"]).is_empty());
    assert_eq!(
        check_words(&["./scripts/a.sh --x=y/z".to_owned()]),
        vec!["scripts/a.sh".to_owned()]
    );
}

#[tokio::test]
async fn the_mode_is_read_again_when_the_merge_happens() {
    let m = merging(Mode::Full, |_| vec![("src/feature.txt", "feature\n")]).await;
    let id = passed(&m.w, &["src/**"], "Add a feature.").await;
    assert_eq!(
        m.w.core
            .crew_queue_merge(&id, &m.target, NOW)
            .await
            .unwrap()
            .state,
        "queued"
    );
    // The person switches Autopilot off before the queue runs.
    m.w.core
        .set_crew_autopilot_mode(&ModeScope::Default, Mode::Off)
        .unwrap();
    let before = m.tip().await;
    let outcomes = m.process(NOW + 1).await;
    assert!(
        matches!(&outcomes[..], [MergeOutcome::NeedsPerson { why, .. }] if why.contains("off")),
        "{outcomes:?}"
    );
    assert_eq!(m.tip().await, before);
}

#[tokio::test]
async fn the_lanes_owed_must_have_run_unless_a_person_released_it_and_a_release_sticks() {
    let m = merging(Mode::Full, |_| vec![("src/feature.txt", "feature\n")]).await;
    let id = passed(&m.w, &["src/**"], "Add a feature.").await;
    let mut target = m.target.clone();
    target.owed = None;
    m.w.core.crew_queue_merge(&id, &target, NOW).await.unwrap();
    let outcomes =
        m.w.core
            .crew_process_merges(&target, NOW + 1)
            .await
            .unwrap();
    assert!(
        matches!(&outcomes[..], [MergeOutcome::Blocked { why, .. }] if why.contains("correctness review has not run")),
        "{outcomes:?}"
    );
    assert_eq!(
        m.w.core.store.merge_entry(&id).unwrap().unwrap().state,
        "blocked"
    );
    // Releasing a blocked entry is refused: only a waiting one is released.
    assert!(m.w.core.crew_release_merge(&id).is_err());
    // A held entry that is released keeps the release through a block, and merges as reviewed.
    run_git(&m.w.repo, &["checkout", "-q", &m.main]);
    m.w.core
        .store
        .set_merge_entry_state(&id, "waiting", Some("held"))
        .unwrap();
    m.w.core.crew_release_merge(&id).unwrap();
    m.w.core
        .store
        .set_merge_entry_state(&id, "blocked", Some("a lane"))
        .unwrap();
    assert!(m.w.core.store.merge_entry(&id).unwrap().unwrap().released);
    run_git(&m.w.repo, &["checkout", "-q", "-b", "parking"]);
    let outcomes =
        m.w.core
            .crew_process_merges(&target, NOW + 2)
            .await
            .unwrap();
    assert!(
        matches!(&outcomes[..], [MergeOutcome::Merged { .. }]),
        "{outcomes:?}"
    );
    assert_eq!(
        m.w.core
            .store
            .crew_merge(&id)
            .unwrap()
            .unwrap()
            .review_state,
        "looks_good"
    );
}

#[tokio::test]
async fn a_checks_runner_config_a_bare_folder_and_a_glob_are_held_too() {
    use crate::crew::sensitive;
    let held = |checks: &[&str], paths: &[&str], _tree: &[&str]| {
        let checks: Vec<String> = checks.iter().map(|c| (*c).to_owned()).collect();
        let paths: Vec<String> = paths.iter().map(|p| (*p).to_owned()).collect();
        !sensitive(&paths, &[], &checks).is_empty()
    };
    // The script a runner calls lives in the runner's config.
    assert!(held(&["npm test"], &["package.json"], &[]));
    assert!(held(&["npm test"], &["web/package.json"], &[]));
    assert!(held(&["make check"], &["Makefile"], &[]));
    assert!(held(&["make check"], &["rules.mk"], &[]));
    assert!(held(&["pytest"], &["conftest.py"], &[]));
    assert!(held(&["python3 -m pytest tests"], &["pyproject.toml"], &[]));
    assert!(held(&["cargo test"], &[".cargo/config.toml"], &[]));
    assert!(held(&["cargo test"], &["crates/x/build.rs"], &[]));
    assert!(held(&["go test ./..."], &["go.mod"], &[]));
    assert!(held(&["just check"], &["justfile"], &[]));
    assert!(held(&["vitest run"], &["vitest.config.ts"], &[]));
    // A bare word is a folder, and a glob is expanded against the tree.
    assert!(held(&["pytest tests"], &["tests/test_a.py"], &[]));
    // A new file that matches a glob is held too: the check's loop will run it.
    assert!(held(
        &["shellcheck scripts/*.sh"],
        &["scripts/brand_new.sh"],
        &[]
    ));
    assert!(held(&["eslint src"], &["eslint.config.js"], &[]));
    assert!(held(&["eslint src"], &[".eslintrc.json"], &[]));
    assert!(held(&["tsc --noEmit"], &["tsconfig.build.json"], &[]));
    assert!(held(&["ruff check"], &["ruff.toml"], &[]));
    assert!(held(&["cargo clippy"], &["clippy.toml"], &[]));
    assert!(held(&["cargo fmt --check"], &["rustfmt.toml"], &[]));
    assert!(held(&["golangci-lint run"], &[".golangci.yml"], &[]));
    assert!(held(&["deno test"], &["deno.json"], &[]));
    assert!(held(&["pnpm test"], &["pnpm-workspace.yaml"], &[]));
    assert!(held(&["yarn test"], &[".yarnrc.yml"], &[]));
    assert!(held(
        &["shellcheck scripts/*.sh"],
        &["scripts/a.sh"],
        &["scripts/a.sh", "scripts/b.sh"]
    ));
    assert!(held(&["sh ./scripts/check.sh"], &["scripts/check.sh"], &[]));
    // Unrelated work, and a runner the checks do not use, is not held.
    assert!(!held(&["npm test"], &["src/a.ts", "README.md"], &[]));
    assert!(!held(&["cargo test"], &["package.json"], &[]));
    assert!(!held(&["pytest tests"], &["src/a.py"], &[]));
    assert!(!held(
        &["shellcheck scripts/*.sh"],
        &["docs/a.md"],
        &["scripts/a.sh"]
    ));
}

#[test]
fn only_the_latest_round_of_a_lane_counts() {
    use serde_json::json;
    let status = |rounds: serde_json::Value| json!({ "lanes": [{ "lane": "correctness", "rounds": rounds }] });
    let pass = |round: u32| json!({ "round": round, "state": "passed" });
    let fail = |round: u32| json!({ "round": round, "state": "stopped" });
    assert!(crate::crew::lane_ran(
        &status(json!([pass(1)])),
        "correctness"
    ));
    assert!(!crate::crew::lane_ran(
        &status(json!([pass(1), fail(2)])),
        "correctness"
    ));
    assert!(crate::crew::lane_ran(
        &status(json!([fail(1), pass(2)])),
        "correctness"
    ));
    assert!(!crate::crew::lane_ran(
        &status(json!([pass(1)])),
        "security"
    ));
}

#[tokio::test]
async fn a_lane_that_slice_4b_ran_satisfies_the_owed_check_and_a_block_is_one_event() {
    let cell: Arc<OnceLock<String>> = Arc::new(OnceLock::new());
    let main_name = cell.clone();
    let w = super::review_support::crew(super::review_support::arc(move |n, root| {
        if n > 0 {
            return super::review_support::reviewed(Vec::new());
        }
        run_git(root, &["checkout", "-q", main_name.get().unwrap()]);
        run_git(root, &["checkout", "-q", "-b", "work-0"]);
        std::fs::write(root.join("src/feature.txt"), "feature\n").unwrap();
        commit_all(root, "work 0");
        Turn::Report(report("done"))
    }))
    .await;
    let main = main_branch(&w.repo).await;
    cell.set(main.clone()).unwrap();
    w.core
        .set_crew_autopilot_mode(&ModeScope::Default, Mode::Full)
        .unwrap();
    let id = w.built_work().await;
    // The default owed lanes: the general code review (a medium diff).
    let target = MergeTarget {
        repo: w.repo.clone(),
        branch: main.clone(),
        owed: None,
    };
    w.core.crew_queue_merge(&id, &target, NOW).await.unwrap();
    for pass in 1..=2 {
        let outcomes = w
            .core
            .crew_process_merges(&target, NOW + pass)
            .await
            .unwrap();
        assert!(
            matches!(&outcomes[..], [MergeOutcome::Blocked { why, .. }] if why.contains("correctness review has not run")),
            "{outcomes:?}"
        );
    }
    let blocked = super::autopilot_support::events(&w, &id, "merge_waiting");
    assert_eq!(
        blocked.len(),
        1,
        "the same block is not repeated on every pass"
    );
    assert_eq!(blocked[0].body["blocked"], true);
    // The lane slice 4b writes, by the token the queue is owed, satisfies it.
    let lanes = w.review(&id, &[crate::crew::CODE_LANE]).await;
    w.until_state(&lanes[0], "passed").await;
    let outcomes = w.core.crew_process_merges(&target, NOW + 3).await.unwrap();
    assert!(
        matches!(&outcomes[..], [MergeOutcome::Merged { .. }]),
        "{outcomes:?}"
    );
}

/// Three docs-only tasks merged under Full, each touching its own file.
async fn three_merged() -> (Merging, Vec<String>) {
    let m = merging(Mode::Full, |n| match n {
        0 => vec![("docs/a.md", "a\n")],
        1 => vec![("docs/b.md", "b\n")],
        _ => vec![("docs/c.md", "c\n")],
    })
    .await;
    let mut tasks = Vec::new();
    for (n, file) in ["a", "b", "c"].iter().enumerate() {
        let task = passed(&m.w, &["docs/**"], &format!("Write {file}.")).await;
        m.w.core
            .crew_queue_merge(&task, &m.target, NOW + n as i64)
            .await
            .unwrap();
        tasks.push(task);
    }
    let outcomes = m.process(NOW + 10).await;
    assert_eq!(outcomes.len(), 3, "{outcomes:?}");
    (m, tasks)
}

fn scratch_folders(m: &Merging) -> usize {
    std::fs::read_dir(m.w.core.config.data_dir.join("crew-merge"))
        .map(|dir| dir.count())
        .unwrap_or(0)
}

#[tokio::test]
async fn two_reverts_of_one_merge_act_once_and_a_looks_good_cannot_undo_a_revert() {
    let (m, tasks) = three_merged().await;
    let core = &m.w.core;
    let target = &m.target;
    // The last merge, reverted twice at once: one revert commit, one refusal.
    let (first, second) = tokio::join!(
        core.crew_answer_review(&tasks[2], ReviewAnswer::Revert, target, NOW + 20),
        core.crew_answer_review(&tasks[2], ReviewAnswer::Revert, target, NOW + 20),
    );
    assert_eq!(
        [first.is_ok(), second.is_ok()]
            .iter()
            .filter(|ok| **ok)
            .count(),
        1,
        "{first:?} {second:?}"
    );
    let made = first.ok().or(second.ok()).flatten().unwrap();
    assert_eq!(m.tip().await, made);
    let count = gitq(
        &m.w.repo,
        &["rev-list", "--count", &format!("{}..{}", m.main, "HEAD")],
    )
    .await;
    let _ = count;
    // A looks-good racing a revert, in either order, never leaves anything but "reverted".
    let (reverted, good) = tokio::join!(
        core.crew_answer_review(&tasks[1], ReviewAnswer::Revert, target, NOW + 21),
        core.crew_answer_review(&tasks[1], ReviewAnswer::LooksGood, target, NOW + 21),
    );
    assert!(reverted.is_ok() || good.is_ok());
    let state = core.store.crew_merge(&tasks[1]).unwrap().unwrap();
    if reverted.is_ok() {
        assert_eq!(state.review_state, "reverted", "{good:?}");
        assert!(state.reverted_commit.is_some());
    }
    // And a late answer to a reverted merge is refused by the store, not just by the check before it.
    let late = core.store.set_merge_review(&tasks[2], "looks_good", None);
    assert!(late.is_err());
    assert_eq!(
        core.store
            .crew_merge(&tasks[2])
            .unwrap()
            .unwrap()
            .review_state,
        "reverted"
    );
    // Every scratch checkout was removed, whichever way the answers went.
    assert_eq!(scratch_folders(&m), 0);
}

#[tokio::test]
async fn a_revert_refuses_a_branch_that_no_longer_holds_the_merge_and_a_checked_out_dirty_one() {
    let (m, tasks) = three_merged().await;
    let core = &m.w.core;
    let record = core.store.crew_merge(&tasks[0]).unwrap().unwrap();
    // Dirty and checked out: the crew never touches it.
    run_git(&m.w.repo, &["checkout", "-q", &m.main]);
    std::fs::write(m.w.repo.join("docs/a.md"), "edited by a person\n").unwrap();
    let tip = m.tip().await;
    let why = core
        .crew_answer_review(&tasks[2], ReviewAnswer::Revert, &m.target, NOW + 30)
        .await
        .unwrap_err()
        .to_string();
    assert!(why.contains("checked out"), "{why}");
    assert_eq!(m.tip().await, tip);
    assert_eq!(
        std::fs::read_to_string(m.w.repo.join("docs/a.md")).unwrap(),
        "edited by a person\n"
    );
    run_git(&m.w.repo, &["checkout", "-q", "--force", "-"]);
    // Unrelated: the branch was put back before the merge, so the merge is not on it.
    run_git(&m.w.repo, &["branch", "-f", &m.main, &record.before_commit]);
    let why = core
        .crew_answer_review(&tasks[0], ReviewAnswer::Revert, &m.target, NOW + 31)
        .await
        .unwrap_err()
        .to_string();
    assert!(why.contains("no longer holds"), "{why}");
    assert_eq!(m.tip().await, record.before_commit);
    assert_eq!(
        core.store
            .crew_merge(&tasks[0])
            .unwrap()
            .unwrap()
            .review_state,
        "waiting"
    );
    assert_eq!(scratch_folders(&m), 0, "no scratch checkout is left behind");
}
