//! The possible-escape scan (slice 4k, PR 3) on a real `Core` with fake engines and a real temporary git repository per case: each
//! signal, each false positive the design names, never confirming, idempotence, a dismissed escape not offered again, a GET that
//! writes nothing, and the bounds. Nothing is pushed and the scan runs no worker. Every repository sets its own user, email and
//! default branch, so the tests do not depend on the machine's git config.

use std::path::PathBuf;
use std::process::Command;

use serde_json::json;

use super::autopilot_areas::{DAY, Rig, rig};
use super::harness::{Turn, brief, key, world};
use crate::crew::Page;
use crate::crew::escapes::{Limits, ScanOutcome};
use crate::store::MergeRecord;
use crate::util::unix_now;
use orbyn_contracts as wire;

struct Repo {
    dir: PathBuf,
}

impl Repo {
    fn new(dir: PathBuf) -> Self {
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let repo = Self { dir };
        repo.git(&["init", "-q"]);
        let body: String = (1..=20).map(|n| format!("line {n}\n")).collect();
        std::fs::write(repo.dir.join("src/a.txt"), &body).unwrap();
        std::fs::write(repo.dir.join("src/b.txt"), &body).unwrap();
        repo.git(&["add", "-A"]);
        repo.git(&["commit", "-q", "-m", "base"]);
        repo
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new("git");
        command
            .current_dir(&self.dir)
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.test",
                "-c",
                "init.defaultBranch=main",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.autocrlf=false",
            ])
            .args(args);
        command
    }

    fn git(&self, args: &[&str]) {
        let status = self.command(args).output().unwrap();
        assert!(
            status.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&status.stderr)
        );
    }

    fn out(&self, args: &[&str]) -> String {
        let output = self.command(args).output().unwrap();
        assert!(output.status.success(), "git {args:?}");
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    fn head(&self) -> String {
        self.out(&["rev-parse", "HEAD"])
    }

    /// Replaces one line of a file and commits it on the current branch.
    fn edit(&self, file: &str, line: usize, text: &str, message: &str) -> String {
        let path = self.dir.join(file);
        let mut lines: Vec<String> = std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect();
        lines[line - 1] = text.to_owned();
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
        self.head()
    }

    /// A feature branch that changes `line` of `src/a.txt`, merged into `main` with a merge commit. Returns (before, merge).
    fn feature(&self, name: &str, line: usize) -> (String, String) {
        self.git(&["checkout", "-q", "-B", "main"]);
        let before = self.head();
        self.git(&["checkout", "-q", "-b", name]);
        self.edit(
            "src/a.txt",
            line,
            &format!("{name} line {line}"),
            &format!("feat: {name}"),
        );
        self.git(&["checkout", "-q", "main"]);
        self.git(&[
            "merge",
            "--no-ff",
            "-q",
            "-m",
            &format!("Merge {name}"),
            name,
        ]);
        (before, self.head())
    }
}

struct Scan {
    rig: Rig,
    repo: Repo,
    next: usize,
}

async fn scan_rig() -> Scan {
    let rig = rig().await;
    let repo = Repo::new(rig.w._dir.path().join("scan-repo"));
    Scan { rig, repo, next: 0 }
}

impl Scan {
    /// A crew merge of `src/a.txt` line `line`, made `days_ago` days before now. Returns (task, merge commit).
    async fn merge(&mut self, line: usize, days_ago: i64) -> (String, String) {
        self.merge_with(line, days_ago, |_| {}).await
    }

    async fn merge_with(
        &mut self,
        line: usize,
        days_ago: i64,
        tweak: impl FnOnce(&mut MergeRecord),
    ) -> (String, String) {
        self.next += 1;
        let n = self.next;
        let (before, merge) = self.repo.feature(&format!("feat{n}"), line);
        let (answer, error) = self
            .rig
            .w
            .start(
                brief(json!({ "files": [{ "path": format!("src/s{n}/**"), "mode": "write" }], "outcome": format!("Task {n}.") })),
                key(),
            )
            .await;
        assert!(!error, "{answer}");
        let task = answer["task"].as_str().unwrap().to_owned();
        let mut record = crate::crew::fixtures::merge_record(
            &task,
            &self.repo.dir.to_string_lossy(),
            "main",
            "x",
        );
        record.crew_id = self.rig.w.task(&task).crew_id;
        record.before_commit = before;
        record.merge_commit = merge.clone();
        record.candidate = merge.clone();
        record.engine = "alpha".into();
        record.merged_unix = unix_now() - days_ago * DAY;
        tweak(&mut record);
        let store = &self.rig.w.core.store;
        store.add_crew_merge(&record).unwrap();
        // The watch's finding is written by its own update, as the product does; the insert starts at `unknown`.
        if record.watch != "unknown" {
            store.set_merge_watch(&task, &record.watch).unwrap();
        }
        (task, merge)
    }

