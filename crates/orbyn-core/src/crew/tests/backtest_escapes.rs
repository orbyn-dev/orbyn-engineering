//! The reviewer backtest's real escape source (slice 4n, after 4k) on a real `Core` and a seeded store: which escapes count, how
//! the pull request and the files are resolved, an escape that cannot be resolved, an unreadable log, an absent log and a read that
//! writes nothing. The fix or revert files come from a fake (`Files`); one case reads a real temporary git repository.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use orbyn_contracts as wire;

use super::autopilot_areas::{DAY, Rig, rig};
use crate::crew::backtest::{
    CommitFiles, EscapeLog, EscapeSource, GitCommitFiles, NotMeasured, ReplayedFinding,
    ScriptedRunner, Window,
};
use crate::util::unix_now;

/// What each commit changed. A commit not listed cannot be read.
#[derive(Default)]
struct Files(BTreeMap<String, Vec<String>>);

impl CommitFiles for Files {
    fn files(&self, _repo: &str, commit: &str) -> Option<Vec<String>> {
        self.0.get(commit).cloned()
    }
}

fn window() -> Window {
    let now = unix_now();
    Window {
        from_unix: now - 180 * DAY,
        until_unix: now - 30 * DAY,
    }
}

fn commit_of(n: u64) -> String {
    format!("{:040x}", 0xfee0_0000 + n)
}

fn pr_of(rig: &Rig, task: &str) -> u64 {
    u64::try_from(rig.w.core.store.crew_merge(task).unwrap().unwrap().id).unwrap()
}

fn confirm(rig: &Rig, task: &str) {
    let core = &rig.w.core;
    let id = core.store.crew_escape_of_merge(task).unwrap().unwrap().id;
    core.crew_escape_confirm(
        &id,
        &wire::EscapeDecide {
            project: rig.project.clone(),
            seen: wire::EscapeSeen {
                state: wire::EscapeState::Possible,
                gap: None,
            },
            note: None,
        },
    )
    .unwrap();
}

fn found_at(rig: &Rig, task: &str, unix: i64) {
    rig.w
        .core
        .store
        .reader()
        .execute(
            "UPDATE crew_escapes SET found_unix = ?1 WHERE merge_task_id = ?2",
            rusqlite::params![unix, task],
        )
        .unwrap();
}

/// A merge made `merged_days` ago and reverted by commit `n`, found `found_days` after the merge. `changed` is what the revert
/// changed; `None` leaves the revert unreadable. A `possible` escape stays possible.
async fn reverted(
    rig: &mut Rig,
    files: &mut Files,
    n: u64,
    merged_days: i64,
    found_days: i64,
    changed: Option<&[&str]>,
    confirmed: bool,
) -> String {
    let merged = unix_now() - merged_days * DAY;
    let task = rig.merge(&["src"], merged).await;
    let commit = commit_of(n);
    rig.w.core.store.set_merge_reverted(&task, &commit).unwrap();
    found_at(rig, &task, merged + found_days * DAY);
    if confirmed {
        confirm(rig, &task);
    }
    if let Some(changed) = changed {
        files
            .0
            .insert(commit, changed.iter().map(|f| (*f).to_owned()).collect());
    }
    task
}

fn log(rig: &Rig, files: Files) -> EscapeLog {
    EscapeLog::new(rig.w.core.store.clone(), Arc::new(files))
}

fn lane(name: &str) -> wire::BacktestLaneInput {
    wire::BacktestLaneInput {
        lane: name.into(),
        engine: "a".into(),
        model: "a-model".into(),
        kind: wire::BacktestLaneKind::Review,
    }
}

fn finding(fp: &str, file: &str) -> ReplayedFinding {
    ReplayedFinding {
        fingerprint: fp.into(),
        files: vec![file.into()],
    }
}

/// Runs a backtest of one lane over `rig`'s merges with the given source and findings per pull request; returns its lane.
async fn run_lane(
    rig: &Rig,
    source: Arc<dyn EscapeSource + Send + Sync>,
    findings: Vec<(u64, ReplayedFinding)>,
) -> wire::BacktestLaneRun {
    let core = &rig.w.core;
    let mut runner = ScriptedRunner::default();
    for (pr, f) in findings {
        runner
            .findings
            .entry(("correctness".into(), "a".into(), pr))
            .or_default()
            .push(f);
    }
    core.crew_backtest_use(Arc::new(runner), Some(source));
    let started = core
        .crew_backtest_start(&wire::BacktestStartInput {
            project: rig.project.clone(),
            lanes: vec![lane("correctness")],
            budgets: vec![wire::BacktestEngineBudget {
                engine: "a".into(),
                tokens: 1000,
            }],
        })
        .await
        .unwrap();
    core.crew_backtest_join().await;
    core.crew_backtest_report(&started.id)
        .unwrap()
        .lanes
        .remove(0)
}

