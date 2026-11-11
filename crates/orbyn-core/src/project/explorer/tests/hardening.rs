use super::*;

#[tokio::test]
async fn a_worktree_whose_gitdir_was_redirected_is_never_read() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let wt = root.with_file_name("app-wt");
    // Two linked worktrees, then Git's own records are pointed at a folder that holds
    // something the repository has no claim on.
    for name in ["gone", "folder"] {
        run_git(
            &root,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                &format!("feat/{name}"),
                &wt.join(name).to_string_lossy(),
            ],
        );
    }
    let secret = dir.path().join("elsewhere-gone");
    write(&secret.join("token-label.txt"), "label\n");
    let folder_secret = dir.path().join("elsewhere-folder");
    write(&folder_secret.join("token-label.txt"), "label\n");
    std::fs::create_dir_all(folder_secret.join(".git")).unwrap();
    let redirect = |name: &str, target: &Path| {
        let file = root.join(".git/worktrees").join(name).join("gitdir");
        std::fs::write(file, format!("{}\n", target.join(".git").display())).unwrap();
    };
    // `gone`: the new target has no `.git`, so Git calls the entry prunable.
    redirect("gone", &secret);
    // `folder`: the new target has a `.git` folder, so Git sees nothing wrong.
    redirect("folder", &folder_secret);
    let listed = core
        .explorer_worktrees(&project.id, None, true)
        .await
        .unwrap();
    for target in [&secret, &folder_secret] {
        let id = worktree_id(&plain(target));
        refused(core.explorer_tree(&project.id, &id, "", "", false).await);
        refused(
            core.explorer_file(&project.id, &id, "token-label.txt")
                .await,
        );
        if let Some(shown) = listed.worktrees.iter().find(|w| w.id == id) {
            assert!(
                shown.dirty.is_none(),
                "no status is read in a folder that is no member"
            );
        }
    }
    // The honest worktrees still open.
    let honest = core
        .explorer_tree(&project.id, &main_id(&core, &project).await, "", "", false)
        .await
        .unwrap();
    assert!(!honest.entries.is_empty());
}

#[tokio::test]
async fn a_bare_repository_is_not_a_worktree_and_its_git_folder_is_out_of_reach() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let source = repo(&dir.path().join("work/source"));
    let bare = dir.path().join("work/bare.git");
    run_git(
        dir.path(),
        &[
            "clone",
            "-q",
            "--bare",
            &source.to_string_lossy(),
            &bare.to_string_lossy(),
        ],
    );
    let checkout = dir.path().join("work/checkout");
    run_git(
        &bare,
        &["worktree", "add", "-q", &checkout.to_string_lossy(), "main"],
    );
    let project = core.add_project(&checkout.to_string_lossy()).await.unwrap();
    let project = core.store.trust_project(&project.id, "test").unwrap();
    let listed = core
        .explorer_worktrees(&project.id, None, false)
        .await
        .unwrap();
    assert_eq!(listed.worktrees.len(), 1, "{:?}", listed.worktrees);
    assert_eq!(listed.worktrees[0].name, "checkout");
    assert!(
        !listed.worktrees[0].is_main,
        "a bare repository is no main checkout"
    );
    let id = worktree_id(&plain(&bare));
    refused(core.explorer_tree(&project.id, &id, "", "", false).await);
    refused(core.explorer_file(&project.id, &id, "config").await);
    let own = core
        .explorer_tree(&project.id, &listed.worktrees[0].id, "", "", false)
        .await
        .unwrap();
    assert!(names(&own.entries).contains(&"a.txt"));
}