    async fn scan(&self) -> ScanOutcome {
        self.rig.w.core.crew_escape_scan(unix_now()).await.unwrap()
    }

    fn log(&self, state: Option<&str>) -> wire::EscapeLog {
        self.rig
            .w
            .core
            .crew_escape_log(
                &self.rig.project,
                state,
                Page::new(Some(200), None).unwrap(),
            )
            .unwrap()
    }

    fn possible(&self) -> wire::EscapePossible {
        self.rig
            .w
            .core
            .crew_escape_possible(&self.rig.project, Page::new(Some(200), None).unwrap())
            .unwrap()
    }

    /// Escapes, events and audit rows: what a write would change.
    fn counts(&self) -> (i64, i64, i64) {
        let conn = self.rig.w.core.store.reader();
        let count = |table: &str| -> i64 {
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap()
        };
        (
            count("crew_escapes"),
            count("crew_escape_events"),
            count("audit_events"),
        )
    }

    fn only_possible(&self, source: wire::EscapeSource) -> wire::EscapeRow {
        let log = self.log(None);
        assert_eq!(
            (log.total, log.possible, log.confirmed, log.dismissed),
            (1, 1, 0, 0)
        );
        let row = log.rows[0].clone();
        assert_eq!(row.source, source);
        assert_eq!(row.state, wire::EscapeState::Possible);
        assert_eq!((row.decided_unix, row.decided_by), (None, None));
        assert!(row.line.contains("has not confirmed"), "{}", row.line);
        row
    }
}

fn nothing_found(out: &ScanOutcome, scanned: u32) {
    assert_eq!(
        (
            out.scanned,
            out.found,
            out.skipped,
            out.refused,
            out.truncated
        ),
        (scanned, 0, 0, 0, false),
        "{out:?}"
    );
}

// Signal 1: a revert made outside the crew.

#[tokio::test]
async fn a_revert_outside_the_crew_is_one_possible_escape() {
    let mut s = scan_rig().await;
    let (task, merge) = s.merge(3, 3).await;
    s.repo.git(&["revert", "-m", "1", "--no-edit", &merge]);
    let revert = s.repo.head();
    let out = s.scan().await;
    assert_eq!(
        (out.scanned, out.found, out.truncated),
        (1, 1, false),
        "{out:?}"
    );
    let row = s.only_possible(wire::EscapeSource::Revert);
    assert_eq!(row.merge_task_id, task);
    assert_eq!(
        (row.engine.as_str(), row.model, row.lane),
        ("alpha", None, None)
    );
    assert!(row.reason.unwrap().contains(&revert[..12]));
    let detail = s.rig.w.core.crew_escape_detail(&row.id).unwrap();
    assert_eq!(detail.events.len(), 1);
    assert_eq!(detail.events[0].kind, wire::EscapeEventKind::Detected);
    assert_eq!(detail.events[0].decided_by, wire::EscapeActor::Core);
    assert_eq!(detail.events[0].cause, format!("revert:{revert}"));
}

#[tokio::test]
async fn a_revert_that_was_reapplied_is_not_offered() {
    let mut s = scan_rig().await;
    s.merge(3, 3).await;
    let merge = s.repo.head();
    s.repo.git(&["revert", "-m", "1", "--no-edit", &merge]);
    let revert = s.repo.head();
    s.repo.git(&["revert", "--no-edit", &revert]);
    nothing_found(&s.scan().await, 1);
    assert_eq!(s.counts(), (0, 0, s.counts().2));
}

#[tokio::test]
async fn a_revert_made_to_bisect_is_not_offered() {
    let mut s = scan_rig().await;
    let (_, merge) = s.merge(3, 3).await;
    s.repo.git(&["revert", "-m", "1", "--no-commit", &merge]);
    s.repo.git(&[
        "commit",
        "-q",
        "-m",
        &format!("Revert \"Merge feat1\"\n\nThis reverts commit {merge}.\nTemporary, to bisect a failure."),
    ]);
    nothing_found(&s.scan().await, 1);
}