const FILES: [&str; 5] = ["a.rs", "b.rs", "c.rs", "d.rs", "e.rs"];

/// Five confirmed escapes, each fixed in one file, on merges 40 days old; and one possible escape that is not counted.
async fn five_confirmed(rig: &mut Rig, files: &mut Files) -> Vec<String> {
    let mut tasks = Vec::new();
    for (i, file) in FILES.iter().enumerate() {
        let n = i as u64 + 1;
        tasks.push(reverted(rig, files, n, 40, 2, Some(&[file]), true).await);
    }
    reverted(rig, files, 9, 40, 2, Some(&["f.rs"]), false).await;
    tasks
}

#[tokio::test]
async fn known_files_give_caught_unmatched_and_the_rate_from_five_escapes() {
    let mut rig = rig().await;
    let mut files = Files::default();
    let tasks = five_confirmed(&mut rig, &mut files).await;
    let pr = |i: usize| pr_of(&rig, &tasks[i]);
    let lane = run_lane(
        &rig,
        Arc::new(log(&rig, files)),
        vec![
            (pr(0), finding("fp1", "./a.rs")),
            (pr(1), finding("fp2", "z.rs")),
            (pr(2), finding("fp3", "c.rs")),
        ],
    )
    .await;
    assert_eq!(lane.outcome, wire::BacktestLaneOutcome::Scored);
    let score = lane.score.unwrap();
    // The possible escape (PR with f.rs) is not an escape: five counted, two caught.
    assert_eq!((score.escapes_n, score.findings_n), (5, 3));
    let recall = score.recall.unwrap();
    assert_eq!(
        (recall.numerator, recall.denominator, recall.permille),
        (2, 5, 400)
    );
    // The finding on b's PR was about z.rs: unmatched, not wrong. Under 10 findings no precision shows.
    assert_eq!(score.unmatched_n, 1);
    assert_eq!(score.precision, None);
    assert_eq!(lane.unresolved_n, Some(0));
}

#[tokio::test]
async fn below_five_escapes_only_counts_show() {
    let mut rig = rig().await;
    let mut files = Files::default();
    let mut tasks = Vec::new();
    for (i, file) in FILES[..3].iter().enumerate() {
        tasks.push(
            reverted(
                &mut rig,
                &mut files,
                i as u64 + 1,
                40,
                2,
                Some(&[file]),
                true,
            )
            .await,
        );
    }
    let lane = run_lane(
        &rig,
        Arc::new(log(&rig, files)),
        vec![(pr_of(&rig, &tasks[0]), finding("fp1", "a.rs"))],
    )
    .await;
    let score = lane.score.unwrap();
    assert_eq!((score.escapes_n, score.findings_n), (3, 1));
    assert_eq!((score.recall, score.precision), (None, None));
}

#[tokio::test]
async fn an_escape_that_cannot_be_resolved_is_counted_as_unresolved_never_dropped_or_invented() {
    let mut rig = rig().await;
    let mut files = Files::default();
    let tasks = five_confirmed(&mut rig, &mut files).await;
    // A revert whose files cannot be read.
    let unreadable = reverted(&mut rig, &mut files, 20, 40, 2, None, true).await;
    // A person's mark names no fix commit at all: nothing to read, nothing to guess.
    let marked = rig.merge(&["src"], unix_now() - 40 * DAY).await;
    rig.w
        .core
        .crew_escape_mark(&wire::EscapeMark {
            project: rig.project.clone(),
            merge_task_id: marked.clone(),
            seen: None,
            note: None,
        })
        .unwrap();
    found_at(&rig, &marked, unix_now() - 38 * DAY);

    let source = log(&rig, files);
    let read = source.read(&rig.project, window()).unwrap();
    assert_eq!(read.escapes.len(), 5);
    let mut unresolved: Vec<_> = read.unresolved.iter().map(|u| u.pr).collect();
    unresolved.sort();
    assert_eq!(
        unresolved,
        vec![Some(pr_of(&rig, &unreadable)), Some(pr_of(&rig, &marked))]
    );
    // No file list was invented for either.
    assert!(read.escapes.iter().all(|e| e.changed_files.len() == 1));

    let first = pr_of(&rig, &tasks[0]);
    let lane = run_lane(
        &rig,
        Arc::new(source),
        vec![
            (first, finding("fp1", "a.rs")),
            (pr_of(&rig, &unreadable), finding("fp2", "q.rs")),
        ],
    )
    .await;
    let score = lane.score.unwrap();
    // The finding on the unresolved PR is neither a hit nor a miss: it is not in precision and does not read "no escape".
    assert_eq!(lane.findings_n, 2);
    assert_eq!(score.findings_n, 1);
    let run = rig
        .w
        .core
        .store
        .crew_backtest_runs_of(&rig.project)
        .unwrap();
    let results = rig.w.core.store.crew_backtest_results(&run[0].id).unwrap();
    let on_unresolved = results.iter().find(|r| r.finding_fp == "fp2").unwrap();
    assert_eq!(
        on_unresolved.outcome,
        crate::store::ResultOutcome::NotMeasured
    );
    // The score is over the five that could be matched; the two others are reported beside it, not counted as misses.
    assert_eq!((score.escapes_n, lane.unresolved_n), (5, Some(2)));
    assert_eq!(score.recall.unwrap().numerator, 1);
}

