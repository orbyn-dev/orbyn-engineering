use std::path::{Path, PathBuf};

use orbyn_contracts::{
    ProjectScopeCatalog, ProjectScopeCheckout, ProjectScopeUnavailableReason, ProjectScopesQuery,
};

use crate::app::Core;
use crate::testing::{TempDir, run_git};

use super::super::git_spawns;
use super::super::tests::{configure, core, repository};

/// The Git processes started in each folder so far.
fn spawns(folders: &[PathBuf]) -> Vec<usize> {
    folders
        .iter()
        .map(|folder| git_spawns::count(folder))
        .collect()
}

/// The Git processes started in each folder since `before` was taken.
fn since(before: &[usize], folders: &[PathBuf]) -> Vec<usize> {
    spawns(folders)
        .iter()
        .zip(before)
        .map(|(now, before)| now - before)
        .collect()
}

fn members(catalog: &ProjectScopeCatalog) -> Vec<&ProjectScopeCheckout> {
    catalog
        .groups
        .iter()
        .flat_map(|group| &group.members)
        .collect()
}

fn only_member(catalog: &ProjectScopeCatalog) -> &ProjectScopeCheckout {
    let members = members(catalog);
    assert_eq!(members.len(), 1);
    members[0]
}

async fn read(core: &Core) -> ProjectScopeCatalog {
    core.project_scope_catalog(&ProjectScopesQuery::default())
        .await
        .unwrap()
}

/// Registers `count` repositories with distinct remotes, so each is its own group.
async fn projects(core: &Core, dir: &TempDir, count: usize) -> Vec<PathBuf> {
    let mut folders = Vec::new();
    for index in 0..count {
        let folder = dir.path().join(format!("repo-{index}"));
        repository(&folder);
        configure(
            &folder,
            "origin",
            &format!("git@host:Owner/Repo{index}.git"),
        );
        core.add_project(folder.to_str().unwrap()).await.unwrap();
        folders.push(folder);
    }
    folders
}

#[tokio::test]
async fn a_repeated_catalog_read_spawns_no_git_for_unchanged_folders() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let folders = projects(&core, &dir, 4).await;
    let before = spawns(&folders);
    let started = std::time::Instant::now();
    let first = read(&core).await;
    let first_took = started.elapsed();
    let observed = since(&before, &folders);
    assert_eq!(
        observed,
        [2, 2, 2, 2],
        "a first read observes every folder with two Git processes"
    );
    let after_first = spawns(&folders);
    let started = std::time::Instant::now();
    let second = read(&core).await;
    let second_took = started.elapsed();
    let reused = since(&after_first, &folders);
    println!(
        "4 projects: first read spawned {} Git processes in {:?}, the repeated read {} in {:?}",
        observed.iter().sum::<usize>(),
        first_took,
        reused.iter().sum::<usize>(),
        second_took
    );
    assert_eq!(
        reused,
        [0, 0, 0, 0],
        "a repeated read reuses every observation"
    );
    assert_eq!(
        first, second,
        "a reused observation answers as the read did"
    );
}

#[tokio::test]
async fn a_branch_switch_is_observed_again_for_that_folder_alone() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let folders = projects(&core, &dir, 2).await;
    let first = read(&core).await;
    run_git(&folders[1], &["checkout", "-q", "-b", "other"]);
    let before = spawns(&folders);
    let second = read(&core).await;
    assert_eq!(
        since(&before, &folders),
        [0, 2],
        "the folder whose HEAD moved is observed again, the other is reused"
    );
    assert_eq!(first, second, "another branch names the same checkout");
}

