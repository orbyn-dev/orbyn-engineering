use super::*;
use orbyn_contracts::{ExplorerFileAction as Action, PolicyEffect};

async fn act(core: &Core, project: &Project, id: &str, action: Action) -> Result<()> {
    core.explorer_file_action(&project.id, id, &action, "test")
        .await
}

#[tokio::test]
async fn actions_create_move_rename_and_delete_without_a_thread() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let id = main_id(&core, &project).await;
    act(
        &core,
        &project,
        &id,
        Action::CreateDirectory { path: "new".into() },
    )
    .await
    .unwrap();
    act(
        &core,
        &project,
        &id,
        Action::CreateFile {
            path: "new/空-🛰.txt".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(root.join("new/空-🛰.txt")).unwrap(), b"");
    act(
        &core,
        &project,
        &id,
        Action::Move {
            path: "new".into(),
            destination: "renamed".into(),
        },
    )
    .await
    .unwrap();
    assert!(!root.join("new").exists());
    assert!(root.join("renamed/空-🛰.txt").is_file());
    act(
        &core,
        &project,
        &id,
        Action::Move {
            path: "renamed/空-🛰.txt".into(),
            destination: "空-🛰.txt".into(),
        },
    )
    .await
    .unwrap();
    act(
        &core,
        &project,
        &id,
        Action::Delete {
            path: "renamed".into(),
        },
    )
    .await
    .unwrap();
    act(
        &core,
        &project,
        &id,
        Action::Delete {
            path: "空-🛰.txt".into(),
        },
    )
    .await
    .unwrap();
    assert!(!root.join("空-🛰.txt").exists());
    let audit = core.store.list_audit(None, 100).unwrap();
    assert_eq!(
        audit
            .iter()
            .filter(|row| row.action == "explorer.file_action_result"
                && row.decision == Some(PolicyEffect::Allow))
            .count(),
        6
    );
}

#[tokio::test]
async fn occupied_destinations_and_nonempty_deletion_preserve_data() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let id = main_id(&core, &project).await;
    for action in [
        Action::CreateFile {
            path: "a.txt".into(),
        },
        Action::CreateDirectory { path: "src".into() },
        Action::Move {
            path: "src/lib.rs".into(),
            destination: "a.txt".into(),
        },
        Action::Move {
            path: "src".into(),
            destination: "docs".into(),
        },
        Action::Move {
            path: "src".into(),
            destination: "src/child".into(),
        },
        Action::Delete { path: "src".into() },
    ] {
        assert!(act(&core, &project, &id, action).await.is_err());
    }
    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).unwrap(),
        "one\n"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("src/lib.rs")).unwrap(),
        "pub fn x() {}\n"
    );
    assert!(root.join("docs/x.md").exists());
}

#[tokio::test]
async fn every_action_refuses_escapes_git_and_the_root_and_audits_denials() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let id = main_id(&core, &project).await;
    for path in [
        "",
        ".",
        "..",
        "../outside",
        ".git/config",
        "src/.GIT",
        "a\\b",
        "/absolute",
    ] {
        for action in [
            Action::CreateFile { path: path.into() },
            Action::CreateDirectory { path: path.into() },
            Action::Delete { path: path.into() },
            Action::Move {
                path: "a.txt".into(),
                destination: path.into(),
            },
            Action::Move {
                path: path.into(),
                destination: "elsewhere".into(),
            },
        ] {
            assert!(act(&core, &project, &id, action).await.is_err(), "{path:?}");
        }
    }
    assert!(root.join("a.txt").exists());
    assert!(!root.join("elsewhere").exists());
    let audit = core.store.list_audit(None, 100).unwrap();
    assert_eq!(
        audit
            .iter()
            .filter(|row| row.action == "explorer.file_action_result"
                && row.decision == Some(PolicyEffect::Deny))
            .count(),
        40
    );
}

