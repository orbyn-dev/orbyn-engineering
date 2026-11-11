use super::*;

#[tokio::test]
async fn paths_outside_the_worktree_or_in_git_are_refused() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let id = main_id(&core, &project).await;
    write(&root.join("../outside.txt"), "outside\n");
    let outside = dir.path().join("work/outside.txt");
    for bad in [
        "..",
        "../outside.txt",
        "src/../../outside.txt",
        "src/..",
        "..\\outside.txt",
        "src\\lib.rs",
        ".git",
        ".git/config",
        ".GIT/config",
        "src/.Git/x",
        "a\0b",
    ] {
        refused(core.explorer_tree(&project.id, &id, bad, "", false).await);
        refused(core.explorer_file(&project.id, &id, bad).await);
    }
    for absolute in [
        outside.to_string_lossy().into_owned(),
        "/etc/hosts".to_owned(),
    ] {
        refused(
            core.explorer_tree(&project.id, &id, &absolute, "", false)
                .await,
        );
        refused(core.explorer_file(&project.id, &id, &absolute).await);
    }
    // A decoded-looking name is only a name, and a missing one.
    refused(
        core.explorer_file(&project.id, &id, "%2e%2e/outside.txt")
            .await,
    );
    refused(core.explorer_file(&project.id, &id, "src").await);
    refused(core.explorer_file(&project.id, &id, "").await);
    refused(core.explorer_file(&project.id, &id, "src/none.rs").await);
    assert!(
        core.explorer_file(&project.id, &id, "src/lib.rs")
            .await
            .is_ok()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_link_out_of_the_worktree_or_into_git_is_listed_but_never_followed() {
    use std::os::unix::fs::symlink;
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let id = main_id(&core, &project).await;
    let outside = dir.path().join("elsewhere");
    write(&outside.join("secret.txt"), "from outside\n");
    symlink(&outside, root.join("out-dir")).unwrap();
    symlink(outside.join("secret.txt"), root.join("out-file")).unwrap();
    symlink(".git", root.join("meta")).unwrap();
    symlink("../src/lib.rs", root.join("docs-link")).unwrap();
    symlink("src", root.join("in-dir")).unwrap();
    symlink("a.txt", root.join("alias.txt")).unwrap();

    let top = core
        .explorer_tree(&project.id, &id, "", "", true)
        .await
        .unwrap();
    for link in ["out-dir", "out-file", "meta", "in-dir", "alias.txt"] {
        let listed = entry(&top.entries, link);
        assert_eq!(listed.kind, ExplorerEntryKind::Symlink, "{link}");
        assert_eq!(listed.size, None);
    }
    refused(
        core.explorer_tree(&project.id, &id, "out-dir", "", false)
            .await,
    );
    refused(
        core.explorer_file(&project.id, &id, "out-dir/secret.txt")
            .await,
    );
    refused(core.explorer_file(&project.id, &id, "out-file").await);
    refused(
        core.explorer_tree(&project.id, &id, "meta", "", false)
            .await,
    );
    refused(core.explorer_file(&project.id, &id, "meta/config").await);
    refused(core.explorer_file(&project.id, &id, "docs-link").await);
    // A link that stays inside the worktree is read at its target.
    let alias = core
        .explorer_file(&project.id, &id, "alias.txt")
        .await
        .unwrap();
    assert_eq!(alias.content, "one\n");
    let inner = core
        .explorer_tree(&project.id, &id, "in-dir", "", false)
        .await
        .unwrap();
    assert!(names(&inner.entries).contains(&"lib.rs"));
}

#[cfg(unix)]
#[tokio::test]
async fn a_pipe_is_neither_listed_nor_opened() {
    use std::os::unix::ffi::OsStrExt;
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let id = main_id(&core, &project).await;
    let name = std::ffi::CString::new(root.join("pipe").as_os_str().as_bytes()).unwrap();
    // SAFETY: name owns a NUL-terminated path for the duration of mkfifo.
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let top = core
        .explorer_tree(&project.id, &id, "", "", true)
        .await
        .unwrap();
    assert!(!names(&top.entries).contains(&"pipe"));
    refused(core.explorer_file(&project.id, &id, "pipe").await);
}

#[tokio::test]
async fn a_linked_worktrees_git_pointer_file_is_out_of_reach() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, _) = project(&core, &dir, "app").await;
    thread_in(&core, &project, Some("feature")).await;
    let listed = core
        .explorer_worktrees(&project.id, None, false)
        .await
        .unwrap();
    let feature = listed
        .worktrees
        .iter()
        .find(|w| w.name == "feature")
        .unwrap();
    assert!(Path::new(&feature.path).join(".git").is_file());
    let top = core
        .explorer_tree(&project.id, &feature.id, "", "", false)
        .await
        .unwrap();
    assert!(!names(&top.entries).contains(&".git"));
    refused(core.explorer_file(&project.id, &feature.id, ".git").await);
    assert_eq!(top.worktree_id, feature.id);
}

