//! A writing worker's own worktree (issue #436). Each test starts a real `Core` with a git project
//! and a lead thread, and asks for the checkout a writing brief's worker would get: the paths are
//! real folders git made beside the clone, so every claim here is read back from git.

use std::path::{Path, PathBuf};

use orbyn_contracts::{Brief, CrewMember, Thread};
use serde_json::json;

use super::fake;
use super::harness::{self, Turn};

/// The path a writing worker's checkout is at, as the contract carries it.
fn at(made: &super::super::worktree::WriterWorktree) -> PathBuf {
    PathBuf::from(&made.worktree.path)
}

/// Git's answer in `cwd`, trimmed.
fn git_out(cwd: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}");
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

/// `<repo>-wt`, the sibling root Orbyn makes every worktree under.
fn worktree_root(repo: &Path) -> PathBuf {
    repo.with_file_name(format!(
        "{}-wt",
        repo.file_name().unwrap().to_string_lossy()
    ))
}

/// Every worktree git lists for `repo`, the primary checkout first. Git spells a path its own way
/// on Windows, so these are paths, compared through [`canonical`] rather than as text.
fn listed(repo: &Path) -> Vec<PathBuf> {
    git_out(repo, &["worktree", "list", "--porcelain"])
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(PathBuf::from)
        .collect()
}

/// The same place, however git spells it.
fn canonical(path: &Path) -> PathBuf {
    crate::util::canonicalize(path).unwrap()
}

/// The branch `cwd` is on.
fn branch(cwd: &Path) -> String {
    git_out(cwd, &["rev-parse", "--abbrev-ref", "HEAD"])
}

/// What the lead's own checkout holds, as a test can see it: the commit, the branch, the index and
/// the working files.
fn lead_checkout(repo: &Path) -> (String, String, String, String) {
    (
        git_out(repo, &["rev-parse", "HEAD"]),
        branch(repo),
        git_out(repo, &["status", "--porcelain"]),
        git_out(repo, &["ls-files", "-s"]),
    )
}

fn member(id: &str) -> CrewMember {
    CrewMember::new(id).unwrap()
}

/// A lead thread and the `Core` around it, with an engine that plays nothing: these tests ask for
/// a checkout, they never start a worker.
async fn setup() -> harness::World {
    harness::world(true, std::sync::Arc::new(|_, _| Turn::Silent)).await
}

fn writing() -> Brief {
    fake::brief(json!({}))
}

fn reading() -> Brief {
    fake::reading(json!({}))
}

fn lead_of(world: &harness::World) -> Thread {
    world.core.store.thread(&world.lead).unwrap()
}

#[tokio::test]
async fn two_writers_get_two_checkouts_of_their_own() {
    let world = setup().await;
    let lead = lead_of(&world);
    let repo = Path::new(&world.repo);
    let root = worktree_root(repo);
    let (one, two) = (member("worker-a"), member("worker-b"));
    let first = world
        .core
        .crew_writer_worktree(&lead, &writing(), &one, None)
        .await
        .unwrap()
        .expect("a brief that writes gets a worktree");
    let second = world
        .core
        .crew_writer_worktree(&lead, &writing(), &two, None)
        .await
        .unwrap()
        .expect("a brief that writes gets a worktree");
    assert_ne!(
        first.worktree.path, second.worktree.path,
        "two writers never share a checkout"
    );
    // Both sit in the sibling root Orbyn already uses for worktrees, one folder each, and never
    // inside the lead's checkout.
    for worktree in [&first, &second] {
        let path = at(worktree);
        assert!(
            path.starts_with(&root),
            "{} is under {root:?}",
            path.display()
        );
        assert!(path.is_dir(), "{} is a checkout", path.display());
        assert_ne!(path, repo, "a writer never writes in the lead's checkout");
        // The checkout is at the project's own content, committed, so the worker starts from it.
        assert!(path.join("src/base.txt").is_file());
        assert_eq!(
            git_out(&path, &["status", "--porcelain"]),
            "",
            "a fresh writer's checkout has nothing uncommitted"
        );
    }
    assert_eq!(
        at(&first).file_name().unwrap().to_string_lossy(),
        "crew-worker-a"
    );
    assert_eq!(
        at(&second).file_name().unwrap().to_string_lossy(),
        "crew-worker-b"
    );
    assert_eq!(first.worktree.branch, "crew-worker-a");
    assert_eq!(second.worktree.branch, "crew-worker-b");
    // And git agrees they are the repository's worktrees, not stray folders.
    let listed = listed(repo);
    assert_eq!(listed.len(), 3, "the clone and its two writers: {listed:?}");
}

#[tokio::test]
async fn making_a_writer_checkout_leaves_the_lead_checkout_untouched() {
    let world = setup().await;
    let lead = lead_of(&world);
    let repo = Path::new(&world.repo);
    let before = lead_checkout(repo);
    for id in ["worker-a", "worker-b"] {
        world
            .core
            .crew_writer_worktree(&lead, &writing(), &member(id), None)
            .await
            .unwrap()
            .expect("a brief that writes gets a worktree");
    }
    assert_eq!(
        lead_checkout(repo),
        before,
        "the lead's commit, branch, index and working files are what they were"
    );
    assert_eq!(branch(repo), "main", "the lead stays on its own branch");
    // The clone is still the first worktree and still its own path.
    assert_eq!(
        listed(repo).first().map(PathBuf::as_path),
        Some(canonical(repo).as_path())
    );
}

