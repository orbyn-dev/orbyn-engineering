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
use crate::crew::escapes::{CURSOR_KEY, Limits, ScanOutcome};
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

    /// Inserts a new line after `line` of a file and commits it on the current branch.
    fn insert(&self, file: &str, line: usize, text: &str, message: &str) {
        let path = self.dir.join(file);
        let mut lines: Vec<String> = std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect();
        lines.insert(line, text.to_owned());
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
    }

    /// Writes a commit object whose message may hold bytes git itself would not let `commit` write (a NUL), on top of HEAD, and moves
    /// `main` to it.
    fn raw_commit(&self, message: &[u8]) -> String {
        use std::io::Write;
        let tree = self.out(&["rev-parse", "HEAD^{tree}"]);
        let mut body = format!(
            "tree {tree}\nparent {}\nauthor t <t@example.test> 1700000000 +0000\ncommitter t <t@example.test> 1700000000 +0000\n\n",
            self.head()
        )
        .into_bytes();
        body.extend_from_slice(message);
        let mut child = self
            .command(&[
                "hash-object",
                "-t",
                "commit",
                "-w",
                "--stdin",
                "--literally",
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(&body).unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success());
        let id = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        self.git(&["update-ref", "refs/heads/main", &id]);
        id
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
        let row = one(&log.rows, "possible escape row").clone();
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

/// The one row a read returned, unpacked by pattern so a test never reads a place in the list. A read that returns
/// none or several fails here with the rows attached.
fn one<'a, T: std::fmt::Debug>(rows: &'a [T], what: &str) -> &'a T {
    match rows {
        [row] => row,
        [] => panic!("no {what}: {rows:?}"),
        _ => panic!("{} {what}s where one is expected: {rows:?}", rows.len()),
    }
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
    let event = one(&detail.events, "escape event");
    assert_eq!(event.kind, wire::EscapeEventKind::Detected);
    assert_eq!(event.decided_by, wire::EscapeActor::Core);
    assert_eq!(event.cause, format!("revert:{revert}"));
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
    let event = one(&detail.events, "escape event");
    assert_eq!(event.cause, format!("follow_up_fix:{fix}"));
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
    // Line 5 is two lines from the merge's line 3 and came from the base commit.
    s.repo
        .edit("src/a.txt", 5, "fixed line 5", "fix: next to it");
    nothing_found(&s.scan().await, 1);
}

#[tokio::test]
async fn a_fix_that_only_adds_lines_next_to_the_merges_lines_is_not_offered() {
    let mut s = scan_rig().await;
    s.merge(3, 3).await;
    // A pure insertion right after the merge's line: it changes none of the merge's lines.
    s.repo.insert("src/a.txt", 3, "guard", "fix: add a guard");
    nothing_found(&s.scan().await, 1);
}

#[tokio::test]
async fn a_commit_message_cannot_forge_or_split_log_entries() {
    let mut s = scan_rig().await;
    let (_, merge) = s.merge(3, 3).await;
    let forged = format!(
        "docs: tidy\x1e{}\x1f1\x1ffix: forged\n\nmore",
        "e".repeat(40)
    );
    s.repo.edit("src/b.txt", 9, "docs", &forged);
    let fix = s
        .repo
        .edit("src/a.txt", 3, "fixed line 3", "fix: the real one");
    let out = s.scan().await;
    assert_eq!((out.scanned, out.found, out.skipped), (1, 1, 0), "{out:?}");
    let row = s.only_possible(wire::EscapeSource::FollowUpFix);
    let detail = s.rig.w.core.crew_escape_detail(&row.id).unwrap();
    let event = one(&detail.events, "escape event");
    assert_eq!(event.cause, format!("follow_up_fix:{fix}"));
    let _ = merge;
}

#[tokio::test]
async fn a_nul_in_a_commit_object_cannot_forge_a_revert() {
    let mut s = scan_rig().await;
    let (_, merge) = s.merge(3, 3).await;
    // The message holds what would parse as a second entry: a made-up id that reverts the merge. Git does not list that id.
    let fake = "f".repeat(40);
    let message =
        format!("docs: tidy\0{fake}\01700000001\0Revert \"M\"\n\nThis reverts commit {merge}.");
    s.repo.raw_commit(message.as_bytes());
    nothing_found(&s.scan().await, 1);
}

