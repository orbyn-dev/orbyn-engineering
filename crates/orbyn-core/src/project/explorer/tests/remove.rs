use super::*;

fn denied(result: Result<()>) {
    assert!(
        matches!(result, Err(CoreError::PolicyDenied(_))),
        "{result:?}"
    );
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

#[tokio::test]
async fn removal_rechecks_trust_inside_the_fence() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let root = repo(&dir.path().join("work/raw"));
    let project = core.add_project(&root.to_string_lossy()).await.unwrap();
    assert!(!project.trusted);
    let result = super::super::remove::effect(
        &core.store,
        &project.id,
        &root,
        &root,
        std::time::Duration::from_secs(20),
    )
    .await;
    assert!(
        matches!(result, Err(CoreError::FolderUntrusted)),
        "{result:?}"
    );
}

#[tokio::test]
async fn a_hung_git_is_given_up_on_with_a_generic_refusal() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let (path, _) = linked(&core, &project, &root, "slow").await;
    let result = super::super::remove::effect(
        &core.store,
        &project.id,
        &root,
        &path,
        std::time::Duration::ZERO,
    )
    .await;
    let error = result.unwrap_err();
    assert!(matches!(error, CoreError::Git(_)), "{error:?}");
    assert!(!error.to_string().contains(&*path.to_string_lossy()));
    assert!(path.is_dir());
}

#[tokio::test]
async fn a_failed_git_removal_names_no_path() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let (path, _) = linked(&core, &project, &root, "locked").await;
    // A locked worktree makes `git worktree remove` fail with a message naming the path.
    run_git(&root, &["worktree", "lock", &path.to_string_lossy()]);
    let error = super::super::remove::effect(
        &core.store,
        &project.id,
        &root,
        &path,
        std::time::Duration::from_secs(20),
    )
    .await
    .unwrap_err();
    let message = error.to_string();
    assert!(matches!(error, CoreError::Git(_)), "{error:?}");
    assert!(!message.contains(&*path.to_string_lossy()) && !message.contains("locked"));
    assert!(path.is_dir());
}
