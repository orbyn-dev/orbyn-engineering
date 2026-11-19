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

/// A linked worktree whose path Git lists under another spelling than the real one: the folder
/// a worktree was made under keeps its name in Git's records (`/var` for `/private/var`, a
/// Windows 8.3 short name). A link above the folder stands in for both here.
#[cfg(unix)]
#[tokio::test]
async fn a_linked_worktree_recorded_through_a_link_still_reads_and_writes_stay_inside_it() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let real = crate::util::canonicalize(dir.path()).unwrap().join("trees");
    std::fs::create_dir_all(&real).unwrap();
    let linked = real.join("linked");
    run_git(
        &root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "other",
            &linked.to_string_lossy(),
        ],
    );
    std::fs::write(linked.join("only-here.txt"), "x").unwrap();
    // Git now names the worktree through a link above it.
    let above = dir.path().join("above");
    std::os::unix::fs::symlink(&real, &above).unwrap();
    let spelled = above.join("linked");
    let record = root.join(".git/worktrees/linked/gitdir");
    std::fs::write(&record, format!("{}\n", spelled.join(".git").display())).unwrap();
    let listed = core
        .explorer_worktrees(&project.id, None, true)
        .await
        .unwrap();
    let entry = listed
        .worktrees
        .iter()
        .find(|w| !w.is_main)
        .expect("the linked worktree is listed");
    let id = entry.id.clone();

    // Reads still work.
    let tree = core
        .explorer_tree(&project.id, &id, "", "", true)
        .await
        .unwrap();
    assert!(tree.entries.iter().any(|e| e.name == "only-here.txt"));
    assert!(
        core.explorer_file(&project.id, &id, "only-here.txt")
            .await
            .is_ok()
    );

    // Writes land in the real folder and nowhere else.
    act(
        &core,
        &project,
        &id,
        Action::CreateFile {
            path: "made.txt".into(),
        },
    )
    .await
    .unwrap();
    assert!(linked.join("made.txt").is_file());
    for escape in ["../outside.txt", "src/../../outside.txt"] {
        assert!(
            act(
                &core,
                &project,
                &id,
                Action::CreateFile {
                    path: escape.into()
                }
            )
            .await
            .is_err(),
            "{escape}"
        );
    }
    assert!(!real.join("outside.txt").exists());
    assert!(!root.join("made.txt").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn an_open_engine_console_blocks_writes_like_a_running_session() {
    use crate::agents::console::CONSOLE_TERMINAL;
    use crate::host::terminal::{PtySpec, TerminalLabel};
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let id = main_id(&core, &project).await;
    let thread = thread_in(&core, &project, None).await;
    let spec = || PtySpec {
        program: "/bin/sleep".into(),
        args: vec!["30".into()],
        cwd: root.clone(),
        cols: 80,
        rows: 24,
        engine: false,
    };
    // A person's own terminal is not an agent and does not block.
    core.terminals
        .open_named(&thread.id, "mine", TerminalLabel::default(), spec())
        .unwrap();
    act(
        &core,
        &project,
        &id,
        Action::CreateFile { path: "one".into() },
    )
    .await
    .unwrap();
    // The engine's console runs an agent with no run record: it blocks.
    core.terminals
        .open_named(
            &thread.id,
            CONSOLE_TERMINAL,
            TerminalLabel::default(),
            spec(),
        )
        .unwrap();
    let denied = act(
        &core,
        &project,
        &id,
        Action::CreateFile { path: "two".into() },
    )
    .await;
    assert!(
        matches!(denied, Err(CoreError::PolicyDenied(_))),
        "{denied:?}"
    );
    assert!(!root.join("two").exists());
    core.terminals
        .named(&thread.id, CONSOLE_TERMINAL)
        .unwrap()
        .kill();
}

/// A move that only changes the case of a name, where the file system folds case (macOS,
/// Windows): the exclusive rename either renames the entry (APFS does) or sees the same entry
/// and refuses it as an existing destination (some volumes do). Both are fine; losing or
/// duplicating the file is not.
#[cfg(any(target_os = "macos", windows))]
#[tokio::test]
async fn a_case_only_rename_is_refused_as_an_existing_destination_where_case_folds() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let id = main_id(&core, &project).await;
    let moved = act(
        &core,
        &project,
        &id,
        Action::Move {
            path: "a.txt".into(),
            destination: "A.TXT".into(),
        },
    )
    .await;
    let names: Vec<String> = std::fs::read_dir(&root)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.eq_ignore_ascii_case("a.txt"))
        .collect();
    // Whatever the file system answers, the file exists once, under one of the two names, and
    // a refusal is the plain "destination exists" error, never a crash or a lost file.
    assert_eq!(names.len(), 1, "{names:?}");
    match moved {
        Ok(()) => assert_eq!(names[0], "A.TXT"),
        Err(error) => {
            assert!(matches!(error, CoreError::Invalid(_)), "{error:?}");
            assert_eq!(names[0], "a.txt");
        }
    }
}