#[tokio::test]
async fn a_damaged_cursor_reads_as_none_and_is_overwritten() {
    let mut s = scan_rig().await;
    let (_, merge) = s.merge(3, 3).await;
    s.repo.git(&["revert", "-m", "1", "--no-edit", &merge]);
    s.rig
        .w
        .core
        .store
        .set_setting(CURSOR_KEY, "not a cursor")
        .unwrap();
    let out = s.scan().await;
    assert_eq!((out.scanned, out.found), (1, 1), "{out:?}");
    assert_eq!(
        s.rig.w.core.store.setting(CURSOR_KEY).unwrap().as_deref(),
        Some("")
    );
}

#[tokio::test]
async fn a_merge_whose_work_never_fits_the_time_does_not_pin_the_scan() {
    let mut s = scan_rig().await;
    let mut merges = Vec::new();
    for (i, line) in [3, 8, 13].into_iter().enumerate() {
        merges.push(s.merge(line, 3 - i as i64).await);
    }
    // No time at all: every git call is past the deadline.
    let limits = Limits {
        merges: 50,
        calls: 300,
        seconds: 0,
    };
    let cursor = |s: &Scan| {
        s.rig
            .w
            .core
            .store
            .setting(CURSOR_KEY)
            .unwrap()
            .unwrap_or_default()
    };
    let key = |s: &Scan, task: &str| {
        let m = s.rig.w.core.store.crew_merge(task).unwrap().unwrap();
        format!("{}:{}", m.merged_unix, m.id)
    };
    // Order is the contract here: the fixture pushed the merges oldest first, so the newest-first scan reads
    // merges[2], then merges[1], then merges[0], and each scan counts the first unread merge and moves past it.
    let first = s
        .rig
        .w
        .core
        .crew_escape_scan_within(unix_now(), limits)
        .await
        .unwrap();
    assert_eq!(
        (first.scanned, first.skipped, first.found, first.truncated),
        (1, 1, 0, true),
        "{first:?}"
    );
    assert_eq!(cursor(&s), key(&s, &merges[2].0));
    s.rig
        .w
        .core
        .crew_escape_scan_within(unix_now(), limits)
        .await
        .unwrap();
    assert_eq!(cursor(&s), key(&s, &merges[1].0));
    s.rig
        .w
        .core
        .crew_escape_scan_within(unix_now(), limits)
        .await
        .unwrap();
    assert_eq!(cursor(&s), key(&s, &merges[0].0));
    // With time again the later merges are reached: a revert of the oldest is found on the next scan.
    s.repo
        .git(&["revert", "-m", "1", "--no-edit", &merges[0].1]);
    let after = s.scan().await;
    assert_eq!(after.found, 1, "{after:?}");
}

#[tokio::test]
async fn a_tag_named_like_the_branch_does_not_stand_in_for_it() {
    let mut s = scan_rig().await;
    let base = s.repo.head();
    let (_, merge) = s.merge(3, 3).await;
    s.repo.git(&["revert", "-m", "1", "--no-edit", &merge]);
    // A tag `main` on the base commit would win a plain `main`; the scan reads refs/heads/main only.
    s.repo.git(&["tag", "main", &base]);
    let out = s.scan().await;
    assert_eq!((out.scanned, out.found), (1, 1), "{out:?}");
}

