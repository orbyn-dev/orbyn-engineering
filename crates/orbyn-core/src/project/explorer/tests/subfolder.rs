//! A project whose own folder is a subfolder of a Git repository is rooted at that folder:
//! one row, and no read reaches above it.

use super::*;

async fn subfolder_project(core: &Core, dir: &TempDir) -> (Project, PathBuf, PathBuf) {
    let root = repo(&dir.path().join("work/mono"));
    write(&root.join("pkg/app/main.rs"), "fn main() {}\n");
    write(&root.join("pkg/app/deep/x.txt"), "x\n");
    write(&root.join("pkg/secret-label.txt"), "above\n");
    commit_all(&root, "pkg");
    let sub = crate::util::canonicalize(&root.join("pkg/app")).unwrap();
    let project = core.add_project(&sub.to_string_lossy()).await.unwrap();
    let project = core.store.trust_project(&project.id, "test").unwrap();
    (project, root, sub)
}

#[tokio::test]
async fn a_subfolder_project_lists_one_row_rooted_at_its_folder() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, _root, sub) = subfolder_project(&core, &dir).await;
    let listed = core
        .explorer_worktrees(&project.id, None, false)
        .await
        .unwrap();
    assert_eq!(listed.worktrees.len(), 1);
    let row = &listed.worktrees[0];
    assert!(row.is_subfolder && !row.is_main && !row.read_only);
    assert!(row.read_only_reason.is_none());
    assert_eq!(Path::new(&row.path), plain(&sub));
    assert_eq!(row.id, worktree_id(&plain(&sub)));
    assert_eq!(row.branch.as_deref(), Some("main"));
    // The id is stable across a fresh read.
    let again = core
        .explorer_worktrees(&project.id, None, true)
        .await
        .unwrap();
    assert_eq!(again.worktrees[0].id, row.id);
}

#[tokio::test]
async fn a_subfolder_project_reads_inside_its_folder() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, _root, _sub) = subfolder_project(&core, &dir).await;
    let id = main_id(&core, &project).await;
    let top = core
        .explorer_tree(&project.id, &id, "", "", false)
        .await
        .unwrap();
    assert_eq!(names(&top.entries), ["deep", "main.rs"]);
    let deep = core
        .explorer_tree(&project.id, &id, "deep", "", false)
        .await
        .unwrap();
    assert_eq!(names(&deep.entries), ["x.txt"]);
    let file = core
        .explorer_file(&project.id, &id, "main.rs")
        .await
        .unwrap();
    assert_eq!(file.content, "fn main() {}\n");
}

#[tokio::test]
async fn a_subfolder_project_cannot_reach_its_parent() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, _root, _sub) = subfolder_project(&core, &dir).await;
    let id = main_id(&core, &project).await;
    for bad in [
        "../secret-label.txt",
        "deep/../../secret-label.txt",
        "%2e%2e/secret-label.txt",
        "/etc/hosts",
        "..\\secret-label.txt",
    ] {
        refused(core.explorer_file(&project.id, &id, bad).await);
    }
    refused(core.explorer_tree(&project.id, &id, "..", "", false).await);
    // The repository's root is not a worktree this project may name.
    let above = worktree_id(&plain(&dir.path().join("work/mono")));
    refused(core.explorer_tree(&project.id, &above, "", "", false).await);
    refused(core.explorer_file(&project.id, &above, "a.txt").await);
}

#[cfg(unix)]
#[tokio::test]
async fn a_link_leaving_a_subfolder_project_is_refused() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, _root, sub) = subfolder_project(&core, &dir).await;
    std::os::unix::fs::symlink("../secret-label.txt", sub.join("up.txt")).unwrap();
    std::os::unix::fs::symlink("..", sub.join("updir")).unwrap();
    let id = main_id(&core, &project).await;
    refused(core.explorer_file(&project.id, &id, "up.txt").await);
    refused(
        core.explorer_file(&project.id, &id, "updir/secret-label.txt")
            .await,
    );
    refused(
        core.explorer_tree(&project.id, &id, "updir", "", false)
            .await,
    );
}

#[tokio::test]
async fn a_repository_root_project_is_unchanged() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, _root) = project(&core, &dir, "plain").await;
    let listed = core
        .explorer_worktrees(&project.id, None, false)
        .await
        .unwrap();
    assert!(listed.worktrees[0].is_main && !listed.worktrees[0].is_subfolder);
}

#[cfg(unix)]
#[tokio::test]
async fn a_subfolder_swapped_for_a_link_into_another_repository_is_refused() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, _root, sub) = subfolder_project(&core, &dir).await;
    let other = repo(&dir.path().join("work/other"));
    write(&other.join("pkg/marker-label.txt"), "label\n");
    commit_all(&other, "pkg");
    for target in [other.join("pkg"), other.clone()] {
        std::fs::remove_dir_all(&sub).unwrap();
        std::os::unix::fs::symlink(&target, &sub).unwrap();
        core.explorer.changed(&project.id);
        refused(core.explorer_worktrees(&project.id, None, true).await);
        let id = worktree_id(&plain(&target));
        refused(core.explorer_tree(&project.id, &id, "", "", true).await);
        refused(core.explorer_file(&project.id, &id, "a.txt").await);
        std::fs::remove_file(&sub).unwrap();
        std::fs::create_dir_all(&sub).unwrap();
    }
}

#[tokio::test]
async fn a_git_file_inside_the_subfolder_is_refused() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root, sub) = subfolder_project(&core, &dir).await;
    write(
        &sub.join(".git"),
        &format!("gitdir: {}\n", root.join(".git").display()),
    );
    core.explorer.changed(&project.id);
    refused(core.explorer_worktrees(&project.id, None, true).await);
}

#[tokio::test]
async fn a_subfolder_in_a_linked_worktree_or_a_locked_one_keeps_its_holder() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let root = repo(&dir.path().join("work/mono"));
    write(&root.join("pkg/app/main.rs"), "fn main() {}\n");
    commit_all(&root, "pkg");
    let linked = root.with_file_name("mono-wt").join("x");
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
    let sub = crate::util::canonicalize(&linked.join("pkg/app")).unwrap();
    let project = core.add_project(&sub.to_string_lossy()).await.unwrap();
    let project = core.store.trust_project(&project.id, "test").unwrap();
    let listed = core
        .explorer_worktrees(&project.id, None, false)
        .await
        .unwrap();
    assert_eq!(listed.worktrees.len(), 1);
    let row = &listed.worktrees[0];
    assert!(row.is_subfolder && !row.read_only);
    assert_eq!(row.branch.as_deref(), Some("feat/x"));
    run_git(&root, &["worktree", "lock", &linked.to_string_lossy()]);
    core.explorer.changed(&project.id);
    let listed = core
        .explorer_worktrees(&project.id, None, true)
        .await
        .unwrap();
    assert_eq!(
        listed.worktrees[0].read_only_reason,
        Some(ExplorerReadOnlyReason::Locked)
    );
}