/// Only the route handler may reach `explorer_file_action`, so no tool, crew task or engine
/// feature inside Orbyn calls it. That is all this checks: an agent that reads the service token
/// from the data folder and calls the HTTP route is a separate hole (#441). The route is a
/// protected one like every token route (`orbynd/tests/routes.json`, the 401 test in
/// `orbynd/tests/explorer.rs`) and needs the per-launch person key (`orbynd/tests/person_key.rs`).
#[test]
fn no_engine_agent_or_crew_code_calls_the_file_action() {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path
                    .file_name()
                    .is_some_and(|n| n != "target" && n != "node_modules")
                {
                    walk(&path, out);
                }
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    walk(&root, &mut files);
    let name = ["explorer_file", "_action"].concat();
    let mut callers: Vec<String> = files
        .iter()
        .filter(|path| {
            std::fs::read_to_string(path).is_ok_and(|text| text.contains(&format!("{name}(")))
        })
        .map(|path| {
            path.canonicalize()
                .unwrap()
                .strip_prefix(root.canonicalize().unwrap())
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    callers.sort();
    assert_eq!(
        callers,
        [
            "orbyn-core/src/project/explorer/tests/write.rs",
            "orbyn-core/src/project/explorer/write.rs",
            "orbynd/src/routes/explorer.rs",
        ]
    );
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn a_file_system_without_exclusive_rename_gets_a_clear_error_and_no_fallback() {
    use super::super::write::platform::unsupported;
    for code in [libc::ENOSYS, libc::ENOTSUP, libc::EINVAL] {
        let error = unsupported(std::io::Error::from_raw_os_error(code));
        // A mapped error is a plain `Unsupported` with no raw system code behind it.
        assert_eq!(error.kind(), std::io::ErrorKind::Unsupported, "{code}");
        assert_eq!(error.raw_os_error(), None, "{code} was not mapped");
    }
    let other = unsupported(std::io::Error::from_raw_os_error(libc::EEXIST));
    assert_eq!(other.raw_os_error(), Some(libc::EEXIST));
}

/// Where names fold case, a folder moved into itself under another spelling is refused by the
/// file system; the answer names that, not an unsupported file system.
#[cfg(any(target_os = "macos", windows))]
#[tokio::test]
async fn moving_a_folder_into_itself_by_another_spelling_says_so() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let id = main_id(&core, &project).await;
    std::fs::create_dir(root.join("Dir")).unwrap();
    if !root.join("dIR").exists() {
        return; // a case-sensitive volume: the spellings are two folders
    }
    let moved = act(
        &core,
        &project,
        &id,
        Action::Move {
            path: "Dir".into(),
            destination: "dIR/inner".into(),
        },
    )
    .await;
    match moved {
        Err(CoreError::Invalid(message)) => assert!(message.contains("inside itself"), "{message}"),
        other => panic!("{other:?}"),
    }
    assert!(root.join("Dir").is_dir());
}

#[tokio::test]
async fn a_folder_into_itself_is_refused_and_into_a_sibling_of_a_similar_name_is_not() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let id = main_id(&core, &project).await;
    std::fs::create_dir_all(root.join("Dir/child")).unwrap();
    std::fs::create_dir(root.join("Dir2")).unwrap();
    for destination in ["Dir/inner", "Dir/child/inner"] {
        let moved = act(
            &core,
            &project,
            &id,
            Action::Move {
                path: "Dir".into(),
                destination: destination.into(),
            },
        )
        .await;
        match moved {
            Err(CoreError::Invalid(message)) => {
                assert!(
                    message.contains("inside itself"),
                    "{destination}: {message}"
                )
            }
            other => panic!("{destination}: {other:?}"),
        }
    }
    assert!(root.join("Dir/child").is_dir());
    // `Dir2` only starts with the letters of `Dir`: it is another folder.
    act(
        &core,
        &project,
        &id,
        Action::Move {
            path: "Dir".into(),
            destination: "Dir2/x".into(),
        },
    )
    .await
    .unwrap();
    assert!(root.join("Dir2/x/child").is_dir());
}

#[test]
fn the_case_blind_inside_check_compares_whole_components() {
    use super::super::write::inside_ignoring_case;
    let outer = Path::new("/r/Dir");
    assert!(inside_ignoring_case(Path::new("/r/dIR/x"), outer));
    assert!(inside_ignoring_case(Path::new("/r/DIR/x/y"), outer));
    assert!(!inside_ignoring_case(Path::new("/r/Dir2/x"), outer));
    assert!(
        !inside_ignoring_case(Path::new("/r/dir"), outer),
        "the same entry is not inside"
    );
    assert!(!inside_ignoring_case(Path::new("/r"), outer));
}

#[tokio::test]
async fn watched_nested_folders_do_not_block_moving_or_deleting_their_ancestor() {
    let dir = TempDir::new();
    let core = std::sync::Arc::new(core(&dir).await);
    let (project, root) = project(&core, &dir, "app").await;
    let id = main_id(&core, &project).await;
    write(&root.join("a/b/c/f.txt"), "x");
    let watches = core
        .explorer_watch(&[(project.id.clone(), id.clone())])
        .await
        .unwrap();
    act(
        &core,
        &project,
        &id,
        Action::Move {
            path: "a/b".into(),
            destination: "a/moved".into(),
        },
    )
    .await
    .unwrap();
    for path in ["a/moved/c/f.txt", "a/moved/c", "a/moved"] {
        act(&core, &project, &id, Action::Delete { path: path.into() })
            .await
            .unwrap();
    }
    assert!(!root.join("a/moved").exists());
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(300), watches[0].stopped())
            .await
            .is_err(),
        "the watch stays live"
    );
}
