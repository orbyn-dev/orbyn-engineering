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