#[tokio::test]
async fn a_changed_remote_is_observed_instead_of_remembered() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let folder = dir.path().join("repo");
    repository(&folder);
    configure(&folder, "origin", "git@host:Owner/Repo.git");
    core.add_project(folder.to_str().unwrap()).await.unwrap();
    let first = read(&core).await;
    assert_eq!(
        only_member(&first).repository_label.as_deref(),
        Some("Owner/Repo")
    );
    configure(&folder, "origin", "git@host:Owner/Other.git");
    let watched = std::slice::from_ref(&folder);
    let before = spawns(watched);
    let second = read(&core).await;
    assert_eq!(
        since(&before, watched),
        [2],
        "a changed remote is read again"
    );
    let member = only_member(&second);
    assert_eq!(member.repository_label.as_deref(), Some("Owner/Other"));
    assert_ne!(
        member.repository_key,
        only_member(&first).repository_key,
        "the group that names the repository changed with it"
    );
}

#[tokio::test]
async fn a_folder_that_becomes_its_own_repository_is_observed_again() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let root = dir.path().join("repo");
    repository(&root);
    configure(&root, "origin", "git@host:Owner/Repo.git");
    let nested = root.join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    core.add_project(nested.to_str().unwrap()).await.unwrap();
    let first = read(&core).await;
    assert_eq!(
        only_member(&first).repository_relative_path.as_deref(),
        Some("nested"),
        "the folder is read as part of the repository above it"
    );
    repository(&nested);
    let watched = std::slice::from_ref(&nested);
    let before = spawns(watched);
    let second = read(&core).await;
    assert_eq!(
        since(&before, watched),
        [2],
        "the repository that appeared in the folder is observed"
    );
    assert_eq!(
        only_member(&second).repository_relative_path.as_deref(),
        Some(""),
        "the folder is now the root of its own repository"
    );
}

/// A folder that becomes a bare repository holds no `.git` either, and Git stops there
/// instead of passing to the repository above it, so the answer it gave while it was an
/// ordinary folder of that repository is not reused.
#[tokio::test]
async fn a_folder_that_becomes_a_bare_repository_is_observed_again() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let root = dir.path().join("repo");
    repository(&root);
    configure(&root, "origin", "git@host:Owner/Repo.git");
    let nested = root.join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    core.add_project(nested.to_str().unwrap()).await.unwrap();
    let first = read(&core).await;
    assert_eq!(
        only_member(&first).repository_relative_path.as_deref(),
        Some("nested"),
        "the folder is read as part of the repository above it"
    );
    run_git(&nested, &["init", "-q", "--bare"]);
    let watched = std::slice::from_ref(&nested);
    let before = spawns(watched);
    let second = read(&core).await;
    assert_eq!(
        since(&before, watched),
        [1],
        "the bare repository that appeared in the folder is observed: Git stops there, \
         which is one read and no configuration"
    );
    let member = only_member(&second);
    assert_eq!(
        member.repository_key, None,
        "a bare repository in the folder is not the repository above it"
    );
    assert_eq!(member.repository_relative_path, None);
    assert!(
        !member.metadata_complete,
        "Git names no work tree for it, and it was registered as a repository, so the \
         checkout is left unobserved instead of described by the repository above it"
    );
    // Nothing is kept for an observation Git could not answer, so the folder is read again
    // rather than answered from the repository above it.
    let watched = std::slice::from_ref(&nested);
    let before = spawns(watched);
    let _ = read(&core).await;
    assert_eq!(
        since(&before, watched),
        [1],
        "the unobserved folder is read again"
    );
}

/// Git exits 128 both for a folder with no repository above it and for one whose enclosing
/// configuration cannot be read. Only the first is a folder without a work tree: the second is a
/// repository Orbyn cannot read, so the checkout is left unobserved instead of called standalone.
#[tokio::test]
async fn a_folder_inside_an_unreadable_repository_is_not_completed_as_standalone() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let root = dir.path().join("repo");
    let plain = root.join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    core.add_project(plain.to_str().unwrap()).await.unwrap();
    let first = read(&core).await;
    let member = only_member(&first);
    assert!(
        member.metadata_complete && member.repository_key.is_none(),
        "the folder is a plain folder, read as one"
    );
    // A repository appears above the folder, and its configuration cannot be read: Git exits 128
    // for the folder too, and the repository above it is not one it found no work tree for.
    repository(&root);
    std::fs::write(
        root.join(".git").join("config"),
        "[core]\n\trepositoryformatversion = 0\n[broken\n",
    )
    .unwrap();
    let watched = std::slice::from_ref(&plain);
    let before = spawns(watched);
    let second = read(&core).await;
    assert!(
        since(&before, watched)[0] > 0,
        "the folder is observed again"
    );
    let member = only_member(&second);
    assert!(
        !member.metadata_complete,
        "a repository Orbyn cannot read is not a folder without one"
    );
    assert_eq!(
        member.repository_key, None,
        "and no identity is invented for it"
    );
}

