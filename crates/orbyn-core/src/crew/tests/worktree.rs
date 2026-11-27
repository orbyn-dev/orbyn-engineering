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

/// Commits everything `cwd` has, as the person's own checkout does.
fn commit(cwd: &Path, message: &str) {
    git_out(cwd, &["add", "."]);
    git_out(
        cwd,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.test",
            "commit",
            "-q",
            "-m",
            message,
        ],
    );
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
    // A folder git no longer lists, left by a crash: Orbyn's own name in Orbyn's own root, with the
    // record Orbyn writes beside it before the folder is made.
    let path = root.join("crew-worker-a");
    std::fs::create_dir_all(root.join("crew-worker-a/stale")).unwrap();
    std::fs::write(path.join("stale/left.txt"), "left behind\n").unwrap();
    std::fs::create_dir_all(root.join(".orbyn")).unwrap();
    std::fs::write(root.join(".orbyn/crew-worker-a"), path.to_str().unwrap()).unwrap();
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

#[tokio::test]
async fn a_folder_orbyn_did_not_make_is_kept_and_the_start_refused() {
    let world = setup().await;
    let lead = lead_of(&world);
    let repo = Path::new(&world.repo);
    let root = worktree_root(repo);
    // A folder with Orbyn's own name in Orbyn's own root, but no record that Orbyn made one there: it
    // can hold a person's work, so it is kept whole and the start says what is in the way.
    let path = root.join("crew-worker-a");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("notes.txt"), "a person's work\n").unwrap();
    let error = world
        .core
        .crew_writer_worktree(&lead, &writing(), &member("worker-a"), None)
        .await
        .expect_err("a folder Orbyn cannot prove it made is refused, never deleted");
    assert!(error.message.contains("did not make"), "{}", error.message);
    assert!(
        path.join("notes.txt").is_file(),
        "the folder and what it held are kept"
    );
    assert_eq!(
        listed(repo).len(),
        1,
        "nothing was made for the refused start"
    );
    assert_eq!(
        git_out(repo, &["branch", "--list", "crew-worker-a"]),
        "",
        "no branch was started"
    );
}

#[tokio::test]
async fn a_checkout_on_another_branch_is_never_taken_for_the_workers() {
    let world = setup().await;
    let lead = lead_of(&world);
    let repo = Path::new(&world.repo);
    let root = worktree_root(repo);
    // A person's own checkout for another branch, at a path whose last part is the member's folder
    // name: git reuses a registered worktree by that name, so it is never taken for the member's.
    let manual = root.join("manual/crew-worker-a");
    std::fs::create_dir_all(manual.parent().unwrap()).unwrap();
    git_out(
        repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feat/manual",
            manual.to_str().unwrap(),
        ],
    );
    let notes = manual.join("notes.txt");
    std::fs::write(&notes, "in progress\n").unwrap();
    let made = world
        .core
        .crew_writer_worktree(&lead, &writing(), &member("worker-a"), None)
        .await
        .unwrap()
        .expect("a brief that writes gets a worktree");
    assert_eq!(
        at(&made),
        root.join("crew-worker-a"),
        "the worker gets its own folder, never the person's checkout"
    );
    assert_eq!(
        branch(&at(&made)),
        "crew-worker-a",
        "the worker is on the member's own branch"
    );
    assert!(
        made.created,
        "this start made that folder, so only it is ever removed"
    );
    // What the person's checkout held is where they left it.
    assert!(notes.is_file(), "the person's checkout keeps what it held");
    assert_eq!(branch(&manual), "feat/manual");
    assert_eq!(
        listed(repo).len(),
        3,
        "the clone, the person's checkout and the worker's: {:?}",
        listed(repo)
    );
}

#[tokio::test]
async fn a_checkout_at_the_workers_path_on_another_branch_refuses_the_start() {
    let world = setup().await;
    let lead = lead_of(&world);
    let repo = Path::new(&world.repo);
    let root = worktree_root(repo);
    // The member's own folder name is already a person's checkout, on a branch the member does not
    // own: it is never taken for the worker's, and never deleted to make room.
    let path = root.join("crew-worker-a");
    std::fs::create_dir_all(&root).unwrap();
    git_out(
        repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feat/manual",
            path.to_str().unwrap(),
        ],
    );
    let notes = path.join("notes.txt");
    std::fs::write(&notes, "in progress\n").unwrap();
    let error = world
        .core
        .crew_writer_worktree(&lead, &writing(), &member("worker-a"), None)
        .await
        .expect_err("a checkout on another branch is never taken for the worker's");
    assert!(
        error.message.contains("another checkout"),
        "{}",
        error.message
    );
    assert!(notes.is_file(), "the checkout and what it held are kept");
    assert_eq!(
        branch(&path),
        "feat/manual",
        "it stays the person's checkout"
    );
    assert_eq!(
        listed(repo).len(),
        2,
        "the clone and that one checkout: {:?}",
        listed(repo)
    );
}