#[tokio::test]
async fn only_confirmed_escapes_inside_the_window_count() {
    let mut rig = rig().await;
    let mut files = Files::default();
    let inside = reverted(&mut rig, &mut files, 1, 40, 30, Some(&["in.rs"]), true).await;
    // Found 31 days after its merge: the outcome window had closed.
    reverted(&mut rig, &mut files, 2, 40, 31, Some(&["late.rs"]), true).await;
    // Merged 10 days ago (too young) and 200 days ago (too old).
    reverted(&mut rig, &mut files, 3, 10, 2, Some(&["young.rs"]), true).await;
    reverted(&mut rig, &mut files, 4, 200, 2, Some(&["old.rs"]), true).await;
    // Possible, and dismissed.
    reverted(
        &mut rig,
        &mut files,
        5,
        40,
        2,
        Some(&["possible.rs"]),
        false,
    )
    .await;
    let dismissed = reverted(
        &mut rig,
        &mut files,
        6,
        40,
        2,
        Some(&["dismissed.rs"]),
        false,
    )
    .await;
    let id = rig
        .w
        .core
        .store
        .crew_escape_of_merge(&dismissed)
        .unwrap()
        .unwrap()
        .id;
    rig.w
        .core
        .crew_escape_dismiss(
            &id,
            &wire::EscapeDecide {
                project: rig.project.clone(),
                seen: wire::EscapeSeen {
                    state: wire::EscapeState::Possible,
                    gap: None,
                },
                note: None,
            },
        )
        .unwrap();

    let read = log(&rig, files).read(&rig.project, window()).unwrap();
    assert_eq!(read.unresolved.len(), 0);
    assert_eq!(read.escapes.len(), 1);
    assert_eq!(read.escapes[0].pr, pr_of(&rig, &inside));
    assert_eq!(read.escapes[0].changed_files, vec!["in.rs".to_owned()]);
}

#[tokio::test]
async fn an_unreadable_log_is_not_measured_never_partial() {
    let mut rig = rig().await;
    let mut files = Files::default();
    let tasks = five_confirmed(&mut rig, &mut files).await;
    // One damaged row, as a damaged table or a log over its cap makes: the store fails the whole read.
    rig.w
        .core
        .store
        .reader()
        .execute_batch(&format!(
            "PRAGMA ignore_check_constraints = ON;
             UPDATE crew_escapes SET state = 'bogus' WHERE merge_task_id = '{}';
             PRAGMA ignore_check_constraints = OFF;",
            tasks[0]
        ))
        .unwrap();
    let source = log(&rig, files);
    assert_eq!(source.read(&rig.project, window()), Err(NotMeasured));
    assert_eq!(source.escapes(&rig.project, window()), Err(NotMeasured));
    assert!(!source.measurable(&rig.project, window()));
    let lane = run_lane(
        &rig,
        Arc::new(source),
        vec![(pr_of(&rig, &tasks[1]), finding("fp1", "b.rs"))],
    )
    .await;
    // Findings are counted; nothing is scored, and no zero is implied.
    assert_eq!(lane.outcome, wire::BacktestLaneOutcome::NotMeasured);
    assert_eq!(
        (lane.score, lane.findings_n, lane.unresolved_n),
        (None, 1, None)
    );
}

#[tokio::test]
async fn with_no_escape_log_the_default_source_says_not_measured() {
    let mut rig = rig().await;
    for _ in 0..3 {
        rig.merge(&["src"], unix_now() - 40 * DAY).await;
    }
    let core = rig.w.core.clone();
    let lanes = [lane("correctness")];
    assert!(
        !core
            .crew_backtest_estimate(&rig.project, &lanes)
            .unwrap()
            .outcomes_measured
    );
    // The default source, not a fixture: the report still reads Not measured.
    core.crew_backtest_use(Arc::new(ScriptedRunner::default()), None);
    let started = core
        .crew_backtest_start(&wire::BacktestStartInput {
            project: rig.project.clone(),
            lanes: vec![lane("correctness")],
            budgets: vec![wire::BacktestEngineBudget {
                engine: "a".into(),
                tokens: 10,
            }],
        })
        .await
        .unwrap();
    core.crew_backtest_join().await;
    let run = core.crew_backtest_report(&started.id).unwrap();
    assert_eq!(run.lanes[0].outcome, wire::BacktestLaneOutcome::NotMeasured);
    assert_eq!(run.lanes[0].score, None);
}

