use crate::store::thread_repos::ThreadRepoRecord;
use orbyn_contracts::{GitAction, OpenInEditor, WriteFile};
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
async fn a_repo_id_opens_previews_and_the_outside_editor_in_the_named_repositorys_worktree() {
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
    // A file only the second repository has: the primary's preview does not see it, the named
    // repository's preview is that repository's file on disk.
    std::fs::write(other.join("only.txt"), "other only\n").unwrap();
    assert!(matches!(
        core.preview_file(&thread, "only.txt"),
        Err(CoreError::NotFound(_))
    ));
    let previewed = crate::in_repo(Some(added.id.clone()), async {
        core.preview_file(&thread, "only.txt")
    })
    .await
    .unwrap();
    assert_eq!(
        previewed,
        crate::util::canonicalize(&other.join("only.txt")).unwrap()
    );
    // The same name in the two repositories is two files, and each preview is its own.
    // With no repository named, a path is the thread's own folder's (the task folder), as a terminal
    // link's is: the primary's file is `<its folder name>/a.txt`, and a bare name is not found.
    let main_name = main.file_name().unwrap().to_string_lossy().into_owned();
    assert_eq!(
        core.preview_file(&thread, &format!("{main_name}/a.txt"))
            .unwrap(),
        crate::util::canonicalize(&main.join("a.txt")).unwrap()
    );
    assert!(matches!(
        core.preview_file(&thread, "a.txt"),
        Err(CoreError::NotFound(_))
    ));
    let in_other = crate::in_repo(Some(added.id.clone()), async {
        core.preview_file(&thread, "a.txt")
    })
    .await
    .unwrap();
    assert_eq!(
        in_other,
        crate::util::canonicalize(&other.join("a.txt")).unwrap()
    );
    assert_eq!(std::fs::read_to_string(&in_other).unwrap(), "other\n");
    // A previewed path that leaves the named repository reads as not found, through `..` or
    // as an absolute path; its own file is still not a folder.
    for path in ["../main/a.txt", "/etc/passwd", ""] {
        assert!(
            matches!(
                crate::in_repo(Some(added.id.clone()), async {
                    core.preview_file(&thread, path)
                })
                .await,
                Err(CoreError::NotFound(_))
            ),
            "{path:?}"
        );
    }
    // The outside editor resolves the same file the same way; the editor itself is chosen
    // after the path, so an unknown one is refused only once the named repository's file
    // resolved, and a path outside that repository is refused before it.
    let edit = |path: &str| {
        let body = OpenInEditor {
            path: path.to_owned(),
            editor: "no-such-editor".into(),
            line: None,
            column: None,
            repo_id: None,
        };
        let core = core.clone();
        let thread = thread.clone();
        let repo = added.id.clone();
        async move { crate::in_repo(Some(repo), core.open_in_editor(&thread, &body, "user")).await }
    };
    assert!(matches!(edit("only.txt").await, Err(CoreError::Invalid(_))));
    assert!(matches!(
        edit("../main/a.txt").await,
        Err(CoreError::Invalid(_))
    ));
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
    // The preview opener and the outside editor read a file through the same resolver, so
    // they refuse the untrusted repository and an unknown one before anything else runs.
    let preview = |repo: &str| {
        let core = core.clone();
        let thread = thread.clone();
        let repo = repo.to_owned();
        async move {
            crate::in_repo(
                Some(repo),
                async move { core.preview_file(&thread, "a.txt") },
            )
            .await
        }
    };
    assert!(matches!(
        preview(&added.id).await,
        Err(CoreError::RepoUntrusted(_))
    ));
    assert!(matches!(
        preview("no-such").await,
        Err(CoreError::NotFound(_))
    ));
    let edit = |repo: &str| {
        let core = core.clone();
        let thread = thread.clone();
        let repo = repo.to_owned();
        let body = OpenInEditor {
            path: "a.txt".into(),
            editor: "no-such-editor".into(),
            line: None,
            column: None,
            repo_id: None,
        };
        async move { crate::in_repo(Some(repo), core.open_in_editor(&thread, &body, "user")).await }
    };
    assert!(matches!(
        edit(&added.id).await,
        Err(CoreError::RepoUntrusted(_))
    ));
    assert!(matches!(edit("no-such").await, Err(CoreError::NotFound(_))));
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
    assert!(core.preview_file(&plain, "a.txt").is_ok());
    assert!(matches!(
        crate::in_repo(Some(project.clone()), async {
            core.preview_file(&plain, "a.txt")
        })
        .await,
        Err(CoreError::NotFound(_))
    ));
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

fn start_input(repos: &[(&str, &str)]) -> orbyn_contracts::StartThreadInput {
    orbyn_contracts::StartThreadInput {
        repos: repos
            .iter()
            .map(|(repo_id, base)| orbyn_contracts::StartRepoChoice {
                repo_id: (*repo_id).to_owned(),
                base_branch: (*base).to_owned(),
            })
            .collect(),
        engine: "claude".into(),
        model: None,
        permission_mode: orbyn_contracts::PermissionMode::Supervised,
        message: None,
        branch_name: None,
    }
}

async fn two_trusted(dir: &TempDir) -> (Arc<Core>, String, String, PathBuf, PathBuf) {
    let core = core(dir, true).await;
    let (main, other) = two_repos(dir);
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
    (core, project, added.id, main, other)
}

fn branches(path: &Path) -> String {
    let out = std::process::Command::new("git")
        .args(["branch", "--format=%(refname:short)"])
        .current_dir(path)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[tokio::test]
async fn a_thread_in_two_repositories_gets_a_folder_with_a_worktree_on_one_branch_in_each() {
    let dir = TempDir::new();
    let (core, project, other_id, main, other) = two_trusted(&dir).await;
    let mut input = start_input(&[(&other_id, "main"), (&project, "main")]);
    input.branch_name = Some("feature/shared".into());
    let thread = core.start_repo_thread(&project, input).await.unwrap();
    let folder = core.task_folder(&thread.id).await.unwrap();
    assert_eq!(folder.branch, "feature/shared");
    assert_eq!(folder.root, thread.cwd);
    assert!(Path::new(&thread.cwd).starts_with(dir.path().join("data/threads")));
    assert!(
        !Path::new(&thread.cwd).join(".git").exists(),
        "the folder is not a repository"
    );
    // The project's own folder is primary and first; each worktree is `<folder>/<name>` on the shared branch.
    assert_eq!(folder.repos[0].repo_id, project);
    assert_eq!(folder.repos.len(), 2);
    for repo in &folder.repos {
        assert_eq!(repo.branch, "feature/shared");
        assert!(
            Path::new(&thread.cwd)
                .join(&repo.name)
                .join("a.txt")
                .exists()
        );
    }
    assert!(
        branches(&main).contains("feature/shared") && branches(&other).contains("feature/shared")
    );
    assert_eq!(folder.max_per_thread, 8);
    // Calls with no repo_id act on the primary worktree; the repo_id picks the other one.
    assert_eq!(
        core.read_file(&thread.id, "a.txt").await.unwrap().content,
        "one\n"
    );
    let second = crate::in_repo(Some(other_id), core.read_file(&thread.id, "a.txt"))
        .await
        .unwrap();
    assert_eq!(second.content, "other\n");
}

#[tokio::test]
async fn a_repo_thread_is_refused_before_anything_is_made_when_the_choice_is_not_usable() {
    let dir = TempDir::new();
    let (core, project, other_id, main, other) = two_trusted(&dir).await;
    let untrusted_folder = repo(&dir.path().join("third"));
    let third = core
        .add_repo_folder(
            &project,
            AddRepoFolder {
                path: path_text(&untrusted_folder),
            },
        )
        .await
        .unwrap();
    let state = |c: &Core| {
        std::fs::read_dir(c.config.data_dir.join("threads"))
            .map(|d| d.count())
            .unwrap_or(0)
    };
    let refuse = |input: orbyn_contracts::StartThreadInput| {
        let core = core.clone();
        let project = project.clone();
        async move { core.start_repo_thread(&project, input).await.unwrap_err() }
    };
    // Untrusted, one repository, nine repositories, twice the same, a bad branch, a taken branch name.
    assert!(matches!(
        refuse(start_input(&[(&project, "main"), (&third.id, "main")])).await,
        CoreError::RepoUntrusted(_)
    ));
    assert!(matches!(
        refuse(start_input(&[(&project, "main")])).await,
        CoreError::Invalid(_)
    ));
    let many: Vec<(String, &str)> = (0..9).map(|n| (format!("r{n}"), "main")).collect();
    let many: Vec<(&str, &str)> = many.iter().map(|(a, b)| (a.as_str(), *b)).collect();
    assert!(matches!(
        refuse(start_input(&many)).await,
        CoreError::RepoLimit(_)
    ));
    assert!(matches!(
        refuse(start_input(&[(&project, "main"), (&project, "main")])).await,
        CoreError::Invalid(_)
    ));
    assert!(matches!(
        refuse(start_input(&[(&project, "main"), (&other_id, "nope")])).await,
        CoreError::Invalid(_)
    ));
    assert!(matches!(
        refuse(start_input(&[(&project, "-x"), (&other_id, "main")])).await,
        CoreError::Invalid(_)
    ));
    assert!(matches!(
        refuse(start_input(&[(&project, "main"), ("no-such", "main")])).await,
        CoreError::NotFound(_)
    ));
    let mut bad = start_input(&[(&project, "main"), (&other_id, "main")]);
    bad.branch_name = Some("bad..name".into());
    assert!(matches!(refuse(bad).await, CoreError::Invalid(_)));
    let mut taken = start_input(&[(&project, "main"), (&other_id, "main")]);
    taken.branch_name = Some("main".into());
    assert!(matches!(refuse(taken).await, CoreError::Invalid(_)));
    let mut mode = start_input(&[(&project, "main"), (&other_id, "main")]);
    mode.engine = "no-such-engine".into();
    assert!(refuse(mode).await.status() >= 400);
    use orbyn_contracts::PermissionMode::{Auto, Supervised};
    assert!(super::threads::check_mode(&[Supervised, Auto], Auto, "e").is_ok());
    assert!(matches!(
        super::threads::check_mode(&[Supervised], Auto, "e"),
        Err(CoreError::PermissionModeUnsupported(_))
    ));
    assert_eq!(state(&core), 0, "nothing was created");
    assert!(!branches(&main).contains("orbyn/") && !branches(&other).contains("orbyn/"));
    assert!(
        core.store
            .list_threads(Some(&project), false)
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn a_failure_part_way_leaves_no_worktree_branch_or_thread() {
    let dir = TempDir::new();
    let (core, project, other_id, main, other) = two_trusted(&dir).await;
    // The second repository's folder is gone, so its worktree cannot be made after the first was.
    std::fs::remove_dir_all(&other).unwrap();
    let error = core
        .start_repo_thread(
            &project,
            start_input(&[(&project, "main"), (&other_id, "main")]),
        )
        .await;
    assert!(error.is_err());
    assert!(!branches(&main).contains("orbyn/"), "{}", branches(&main));
    assert!(
        std::fs::read_dir(dir.path().join("data/threads")).map_or(true, |mut d| d.next().is_none())
    );
    assert!(
        core.store
            .list_threads(Some(&project), false)
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn a_third_repository_joins_a_thread_on_its_branch_and_the_eighth_is_the_last() {
    let dir = TempDir::new();
    let (core, project, other_id, _main, _other) = two_trusted(&dir).await;
    let thread = core
        .start_repo_thread(
            &project,
            start_input(&[(&project, "main"), (&other_id, "main")]),
        )
        .await
        .unwrap();
    let branch = core.task_folder(&thread.id).await.unwrap().branch;
    let extra = core
        .create_repo(
            &project,
            CreateRepo {
                name: "extra".into(),
            },
        )
        .await
        .unwrap();
    // A new empty repository has no commit, so it has no branch to cut from.
    assert!(
        core.add_thread_repo(
            &thread.id,
            orbyn_contracts::AddThreadRepo {
                repo_id: extra.id.clone()
            }
        )
        .await
        .is_err()
    );
    let made = repo(&dir.path().join("made"));
    let row = core
        .add_repo_folder(
            &project,
            AddRepoFolder {
                path: path_text(&made),
            },
        )
        .await
        .unwrap();
    // Untrusted is refused; trusted joins on the thread's branch.
    assert!(matches!(
        core.add_thread_repo(
            &thread.id,
            orbyn_contracts::AddThreadRepo {
                repo_id: row.id.clone()
            }
        )
        .await,
        Err(CoreError::RepoUntrusted(_))
    ));
    core.trust_repo(&project, &row.id).await.unwrap();
    let joined = core
        .add_thread_repo(
            &thread.id,
            orbyn_contracts::AddThreadRepo {
                repo_id: row.id.clone(),
            },
        )
        .await
        .unwrap();
    assert_eq!(joined.branch, branch);
    assert!(branches(&made).contains(&branch));
    assert!(matches!(
        core.add_thread_repo(
            &thread.id,
            orbyn_contracts::AddThreadRepo {
                repo_id: row.id.clone()
            }
        )
        .await,
        Err(CoreError::Invalid(_))
    ));
    assert_eq!(core.task_folder(&thread.id).await.unwrap().repos.len(), 3);
    // An ordinary thread has no task folder and takes no repository.
    let plain = core
        .store
        .create_thread(crate::store::NewThread {
            project_id: project.clone(),
            title: "p".into(),
            engine: "claude".into(),
            model: None,
            cwd: path_text(&dir.path().join("main")),
            worktree: None,
            skill_profile: None,
            parent_thread_id: None,
            relation: None,
            scratch: false,
        })
        .unwrap()
        .id;
    assert!(matches!(
        core.task_folder(&plain).await,
        Err(CoreError::NotFound(_))
    ));
    assert!(matches!(
        core.add_thread_repo(&plain, orbyn_contracts::AddThreadRepo { repo_id: row.id })
            .await,
        Err(CoreError::NotFound(_))
    ));
}

#[tokio::test]
async fn an_untrusted_primary_folder_cannot_start_a_repo_thread_either() {
    let dir = TempDir::new();
    let core = core(&dir, true).await;
    let (main, other) = two_repos(&dir);
    let project = core.add_project(&path_text(&main)).await.unwrap().id;
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
    let error = core
        .start_repo_thread(
            &project,
            start_input(&[(&project, "main"), (&added.id, "main")]),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::RepoUntrusted(_)), "{error:?}");
    assert!(
        !dir.path().join("data/threads").exists()
            || std::fs::read_dir(dir.path().join("data/threads"))
                .unwrap()
                .next()
                .is_none()
    );
}

fn pull_of(number: u64, title: &str, state: &str) -> orbyn_contracts::PullRequest {
    orbyn_contracts::PullRequest {
        number,
        title: title.into(),
        url: format!("https://example.test/pull/{number}"),
        state: state.into(),
        head: "feature/x".into(),
        base: "main".into(),
        author: None,
        draft: false,
        updated_at: None,
        labels: Vec::new(),
        review_state: None,
        checks_state: None,
        checks: None,
    }
}

#[tokio::test]
async fn each_repository_of_a_thread_shows_its_own_pull_request_even_with_the_same_number() {
    let dir = TempDir::new();
    let (core, project, other_id, _main, _other) = two_trusted(&dir).await;
    let thread = core
        .start_repo_thread(
            &project,
            start_input(&[(&project, "main"), (&other_id, "main")]),
        )
        .await
        .unwrap();
    assert!(
        core.thread_repos(&thread.id)
            .await
            .unwrap()
            .iter()
            .all(|r| r.pull_request.is_none())
    );
    core.store
        .link_pull(&thread.id, &project, &pull_of(7, "In the app", "open"))
        .unwrap();
    core.store
        .link_pull(&thread.id, &other_id, &pull_of(7, "In the library", "open"))
        .unwrap();
    core.store
        .link_pull(
            &thread.id,
            &other_id,
            &pull_of(3, "Older, merged", "merged"),
        )
        .unwrap();
    let repos = core.thread_repos(&thread.id).await.unwrap();
    let title = |id: &str| {
        repos
            .iter()
            .find(|r| r.repo_id == id)
            .and_then(|r| r.pull_request.as_ref())
            .map(|p| p.title.clone())
    };
    assert_eq!(title(&project).as_deref(), Some("In the app"));
    assert_eq!(title(&other_id).as_deref(), Some("In the library"));
    // A call scoped to the second repository links and reads in that repository.
    assert_eq!(
        crate::in_repo(Some(other_id.clone()), async { core.pull_repo(&thread.id) })
            .await
            .unwrap(),
        other_id
    );
    assert_eq!(core.pull_repo(&thread.id).unwrap(), project);
    // A diff names the repository only for a multi-repository thread.
    assert_eq!(
        core.diff_repo(&thread.id, None).unwrap().as_deref(),
        Some(project.as_str())
    );
    assert_eq!(
        core.diff_repo(&thread.id, Some(&other_id))
            .unwrap()
            .as_deref(),
        Some(other_id.as_str())
    );
}

#[tokio::test]
async fn deleting_a_repo_thread_removes_clean_worktrees_keeps_branches_and_keeps_a_dirty_one() {
    let dir = TempDir::new();
    let (core, project, other_id, main, other) = two_trusted(&dir).await;
    let thread = core
        .start_repo_thread(
            &project,
            start_input(&[(&project, "main"), (&other_id, "main")]),
        )
        .await
        .unwrap();
    let folder = core.task_folder(&thread.id).await.unwrap();
    let clean = Path::new(&thread.cwd).join(&folder.repos[0].name);
    let dirty = Path::new(&thread.cwd).join(&folder.repos[1].name);
    std::fs::write(dirty.join("notes.txt"), "mine\n").unwrap();
    core.delete_thread_and_worktrees(&thread.id, "tester")
        .await
        .unwrap();
    assert!(core.store.thread(&thread.id).is_err(), "the thread is gone");
    assert!(!clean.exists(), "a clean worktree is removed");
    assert!(
        dirty.join("notes.txt").exists(),
        "a dirty one is kept with its file"
    );
    assert!(
        branches(&main).contains(&folder.branch) && branches(&other).contains(&folder.branch),
        "branches stay, as for a single-repository thread"
    );
    assert!(
        Path::new(&thread.cwd).exists(),
        "the folder stays while a worktree is in it"
    );
}

#[tokio::test]
async fn a_repository_leaves_a_thread_only_when_it_is_not_primary_and_holds_nothing_to_lose() {
    let dir = TempDir::new();
    let (core, project, other_id, _main, other) = two_trusted(&dir).await;
    let thread = core
        .start_repo_thread(
            &project,
            start_input(&[(&project, "main"), (&other_id, "main")]),
        )
        .await
        .unwrap();
    let branch = core.task_folder(&thread.id).await.unwrap().branch;
    assert!(core.remove_thread_repo(&thread.id, &project).await.is_err());
    assert!(core.remove_thread_repo(&thread.id, "nope").await.is_err());
    let worktree = core
        .store
        .thread_repos(&thread.id)
        .unwrap()
        .into_iter()
        .find(|r| r.repo_id == other_id)
        .map(|r| PathBuf::from(r.worktree_path))
        .unwrap();
    std::fs::write(worktree.join("wip.txt"), "unsaved\n").unwrap();
    let refused = core
        .remove_thread_repo(&thread.id, &other_id)
        .await
        .unwrap_err();
    assert!(
        refused.public_message().contains("Commit or discard"),
        "{refused:?}"
    );
    assert!(worktree.join("wip.txt").exists());
    core.store
        .link_pull(&thread.id, &other_id, &pull_of(9, "Linked", "open"))
        .unwrap();
    std::fs::remove_file(worktree.join("wip.txt")).unwrap();
    core.remove_thread_repo(&thread.id, &other_id)
        .await
        .unwrap();
    assert!(!worktree.exists());
    assert!(branches(&other).contains(&branch), "the branch is kept");
    let left = core.thread_repos(&thread.id).await.unwrap();
    assert_eq!(left.len(), 1);
    assert!(
        core.store
            .linked_pulls(&thread.id, &other_id)
            .unwrap()
            .is_empty()
    );
}

/// A navigation request for one repository of a multi-repository thread. Which repository is the
/// scope's question: a route sets it with `in_repo`, so the body's own `repo_id` is not read here.
fn navigation(cwd: &str, path: &str, repo_id: Option<&str>) -> orbyn_contracts::ResolveThreadPath {
    orbyn_contracts::ResolveThreadPath {
        path: path.to_owned(),
        basis: orbyn_contracts::ThreadPathBasis::Repository,
        cwd: cwd.to_owned(),
        repo_id: repo_id.map(str::to_owned),
    }
}

/// A navigation request for a reference the engine wrote relative to its task folder, as a message link is.
fn task_folder_navigation(cwd: &str, path: &str) -> orbyn_contracts::ResolveThreadPath {
    orbyn_contracts::ResolveThreadPath {
        basis: orbyn_contracts::ThreadPathBasis::Thread,
        ..navigation(cwd, path, None)
    }
}

/// A task-folder reference names its repository by the folder the worktree sits in, so the coordinates are
/// that repository's and the answer says which one it is (issue #1581).
#[tokio::test]
async fn a_task_folder_reference_resolves_in_the_repository_its_first_segment_names() {
    let dir = TempDir::new();
    let (core, project, other_id, _main, _other) = two_trusted(&dir).await;
    let thread = core
        .start_repo_thread(
            &project,
            start_input(&[(&project, "main"), (&other_id, "main")]),
        )
        .await
        .unwrap();
    let folder = core.task_folder(&thread.id).await.unwrap();
    let primary = Path::new(&thread.cwd).join(&folder.repos[0].name);
    let second = Path::new(&thread.cwd).join(&folder.repos[1].name);
    // One name with different bytes in each repository, so the answer says whose file it resolved.
    std::fs::write(primary.join("note.txt"), "primary\n").unwrap();
    std::fs::write(second.join("note.txt"), "other\n").unwrap();
    assert_ne!(folder.repos[0].name, folder.repos[1].name);
    let second_link = format!("{}/note.txt", folder.repos[1].name);
    let resolved = core
        .resolve_thread_path(
            &thread.id,
            &task_folder_navigation(&thread.cwd, &second_link),
        )
        .await
        .unwrap();
    assert_eq!(resolved.checkout_path.as_deref(), Some("note.txt"));
    assert_eq!(resolved.repository_path.as_deref(), Some("note.txt"));
    assert_eq!(
        resolved.repo_id.as_deref(),
        Some(other_id.as_str()),
        "the answer names the repository the coordinates are in"
    );
    assert_eq!(
        resolved.cwd, thread.cwd,
        "the fence is still the task folder"
    );
    // The primary's own folder name is its file, and the answer names the primary repository.
    let first_link = format!("{}/note.txt", folder.repos[0].name);
    let own = core
        .resolve_thread_path(
            &thread.id,
            &task_folder_navigation(&thread.cwd, &first_link),
        )
        .await
        .unwrap();
    assert_eq!(own.checkout_path.as_deref(), Some("note.txt"));
    assert_eq!(own.repo_id.as_deref(), Some(project.as_str()));
    // A path under no repository's folder keeps its older meaning: the primary repository's worktree.
    let plain = core
        .resolve_thread_path(&thread.id, &task_folder_navigation(&thread.cwd, "note.txt"))
        .await
        .unwrap();
    assert_eq!(plain.checkout_path.as_deref(), Some("note.txt"));
    assert_eq!(plain.repo_id.as_deref(), Some(project.as_str()));
}

/// A reference may spell out the folder it is relative to (`./planner/note.txt`), and once its first segment
/// names a repository the rest is relative to that repository's worktree, so a folder there that repeats the
/// checkout's own name is a real folder and is not stripped a second time.
#[tokio::test]
async fn a_task_folder_reference_keeps_its_path_after_the_repository_prefix() {
    let dir = TempDir::new();
    let (core, project, other_id, _main, _other) = two_trusted(&dir).await;
    let thread = core
        .start_repo_thread(
            &project,
            start_input(&[(&project, "main"), (&other_id, "main")]),
        )
        .await
        .unwrap();
    let folder = core.task_folder(&thread.id).await.unwrap();
    let name = folder.repos[1].name.clone();
    let second = Path::new(&thread.cwd).join(&name);
    // A folder inside that checkout repeating its own name, with a same-named file at the root as well, so a
    // second repeated-workspace strip would answer the root file instead of the nested one.
    std::fs::create_dir_all(second.join(&name)).unwrap();
    std::fs::write(second.join(&name).join("note.txt"), "nested\n").unwrap();
    std::fs::write(second.join("note.txt"), "root\n").unwrap();
    // A leading `./` still names the repository.
    let dotted = format!("./{name}/note.txt");
    let resolved = core
        .resolve_thread_path(&thread.id, &task_folder_navigation(&thread.cwd, &dotted))
        .await
        .unwrap();
    assert_eq!(
        resolved.repo_id.as_deref(),
        Some(other_id.as_str()),
        "the `./` is not the first segment"
    );
    assert_eq!(resolved.checkout_path.as_deref(), Some("note.txt"));
    // `planner/planner/note.txt` is the `planner` folder inside the `planner` checkout, never the root file.
    let nested = format!("{name}/{name}/note.txt");
    let resolved = core
        .resolve_thread_path(&thread.id, &task_folder_navigation(&thread.cwd, &nested))
        .await
        .unwrap();
    assert_eq!(resolved.repo_id.as_deref(), Some(other_id.as_str()));
    assert_eq!(
        resolved.checkout_path.as_deref(),
        Some(format!("{name}/note.txt").as_str())
    );
}

#[tokio::test]
async fn navigation_coordinates_are_resolved_in_the_repository_its_scope_names() {
    use orbyn_contracts::ResolveThreadPath;

    let dir = TempDir::new();
    let (core, project, other_id, _main, _other) = two_trusted(&dir).await;
    let thread = core
        .start_repo_thread(
            &project,
            start_input(&[(&project, "main"), (&other_id, "main")]),
        )
        .await
        .unwrap();
    let folder = core.task_folder(&thread.id).await.unwrap();
    let primary = Path::new(&thread.cwd).join(&folder.repos[0].name);
    let second = Path::new(&thread.cwd).join(&folder.repos[1].name);
    // A file of each repository, so the answer says which worktree the path was resolved in.
    std::fs::write(primary.join("primary.txt"), "primary\n").unwrap();
    std::fs::write(second.join("second.txt"), "second\n").unwrap();
    // No repository named is the primary one, and the answer's cwd stays the thread's own folder.
    let resolved = core
        .resolve_thread_path(&thread.id, &navigation(&thread.cwd, "primary.txt", None))
        .await
        .unwrap();
    assert_eq!(resolved.cwd, thread.cwd);
    assert_eq!(resolved.checkout_path.as_deref(), Some("primary.txt"));
    assert_eq!(resolved.repository_path.as_deref(), Some("primary.txt"));
    // The second repository's file is not in the primary one; it resolves when the scope names it.
    let unresolved = core
        .resolve_thread_path(&thread.id, &navigation(&thread.cwd, "second.txt", None))
        .await
        .unwrap();
    assert_eq!(unresolved.checkout_path, None);
    let resolved = crate::in_repo(
        Some(other_id.clone()),
        core.resolve_thread_path(
            &thread.id,
            &navigation(&thread.cwd, "second.txt", Some(&other_id)),
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        resolved.cwd, thread.cwd,
        "the task folder is still what the caller fenced against"
    );
    assert_eq!(resolved.checkout_path.as_deref(), Some("second.txt"));
    assert_eq!(resolved.repository_path.as_deref(), Some("second.txt"));
    // A repository that is not the thread's is not found.
    assert!(matches!(
        crate::in_repo(
            Some("no-such-repo".into()),
            core.resolve_thread_path(
                &thread.id,
                &navigation(&thread.cwd, "primary.txt", Some("no-such-repo")),
            ),
        )
        .await,
        Err(CoreError::NotFound(_))
    ));
    // The fence is the thread's stored folder, not a worktree inside it.
    let moved = ResolveThreadPath {
        cwd: path_text(&primary),
        ..navigation(&thread.cwd, "primary.txt", None)
    };
    assert!(matches!(
        core.resolve_thread_path(&thread.id, &moved).await,
        Err(CoreError::StaleOwner)
    ));
}

#[tokio::test]
async fn navigation_coordinates_of_an_untrusted_repository_are_refused() {
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
    assert!(!added.trusted);
    // A task folder over both, as a started thread has, without the start's trust check.
    let thread = multi_thread(
        &core,
        &project,
        &[(&project, "main", &main), (&added.id, "other", &other)],
    );
    let stored = core.store.thread(&thread).unwrap();
    let request = navigation(&stored.cwd, "a.txt", None);
    // The trusted primary still resolves; the untrusted repository is refused before it is read.
    assert_eq!(
        core.resolve_thread_path(&thread, &request)
            .await
            .unwrap()
            .checkout_path
            .as_deref(),
        Some("a.txt")
    );
    assert!(matches!(
        crate::in_repo(
            Some(added.id.clone()),
            core.resolve_thread_path(&thread, &request)
        )
        .await,
        Err(CoreError::RepoUntrusted(_))
    ));
    // A task-folder reference that names the untrusted repository's folder is refused the same way.
    let name = core
        .store
        .thread_repos(&thread)
        .unwrap()
        .into_iter()
        .find(|record| record.repo_id == added.id)
        .unwrap()
        .name;
    assert!(matches!(
        core.resolve_thread_path(
            &thread,
            &task_folder_navigation(&stored.cwd, &format!("{name}/a.txt"))
        )
        .await,
        Err(CoreError::RepoUntrusted(_))
    ));
}

/// A multi-repository thread whose folder is a task folder outside every repository: one worktree of each
/// `(repo_id, name, source repository)` under it, on one branch, as a started thread has. No engine runs here.
fn repo_thread(core: &Core, project: &str, repos: &[(&str, &str, &Path)]) -> (String, PathBuf) {
    let folder = core.config.data_dir.join("threads").join("task");
    std::fs::create_dir_all(&folder).unwrap();
    let mut worktrees: Vec<(&str, &str, PathBuf)> = Vec::new();
    for &(repo_id, name, source) in repos {
        let worktree = folder.join(name);
        let target = path_text(&worktree);
        run_git(
            source,
            &["worktree", "add", "-q", "-b", "orbyn/thread", "--", &target],
        );
        worktrees.push((repo_id, name, worktree));
    }
    let paths: Vec<(&str, &str, &Path)> = worktrees
        .iter()
        .map(|(repo_id, name, path)| (*repo_id, *name, path.as_path()))
        .collect();
    (multi_thread(core, project, &paths), folder)
}

fn diff_paths(files: &[orbyn_contracts::FileDiff]) -> Vec<String> {
    files.iter().map(|file| file.path.clone()).collect()
}

#[tokio::test]
async fn a_diff_reads_only_the_repository_its_scope_names() {
    let dir = TempDir::new();
    let (core, project, other_id, main, other) = two_trusted(&dir).await;
    let (thread, folder) = repo_thread(
        &core,
        &project,
        &[(&project, "main", &main), (&other_id, "other", &other)],
    );
    assert_eq!(core.store.thread(&thread).unwrap().cwd, path_text(&folder));
    assert!(
        !folder.join(".git").exists(),
        "the task folder is not a repository"
    );
    // The same path in the two repositories is changed to different bytes.
    std::fs::write(folder.join("main/a.txt"), "from the primary\n").unwrap();
    std::fs::write(folder.join("other/a.txt"), "from the second\n").unwrap();
    // No repository named is the primary one.
    let primary = core.thread_diff(&thread).await.unwrap();
    assert_eq!(diff_paths(&primary), ["a.txt"]);
    assert!(primary[0].patch.contains("from the primary"), "{primary:?}");
    // The scope names the second repository, and its diff is that repository's file.
    let second = crate::in_repo(Some(other_id.clone()), core.thread_diff(&thread))
        .await
        .unwrap();
    assert_eq!(diff_paths(&second), ["a.txt"]);
    assert!(second[0].patch.contains("from the second"), "{second:?}");
    assert!(
        !second[0].patch.contains("from the primary"),
        "the primary's bytes are not the second repository's file"
    );
    // A repository that is not the thread's is not found.
    assert!(matches!(
        crate::in_repo(Some("no-such-repo".into()), core.thread_diff(&thread)).await,
        Err(CoreError::NotFound(_))
    ));
}

#[tokio::test]
async fn a_diff_of_an_untrusted_repository_is_refused() {
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
    assert!(!added.trusted);
    let thread = multi_thread(
        &core,
        &project,
        &[(&project, "main", &main), (&added.id, "other", &other)],
    );
    assert!(matches!(
        crate::in_repo(Some(added.id.clone()), core.thread_diff(&thread)).await,
        Err(CoreError::RepoUntrusted(_))
    ));
    assert!(
        core.thread_diff(&thread).await.is_ok(),
        "the trusted primary still reads"
    );
}

#[tokio::test]
async fn a_turn_diff_reads_the_checkpoint_in_the_repository_its_scope_names() {
    use crate::source_control::git;

    let dir = TempDir::new();
    let (core, project, other_id, main, other) = two_trusted(&dir).await;
    let (thread, folder) = repo_thread(
        &core,
        &project,
        &[(&project, "main", &main), (&other_id, "other", &other)],
    );
    let index = dir.path().join("checkpoint-index");
    // A checkpoint of the primary repository, read with no repository named.
    let start = git::snapshot(&folder.join("main"), None, &index)
        .await
        .unwrap();
    std::fs::write(folder.join("main/primary.txt"), "primary\n").unwrap();
    let end = git::snapshot(&folder.join("main"), None, &index)
        .await
        .unwrap();
    core.store
        .add_checkpoint(&thread, "run-primary", &start)
        .unwrap();
    core.store.finish_checkpoint("run-primary", &end).unwrap();
    let files = core.turn_diff(&thread, "run-primary").await.unwrap();
    assert_eq!(diff_paths(&files), ["primary.txt"]);
    // A checkpoint of the second repository, read through its scope.
    let start = git::snapshot(&folder.join("other"), None, &index)
        .await
        .unwrap();
    std::fs::write(folder.join("other/second.txt"), "second\n").unwrap();
    let end = git::snapshot(&folder.join("other"), None, &index)
        .await
        .unwrap();
    core.store
        .add_checkpoint(&thread, "run-second", &start)
        .unwrap();
    core.store.finish_checkpoint("run-second", &end).unwrap();
    let files = crate::in_repo(Some(other_id), core.turn_diff(&thread, "run-second"))
        .await
        .unwrap();
    assert_eq!(diff_paths(&files), ["second.txt"]);
}

#[tokio::test]
async fn commits_and_commit_diffs_read_the_repository_its_scope_names() {
    let dir = TempDir::new();
    let (core, project, other_id, main, other) = two_trusted(&dir).await;
    let thread = multi_thread(
        &core,
        &project,
        &[(&project, "main", &main), (&other_id, "other", &other)],
    );
    // The stored folder is not a repository; the default repository is the primary one.
    assert_eq!(
        core.thread_commits(&thread).await.unwrap()[0].subject,
        "first"
    );
    let commits = crate::in_repo(Some(other_id.clone()), core.thread_commits(&thread))
        .await
        .unwrap();
    assert_eq!(commits[0].subject, "other");
    // A commit diff runs in the repository that has the commit.
    let files = crate::in_repo(
        Some(other_id.clone()),
        core.commit_diff(&thread, &commits[0].sha),
    )
    .await
    .unwrap();
    assert_eq!(diff_paths(&files), ["a.txt"]);
    // A branch comparison reads the named repository's branch.
    run_git(&other, &["checkout", "-q", "-b", "feature"]);
    std::fs::write(other.join("feature.txt"), "feature\n").unwrap();
    run_git(&other, &["add", "."]);
    run_git(&other, &["commit", "-q", "-m", "feature"]);
    let files = crate::in_repo(Some(other_id), core.branch_diff(&thread, "main"))
        .await
        .unwrap();
    assert!(
        diff_paths(&files).contains(&"feature.txt".to_owned()),
        "{files:?}"
    );
}

#[tokio::test]
async fn the_first_repository_is_the_primary_even_when_it_is_not_the_project() {
    let dir = TempDir::new();
    let (core, project, other_id, main, other) = two_trusted(&dir).await;
    // A thread need not work in the project's own folder: its first stored repository is the primary one.
    let thread = multi_thread(
        &core,
        &project,
        &[(&other_id, "other", &other), (&project, "main", &main)],
    );
    assert_eq!(
        core.diff_repo(&thread, None).unwrap().as_deref(),
        Some(other_id.as_str()),
        "the label names the repository the read uses"
    );
    assert_eq!(core.scoped_thread(&thread).unwrap().cwd, path_text(&other));
    std::fs::write(other.join("second.txt"), "second\n").unwrap();
    let files = core.thread_diff(&thread).await.unwrap();
    assert_eq!(diff_paths(&files), ["second.txt"]);
}

#[tokio::test]
async fn a_file_restore_stays_in_the_engine_folder_of_a_repository_thread() {
    let dir = TempDir::new();
    let (core, project, other_id, main, other) = two_trusted(&dir).await;
    let (thread, folder) = repo_thread(
        &core,
        &project,
        &[(&project, "main", &main), (&other_id, "other", &other)],
    );
    // The engine writes in the task folder, so a restore of its files is relative to it; only the reads
    // resolve one repository's worktree.
    assert_eq!(core.idle_thread(&thread).unwrap().cwd, path_text(&folder));
    assert_eq!(
        core.scoped_thread(&thread).unwrap().cwd,
        path_text(&folder.join("main"))
    );
}