#[tokio::test]
async fn an_id_that_cannot_name_a_checkout_is_refused() {
    let world = setup().await;
    let lead = lead_of(&world);
    let repo = Path::new(&world.repo);
    let root = worktree_root(repo);
    // `worker.a` and `worker-a` are both member ids, and a name built by replacing what git does not
    // keep would give both one folder and one branch: the id that cannot be its own name is refused
    // a checkout instead, so two members can never share one.
    let error = world
        .core
        .crew_writer_worktree(&lead, &writing(), &member("worker.a"), None)
        .await
        .expect_err("an id that is not a checkout name is refused");
    assert!(error.message.contains("lowercase"), "{}", error.message);
    assert!(
        !root.join("crew-worker-a").exists(),
        "no folder is made for the refused id"
    );
    // The longest id a name holds works; one character more is refused rather than cut short, which
    // would give it the folder and branch of the id cut to that length.
    let longest = "a".repeat(59);
    let made = world
        .core
        .crew_writer_worktree(&lead, &writing(), &member(&longest), None)
        .await
        .unwrap()
        .expect("a brief that writes gets a worktree");
    assert_eq!(
        at(&made).file_name().unwrap().to_string_lossy(),
        format!("crew-{longest}")
    );
    let longer = "a".repeat(60);
    let error = world
        .core
        .crew_writer_worktree(&lead, &writing(), &member(&longer), None)
        .await
        .expect_err("an id longer than the name is refused, never cut");
    assert!(error.message.contains("59"), "{}", error.message);
    assert_eq!(
        listed(repo).len(),
        2,
        "the clone and the one checkout made: {:?}",
        listed(repo)
    );
}

#[tokio::test]
async fn a_reused_checkout_records_the_base_the_start_admitted() {
    let world = setup().await;
    let lead = lead_of(&world);
    let repo = Path::new(&world.repo);
    let made = world
        .core
        .crew_writer_worktree(&lead, &writing(), &member("worker-a"), None)
        .await
        .unwrap()
        .expect("a brief that writes gets a worktree");
    // The lead's checkout moves on after the member's folder was made, so the member's branch and
    // this clone's HEAD no longer share their latest commit.
    std::fs::write(repo.join("src/next.txt"), "next\n").unwrap();
    commit(repo, "next");
    let admitted = git_out(repo, &["rev-parse", "HEAD"]);
    let forked = git_out(repo, &["rev-parse", "HEAD~1"]);
    let reused = world
        .core
        .crew_writer_worktree(&lead, &writing(), &member("worker-a"), Some(&admitted))
        .await
        .unwrap()
        .expect("a brief that writes gets a worktree");
    assert_eq!(
        at(&reused),
        at(&made),
        "the member's own checkout is reused"
    );
    assert!(!reused.created, "this start did not make it");
    // The run records the commit the task was admitted at, which is the commit the start reads the
    // repository's own checks at. Recording the branch's fork point instead hands the verifier a
    // commit that can declare no checks, so `repo_required` degrades to nothing.
    assert_eq!(
        reused.worktree.base, admitted,
        "the admitted base is the recorded base, not the fork point {forked}"
    );
}

#[tokio::test]
async fn a_folder_deleted_by_hand_is_made_again_instead_of_refused() {
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
    // A person removes the folder itself instead of `git worktree remove`: the registration, and with
    // it the branch's checkout, stays behind.
    std::fs::remove_dir_all(&path).unwrap();
    assert_eq!(
        listed(repo).len(),
        2,
        "git still lists the deleted checkout: {:?}",
        listed(repo)
    );
    let again = world
        .core
        .crew_writer_worktree(&lead, &writing(), &member("worker-a"), None)
        .await
        .expect("a deleted folder's registration is cleared, never a refusal");
    let again = again.expect("a brief that writes gets a worktree");
    assert_eq!(at(&again), path, "the member's own folder is made again");
    assert!(again.created, "the folder was gone, so this start made it");
    assert!(path.is_dir(), "{} is a checkout again", path.display());
    assert_eq!(git_out(&path, &["status", "--porcelain"]), "");
    assert_eq!(
        listed(repo).len(),
        2,
        "one registration for it, not two: {:?}",
        listed(repo)
    );
}

#[tokio::test]
async fn a_branch_another_checkout_holds_is_reused_and_kept() {
    let world = setup().await;
    let lead = lead_of(&world);
    let repo = Path::new(&world.repo);
    let root = worktree_root(repo);
    // A person's own `git worktree add` for the member's branch: the branch is checked out at a path
    // Orbyn did not choose, and git keeps one checkout for each branch.
    let elsewhere = root.join("manual/crew-worker-a");
    std::fs::create_dir_all(elsewhere.parent().unwrap()).unwrap();
    git_out(
        repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "crew-worker-a",
            elsewhere.to_str().unwrap(),
        ],
    );
    let notes = elsewhere.join("notes.txt");
    std::fs::write(&notes, "in progress\n").unwrap();
    let made = world
        .core
        .crew_writer_worktree(&lead, &writing(), &member("worker-a"), None)
        .await
        .expect("a branch an existing checkout holds is reused, never refused");
    let made = made.expect("a brief that writes gets a worktree");
    assert_eq!(
        canonical(&at(&made)),
        canonical(&elsewhere),
        "the checkout that already holds the branch is the worker's"
    );
    assert!(
        !made.created,
        "this start did not make it, so a failed start leaves it alone"
    );
    assert!(notes.is_file(), "what the checkout held is kept");
    assert_eq!(
        listed(repo).len(),
        2,
        "the clone and that one checkout: {:?}",
        listed(repo)
    );
    // The same folder deleted by hand leaves the branch checked out there in git's records, which
    // refuses every retry: that registration goes and the member's own folder is made instead.
    std::fs::remove_dir_all(&elsewhere).unwrap();
    let recovered = world
        .core
        .crew_writer_worktree(&lead, &writing(), &member("worker-a"), None)
        .await
        .expect("a deleted checkout's registration is cleared, never a refusal");
    let recovered = recovered.expect("a brief that writes gets a worktree");
    assert_eq!(
        at(&recovered),
        root.join("crew-worker-a"),
        "the member's own folder is where the branch is taken back"
    );
    assert!(recovered.created, "this start made that folder");
    assert_eq!(branch(&at(&recovered)), "crew-worker-a");
}