#[tokio::test]
async fn an_ordinary_later_commit_that_only_mentions_the_merge_is_not_a_revert() {
    let mut s = scan_rig().await;
    let (_, merge) = s.merge(3, 3).await;
    s.repo
        .edit("src/b.txt", 9, "docs", &format!("docs: notes on {merge}"));
    nothing_found(&s.scan().await, 1);
}

// Signal 2: a follow-up fix on the same lines.

#[tokio::test]
async fn a_fix_within_the_window_on_lines_the_merge_added_is_one_possible_escape() {
    let mut s = scan_rig().await;
    let (task, _) = s.merge(3, 3).await;
    let fix = s
        .repo
        .edit("src/a.txt", 3, "fixed line 3", "fix: handle the empty case");
    let out = s.scan().await;
    assert_eq!((out.scanned, out.found), (1, 1), "{out:?}");
    let row = s.only_possible(wire::EscapeSource::FollowUpFix);
    assert_eq!(row.merge_task_id, task);
    let reason = row.reason.unwrap();
    assert!(
        reason.contains(&fix[..12]) && reason.contains("ordinary follow-up"),
        "{reason}"
    );
    let detail = s.rig.w.core.crew_escape_detail(&row.id).unwrap();
    assert_eq!(detail.events[0].cause, format!("follow_up_fix:{fix}"));
}

#[tokio::test]
async fn a_fix_on_other_lines_of_the_same_file_is_not_offered() {
    let mut s = scan_rig().await;
    s.merge(3, 3).await;
    s.repo
        .edit("src/a.txt", 18, "fixed line 18", "fix: a different problem");
    nothing_found(&s.scan().await, 1);
}

#[tokio::test]
async fn a_fix_on_another_file_is_not_offered() {
    let mut s = scan_rig().await;
    s.merge(3, 3).await;
    s.repo
        .edit("src/b.txt", 3, "fixed line 3", "fix: the other file");
    nothing_found(&s.scan().await, 1);
}

#[tokio::test]
async fn a_later_change_that_is_not_called_a_fix_is_not_offered() {
    let mut s = scan_rig().await;
    s.merge(3, 3).await;
    s.repo
        .edit("src/a.txt", 3, "reworded line 3", "feat: reword it");
    s.repo
        .edit("src/a.txt", 3, "fixture line 3", "fixture: new sample");
    nothing_found(&s.scan().await, 1);
}

#[tokio::test]
async fn a_fix_after_the_follow_up_window_is_not_offered() {
    let mut s = scan_rig().await;
    // The merge reached the branch 20 days ago; the fix commit is made now, past the 14 days.
    s.merge(3, 20).await;
    s.repo
        .edit("src/a.txt", 3, "fixed line 3", "fix: very late");
    nothing_found(&s.scan().await, 1);
}

#[tokio::test]
async fn a_fix_of_something_the_merge_did_not_add_is_not_offered_even_in_the_same_hunk_area() {
    let mut s = scan_rig().await;
    s.merge(3, 3).await;
    // Line 4 sits next to the merge's line 3 but came from the base commit.
    s.repo
        .edit("src/a.txt", 5, "fixed line 5", "fix: next to it");
    nothing_found(&s.scan().await, 1);
}

// Signal 3: a post-merge check failure.

#[tokio::test]
async fn a_broken_watch_on_a_merge_that_was_not_reverted_is_one_possible_escape() {
    let mut s = scan_rig().await;
    let (task, _) = s.merge_with(3, 3, |m| m.watch = "broken".into()).await;
    let out = s.scan().await;
    assert_eq!((out.scanned, out.found), (1, 1), "{out:?}");
    let row = s.only_possible(wire::EscapeSource::PostMergeCheck);
    assert_eq!(row.merge_task_id, task);
    assert!(row.reason.unwrap().contains("check failed"));
}

#[tokio::test]
async fn a_green_or_unknown_watch_and_a_broken_one_the_crew_already_reverted_are_not_offered() {
    let mut s = scan_rig().await;
    s.merge_with(3, 3, |m| m.watch = "green".into()).await;
    s.merge_with(4, 3, |m| m.watch = "unknown".into()).await;
    // Broken, and the crew reverted it: the schema already logged that revert, so the scan has nothing to add for it.
    let (reverted, _) = s.merge_with(10, 3, |m| m.watch = "broken".into()).await;
    s.rig
        .w
        .core
        .store
        .set_merge_reverted(&reverted, &"c".repeat(40))
        .unwrap();
    nothing_found(&s.scan().await, 2);
    let row = s.only_possible(wire::EscapeSource::Revert);
    assert_eq!(row.merge_task_id, reverted);
}

// What every scan promises.

