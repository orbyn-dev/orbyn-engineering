use super::*;

#[tokio::test]
async fn change_letters_ignored_files_and_folders_come_from_one_status() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let id = main_id(&core, &project).await;
    write(&root.join("a.txt"), "changed\n");
    run_git(&root, &["mv", "src/old.rs", "src/renamed.rs"]);
    write(&root.join("src/new.rs"), "pub fn n() {}\n");
    run_git(&root, &["add", "src/new.rs"]);
    std::fs::remove_file(root.join("docs/x.md")).unwrap();
    std::fs::remove_dir(root.join("docs")).unwrap();
    write(&root.join("notes.txt"), "loose\n");
    write(&root.join("fresh/inner.txt"), "new\n");
    write(&root.join("x.log"), "ignored\n");
    write(&root.join("build/out.o"), "ignored\n");

    let top = core
        .explorer_tree(&project.id, &id, "", "", true)
        .await
        .unwrap();
    assert_eq!(top.changes, ExplorerChanges::Ready);
    assert_eq!(
        names(&top.entries),
        [
            "build",
            "docs",
            "fresh",
            "src",
            ".gitignore",
            "a.txt",
            "notes.txt",
            "x.log"
        ],
        "folders first, no .git"
    );
    assert!(entry(&top.entries, "build").ignored);
    assert!(entry(&top.entries, "x.log").ignored);
    assert!(!entry(&top.entries, "a.txt").ignored);
    assert_eq!(
        entry(&top.entries, "a.txt").change,
        Some(ExplorerChange::Modified)
    );
    assert_eq!(entry(&top.entries, "a.txt").size, Some(8));
    assert_eq!(
        entry(&top.entries, "notes.txt").change,
        Some(ExplorerChange::Untracked)
    );
    assert_eq!(
        entry(&top.entries, "fresh").change,
        Some(ExplorerChange::Untracked)
    );
    assert!(entry(&top.entries, "fresh").has_changes);
    assert!(entry(&top.entries, "src").has_changes);
    assert!(
        entry(&top.entries, "docs").has_changes,
        "a folder of deleted files"
    );
    assert_eq!(
        entry(&top.entries, "docs").kind,
        ExplorerEntryKind::Directory
    );

    let src = core
        .explorer_tree(&project.id, &id, "src", "", false)
        .await
        .unwrap();
    assert_eq!(names(&src.entries), ["lib.rs", "new.rs", "renamed.rs"]);
    assert_eq!(entry(&src.entries, "lib.rs").change, None);
    assert_eq!(
        entry(&src.entries, "new.rs").change,
        Some(ExplorerChange::Added)
    );
    assert_eq!(
        entry(&src.entries, "renamed.rs").change,
        Some(ExplorerChange::Renamed)
    );
    assert_eq!(src.path, "src");

    let docs = core
        .explorer_tree(&project.id, &id, "docs/", "", false)
        .await
        .unwrap();
    assert_eq!(docs.path, "docs");
    assert_eq!(docs.entries.len(), 1);
    assert_eq!(docs.entries[0].change, Some(ExplorerChange::Deleted));
    assert_eq!(docs.entries[0].path, "docs/x.md");

    let fresh = core
        .explorer_tree(&project.id, &id, "fresh", "", false)
        .await
        .unwrap();
    assert_eq!(fresh.entries[0].change, Some(ExplorerChange::Untracked));
    let build = core
        .explorer_tree(&project.id, &id, "build", "", false)
        .await
        .unwrap();
    assert!(
        build.entries[0].ignored,
        "everything under an ignored folder is ignored"
    );

    // A filter keeps names containing it, whatever the case.
    let filtered = core
        .explorer_tree(&project.id, &id, "src", "LIB", false)
        .await
        .unwrap();
    assert_eq!(names(&filtered.entries), ["lib.rs"]);

    // The dirty summary comes from the same kept status.
    let listed = core
        .explorer_worktrees(&project.id, None, false)
        .await
        .unwrap();
    let dirty = listed.worktrees[0].dirty.clone().unwrap();
    assert_eq!(
        (dirty.changed, dirty.untracked, dirty.conflicted),
        (4, 2, 0)
    );
}

#[tokio::test]
async fn a_repeat_read_is_kept_until_it_is_dropped_or_forced() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let id = main_id(&core, &project).await;
    let letter = |tree: &orbyn_contracts::ExplorerTree| entry(&tree.entries, "a.txt").change;
    let first = core
        .explorer_tree(&project.id, &id, "", "", true)
        .await
        .unwrap();
    assert_eq!(letter(&first), None);
    write(&root.join("a.txt"), "edited\n");
    // Within its fresh time the kept status answers, so the new edit is not there yet.
    let kept = core
        .explorer_tree(&project.id, &id, "", "", false)
        .await
        .unwrap();
    assert_eq!(letter(&kept), None);
    // A refresh the person pressed reads now.
    let forced = core
        .explorer_tree(&project.id, &id, "", "", true)
        .await
        .unwrap();
    assert_eq!(letter(&forced), Some(ExplorerChange::Modified));
    write(&root.join("a.txt"), "one\n");
    core.explorer.changed(&project.id);
    let dropped = core
        .explorer_tree(&project.id, &id, "", "", false)
        .await
        .unwrap();
    assert_eq!(letter(&dropped), None);
    write(&root.join("a.txt"), "again\n");
    core.explorer.invalidate();
    let silent = core
        .explorer_tree(&project.id, &id, "", "", false)
        .await
        .unwrap();
    assert_eq!(letter(&silent), Some(ExplorerChange::Modified));
}