#[tokio::test]
async fn a_missing_folder_is_unavailable_and_starts_nothing() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let folders = projects(&core, &dir, 2).await;
    read(&core).await;
    std::fs::remove_dir_all(&folders[0]).unwrap();
    let before = spawns(&folders);
    for _ in 0..2 {
        let catalog = read(&core).await;
        let missing = members(&catalog)
            .into_iter()
            .find(|member| member.path.as_deref() == folders[0].to_str())
            .unwrap();
        assert_eq!(
            missing.unavailable,
            Some(ProjectScopeUnavailableReason::CheckoutMissing)
        );
        assert!(!missing.metadata_complete);
    }
    assert_eq!(
        since(&before, &folders),
        [0, 0],
        "a missing folder is answered without Git, and keeps being answered"
    );
}

#[tokio::test]
async fn a_folder_whose_git_fails_is_observed_again_on_the_next_read() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let folder = dir.path().join("broken");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join(".git"), "gitdir: ./missing\n").unwrap();
    core.add_project(folder.to_str().unwrap()).await.unwrap();
    let first = read(&core).await;
    assert!(
        !only_member(&first).metadata_complete,
        "a failed observation leaves the checkout unknown"
    );
    let watched = std::slice::from_ref(&folder);
    let before = spawns(watched);
    let second = read(&core).await;
    assert!(
        since(&before, watched)[0] > 0,
        "a failed observation is not remembered"
    );
    assert!(!only_member(&second).metadata_complete);
}

#[tokio::test]
async fn a_plain_folder_becomes_a_repository_under_the_observation() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let folder = dir.path().join("plain");
    std::fs::create_dir_all(&folder).unwrap();
    core.add_project(folder.to_str().unwrap()).await.unwrap();
    let first = read(&core).await;
    let member = only_member(&first);
    assert!(member.metadata_complete && member.repository_key.is_none());
    repository(&folder);
    configure(&folder, "origin", "git@host:Owner/Plain.git");
    let watched = std::slice::from_ref(&folder);
    let before = spawns(watched);
    let second = read(&core).await;
    assert_eq!(
        since(&before, watched),
        [2],
        "the repository that appeared around the folder is observed"
    );
    let member = only_member(&second);
    assert!(member.metadata_complete && member.repository_key.is_some());
    assert_eq!(member.repository_relative_path.as_deref(), Some(""));
}

#[test]
fn a_watched_file_that_differs_never_describes_the_folder_again() {
    let dir = TempDir::new();
    let folder = dir.path().join("repo");
    repository(&folder);
    let real = crate::util::canonicalize(&folder).unwrap();
    let git_dir = real.join(".git");
    let facts = super::GitFacts::work_tree(
        &real,
        real.clone(),
        super::Watch::work_tree(&real, &real, &git_dir),
        None,
    );
    assert!(facts.still_describes(&real));
    configure(&folder, "origin", "git@host:Owner/Repo.git");
    assert!(
        !facts.still_describes(&real),
        "the configuration the answer read changed"
    );
    let other = dir.path().join("other");
    repository(&other);
    let other = crate::util::canonicalize(&other).unwrap();
    let facts = super::GitFacts::work_tree(
        &real,
        real.clone(),
        super::Watch::work_tree(&real, &real, &git_dir),
        None,
    );
    assert!(
        !facts.still_describes(Path::new(&other)),
        "the folder resolved elsewhere"
    );
}

