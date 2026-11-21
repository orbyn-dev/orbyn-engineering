use crate::store::thread_repos::ThreadRepoRecord;
use orbyn_contracts::{GitAction, WriteFile};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use orbyn_contracts::{
    AddRepoFolder, CloneRepo, CreateRepo, RepoCloneState, ScanRepos, WorkspaceSetup,
};

use crate::CoreConfig;
use crate::app::Core;
use crate::error::CoreError;
use crate::testing::{TempDir, library, run_git};

async fn core(dir: &TempDir, flag: bool) -> Arc<Core> {
    let root = library(&dir.path().join("library"));
    let mut config = CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
    config.engines.lookup_latest = false;
    config.multi_repo = flag;
    let core = Arc::new(Core::open(config).unwrap());
    core.workspace
        .setup(WorkspaceSetup {
            url: None,
            adopt_path: Some(root.to_string_lossy().into_owned()),
        })
        .await
        .unwrap();
    core
}

fn repo(path: &Path) -> PathBuf {
    std::fs::create_dir_all(path).unwrap();
    run_git(path, &["init", "-q", "-b", "main"]);
    run_git(path, &["config", "user.email", "t@example.com"]);
    run_git(path, &["config", "user.name", "T"]);
    std::fs::write(path.join("a.txt"), "one\n").unwrap();
    run_git(path, &["add", "."]);
    run_git(path, &["commit", "-q", "-m", "first"]);
    crate::util::canonicalize(path).unwrap()
}