#[tokio::test]
async fn a_project_cannot_reach_another_projects_worktree() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (first, _) = project(&core, &dir, "first").await;
    let (second, second_root) = project(&core, &dir, "second").await;
    let (own, foreign) = (main_id(&core, &first).await, main_id(&core, &second).await);
    assert_ne!(own, foreign);
    // The id is a digest of the path, so one computed by hand is no more than a guess.
    let forged = worktree_id(&plain(&second_root));
    assert_eq!(forged, foreign);
    for id in [
        foreign.as_str(),
        "wt-0000000000000000",
        "wt-xyz",
        "../second",
        "",
        "main",
    ] {
        refused(core.explorer_tree(&first.id, id, "", "", false).await);
        refused(core.explorer_file(&first.id, id, "a.txt").await);
    }
    assert!(
        core.explorer_tree(&second.id, &foreign, "", "", false)
            .await
            .is_ok()
    );
    refused(
        core.explorer_worktrees("no-such-project", None, false)
            .await,
    );
    // A thread of another project cannot stand in for one of this project.
    let alien = thread_in(&core, &second, None).await;
    refused(
        core.explorer_worktrees(&first.id, Some(&alien.id), false)
            .await,
    );
}

#[tokio::test]
async fn an_untrusted_project_is_not_browsed() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let root = repo(&dir.path().join("work/raw"));
    let project = core.add_project(&root.to_string_lossy()).await.unwrap();
    assert!(!project.trusted);
    assert!(matches!(
        core.explorer_worktrees(&project.id, None, false).await,
        Err(CoreError::FolderUntrusted)
    ));
    assert!(matches!(
        core.explorer_tree(&project.id, "wt-0000000000000000", "", "", false)
            .await,
        Err(CoreError::FolderUntrusted)
    ));
}

#[test]
fn the_guard_hides_orbyns_own_data_and_the_librarys_repo_folder() {
    let dir = TempDir::new();
    let root = dir.path().join("lib");
    write(&root.join("data/token"), "label\n");
    write(&root.join("repo/other/x"), "x\n");
    write(&root.join("skills/a.md"), "a\n");
    let guard = Guard::new(&root, &root.join("data"), Some(root.join("repo"))).unwrap();
    assert!(guard.resolve("skills/a.md").is_ok());
    for hidden in ["data", "data/token", "repo", "repo/other/x"] {
        assert!(guard.resolve(hidden).is_err(), "{hidden}");
    }
    assert!(guard.read("data/token", 100).is_err());
    assert!(!guard.lists(&crate::util::canonicalize(&root).unwrap(), "data"));
    assert!(!guard.lists(&crate::util::canonicalize(&root).unwrap(), "repo"));
    assert!(guard.lists(&crate::util::canonicalize(&root).unwrap(), "skills"));
}
