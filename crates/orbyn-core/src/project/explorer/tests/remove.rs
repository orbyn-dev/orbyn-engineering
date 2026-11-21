use super::*;

fn denied(result: Result<()>) {
    assert!(matches!(result, Err(CoreError::PolicyDenied(_))), "{result:?}");
}

/// A linked worktree `name` under the project's sibling `<repo>-wt` root, and its id.
async fn linked(core: &Core, project: &Project, root: &Path, name: &str) -> (PathBuf, String) {
    let path = root.with_file_name("app-wt").join(name);
    run_git(
        root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            &format!("feat/{name}"),
            &path.to_string_lossy(),
        ],
    );
    let listed = core
        .explorer_worktrees(&project.id, None, true)
        .await
        .unwrap();
    let id = listed
        .worktrees
        .iter()
        .find(|worktree| worktree.name == name)
        .unwrap_or_else(|| panic!("no worktree {name}"))
        .id
        .clone();
    (path, id)
}

#[tokio::test]
async fn a_clean_worktree_is_removed() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let (path, id) = linked(&core, &project, &root, "clean").await;
    core.remove_worktree(&project.id, &id, "test")
        .await
        .unwrap();
    assert!(!path.exists());
}

#[tokio::test]
async fn a_worktree_with_an_ignored_file_is_refused_and_keeps_it() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let (path, id) = linked(&core, &project, &root, "env").await;
    // `*.log` and `build/` are ignored by the fixture's .gitignore.
    write(&path.join("local.log"), "secret\n");
    write(&path.join("build/out.bin"), "x");
    let error = core
        .remove_worktree(&project.id, &id, "test")
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::PolicyDenied(_)), "{error:?}");
    let message = error.to_string();
    assert!(!message.contains("local.log") && !message.contains(&*path.to_string_lossy()));
    assert_eq!(std::fs::read(path.join("local.log")).unwrap(), b"secret\n");
    assert!(path.join("build/out.bin").is_file());
}

#[tokio::test]
async fn a_worktree_with_an_untracked_file_is_refused() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let (path, id) = linked(&core, &project, &root, "dirty").await;
    write(&path.join("notes.txt"), "mine\n");
    denied(core.remove_worktree(&project.id, &id, "test").await);
    assert!(path.join("notes.txt").is_file());
}

#[tokio::test]
async fn a_worktree_a_thread_uses_is_refused() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let (path, id) = linked(&core, &project, &root, "busy").await;
    thread_in(&core, &project, Some("busy")).await;
    denied(core.remove_worktree(&project.id, &id, "test").await);
    assert!(path.is_dir());
}
