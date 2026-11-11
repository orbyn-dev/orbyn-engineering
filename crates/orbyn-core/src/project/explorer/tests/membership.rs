//! Git's records can be rewritten by anything that writes inside a `.git` folder. None of
//! these may lead the explorer to a folder outside the trusted project.

use super::*;

fn denied<T: std::fmt::Debug>(result: Result<T>) {
    assert!(result.is_err(), "{result:?}");
}

/// A second repository with a file the trusted project must never show.
fn other(dir: &TempDir) -> PathBuf {
    let other = repo(&dir.path().join("work/other"));
    write(&other.join("marker-label.txt"), "label\n");
    other
}

async fn cannot_reach(core: &Core, project: &Project, folder: &Path, file: &str) {
    let id = worktree_id(&plain(folder));
    denied(core.explorer_tree(&project.id, &id, "", "", false).await);
    denied(core.explorer_file(&project.id, &id, file).await);
}

async fn trusted(core: &Core, path: &Path) -> Project {
    let project = core.add_project(&path.to_string_lossy()).await.unwrap();
    core.store.trust_project(&project.id, "test").unwrap()
}

/// A linked worktree `<repo>-wt/x` of a new repository, added as the project.
async fn linked_project(core: &Core, dir: &TempDir) -> (Project, PathBuf, PathBuf) {
    let root = repo(&dir.path().join("work/home"));
    let linked = root.with_file_name("home-wt").join("x");
    run_git(
        &root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feat/x",
            &linked.to_string_lossy(),
        ],
    );
    let linked = crate::util::canonicalize(&linked).unwrap();
    (trusted(core, &linked).await, root, linked)
}

#[tokio::test]
async fn a_repository_with_a_separate_git_dir_shows_its_checkout_not_its_git_folder() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (store, src) = (dir.path().join("store.git"), dir.path().join("work/sep"));
    std::fs::create_dir_all(src.parent().unwrap()).unwrap();
    run_git(
        dir.path(),
        &[
            "init",
            "-q",
            "-b",
            "main",
            "--separate-git-dir",
            &store.to_string_lossy(),
            &src.to_string_lossy(),
        ],
    );
    run_git(&src, &["config", "user.email", "t@example.com"]);
    run_git(&src, &["config", "user.name", "T"]);
    write(&src.join("a.txt"), "one\n");
    commit_all(&src, "first");
    let project = trusted(&core, &src).await;
    let listed = core
        .explorer_worktrees(&project.id, None, false)
        .await
        .unwrap();
    assert!(
        listed
            .worktrees
            .iter()
            .all(|w| Path::new(&w.path) != plain(&store))
    );
    let main = listed
        .worktrees
        .iter()
        .find(|w| w.is_main)
        .expect("the checkout is main");
    assert_eq!(Path::new(&main.path), plain(&src));
    let own = core
        .explorer_tree(&project.id, &main.id, "", "", false)
        .await
        .unwrap();
    assert!(names(&own.entries).contains(&"a.txt"));
    cannot_reach(&core, &project, &store, "config").await;
    // A linked worktree redirected at another repository's `.git` folder is not a member.
    let other = other(&dir);
    run_git(
        &src,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feat/x",
            &dir.path().join("sep-wt/x").to_string_lossy(),
        ],
    );
    std::fs::write(
        store.join("worktrees/x/gitdir"),
        format!("{}\n", other.join(".git").display()),
    )
    .unwrap();
    core.explorer.changed(&project.id);
    let _ = core.explorer_worktrees(&project.id, None, true).await;
    cannot_reach(&core, &project, &other, "marker-label.txt").await;
}

#[tokio::test]
async fn a_git_file_in_the_main_checkout_is_not_taken_for_a_member_check_pass() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let src = repo(&dir.path().join("work/sub"));
    let moved = dir.path().join("mods/sub.git");
    std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
    std::fs::rename(src.join(".git"), &moved).unwrap();
    std::fs::write(src.join(".git"), "gitdir: ../../mods/sub.git\n").unwrap();
    let project = trusted(&core, &src).await;
    let listed = core
        .explorer_worktrees(&project.id, None, false)
        .await
        .unwrap();
    assert!(
        listed
            .worktrees
            .iter()
            .all(|w| Path::new(&w.path) != plain(&moved))
    );
    assert!(listed.worktrees.iter().any(|w| w.is_main));
    cannot_reach(&core, &project, &moved, "config").await;
    let other = other(&dir);
    run_git(
        &src,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feat/x",
            &dir.path().join("sub-wt/x").to_string_lossy(),
        ],
    );
    std::fs::write(
        moved.join("worktrees/x/gitdir"),
        format!("{}\n", other.join(".git").display()),
    )
    .unwrap();
    core.explorer.changed(&project.id);
    let _ = core.explorer_worktrees(&project.id, None, true).await;
    cannot_reach(&core, &project, &other, "marker-label.txt").await;
}

#[tokio::test]
async fn a_rewritten_commondir_cannot_swap_in_another_repository() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root, _) = linked_project(&core, &dir).await;
    assert!(
        core.explorer_worktrees(&project.id, None, false)
            .await
            .is_ok()
    );
    let other = other(&dir);
    std::fs::write(
        root.join(".git/worktrees/x/commondir"),
        format!("{}\n", other.join(".git").display()),
    )
    .unwrap();
    core.explorer.changed(&project.id);
    denied(core.explorer_worktrees(&project.id, None, true).await);
    cannot_reach(&core, &project, &other, "marker-label.txt").await;
}

#[tokio::test]
async fn a_rewritten_project_git_file_cannot_swap_in_another_repository() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, _, linked) = linked_project(&core, &dir).await;
    assert!(
        core.explorer_worktrees(&project.id, None, false)
            .await
            .is_ok()
    );
    let other = other(&dir);
    std::fs::write(
        linked.join(".git"),
        format!("gitdir: {}\n", other.join(".git").display()),
    )
    .unwrap();
    core.explorer.changed(&project.id);
    denied(core.explorer_worktrees(&project.id, None, true).await);
    cannot_reach(&core, &project, &other, "marker-label.txt").await;
}

#[test]
fn the_gate_map_does_not_grow_without_bound() {
    let reads = ExplorerReads::default();
    let held = reads.gate("busy").try_acquire_owned().unwrap();
    for n in 0..(MAX_GATES * 3) {
        reads.gate(&format!("project-{n}"));
    }
    assert!(locked(&reads.gates).len() <= MAX_GATES + 1);
    // A gate a read still holds is kept, so its limit still counts.
    assert!(locked(&reads.gates).contains_key("busy"));
    drop(held);
}