#[tokio::test]
async fn a_huge_folder_is_cut_at_the_cap_and_says_so() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let id = main_id(&core, &project).await;
    for n in 0..(tree::TREE_LIMIT + 50) {
        write(&root.join("many").join(format!("f{n:05}.txt")), "");
    }
    let level = core
        .explorer_tree(&project.id, &id, "many", "", true)
        .await
        .unwrap();
    assert_eq!(level.entries.len(), tree::TREE_LIMIT);
    assert!(level.truncated);
    assert_eq!(level.entries[0].name, "f00000.txt");
    let narrowed = core
        .explorer_tree(&project.id, &id, "many", "f0100", false)
        .await
        .unwrap();
    assert!(!narrowed.truncated);
    assert_eq!(narrowed.entries.len(), 10);
    let small = core
        .explorer_tree(&project.id, &id, "src", "", false)
        .await
        .unwrap();
    assert!(!small.truncated);
}

#[tokio::test]
async fn files_read_as_text_binary_or_too_large() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let id = main_id(&core, &project).await;
    std::fs::write(root.join("blob.bin"), [0u8, 1, 2, 3]).unwrap();
    std::fs::write(root.join("latin.txt"), [0xffu8, 0xfe, b'a']).unwrap();
    let huge = std::fs::File::create(root.join("huge.log")).unwrap();
    huge.set_len(MAX_FILE_BYTES + 1).unwrap();
    write(&root.join("a.txt"), "edited\n");
    core.explorer_tree(&project.id, &id, "", "", true)
        .await
        .unwrap();

    let text = core.explorer_file(&project.id, &id, "a.txt").await.unwrap();
    assert_eq!(text.kind, ExplorerFileKind::Text);
    assert_eq!(text.content, "edited\n");
    assert_eq!(text.size, 7);
    assert_eq!(text.change, Some(ExplorerChange::Modified));
    assert_eq!(
        text.revision.as_deref(),
        Some(sha256_hex(b"edited\n").as_str())
    );
    assert!(!text.read_only);
    let plain = core
        .explorer_file(&project.id, &id, "src/lib.rs")
        .await
        .unwrap();
    assert_eq!(plain.change, None);
    for binary in ["blob.bin", "latin.txt"] {
        let read = core.explorer_file(&project.id, &id, binary).await.unwrap();
        assert_eq!(read.kind, ExplorerFileKind::Binary, "{binary}");
        assert!(read.content.is_empty() && read.revision.is_none());
    }
    let large = core
        .explorer_file(&project.id, &id, "huge.log")
        .await
        .unwrap();
    assert_eq!(large.kind, ExplorerFileKind::TooLarge);
    assert_eq!(large.size, MAX_FILE_BYTES + 1);
    assert!(large.content.is_empty());
    // Reading is read-only: the bytes on disk are what they were.
    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).unwrap(),
        "edited\n"
    );
}

#[tokio::test]
async fn a_status_past_its_maximum_age_is_never_shown() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let id = main_id(&core, &project).await;
    let letter = |tree: &orbyn_contracts::ExplorerTree| entry(&tree.entries, "a.txt").change;
    let first = core
        .explorer_tree(&project.id, &id, "", "", true)
        .await
        .unwrap();
    assert_eq!(letter(&first), None);
    write(&root.join("a.txt"), "edited\n");
    core.explorer
        .statuses
        .age_all(STATUS_MAX_AGE + Duration::from_secs(1));
    // Too old to show: this call waits for a new read instead of answering with it.
    let tree = core
        .explorer_tree(&project.id, &id, "", "", false)
        .await
        .unwrap();
    assert_eq!(letter(&tree), Some(ExplorerChange::Modified));
}

#[tokio::test]
async fn a_change_is_announced_to_the_windows_by_project() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, _) = project(&core, &dir, "app").await;
    let mut windows = core.shell.watch();
    core.explorer.changed(&project.id);
    assert_eq!(
        windows.events.try_recv().unwrap(),
        orbyn_contracts::ShellEvent::DataChanged {
            what: format!("explorer:{}", project.id)
        }
    );
    // A silent drop, as a write request makes, tells nobody.
    core.explorer.invalidate();
    assert!(windows.events.try_recv().is_err());
}