#[cfg(unix)]
#[tokio::test]
async fn a_repository_filter_does_not_run_when_status_is_read() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let id = main_id(&core, &project).await;
    write(&root.join(".gitattributes"), "*.txt filter=spy\n");
    commit_all(&root, "attributes");
    let marker = dir.path().join("filter-ran");
    let script = dir.path().join("spy.sh");
    std::fs::write(&script, "#!/bin/sh\ntouch \"$1\"\ncat\n").unwrap();
    run_git(
        &root,
        &[
            "config",
            "filter.spy.clean",
            &format!("sh {} {}", script.display(), marker.display()),
        ],
    );
    // Control: plain Git runs the filter on a file whose timestamp moved.
    write(&root.join("a.txt"), "one\n");
    run_git(&root, &["status", "--porcelain"]);
    assert!(
        marker.exists(),
        "the control must show the filter would run"
    );
    std::fs::remove_file(&marker).unwrap();
    write(&root.join("a.txt"), "one\n");
    let tree = core
        .explorer_tree(&project.id, &id, "", "", true)
        .await
        .unwrap();
    assert_eq!(tree.changes, ExplorerChanges::Ready);
    assert!(
        !marker.exists(),
        "the explorer's status ran a repository filter"
    );
}

#[tokio::test]
async fn a_made_up_id_does_not_make_git_list_on_every_request() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let (project, root) = project(&core, &dir, "app").await;
    let bogus = "wt-0123456789abcdef";
    refused(core.explorer_tree(&project.id, bogus, "", "", false).await);
    // The miss was recorded: a worktree made now is not looked for until the list is
    // dropped, which a run ending or a write does.
    let fresh = root.with_file_name("app-wt").join("fresh");
    run_git(
        &root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feat/fresh",
            &fresh.to_string_lossy(),
        ],
    );
    let id = worktree_id(&plain(&fresh));
    refused(core.explorer_tree(&project.id, &id, "", "", false).await);
    core.explorer.changed(&project.id);
    assert!(
        core.explorer_tree(&project.id, &id, "", "", false)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn a_burst_of_changes_is_announced_once_then_once_more() {
    let reads = ExplorerReads::default();
    let heard = Arc::new(Mutex::new(Vec::new()));
    let sink = heard.clone();
    reads.set_notify(
        &(Arc::new(move |topic: &str| sink.lock().unwrap().push(topic.to_owned())) as Notify),
    );
    for _ in 0..32 {
        reads.lists.announce("explorer:p");
    }
    reads.statuses.announce("explorer:other");
    assert_eq!(*heard.lock().unwrap(), ["explorer:p", "explorer:other"]);
    tokio::time::sleep(ANNOUNCE_GAP * 3).await;
    assert_eq!(
        *heard.lock().unwrap(),
        ["explorer:p", "explorer:other", "explorer:p"]
    );
}

#[test]
fn one_project_cannot_hold_another_projects_status_permits() {
    let reads = ExplorerReads::default();
    let held: Vec<_> = (0..STATUS_READS)
        .map(|_| reads.gate("slow").try_acquire_owned().unwrap())
        .collect();
    assert!(reads.gate("slow").try_acquire_owned().is_err());
    assert!(reads.gate("other").try_acquire_owned().is_ok());
    drop(held);
}

#[cfg(unix)]
#[test]
fn a_folder_that_is_a_link_is_not_opened() {
    let dir = TempDir::new();
    let root = dir.path().join("tree");
    write(&root.join("src/a.rs"), "a\n");
    write(&dir.path().join("elsewhere/secret-label.txt"), "label\n");
    std::os::unix::fs::symlink(dir.path().join("elsewhere"), root.join("link")).unwrap();
    let guard = Guard::new(&root, &dir.path().join("data"), None).unwrap();
    let real = crate::util::canonicalize(&root).unwrap();
    assert!(guard.open_dir(&real.join("src")).is_ok());
    assert!(guard.open_dir(&real.join("link")).is_err());
    // A handle to a folder outside is refused by its own path.
    let outside = std::fs::File::open(dir.path().join("elsewhere")).unwrap();
    assert!(guard.confirm_dir(&outside, &real.join("src")).is_err());
}