/// Points `folder/.git` at `target` with a link the platform can make without a privilege: a
/// symlink on Unix, and on Windows a junction, which needs none either.
fn point_git_at(folder: &Path, target: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, folder.join(".git")).unwrap();
    #[cfg(windows)]
    {
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(folder.join(".git"))
            .arg(target)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(made.success(), "a junction could not be made");
    }
}

/// Drops the link at `folder/.git`, which is the link and not what it points at, so the next
/// [`point_git_at`] retargets the same path.
fn drop_git_link(folder: &Path) {
    #[cfg(unix)]
    std::fs::remove_file(folder.join(".git")).unwrap();
    #[cfg(windows)]
    std::fs::remove_dir(folder.join(".git")).unwrap();
}

/// A `.git` link retargeted at another repository changes no file the answer read, because Git
/// names the directory it resolved to, so the folder has to be observed again.
#[tokio::test]
async fn a_git_link_retargeted_at_another_repository_is_observed_again() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let alpha = dir.path().join("alpha");
    repository(&alpha);
    configure(&alpha, "origin", "git@host:Owner/Alpha.git");
    let beta = dir.path().join("beta");
    repository(&beta);
    configure(&beta, "origin", "git@host:Owner/Beta.git");
    let checkout = dir.path().join("checkout");
    std::fs::create_dir_all(&checkout).unwrap();
    point_git_at(&checkout, &alpha.join(".git"));
    core.add_project(checkout.to_str().unwrap()).await.unwrap();
    let first = read(&core).await;
    assert_eq!(
        only_member(&first).repository_label.as_deref(),
        Some("Owner/Alpha"),
        "the checkout is read as the repository its link names"
    );
    drop_git_link(&checkout);
    point_git_at(&checkout, &beta.join(".git"));
    let watched = std::slice::from_ref(&checkout);
    let before = spawns(watched);
    let second = read(&core).await;
    assert_eq!(
        since(&before, watched),
        [2],
        "the link names another repository now, so the folder is observed again"
    );
    assert_eq!(
        only_member(&second).repository_label.as_deref(),
        Some("Owner/Beta"),
        "the repository the answer names follows the link"
    );
}

/// Git accepts a directory as a repository only while its common directory holds `objects` and
/// `refs`, both checked for access and never read. Renaming either leaves every file the answer
/// was read from as it was, so the signatures are watched too.
#[tokio::test]
async fn a_repository_that_lost_a_signature_is_observed_again() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let folder = dir.path().join("repo");
    repository(&folder);
    configure(&folder, "origin", "git@host:Owner/Repo.git");
    core.add_project(folder.to_str().unwrap()).await.unwrap();
    let first = read(&core).await;
    assert_eq!(
        only_member(&first).repository_label.as_deref(),
        Some("Owner/Repo")
    );
    let watched = std::slice::from_ref(&folder);
    for name in ["objects", "refs"] {
        let signature = folder.join(".git").join(name);
        let moved = folder.join(".git").join(format!("{name}-moved"));
        std::fs::rename(&signature, &moved).unwrap();
        let before = spawns(watched);
        let second = read(&core).await;
        assert!(
            since(&before, watched)[0] > 0,
            "the repository Git no longer finds without its {name} is observed again"
        );
        assert!(
            !only_member(&second).metadata_complete,
            "a folder Git cannot read as a repository is left unobserved"
        );
        std::fs::rename(&moved, &signature).unwrap();
        read(&core).await;
        let before = spawns(watched);
        let restored = read(&core).await;
        assert_eq!(
            since(&before, watched),
            [0],
            "the repository Git finds again, with its {name}, is kept again"
        );
        assert_eq!(
            only_member(&restored).repository_label.as_deref(),
            Some("Owner/Repo"),
            "and it is the repository it was"
        );
    }
}