#[tokio::test]
async fn a_writer_is_never_on_the_default_branch() {
    let world = setup().await;
    let lead = lead_of(&world);
    let repo = Path::new(&world.repo);
    for id in ["worker-a", "worker-b"] {
        let worktree = world
            .core
            .crew_writer_worktree(&lead, &writing(), &member(id), None)
            .await
            .unwrap()
            .expect("a brief that writes gets a worktree");
        let on = branch(&at(&worktree));
        assert_eq!(on, format!("crew-{id}"), "the worker's own branch");
        assert_ne!(on, "main", "a worker is never on the default branch");
        assert_ne!(on, "master");
        // The commit is the lead's, so the worker starts where the person was.
        assert_eq!(
            git_out(&at(&worktree), &["rev-parse", "HEAD"]),
            git_out(repo, &["rev-parse", "HEAD"])
        );
    }
}

#[tokio::test]
async fn a_read_only_brief_keeps_the_lead_checkout() {
    let world = setup().await;
    let lead = lead_of(&world);
    let repo = Path::new(&world.repo);
    let none = world
        .core
        .crew_writer_worktree(&lead, &reading(), &member("worker-a"), None)
        .await
        .unwrap();
    assert!(
        none.is_none(),
        "a brief that writes nothing needs no checkout"
    );
    assert!(
        !worktree_root(repo).exists(),
        "and nothing was made for it: {:?}",
        worktree_root(repo)
    );
    assert_eq!(listed(repo).len(), 1, "the lead's checkout alone");
}

#[tokio::test]
async fn a_failed_creation_reports_it_and_leaves_no_checkout() {
    let world = setup().await;
    let lead = lead_of(&world);
    let repo = Path::new(&world.repo);
    let root = worktree_root(repo);
    std::fs::create_dir_all(&root).unwrap();
    // A file where the writer's folder goes: the start must report it rather than swallow it.
    let target = root.join("crew-worker-a");
    std::fs::write(&target, "in the way\n").unwrap();
    let error = world
        .core
        .crew_writer_worktree(&lead, &writing(), &member("worker-a"), None)
        .await
        .expect_err("a folder in the way is reported, never ignored");
    assert!(
        error.message.contains("worktree"),
        "the error names the checkout: {}",
        error.message
    );
    // Nothing half-made: no folder, no registration, and no branch was started.
    assert!(target.is_file(), "the file in the way is still a file");
    assert_eq!(listed(repo).len(), 1, "git lists the lead's checkout alone");
    assert_eq!(
        git_out(repo, &["branch", "--list", "crew-worker-a"]),
        "",
        "no branch was left behind"
    );
}

#[tokio::test]
async fn a_start_that_fails_removes_the_checkout_it_made() {
    let world = setup().await;
    let lead = lead_of(&world);
    let repo = Path::new(&world.repo);
    let made = world
        .core
        .crew_writer_worktree(&lead, &writing(), &member("worker-a"), None)
        .await
        .unwrap()
        .expect("a brief that writes gets a worktree");
    let path = at(&made);
    // A worker that never ran: what its start does when the thread cannot be made.
    world.core.discard_crew_worktree(&lead, &path).await;
    assert!(!path.exists(), "{} is gone", path.display());
    assert_eq!(
        listed(repo).len(),
        1,
        "git no longer lists it: {:?}",
        listed(repo)
    );
    // The member's folder is usable again, so a retry is not blocked by the failed attempt.
    let again = world
        .core
        .crew_writer_worktree(&lead, &writing(), &member("worker-a"), None)
        .await
        .unwrap()
        .expect("a brief that writes gets a worktree");
    assert_eq!(at(&again), path);
    assert!(at(&again).is_dir());
    assert_eq!(git_out(&at(&again), &["status", "--porcelain"]), "");
    // A checkout that is already registered is the member's own, so this start did not make it and a
    // failed start would leave it alone.
    assert!(again.created, "the folder was gone, so this start made it");
    let reused = world
        .core
        .crew_writer_worktree(&lead, &writing(), &member("worker-a"), None)
        .await
        .unwrap()
        .expect("a brief that writes gets a worktree");
    assert_eq!(at(&reused), path);
    assert!(
        !reused.created,
        "an existing checkout is reused, never claimed as this start's own"
    );
}

#[tokio::test]
async fn a_crashed_attempts_folder_is_cleared_instead_of_blocking_every_retry() {
    let world = setup().await;
    let lead = lead_of(&world);
    let repo = Path::new(&world.repo);
    let root = worktree_root(repo);
    // A folder git no longer lists, left by a crash: Orbyn's own name in Orbyn's own root.
    let path = root.join("crew-worker-a");
    std::fs::create_dir_all(root.join("crew-worker-a/stale")).unwrap();
    std::fs::write(path.join("stale/left.txt"), "left behind\n").unwrap();
    let made = world
        .core
        .crew_writer_worktree(&lead, &writing(), &member("worker-a"), None)
        .await
        .unwrap()
        .expect("a brief that writes gets a worktree");
    assert_eq!(at(&made), path);
    assert!(
        !path.join("stale").exists(),
        "the crashed attempt's files are gone, not inherited"
    );
    assert!(path.join("src/base.txt").is_file());
}