#[tokio::test]
async fn a_scan_never_confirms_and_a_repeat_scan_adds_nothing() {
    let mut s = scan_rig().await;
    let (_, merge) = s.merge(3, 3).await;
    s.merge_with(10, 3, |m| m.watch = "broken".into()).await;
    s.repo.git(&["revert", "-m", "1", "--no-edit", &merge]);
    let first = s.scan().await;
    assert_eq!((first.scanned, first.found), (2, 2), "{first:?}");
    let log = s.log(None);
    assert_eq!((log.possible, log.confirmed, log.dismissed), (2, 0, 0));
    for row in &log.rows {
        assert_eq!(row.state, wire::EscapeState::Possible);
        assert_ne!(row.source, wire::EscapeSource::PersonMarked);
        assert!(row.decided_by.is_none());
    }
    let after_first = s.counts();
    let again = s.scan().await;
    assert_eq!((again.scanned, again.found), (0, 0), "{again:?}");
    assert_eq!(s.counts(), after_first);
}

#[tokio::test]
async fn a_dismissed_escape_is_not_offered_again_by_any_later_scan() {
    let mut s = scan_rig().await;
    let (_, merge) = s.merge(3, 3).await;
    s.repo.git(&["revert", "-m", "1", "--no-edit", &merge]);
    s.scan().await;
    let row = s.only_possible(wire::EscapeSource::Revert);
    s.rig
        .w
        .core
        .crew_escape_dismiss(
            &row.id,
            &wire::EscapeDecide {
                project: s.rig.project.clone(),
                seen: wire::EscapeSeen {
                    state: wire::EscapeState::Possible,
                    gap: None,
                },
                note: None,
            },
        )
        .unwrap();
    // New evidence for the same merge: a fix on the same lines would also have fired. Nothing comes back.
    s.repo.edit("src/a.txt", 3, "fixed line 3", "fix: again");
    let before = s.counts();
    let out = s.scan().await;
    assert_eq!((out.scanned, out.found), (0, 0), "{out:?}");
    assert_eq!(s.counts(), before);
    assert_eq!(s.possible().total, 0);
    let log = s.log(None);
    assert_eq!((log.possible, log.confirmed, log.dismissed), (0, 0, 1));
}

#[tokio::test]
async fn a_get_writes_nothing_and_does_not_run_the_scan() {
    let mut s = scan_rig().await;
    let (_, merge) = s.merge(3, 3).await;
    s.repo.git(&["revert", "-m", "1", "--no-edit", &merge]);
    let before = s.counts();
    for _ in 0..2 {
        let possible = s.possible();
        assert_eq!((possible.total, possible.rows.len()), (0, 0));
    }
    assert_eq!(s.counts(), before, "a GET wrote");
    s.scan().await;
    let after_scan = s.counts();
    for _ in 0..2 {
        assert_eq!(s.possible().total, 1);
        s.log(None);
        s.log(Some("possible"));
    }
    assert_eq!(s.counts(), after_scan, "a GET wrote");
}

#[tokio::test]
async fn the_possible_list_holds_only_possible_escapes_newest_first_with_a_page() {
    let mut s = scan_rig().await;
    s.merge_with(3, 3, |m| m.watch = "broken".into()).await;
    s.merge_with(8, 2, |m| m.watch = "broken".into()).await;
    s.merge_with(13, 1, |m| m.watch = "broken".into()).await;
    s.scan().await;
    let all = s.possible();
    assert_eq!((all.total, all.rows.len()), (3, 3));
    let found: Vec<i64> = all.rows.iter().map(|r| r.found_unix).collect();
    assert!(found.windows(2).all(|w| w[0] >= w[1]));
    let page = s
        .rig
        .w
        .core
        .crew_escape_possible(&s.rig.project, Page::new(Some(2), Some(2)).unwrap())
        .unwrap();
    assert_eq!((page.total, page.rows.len()), (3, 1));
    // Confirm one (a person's act): it leaves the possible list.
    let first = &all.rows[0];
    s.rig
        .w
        .core
        .crew_escape_confirm(
            &first.id,
            &wire::EscapeDecide {
                project: s.rig.project.clone(),
                seen: wire::EscapeSeen {
                    state: wire::EscapeState::Possible,
                    gap: None,
                },
                note: None,
            },
        )
        .unwrap();
    assert_eq!(s.possible().total, 2);
}