/// A linked worktree's own Git directory holds neither `objects` nor `refs`: Git finds the
/// signatures in the common directory `commondir` names, and checks them there.
#[tokio::test]
async fn a_linked_worktree_whose_common_directory_lost_a_signature_is_observed_again() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let main = dir.path().join("main");
    repository(&main);
    configure(&main, "origin", "git@host:Owner/Main.git");
    let checkout = dir.path().join("checkout");
    run_git(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "other",
            checkout.to_str().unwrap(),
        ],
    );
    core.add_project(checkout.to_str().unwrap()).await.unwrap();
    let first = read(&core).await;
    assert_eq!(
        only_member(&first).repository_label.as_deref(),
        Some("Owner/Main"),
        "the worktree is read as the repository its common directory holds"
    );
    let objects = main.join(".git").join("objects");
    std::fs::rename(&objects, main.join(".git").join("objects-moved")).unwrap();
    let watched = std::slice::from_ref(&checkout);
    let before = spawns(watched);
    let second = read(&core).await;
    assert!(
        since(&before, watched)[0] > 0,
        "the worktree whose common directory lost objects is observed again"
    );
    assert!(
        !only_member(&second).metadata_complete,
        "a worktree Git cannot read as a repository is left unobserved"
    );
}

/// Git accepts a signature only while it can search it: `is_git_directory` checks both `objects`
/// and `refs` with `access(X_OK)`, so a directory whose search permission was dropped is one Git
/// no longer finds, however unchanged its contents are. Root searches regardless of the mode, as
/// Git does, so there is nothing to observe when the test itself runs as root.
#[cfg(unix)]
#[tokio::test]
async fn a_signature_that_lost_its_search_permission_is_observed_again() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let dir = TempDir::new();
    let core = core(&dir).await;
    let folder = dir.path().join("repo");
    repository(&folder);
    configure(&folder, "origin", "git@host:Owner/Repo.git");
    core.add_project(folder.to_str().unwrap()).await.unwrap();
    let first = read(&core).await;
    assert_eq!(
        only_member(&first).repository_label.as_deref(),
        Some("Owner/Repo")
    );
    if std::fs::metadata(dir.path()).unwrap().uid() == 0 {
        return;
    }
    let signature = folder.join(".git").join("objects");
    let mode = std::fs::metadata(&signature).unwrap().permissions().mode();
    std::fs::set_permissions(&signature, std::fs::Permissions::from_mode(mode & !0o111)).unwrap();
    let watched = std::slice::from_ref(&folder);
    let before = spawns(watched);
    let second = read(&core).await;
    std::fs::set_permissions(&signature, std::fs::Permissions::from_mode(mode)).unwrap();
    assert!(
        since(&before, watched)[0] > 0,
        "the repository Git can no longer search is observed again"
    );
    assert!(
        !only_member(&second).metadata_complete,
        "a folder Git cannot search as a repository is left unobserved"
    );
}

/// Git drops only a trailing newline and carriage return from `commondir`, so a common directory
/// whose name ends in a space is another directory: the watch has to cover what Git resolved, or
/// a remote changed there is never noticed. An ordinary Windows path cannot open a name ending in
/// a space, so this pins the path watched rather than a read of it.
#[test]
fn a_common_directory_ending_in_a_space_is_watched_where_git_names_it() {
    let dir = TempDir::new();
    let folder = dir.path().join("worktree");
    let git_dir = folder.join(".git");
    std::fs::create_dir_all(&git_dir).unwrap();
    let shared = dir.path().join("shared ");
    let trimmed = dir.path().join("shared");
    let real = crate::util::canonicalize(&folder).unwrap();
    for ending in ["\n", "\r\n"] {
        std::fs::write(
            git_dir.join("commondir"),
            format!("{}{ending}", shared.display()),
        )
        .unwrap();
        let watch = super::Watch::work_tree(&real, &real, &real.join(".git")).unwrap();
        let watched: Vec<PathBuf> = watch.files.iter().map(|file| file.path.clone()).collect();
        for name in ["objects", "refs", "config", "config.worktree"] {
            assert!(
                watched.contains(&shared.join(name)),
                "the {name} Git resolved in the common directory ending in a space is watched, \
                 with a {ending:?} after it"
            );
            assert!(
                !watched.contains(&trimmed.join(name)),
                "the {name} of the directory the trailing space was trimmed to is not watched"
            );
        }
    }
}