#[tokio::test]
async fn writes_refresh_locked_worktrees_instead_of_using_the_read_cache() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let linked = root.with_file_name("app-wt").join("feature");
    run_git(
        &root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feature",
            &linked.to_string_lossy(),
        ],
    );
    let listed = core
        .explorer_worktrees(&project.id, None, true)
        .await
        .unwrap();
    let id = &listed
        .worktrees
        .iter()
        .find(|w| w.name == "feature")
        .unwrap()
        .id;
    run_git(&root, &["worktree", "lock", &linked.to_string_lossy()]);
    for action in [
        Action::CreateFile { path: "new".into() },
        Action::CreateDirectory {
            path: "new-dir".into(),
        },
        Action::Move {
            path: "a.txt".into(),
            destination: "renamed".into(),
        },
        Action::Delete {
            path: "a.txt".into(),
        },
    ] {
        assert!(act(&core, &project, id, action).await.is_err());
    }
    assert!(linked.join("a.txt").exists());
    assert!(!linked.join("new").exists());
    run_git(&root, &["worktree", "unlock", &linked.to_string_lossy()]);
    act(
        &core,
        &project,
        id,
        Action::CreateFile { path: "new".into() },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn untrusted_unknown_and_running_targets_refuse_mutation() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let id = main_id(&core, &project).await;
    let untrusted_root = repo(&dir.path().join("untrusted"));
    let untrusted = core
        .add_project(&untrusted_root.to_string_lossy())
        .await
        .unwrap();
    assert!(matches!(
        act(
            &core,
            &untrusted,
            &id,
            Action::Delete {
                path: "a.txt".into()
            }
        )
        .await,
        Err(CoreError::FolderUntrusted)
    ));
    assert!(
        act(
            &core,
            &project,
            "wt-0000000000000000",
            Action::Delete {
                path: "a.txt".into()
            }
        )
        .await
        .is_err()
    );
    let thread = thread_in(&core, &project, None).await;
    core.store
        .admit_run(Admission {
            thread_id: &thread.id,
            prompt: "go",
            actor: "test",
        })
        .unwrap();
    assert!(
        act(
            &core,
            &project,
            &id,
            Action::Delete {
                path: "a.txt".into()
            }
        )
        .await
        .is_err()
    );
    assert!(root.join("a.txt").exists());
}

#[tokio::test]
async fn concurrent_creates_have_one_winner() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, _) = project(&core, &dir, "app").await;
    let id = main_id(&core, &project).await;
    let action = Action::CreateFile { path: "one".into() };
    let (a, b) = tokio::join!(
        core.explorer_file_action(&project.id, &id, &action, "a"),
        core.explorer_file_action(&project.id, &id, &action, "b")
    );
    assert_ne!(a.is_ok(), b.is_ok());
}

#[tokio::test]
async fn application_data_is_not_a_mutation_destination() {
    let dir = TempDir::new();
    let library = library(&dir.path().join("library"));
    let root = dir.path().join("work/app");
    std::fs::create_dir_all(&root).unwrap();
    let mut config = CoreConfig::new(root.join("data"), "http://127.0.0.1:1".into());
    config.engines.lookup_latest = false;
    let core = Core::open(config).unwrap();
    core.workspace
        .setup(WorkspaceSetup {
            url: None,
            adopt_path: Some(library.to_string_lossy().into_owned()),
        })
        .await
        .unwrap();
    let project = core.add_project(&root.to_string_lossy()).await.unwrap();
    let project = core.store.trust_project(&project.id, "test").unwrap();
    let id = main_id(&core, &project).await;
    for action in [
        Action::CreateFile {
            path: "data/new".into(),
        },
        Action::Delete {
            path: "data".into(),
        },
        Action::Move {
            path: "data".into(),
            destination: "moved".into(),
        },
    ] {
        assert!(act(&core, &project, &id, action).await.is_err());
    }
    assert!(!root.join("data/new").exists());
    assert!(root.join("data").is_dir());
    act(
        &core,
        &project,
        &id,
        Action::CreateFile {
            path: "normal".into(),
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn a_disappeared_worktree_cannot_be_recreated_by_an_action() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let linked = root.with_file_name("app-wt").join("gone");
    run_git(
        &root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "gone",
            &linked.to_string_lossy(),
        ],
    );
    // Keep a stale membership list without starting a background status process
    // in the directory the fixture is about to remove (Windows holds its cwd).
    core.explorer_members(&project, true).await.unwrap();
    let id = worktree_id(&plain(&linked));
    std::fs::remove_dir_all(&linked).unwrap();
    assert!(
        act(
            &core,
            &project,
            &id,
            Action::CreateDirectory { path: "new".into() }
        )
        .await
        .is_err()
    );
    assert!(!linked.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn links_and_folder_moves_carrying_git_are_refused() {
    use std::os::unix::fs::symlink;
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let id = main_id(&core, &project).await;
    symlink("src", root.join("alias")).unwrap();
    symlink("a.txt", root.join("linked-file")).unwrap();
    for action in [
        Action::CreateFile {
            path: "alias/new".into(),
        },
        Action::Delete {
            path: "linked-file".into(),
        },
        Action::Move {
            path: "alias".into(),
            destination: "moved".into(),
        },
    ] {
        assert!(act(&core, &project, &id, action).await.is_err());
    }
    write(&root.join("nested/.git/config"), "protected");
    assert!(
        act(
            &core,
            &project,
            &id,
            Action::Move {
                path: "nested".into(),
                destination: "moved".into()
            }
        )
        .await
        .is_err()
    );
    assert!(root.join("nested/.git/config").exists());
    assert!(!root.join("src/new").exists());
    assert!(root.join("a.txt").exists());
}