#[tokio::test]
async fn a_scan_that_is_cut_short_resumes_so_every_merge_is_looked_at_over_successive_scans() {
    let mut s = scan_rig().await;
    let mut merges = Vec::new();
    for (i, line) in [3, 6, 9, 12, 15].into_iter().enumerate() {
        merges.push(s.merge(line, 5 - i as i64).await);
    }
    // The fixture pushes oldest first, so the first entry is the merge the newest-first scan reaches last.
    let (oldest_task, oldest_merge) = merges.first().expect("the oldest merge");
    let limits = Limits {
        merges: 2,
        calls: 200,
        ..Limits::default()
    };
    async fn scan(s: &Scan, limits: Limits) -> Result<ScanOutcome, crate::error::CoreError> {
        s.rig
            .w
            .core
            .crew_escape_scan_within(unix_now(), limits)
            .await
    }
    // Nothing shows on any of them yet: the newest two are looked at and no row is made.
    let first = scan(&s, limits).await.unwrap();
    assert_eq!(
        (first.scanned, first.found, first.truncated),
        (2, 0, true),
        "{first:?}"
    );
    // The oldest merge is reverted later. A scan that kept re-reading the newest two would never see it.
    s.repo
        .git(&["revert", "-m", "1", "--no-edit", oldest_merge]);
    let second = scan(&s, limits).await.unwrap();
    assert_eq!((second.scanned, second.found), (2, 0), "{second:?}");
    let third = scan(&s, limits).await.unwrap();
    assert_eq!(third.found, 1, "{third:?}");
    assert!(
        s.rig
            .w
            .core
            .store
            .crew_escape_of_merge(oldest_task)
            .unwrap()
            .is_some()
    );
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
    let (newest, _) = s.merge_with(13, 1, |m| m.watch = "broken".into()).await;
    s.scan().await;
    let all = s.possible();
    assert_eq!((all.total, all.rows.len()), (3, 3));
    let found: Vec<i64> = all.rows.iter().map(|r| r.found_unix).collect();
    // Newest first: every adjacent pair, read together rather than by its places.
    assert!(found.is_sorted_by(|a, b| a >= b), "newest first: {found:?}");
    let page = s
        .rig
        .w
        .core
        .crew_escape_possible(&s.rig.project, Page::new(Some(2), Some(2)).unwrap())
        .unwrap();
    assert_eq!((page.total, page.rows.len()), (3, 1));
    // Confirm one (a person's act): it leaves the possible list. The row is found by the merge it is about, not
    // by the list's first place.
    let confirmed = all
        .rows
        .iter()
        .find(|row| row.merge_task_id == newest)
        .expect("the newest merge's possible escape");
    s.rig
        .w
        .core
        .crew_escape_confirm(
            &confirmed.id,
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
        ..Limits::default()
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
        ..Limits::default()
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
async fn a_batch_the_writer_refuses_does_not_pin_the_cursor() {
    let mut s = scan_rig().await;
    let mut merges = Vec::new();
    for (i, line) in [3, 8, 13].into_iter().enumerate() {
        merges.push(
            s.merge_with(line, 3 - i as i64, |m| m.watch = "broken".into())
                .await,
        );
    }
    // One short of the limit: not full before git runs, but a batch of two does not fit.
    s.rig
        .w
        .core
        .store
        .fill_escape_log_for_test(&s.rig.project, 4999, 0)
        .unwrap();
    let two = Limits {
        merges: 2,
        ..Limits::default()
    };
    let before = s.counts();
    let first = s
        .rig
        .w
        .core
        .crew_escape_scan_within(unix_now(), two)
        .await
        .unwrap();
    assert_eq!(
        (first.scanned, first.found, first.refused),
        (2, 0, 2),
        "{first:?}"
    );
    assert_eq!(s.counts(), before, "a refused batch wrote");
    // The batch reads newest first, so it passed merges[2] and then merges[1]: the cursor names the merge the batch
    // ended on, which is merges[1]. Order is the contract here.
    let mid = s
        .rig
        .w
        .core
        .store
        .crew_merge(&merges[1].0)
        .unwrap()
        .unwrap();
    assert_eq!(
        s.rig.w.core.store.setting(CURSOR_KEY).unwrap().as_deref(),
        Some(format!("{}:{}", mid.merged_unix, mid.id).as_str())
    );
    // The cursor passed the refused merges, so the older one is reached and fits.
    let one = Limits {
        merges: 1,
        ..Limits::default()
    };
    let second = s
        .rig
        .w
        .core
        .crew_escape_scan_within(unix_now(), one)
        .await
        .unwrap();
    assert_eq!(
        (second.scanned, second.found, second.refused),
        (1, 1, 0),
        "{second:?}"
    );
    assert!(
        s.rig
            .w
            .core
            .store
            .crew_escape_of_merge(&merges[0].0)
            .unwrap()
            .is_some()
    );
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