#[tokio::test]
async fn the_flag_off_hides_the_scan_and_the_list() {
    let s = scan_rig().await;
    let off = world(false, std::sync::Arc::new(|_, _| Turn::Hang)).await;
    assert!(off.core.crew_escape_scan(unix_now()).await.is_err());
    assert!(
        off.core
            .crew_escape_possible("p", Page::new(None, None).unwrap())
            .is_err()
    );
    let _ = s;
}

// Bounds.

#[tokio::test]
async fn a_scan_looks_at_the_newest_merges_only_up_to_its_bound() {
    let mut s = scan_rig().await;
    let (old_task, _) = s.merge_with(3, 3, |m| m.watch = "broken".into()).await;
    s.merge_with(8, 2, |m| m.watch = "broken".into()).await;
    s.merge_with(13, 1, |m| m.watch = "broken".into()).await;
    let limits = Limits {
        merges: 2,
        calls: 100,
    };
    let out = s
        .rig
        .w
        .core
        .crew_escape_scan_within(unix_now(), limits)
        .await
        .unwrap();
    assert_eq!(
        (out.scanned, out.found, out.truncated),
        (2, 2, true),
        "{out:?}"
    );
    assert!(
        s.rig
            .w
            .core
            .store
            .crew_escape_of_merge(&old_task)
            .unwrap()
            .is_none()
    );
    // The next scan reaches the one it left, and then nothing is left.
    let next = s.scan().await;
    assert_eq!(
        (next.scanned, next.found, next.truncated),
        (1, 1, false),
        "{next:?}"
    );
    assert!(
        s.rig
            .w
            .core
            .store
            .crew_escape_of_merge(&old_task)
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn a_scan_stops_at_its_git_call_bound_and_leaves_the_rest_for_the_next() {
    let mut s = scan_rig().await;
    let (task, merge) = s.merge(3, 3).await;
    s.repo.git(&["revert", "-m", "1", "--no-edit", &merge]);
    let limits = Limits {
        merges: 50,
        calls: 1,
    };
    let out = s
        .rig
        .w
        .core
        .crew_escape_scan_within(unix_now(), limits)
        .await
        .unwrap();
    assert_eq!((out.found, out.truncated), (0, true), "{out:?}");
    assert!(
        s.rig
            .w
            .core
            .store
            .crew_escape_of_merge(&task)
            .unwrap()
            .is_none()
    );
    let next = s.scan().await;
    assert_eq!((next.found, next.truncated), (1, false), "{next:?}");
}

#[tokio::test]
async fn a_full_log_refuses_the_scan_whole_with_nothing_written() {
    let mut s = scan_rig().await;
    s.merge_with(3, 3, |m| m.watch = "broken".into()).await;
    s.rig
        .w
        .core
        .store
        .fill_escape_log_for_test(&s.rig.project, 5000, 0)
        .unwrap();
    let before = s.counts();
    let out = s.scan().await;
    assert_eq!((out.found, out.refused), (0, 1), "{out:?}");
    assert_eq!(s.counts(), before);

    let mut e = scan_rig().await;
    e.merge_with(3, 3, |m| m.watch = "broken".into()).await;
    e.rig
        .w
        .core
        .store
        .fill_escape_log_for_test(&e.rig.project, 0, 20_000)
        .unwrap();
    let before = e.counts();
    let out = e.scan().await;
    assert_eq!((out.found, out.refused), (0, 1), "{out:?}");
    assert_eq!(e.counts(), before);
}

#[tokio::test]
async fn a_missing_or_rewritten_repository_is_skipped_not_guessed() {
    let mut s = scan_rig().await;
    s.merge_with(3, 3, |m| m.repo = "/definitely/not/a/repo".into())
        .await;
    // A merge commit that is not on the branch any more.
    s.merge_with(8, 3, |m| m.merge_commit = "d".repeat(40))
        .await;
    let out = s.scan().await;
    assert_eq!((out.scanned, out.found, out.skipped), (2, 0, 2), "{out:?}");
    assert_eq!(s.counts().0, 0);
}

#[tokio::test]
async fn the_scan_leaves_the_repository_exactly_as_it_was() {
    let mut s = scan_rig().await;
    let (_, merge) = s.merge(3, 3).await;
    s.repo.git(&["revert", "-m", "1", "--no-edit", &merge]);
    s.repo.edit("src/a.txt", 3, "fixed line 3", "fix: x");
    let snapshot = |r: &Repo| {
        (
            r.head(),
            r.out(&["status", "--porcelain"]),
            r.out(&["for-each-ref"]),
            r.out(&["reflog", "-n", "50"]),
        )
    };
    let before = snapshot(&s.repo);
    s.scan().await;
    assert_eq!(snapshot(&s.repo), before);
}
