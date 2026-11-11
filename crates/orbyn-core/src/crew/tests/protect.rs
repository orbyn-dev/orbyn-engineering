//! The protected paths, compared with the starting commit by reading both directly.

use orbyn_contracts::CrewPathPattern;

use crate::crew::protected_changes;
use crate::testing::{TempDir, run_git};

fn repo() -> TempDir {
    let dir = TempDir::new();
    let root = dir.path();
    run_git(root, &["init", "-q", "-b", "main"]);
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::create_dir_all(root.join("scripts")).unwrap();
    std::fs::write(root.join("tests/a.txt"), "one\ntwo\n").unwrap();
    std::fs::write(root.join("scripts/check.sh"), "exit 0\n").unwrap();
    std::fs::write(root.join("other.txt"), "free\n").unwrap();
    run_git(root, &["add", "."]);
    let id = ["-c", "user.name=t", "-c", "user.email=t@example.test"];
    let mut args = id.to_vec();
    args.extend(["commit", "-q", "-m", "base"]);
    run_git(root, &args);
    dir
}

fn base(root: &std::path::Path) -> String {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

fn protected() -> Vec<CrewPathPattern> {
    ["tests/**", "scripts/check.sh"]
        .map(|p| CrewPathPattern::new(p).unwrap())
        .to_vec()
}

async fn changes(dir: &TempDir) -> Vec<String> {
    protected_changes(dir.path(), &base(dir.path()), &protected())
        .await
        .unwrap()
}

#[tokio::test]
async fn untouched_files_and_unprotected_edits_are_not_changes() {
    let dir = repo();
    assert!(changes(&dir).await.is_empty());
    std::fs::write(dir.path().join("other.txt"), "changed\n").unwrap();
    std::fs::write(dir.path().join("new.txt"), "new\n").unwrap();
    assert!(changes(&dir).await.is_empty());
    // A checkout that rewrote line endings is not a change.
    std::fs::write(dir.path().join("tests/a.txt"), "one\r\ntwo\r\n").unwrap();
    assert!(changes(&dir).await.is_empty());
}

#[tokio::test]
async fn an_edit_a_deletion_and_a_new_file_are_changes() {
    let dir = repo();
    std::fs::write(dir.path().join("tests/a.txt"), "hacked\n").unwrap();
    std::fs::remove_file(dir.path().join("scripts/check.sh")).unwrap();
    std::fs::write(dir.path().join("tests/new.txt"), "x\n").unwrap();
    assert_eq!(
        changes(&dir).await,
        ["scripts/check.sh", "tests/a.txt", "tests/new.txt"]
    );
}

/// `git diff --name-only` prints only the new name of a rename.
#[tokio::test]
async fn a_rename_is_a_missing_file_and_a_new_one() {
    let dir = repo();
    run_git(dir.path(), &["mv", "tests/a.txt", "other_dir_a.txt"]);
    assert_eq!(changes(&dir).await, ["tests/a.txt"]);
    run_git(dir.path(), &["mv", "other_dir_a.txt", "tests/b.txt"]);
    assert_eq!(changes(&dir).await, ["tests/a.txt", "tests/b.txt"]);
}

/// `ls-files --others --exclude-standard` follows `.gitignore`, which the worker can edit.
#[tokio::test]
async fn a_new_ignored_file_is_a_change() {
    let dir = repo();
    std::fs::write(dir.path().join(".gitignore"), "tests/secret.txt\n").unwrap();
    std::fs::write(dir.path().join("tests/secret.txt"), "x\n").unwrap();
    let status = std::process::Command::new("git")
        .args(["status", "--porcelain", "--", "tests"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(status.stdout.is_empty(), "git itself does not see the file");
    assert_eq!(changes(&dir).await, ["tests/secret.txt"]);
}

/// `update-index` flags and a configured fsmonitor can make git report nothing.
#[tokio::test]
async fn an_edit_git_has_been_told_to_ignore_is_still_a_change() {
    let dir = repo();
    std::fs::write(dir.path().join("tests/a.txt"), "hacked\n").unwrap();
    run_git(
        dir.path(),
        &["update-index", "--skip-worktree", "tests/a.txt"],
    );
    run_git(
        dir.path(),
        &["update-index", "--assume-unchanged", "scripts/check.sh"],
    );
    std::fs::write(dir.path().join("scripts/check.sh"), "exit 1\n").unwrap();
    run_git(dir.path(), &["config", "core.fsmonitor", "true"]);
    let status = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&status.stdout).contains("tests/a.txt"));
    assert_eq!(changes(&dir).await, ["scripts/check.sh", "tests/a.txt"]);
}
