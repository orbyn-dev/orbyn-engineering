//! The protected paths, recorded when the task starts and compared when it is verified. The
//! comparison reads the files; git is only used here to arrange a repository and to attack it.

use std::path::Path;

use orbyn_contracts::CrewPathPattern;

use crate::crew::protect::{Snapshot, protected_changes, snapshot};
use crate::testing::{TempDir, run_git};

const ID: [&str; 4] = ["-c", "user.name=t", "-c", "user.email=t@example.test"];

fn commit_all(root: &Path) {
    run_git(root, &["add", "-A"]);
    let mut args = ID.to_vec();
    args.extend(["commit", "-q", "-m", "base"]);
    run_git(root, &args);
}

fn git_out(root: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
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

/// A repository with protected files, and the record taken as a task starts.
fn repo() -> (TempDir, Snapshot) {
    let dir = TempDir::new();
    let root = dir.path();
    run_git(root, &["init", "-q", "-b", "main"]);
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::create_dir_all(root.join("scripts")).unwrap();
    std::fs::write(root.join("tests/a.txt"), "one\ntwo\n").unwrap();
    std::fs::write(root.join("scripts/check.sh"), "exit 0\n").unwrap();
    std::fs::write(root.join("other.txt"), "free\n").unwrap();
    commit_all(root);
    let before = snapshot(root, &protected());
    (dir, before)
}

fn changes(dir: &TempDir, before: &Snapshot) -> Vec<String> {
    protected_changes(before, dir.path(), &protected())
}

#[test]
fn untouched_files_and_unprotected_edits_are_not_changes() {
    let (dir, before) = repo();
    assert_eq!(before.len(), 2);
    assert!(changes(&dir, &before).is_empty());
    std::fs::write(dir.path().join("other.txt"), "changed\n").unwrap();
    std::fs::write(dir.path().join("new.txt"), "new\n").unwrap();
    assert!(changes(&dir, &before).is_empty());
}

#[test]
fn an_edit_a_deletion_a_new_file_and_a_cr_are_changes() {
    let (dir, before) = repo();
    std::fs::write(dir.path().join("tests/a.txt"), "hacked\n").unwrap();
    std::fs::remove_file(dir.path().join("scripts/check.sh")).unwrap();
    std::fs::write(dir.path().join("tests/new.txt"), "x\n").unwrap();
    assert_eq!(
        changes(&dir, &before),
        ["scripts/check.sh", "tests/a.txt", "tests/new.txt"]
    );
    // Exact bytes: adding a CR to an expected-output file is a change.
    let (dir, before) = repo();
    std::fs::write(dir.path().join("tests/a.txt"), "one\r\ntwo\n").unwrap();
    assert_eq!(changes(&dir, &before), ["tests/a.txt"]);
}

#[test]
fn a_rename_is_a_missing_file_and_a_new_one() {
    let (dir, before) = repo();
    std::fs::rename(
        dir.path().join("tests/a.txt"),
        dir.path().join("tests/b.txt"),
    )
    .unwrap();
    assert_eq!(changes(&dir, &before), ["tests/a.txt", "tests/b.txt"]);
}

#[test]
fn a_new_ignored_file_is_a_change() {
    let (dir, before) = repo();
    std::fs::write(dir.path().join(".gitignore"), "tests/secret.txt\n").unwrap();
    std::fs::write(dir.path().join("tests/secret.txt"), "x\n").unwrap();
    assert!(git_out(dir.path(), &["status", "--porcelain", "--", "tests"]).is_empty());
    assert_eq!(changes(&dir, &before), ["tests/secret.txt"]);
}

#[test]
fn an_edit_git_has_been_told_to_ignore_is_still_a_change() {
    let (dir, before) = repo();
    let root = dir.path();
    std::fs::write(root.join("tests/a.txt"), "hacked\n").unwrap();
    run_git(root, &["update-index", "--skip-worktree", "tests/a.txt"]);
    run_git(
        root,
        &["update-index", "--assume-unchanged", "scripts/check.sh"],
    );
    std::fs::write(root.join("scripts/check.sh"), "exit 1\n").unwrap();
    run_git(root, &["config", "core.fsmonitor", "true"]);
    assert!(!git_out(root, &["status", "--porcelain"]).contains("tests/a.txt"));
    assert_eq!(changes(&dir, &before), ["scripts/check.sh", "tests/a.txt"]);
}

/// A replacement ref makes git show other content for an object; nothing here asks git.
#[test]
fn a_replace_ref_cannot_hide_an_edit() {
    let (dir, before) = repo();
    let root = dir.path();
    std::fs::write(root.join("forged.txt"), "hacked\n").unwrap();
    let original = git_out(root, &["rev-parse", "HEAD:tests/a.txt"]);
    let forged = git_out(root, &["hash-object", "-w", "forged.txt"]);
    run_git(root, &["replace", &original, &forged]);
    std::fs::write(root.join("tests/a.txt"), "hacked\n").unwrap();
    // Git now shows the edited content as the committed one.
    assert_eq!(git_out(root, &["show", "HEAD:tests/a.txt"]), "hacked");
    assert_eq!(changes(&dir, &before), ["tests/a.txt"]);
}

/// An alternates file in the checkout's own `.git` changes where git reads objects from.
#[test]
fn an_alternates_file_cannot_hide_an_edit_or_invent_one() {
    let (dir, before) = repo();
    let other = TempDir::new();
    let info = dir.path().join(".git/objects/info");
    std::fs::create_dir_all(&info).unwrap();
    std::fs::write(
        info.join("alternates"),
        format!("{}\n", other.path().display()),
    )
    .unwrap();
    assert!(changes(&dir, &before).is_empty());
    std::fs::write(dir.path().join("tests/a.txt"), "hacked\n").unwrap();
    assert_eq!(changes(&dir, &before), ["tests/a.txt"]);
}

#[cfg(unix)]
#[test]
fn a_mode_change_is_a_change_and_a_link_is_recorded_and_not_followed() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let (dir, _) = repo();
    let root = dir.path();
    let outside = TempDir::new();
    std::fs::write(outside.path().join("target.txt"), "before\n").unwrap();
    symlink(outside.path().join("target.txt"), root.join("tests/link")).unwrap();
    let before = snapshot(root, &protected());
    assert_eq!(before["tests/link"].kind, "symlink");
    // The link is unchanged even when what it points at changes; a new target is a change.
    std::fs::write(outside.path().join("target.txt"), "after\n").unwrap();
    assert!(changes(&dir, &before).is_empty());
    std::fs::remove_file(root.join("tests/link")).unwrap();
    symlink(outside.path().join("other"), root.join("tests/link")).unwrap();
    assert_eq!(changes(&dir, &before), ["tests/link"]);
    std::fs::remove_file(root.join("tests/link")).unwrap();
    symlink(outside.path().join("target.txt"), root.join("tests/link")).unwrap();
    assert!(changes(&dir, &before).is_empty());
    // Making a check script executable is a change.
    let path = root.join("scripts/check.sh");
    let mut mode = std::fs::metadata(&path).unwrap().permissions();
    mode.set_mode(mode.mode() | 0o111);
    std::fs::set_permissions(&path, mode).unwrap();
    assert_eq!(changes(&dir, &before), ["scripts/check.sh"]);
}

/// A submodule under a protected path is part of the record, unchanged until someone edits it.
#[test]
fn a_submodule_under_a_protected_path_is_unchanged_until_edited() {
    let (dir, _) = repo();
    let root = dir.path();
    let sub = TempDir::new();
    run_git(sub.path(), &["init", "-q", "-b", "main"]);
    std::fs::write(sub.path().join("lib.txt"), "lib\n").unwrap();
    commit_all(sub.path());
    let url = sub.path().to_str().unwrap();
    let mut args = ID.to_vec();
    args.extend([
        "-c",
        "protocol.file.allow=always",
        "submodule",
        "add",
        "-q",
        url,
        "tests/sub",
    ]);
    run_git(root, &args);
    let before = snapshot(root, &protected());
    assert!(before.contains_key("tests/sub/lib.txt"));
    assert!(before.keys().all(|p| !p.contains(".git")));
    assert!(changes(&dir, &before).is_empty());
    std::fs::write(root.join("tests/sub/lib.txt"), "edited\n").unwrap();
    assert_eq!(changes(&dir, &before), ["tests/sub/lib.txt"]);
}