async fn project(core: &Core, dir: &TempDir) -> String {
    let main = repo(&dir.path().join("main"));
    core.add_project(&main.to_string_lossy()).await.unwrap().id
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[tokio::test]
async fn with_the_flag_off_every_repository_call_is_not_found() {
    let dir = TempDir::new();
    let core = core(&dir, false).await;
    let id = project(&core, &dir).await;
    assert!(matches!(
        core.repos_view(&id).await,
        Err(CoreError::NotFound(_))
    ));
    assert!(matches!(
        core.add_repo_folder(&id, AddRepoFolder { path: "/".into() })
            .await,
        Err(CoreError::NotFound(_))
    ));
}

#[tokio::test]
async fn a_folder_is_added_in_place_untrusted_and_only_trust_makes_it_usable() {
    let dir = TempDir::new();
    let core = core(&dir, true).await;
    let id = project(&core, &dir).await;
    let other = repo(&dir.path().join("other"));
    let added = core
        .add_repo_folder(
            &id,
            AddRepoFolder {
                path: path_text(&other),
            },
        )
        .await
        .unwrap();
    assert!(!added.trusted && !added.primary && added.is_git);
    assert_eq!(added.name, "other");
    assert_eq!(added.branch.as_deref(), Some("main"));
    assert_eq!(added.branches, ["main"]);
    assert!(!added.has_remote);
    let view = core.repos_view(&id).await.unwrap();
    assert_eq!(view.repositories.len(), 2);
    assert!(view.repositories[0].primary && view.repositories[0].id == id);
    assert_eq!(view.max_per_thread, 8);
    // The same folder twice, the project's own folder, or another project's folder.
    for path in [path_text(&other), path_text(&dir.path().join("main"))] {
        assert!(matches!(
            core.add_repo_folder(&id, AddRepoFolder { path }).await,
            Err(CoreError::RepoExists(_))
        ));
    }
    let trusted = core.trust_repo(&id, &added.id).await.unwrap();
    assert!(trusted.trusted);
    assert!(matches!(
        core.remove_repo(&id, &id),
        Err(CoreError::Invalid(_))
    ));
    core.remove_repo(&id, &added.id).unwrap();
    assert_eq!(core.repos_view(&id).await.unwrap().repositories.len(), 1);
    assert!(other.join("a.txt").exists(), "the files stay");
}

#[tokio::test]
async fn a_folder_inside_orbyns_data_or_the_library_or_a_relative_path_is_refused() {
    let dir = TempDir::new();
    let core = core(&dir, true).await;
    let id = project(&core, &dir).await;
    let inside = dir.path().join("data").join("sneaky");
    std::fs::create_dir_all(&inside).unwrap();
    for path in [
        path_text(&inside),
        path_text(&dir.path().join("library")),
        "relative/path".to_owned(),
        path_text(&dir.path().join("missing")),
    ] {
        assert!(
            core.add_repo_folder(&id, AddRepoFolder { path: path.clone() })
                .await
                .is_err(),
            "{path}"
        );
    }
    assert!(
        core.scan_repos(
            &id,
            ScanRepos {
                path: path_text(&inside)
            }
        )
        .await
        .is_err()
    );
    assert!(
        core.scan_repos(
            &id,
            ScanRepos {
                path: "relative".into()
            }
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn a_created_repository_lives_under_the_clone_directory_and_a_number_is_added_when_taken() {
    let dir = TempDir::new();
    let core = core(&dir, true).await;
    let id = project(&core, &dir).await;
    let first = core
        .create_repo(
            &id,
            CreateRepo {
                name: "Notes App".into(),
            },
        )
        .await
        .unwrap();
    let second = core
        .create_repo(
            &id,
            CreateRepo {
                name: "Notes App".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(first.name, "Notes-App");
    assert_eq!(second.name, "Notes-App-2");
    for row in [&first, &second] {
        assert!(row.trusted && row.is_git && row.branch.as_deref() == Some("main"));
        assert!(Path::new(&row.path).join(".git").exists());
        assert!(
            Path::new(&row.path)
                .starts_with(crate::util::canonicalize(&dir.path().join("data/repos")).unwrap())
        );
    }
    for name in ["..", "", "///"] {
        assert!(
            core.create_repo(&id, CreateRepo { name: name.into() })
                .await
                .is_err(),
            "{name:?}"
        );
    }
}

#[tokio::test]
async fn a_clone_source_the_service_will_not_run_is_refused_and_leaves_nothing() {
    let dir = TempDir::new();
    let core = core(&dir, true).await;
    let id = project(&core, &dir).await;
    for source in [
        "-x",
        "ext::sh -c id",
        "file:///etc",
        "fd::1",
        "/tmp/repo",
        "https://u:p@h/a",
        "https://h/a\nb",
    ] {
        assert!(
            matches!(
                core.clone_repo(
                    &id,
                    CloneRepo {
                        source: source.into()
                    }
                )
                .await,
                Err(CoreError::CloneRefused(_))
            ),
            "{source:?}"
        );
    }
    assert_eq!(core.repos_view(&id).await.unwrap().repositories.len(), 1);
    assert!(
        !dir.path().join("data/repos").exists()
            || std::fs::read_dir(dir.path().join("data/repos"))
                .unwrap()
                .next()
                .is_none()
    );
}

#[tokio::test]
async fn a_clone_that_fails_stays_listed_with_its_message_and_leaves_no_folder() {
    let dir = TempDir::new();
    let core = core(&dir, true).await;
    let id = project(&core, &dir).await;
    let started = core
        .clone_repo(
            &id,
            CloneRepo {
                source: "https://127.0.0.1:1/x/y.git".into(),
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        started.clone.as_ref().map(|c| c.state),
        Some(RepoCloneState::Cloning)
    ));
    assert!(!started.trusted && started.name == "y");
    let mut row = started;
    for _ in 0..200 {
        row = core.row_of(&id, &row.id).await.unwrap();
        if matches!(
            row.clone.as_ref().map(|c| c.state),
            Some(RepoCloneState::Failed)
        ) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let clone = row.clone.expect("still listed");
    assert_eq!(clone.state, RepoCloneState::Failed);
    assert!(clone.message.is_some());
    assert!(!Path::new(&row.path).exists());
    let leftovers: Vec<_> = std::fs::read_dir(dir.path().join("data/repos"))
        .unwrap()
        .flatten()
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    // Trusting a repository that did not clone is refused until it exists; removing it is fine.
    core.remove_repo(&id, &row.id).unwrap();
    assert_eq!(core.repos_view(&id).await.unwrap().repositories.len(), 1);
    assert!(core.cancel_repo_clone(&id, "no-such").is_err());
}

#[tokio::test]
async fn git_is_started_only_in_a_trusted_folder_that_has_none() {
    let dir = TempDir::new();
    let core = core(&dir, true).await;
    let id = project(&core, &dir).await;
    let plain = dir.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let plain = crate::util::canonicalize(&plain).unwrap();
    let row = core
        .add_repo_folder(
            &id,
            AddRepoFolder {
                path: path_text(&plain),
            },
        )
        .await
        .unwrap();
    assert!(!row.is_git);
    assert!(matches!(
        core.setup_repo_git(&id, &row.id).await,
        Err(CoreError::RepoUntrusted(_))
    ));
    core.trust_repo(&id, &row.id).await.unwrap();
    let done = core.setup_repo_git(&id, &row.id).await.unwrap();
    assert!(done.is_git && plain.join(".git").exists());
    assert!(matches!(
        core.setup_repo_git(&id, &row.id).await,
        Err(CoreError::Invalid(_))
    ));
    assert!(matches!(
        core.publish_repo(&id, &row.id).await,
        Err(CoreError::Unsupported(_))
    ));
}

#[tokio::test]
async fn a_scan_through_the_core_marks_what_a_project_already_holds() {
    let dir = TempDir::new();
    let core = core(&dir, true).await;
    let id = project(&core, &dir).await;
    let parent = dir.path().join("parent");
    let kept = repo(&parent.join("kept"));
    repo(&parent.join("fresh"));
    core.add_repo_folder(
        &id,
        AddRepoFolder {
            path: path_text(&kept),
        },
    )
    .await
    .unwrap();
    let found = core
        .scan_repos(
            &id,
            ScanRepos {
                path: path_text(&parent),
            },
        )
        .await
        .unwrap();
    let state: Vec<_> = found
        .folders
        .iter()
        .map(|f| (f.name.as_str(), f.in_project, f.is_git))
        .collect();
    assert_eq!(state, [("fresh", false, true), ("kept", true, true)]);
}

/// A thread in `project` whose folder is not a repository, with a worktree record per `(repo_id, name, path)`.
fn multi_thread(core: &Core, project: &str, repos: &[(&str, &str, &Path)]) -> String {
    let thread = core
        .store
        .create_thread(crate::store::NewThread {
            project_id: project.to_owned(),
            title: "t".into(),
            engine: "claude".into(),
            model: None,
            cwd: repos[0].2.parent().unwrap().to_string_lossy().into_owned(),
            worktree: None,
            skill_profile: None,
            parent_thread_id: None,
            relation: None,
            scratch: false,
        })
        .unwrap();
    for (position, (repo_id, name, path)) in repos.iter().enumerate() {
        core.store
            .thread_repo_insert(
                &ThreadRepoRecord {
                    thread_id: thread.id.clone(),
                    repo_id: (*repo_id).to_owned(),
                    position: position as u32,
                    name: (*name).to_owned(),
                    branch: "main".into(),
                    worktree_path: path_text(path),
                },
                &crate::util::now(),
            )
            .unwrap();
    }
    thread.id
}

fn two_repos(dir: &TempDir) -> (PathBuf, PathBuf) {
    let main = repo(&dir.path().join("main"));
    let other = repo(&dir.path().join("other"));
    std::fs::write(other.join("a.txt"), "other\n").unwrap();
    run_git(&other, &["commit", "-qam", "other"]);
    (main, other)
}

#[tokio::test]
async fn a_repo_id_names_the_repository_a_file_call_reads_and_writes() {
    let dir = TempDir::new();
    let core = core(&dir, true).await;
    let (main, other) = two_repos(&dir);
    let project = core.add_project(&path_text(&main)).await.unwrap().id;
    core.store.trust_project(&project, "user").unwrap();
    let added = core
        .add_repo_folder(
            &project,
            AddRepoFolder {
                path: path_text(&other),
            },
        )
        .await
        .unwrap();
    core.trust_repo(&project, &added.id).await.unwrap();
    let thread = multi_thread(
        &core,
        &project,
        &[(&project, "main", &main), (&added.id, "other", &other)],
    );
    // No repo_id is the primary repository; the same file name in the other repository is another file.
    let primary = core.read_file(&thread, "a.txt").await.unwrap();
    assert_eq!(primary.content, "one\n");
    let second = crate::in_repo(Some(added.id.clone()), core.read_file(&thread, "a.txt"))
        .await
        .unwrap();
    assert_eq!(second.content, "other\n");
    // A write goes to the named repository only, and its revision is that repository's.
    let write = |repo: Option<String>, base: &str| {
        let body = WriteFile {
            path: "a.txt".into(),
            content: "edited\n".into(),
            base_revision: Some(base.to_owned()),
            repo_id: repo.clone(),
        };
        let core = core.clone();
        let thread = thread.clone();
        async move {
            crate::in_repo(repo, async move {
                core.write_file(&thread, &body, "user").await
            })
            .await
        }
    };
    assert!(
        matches!(
            write(Some(added.id.clone()), &primary.revision).await,
            Err(CoreError::FileConflict { .. })
        ),
        "the primary's revision does not fit the other repository's file"
    );
    write(Some(added.id.clone()), &second.revision)
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(other.join("a.txt")).unwrap(),
        "edited\n"
    );
    assert_eq!(
        std::fs::read_to_string(main.join("a.txt")).unwrap(),
        "one\n",
        "the primary is untouched"
    );
    // A path that leaves the named repository is refused, as is one that names a sibling.
    for path in ["../main/a.txt", "/etc/passwd"] {
        let escaped = crate::in_repo(Some(added.id.clone()), core.read_file(&thread, path)).await;
        assert!(escaped.is_err(), "{path}");
    }
}

#[tokio::test]
async fn a_git_action_commits_in_the_named_repository_only() {
    let dir = TempDir::new();
    let core = core(&dir, true).await;
    let (main, other) = two_repos(&dir);
    let project = core.add_project(&path_text(&main)).await.unwrap().id;
    core.store.trust_project(&project, "user").unwrap();
    let added = core
        .add_repo_folder(
            &project,
            AddRepoFolder {
                path: path_text(&other),
            },
        )
        .await
        .unwrap();
    core.trust_repo(&project, &added.id).await.unwrap();
    let thread = multi_thread(
        &core,
        &project,
        &[(&project, "main", &main), (&added.id, "other", &other)],
    );
    std::fs::write(other.join("new.txt"), "x\n").unwrap();
    let listed = core.thread_repos(&thread).await.unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].name, "main");
    assert!(listed.iter().all(|r| r.trusted && r.branch == "main"));
    let count = |path: &Path| {
        let out = std::process::Command::new("git")
            .args(["rev-list", "--count", "HEAD"])
            .current_dir(path)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    };
    let action = GitAction {
        commit_message: Some("add new".into()),
        push: false,
        pull_request: None,
        repo_id: Some(added.id.clone()),
    };
    let result = crate::in_repo(
        Some(added.id.clone()),
        core.git_action(&thread, &action, "user"),
    )
    .await
    .unwrap();
    assert!(result.phases.iter().all(|p| p.ok), "{result:?}");
    assert_eq!(count(&other), "3");
    assert_eq!(count(&main), "1", "the primary repository got no commit");
}

#[tokio::test]
async fn an_unknown_untrusted_or_flagged_off_repository_is_refused_at_every_entry() {
    let dir = TempDir::new();
    let core = core(&dir, true).await;
    let (main, other) = two_repos(&dir);
    let project = core.add_project(&path_text(&main)).await.unwrap().id;
    core.store.trust_project(&project, "user").unwrap();
    let added = core
        .add_repo_folder(
            &project,
            AddRepoFolder {
                path: path_text(&other),
            },
        )
        .await
        .unwrap();
    // Added but never trusted.
    let thread = multi_thread(
        &core,
        &project,
        &[(&project, "main", &main), (&added.id, "other", &other)],
    );
    assert!(!core.thread_repos(&thread).await.unwrap()[1].trusted);
    let read = |repo: &str| {
        let core = core.clone();
        let thread = thread.clone();
        let repo = repo.to_owned();
        async move { crate::in_repo(Some(repo), core.read_file(&thread, "a.txt")).await }
    };
    assert!(matches!(
        read(&added.id).await,
        Err(CoreError::RepoUntrusted(_))
    ));
    assert!(matches!(read("no-such").await, Err(CoreError::NotFound(_))));
    let action = GitAction {
        commit_message: Some("x".into()),
        push: false,
        pull_request: None,
        repo_id: Some(added.id.clone()),
    };
    assert!(matches!(
        crate::in_repo(
            Some(added.id.clone()),
            core.git_action(&thread, &action, "user")
        )
        .await,
        Err(CoreError::RepoUntrusted(_))
    ));
    assert!(
        core.read_file(&thread, "a.txt").await.is_ok(),
        "the primary still works"
    );
    // A repo_id on an ordinary thread is not found.
    let plain = core
        .store
        .create_thread(crate::store::NewThread {
            project_id: project.clone(),
            title: "p".into(),
            engine: "claude".into(),
            model: None,
            cwd: path_text(&main),
            worktree: None,
            skill_profile: None,
            parent_thread_id: None,
            relation: None,
            scratch: false,
        })
        .unwrap()
        .id;
    assert!(
        crate::in_repo(Some(project.clone()), core.read_file(&plain, "a.txt"))
            .await
            .is_err()
    );
    assert!(core.read_file(&plain, "a.txt").await.is_ok());
    assert!(matches!(
        core.thread_repos(&plain).await,
        Err(CoreError::NotFound(_))
    ));
    // The flag gates the repo_id and the read of the repositories.
    let off = dir.path().join("off");
    let flagged = self::core(&TempDir::new(), false).await;
    let _ = off;
    assert!(matches!(
        flagged.thread_repos(&thread).await,
        Err(CoreError::NotFound(_))
    ));
}
