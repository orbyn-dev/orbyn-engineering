use super::*;

#[tokio::test]
async fn worktrees_list_the_primary_first_with_branch_threads_and_flags() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let main_thread = thread_in(&core, &project, None).await;
    let feature_thread = thread_in(&core, &project, Some("feature")).await;
    let wt = root.with_file_name("app-wt");
    run_git(
        &root,
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            &wt.join("loose").to_string_lossy(),
        ],
    );
    run_git(
        &root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feat/locked",
            &wt.join("locked").to_string_lossy(),
        ],
    );
    run_git(
        &root,
        &["worktree", "lock", &wt.join("locked").to_string_lossy()],
    );
    run_git(
        &root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feat/gone",
            &wt.join("gone").to_string_lossy(),
        ],
    );
    std::fs::remove_dir_all(wt.join("gone")).unwrap();

    let listed = core
        .explorer_worktrees(&project.id, None, false)
        .await
        .unwrap();
    assert!(listed.git);
    let by = |name: &str| {
        listed
            .worktrees
            .iter()
            .find(|worktree| worktree.name == name)
            .unwrap_or_else(|| panic!("no worktree {name}"))
    };
    let main = &listed.worktrees[0];
    assert!(main.is_main && main.branch.as_deref() == Some("main") && !main.read_only);
    assert_eq!(main.head.len(), 7);
    assert_eq!(main.threads.len(), 1);
    assert_eq!(main.threads[0].id, main_thread.id);
    assert_eq!(by("feature").threads[0].id, feature_thread.id);
    assert_eq!(by("feature").threads[0].title, "work feature");
    assert_eq!(by("feature").branch.as_deref(), Some("feat/feature"));
    assert!(!by("feature").is_main);
    assert!(by("loose").detached && by("loose").branch.is_none());
    assert_eq!(
        by("locked").read_only_reason,
        Some(ExplorerReadOnlyReason::Locked)
    );
    assert!(by("locked").read_only);
    assert_eq!(
        by("gone").read_only_reason,
        Some(ExplorerReadOnlyReason::Missing)
    );
    // Ids are stable and distinct, and say nothing the path does not.
    let ids: std::collections::BTreeSet<_> =
        listed.worktrees.iter().map(|w| w.id.clone()).collect();
    assert_eq!(ids.len(), listed.worktrees.len());
    assert!(listed.worktrees.iter().all(|w| well_formed(&w.id)));
}

#[tokio::test]
async fn a_worktree_another_thread_runs_in_is_read_only_for_the_asking_thread() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, _) = project(&core, &dir, "app").await;
    let main_thread = thread_in(&core, &project, None).await;
    let feature = thread_in(&core, &project, Some("feature")).await;
    core.store
        .admit_run(Admission {
            thread_id: &feature.id,
            prompt: "go",
            actor: "test",
        })
        .unwrap();
    let state = |listed: &orbyn_contracts::ExplorerWorktrees| {
        let worktree = listed
            .worktrees
            .iter()
            .find(|w| w.name == "feature")
            .unwrap();
        (worktree.read_only_reason, worktree.threads[0].running)
    };
    let alone = core
        .explorer_worktrees(&project.id, None, false)
        .await
        .unwrap();
    assert_eq!(state(&alone), (None, true));
    let from_main = core
        .explorer_worktrees(&project.id, Some(&main_thread.id), false)
        .await
        .unwrap();
    assert_eq!(
        state(&from_main),
        (Some(ExplorerReadOnlyReason::OtherSession), true)
    );
    let from_owner = core
        .explorer_worktrees(&project.id, Some(&feature.id), false)
        .await
        .unwrap();
    assert_eq!(state(&from_owner), (None, true));
    refused(
        core.explorer_worktrees(&project.id, Some("nope"), false)
            .await,
    );
}

#[tokio::test]
async fn a_missing_worktree_is_listed_read_only_and_cannot_be_browsed() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let gone = root.with_file_name("app-wt").join("gone");
    run_git(
        &root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feat/gone",
            &gone.to_string_lossy(),
        ],
    );
    let locked = root.with_file_name("app-wt").join("locked");
    run_git(
        &root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feat/locked",
            &locked.to_string_lossy(),
        ],
    );
    run_git(&root, &["worktree", "lock", &locked.to_string_lossy()]);
    // Gone before anything lists it: a status read running in a worktree would hold the
    // folder open on Windows.
    let gone_id = worktree_id(&plain(&gone));
    std::fs::remove_dir_all(&gone).unwrap();
    let listed = core
        .explorer_worktrees(&project.id, None, false)
        .await
        .unwrap();
    let id_of = |name: &str| {
        if name == "gone" {
            return gone_id.clone();
        }
        listed
            .worktrees
            .iter()
            .find(|w| w.name == name)
            .unwrap()
            .id
            .clone()
    };
    assert!(
        listed
            .worktrees
            .iter()
            .any(|w| w.id == gone_id && w.read_only_reason == Some(ExplorerReadOnlyReason::Missing))
    );
    assert!(matches!(
        core.explorer_tree(&project.id, &id_of("gone"), "", "", false)
            .await,
        Err(CoreError::NotFound(_))
    ));
    // A locked one can be read, and says it is read-only.
    let tree = core
        .explorer_tree(&project.id, &id_of("locked"), "", "", false)
        .await
        .unwrap();
    assert!(tree.read_only);
    let file = core
        .explorer_file(&project.id, &id_of("locked"), "a.txt")
        .await
        .unwrap();
    assert!(file.read_only);
}

#[tokio::test]
async fn a_folder_outside_git_has_one_worktree_and_no_letters() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let plain_dir = dir.path().join("work/plain");
    write(&plain_dir.join("notes.txt"), "hello\n");
    let project = core
        .add_project(&plain_dir.to_string_lossy())
        .await
        .unwrap();
    let project = core.store.trust_project(&project.id, "test").unwrap();
    assert!(!project.is_git);
    let listed = core
        .explorer_worktrees(&project.id, None, false)
        .await
        .unwrap();
    assert!(!listed.git);
    assert_eq!(listed.worktrees.len(), 1);
    let only = &listed.worktrees[0];
    assert!(only.is_main && only.branch.is_none() && only.dirty.is_none());
    let top = core
        .explorer_tree(&project.id, &only.id, "", "", false)
        .await
        .unwrap();
    assert_eq!(top.changes, ExplorerChanges::Unavailable);
    assert_eq!(names(&top.entries), ["notes.txt"]);
    let file = core
        .explorer_file(&project.id, &only.id, "notes.txt")
        .await
        .unwrap();
    assert_eq!(file.content, "hello\n");
}