#[tokio::test]
async fn a_project_with_an_escape_log_is_measured_by_default_and_reading_writes_nothing() {
    let mut rig = rig().await;
    let mut files = Files::default();
    five_confirmed(&mut rig, &mut files).await;
    let core = rig.w.core.clone();
    assert!(
        core.crew_backtest_estimate(&rig.project, &[lane("correctness")])
            .unwrap()
            .outcomes_measured
    );
    let counts = || -> (i64, i64, i64) {
        let conn = core.store.reader();
        let one = |sql: &str| conn.query_row(sql, [], |r| r.get(0)).unwrap();
        (
            one("SELECT COUNT(*) FROM crew_escapes"),
            one("SELECT COUNT(*) FROM crew_escape_events"),
            one("SELECT COUNT(*) FROM crew_merges"),
        )
    };
    let before = counts();
    let source = log(&rig, files);
    source.read(&rig.project, window()).unwrap();
    assert!(source.measurable(&rig.project, window()));
    assert_eq!(counts(), before);
}

struct Git {
    dir: PathBuf,
}

impl Git {
    fn run(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .current_dir(&self.dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.test"])
            .args([
                "-c",
                "commit.gpgsign=false",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_git_source_reads_the_files_of_a_commit_read_only() {
    let dir = std::env::temp_dir().join(format!("orbyn-escape-files-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let git = Git { dir: dir.clone() };
    git.run(&["init", "-q"]);
    std::fs::write(dir.join("src/a.rs"), "1\n").unwrap();
    std::fs::write(dir.join("b.rs"), "1\n").unwrap();
    git.run(&["add", "-A"]);
    git.run(&["commit", "-q", "-m", "base"]);
    std::fs::write(dir.join("src/a.rs"), "2\n").unwrap();
    git.run(&["commit", "-q", "-am", "revert"]);
    let commit = git.run(&["rev-parse", "HEAD"]);
    let before = git.run(&["status", "--porcelain"]);

    let path = dir.to_string_lossy().into_owned();
    let read = {
        let path = path.clone();
        let commit = commit.clone();
        tokio::task::spawn_blocking(move || {
            (
                GitCommitFiles.files(&path, &commit),
                // Not an id (it could be an option), absent from the repository, and not a repository.
                GitCommitFiles.files(&path, "--all"),
                GitCommitFiles.files(&path, &"0".repeat(40)),
                GitCommitFiles.files("/definitely/not/here", &commit),
            )
        })
        .await
        .unwrap()
    };
    assert_eq!(read.0, Some(vec!["src/a.rs".to_owned()]));
    assert_eq!((read.1, read.2, read.3), (None, None, None));
    // Nothing was written to the repository.
    assert_eq!(git.run(&["status", "--porcelain"]), before);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_fix_that_changed_more_than_the_file_limit_is_unresolved_never_shortened() {
    let dir = std::env::temp_dir().join(format!("orbyn-escape-many-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let git = Git { dir: dir.clone() };
    git.run(&["init", "-q"]);
    std::fs::write(dir.join("base.txt"), "x\n").unwrap();
    git.run(&["add", "-A"]);
    git.run(&["commit", "-q", "-m", "base"]);
    let write = |n: usize| {
        for i in 0..n {
            std::fs::write(dir.join(format!("f{i}.txt")), format!("{n}\n")).unwrap();
        }
    };
    write(2000);
    git.run(&["add", "-A"]);
    git.run(&["commit", "-q", "-m", "exactly the limit"]);
    let at_limit = git.run(&["rev-parse", "HEAD"]);
    write(2001);
    git.run(&["add", "-A"]);
    git.run(&["commit", "-q", "-m", "over the limit"]);
    let over = git.run(&["rev-parse", "HEAD"]);
    let path = dir.to_string_lossy().into_owned();
    let (a, b) = tokio::task::spawn_blocking(move || {
        (
            GitCommitFiles.files(&path, &at_limit),
            GitCommitFiles.files(&path, &over),
        )
    })
    .await
    .unwrap();
    assert_eq!(a.map(|f| f.len()), Some(2000));
    assert_eq!(b, None);
    let _ = std::fs::remove_dir_all(&dir);
}
