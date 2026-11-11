use std::path::{Path, PathBuf};

use orbyn_contracts::{
    CreateThread, ExplorerChange, ExplorerChanges, ExplorerEntry, ExplorerEntryKind,
    ExplorerFileKind, ExplorerReadOnlyReason, WorkspaceSetup,
};

use super::*;
use crate::CoreConfig;
use crate::store::Admission;
use crate::testing::{TempDir, library, run_git};

pub(super) async fn core(dir: &TempDir) -> Core {
    let root = library(&dir.path().join("library"));
    let mut config = CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
    config.engines.lookup_latest = false;
    let core = Core::open(config).unwrap();
    core.workspace
        .setup(WorkspaceSetup {
            url: None,
            adopt_path: Some(root.to_string_lossy().into_owned()),
        })
        .await
        .unwrap();
    core
}

pub(super) fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

pub(super) fn commit_all(repo: &Path, message: &str) {
    run_git(repo, &["add", "."]);
    run_git(repo, &["commit", "-q", "-m", message]);
}

/// A repository with a few tracked files, outside the library.
pub(super) fn repo(path: &Path) -> PathBuf {
    std::fs::create_dir_all(path).unwrap();
    run_git(path, &["init", "-q", "-b", "main"]);
    run_git(path, &["config", "user.email", "t@example.com"]);
    run_git(path, &["config", "user.name", "T"]);
    run_git(path, &["config", "gc.auto", "0"]);
    write(&path.join("a.txt"), "one\n");
    write(&path.join("src/lib.rs"), "pub fn x() {}\n");
    write(&path.join("src/old.rs"), "pub fn old() {}\n");
    write(&path.join("docs/x.md"), "# x\n");
    write(&path.join(".gitignore"), "*.log\nbuild/\n");
    commit_all(path, "first");
    crate::util::canonicalize(path).unwrap()
}

/// A trusted project over a new repository at `<dir>/work/<name>`.
pub(super) async fn project(core: &Core, dir: &TempDir, name: &str) -> (Project, PathBuf) {
    let root = repo(&dir.path().join("work").join(name));
    let project = core.add_project(&root.to_string_lossy()).await.unwrap();
    let project = core.store.trust_project(&project.id, "test").unwrap();
    (project, root)
}

pub(super) async fn thread_in(
    core: &Core,
    project: &Project,
    purpose: Option<&str>,
) -> orbyn_contracts::Thread {
    core.create_thread(CreateThread {
        project_id: Some(project.id.clone()),
        engine: "claude".into(),
        model: None,
        title: Some(format!("work {}", purpose.unwrap_or("main"))),
        worktree: purpose.map(str::to_owned),
        skill_profile: None,
        base_branch: None,
        effort: None,
        options: Default::default(),
    })
    .await
    .unwrap()
}

pub(super) async fn main_id(core: &Core, project: &Project) -> String {
    let listed = core
        .explorer_worktrees(&project.id, None, false)
        .await
        .unwrap();
    listed.worktrees[0].id.clone()
}

pub(super) fn entry<'a>(entries: &'a [ExplorerEntry], name: &str) -> &'a ExplorerEntry {
    entries
        .iter()
        .find(|entry| entry.name == name)
        .unwrap_or_else(|| panic!("no entry named {name}"))
}

pub(super) fn names(entries: &[ExplorerEntry]) -> Vec<&str> {
    entries.iter().map(|entry| entry.name.as_str()).collect()
}

pub(super) fn refused<T: std::fmt::Debug>(result: Result<T>) {
    assert!(
        matches!(result, Err(CoreError::Invalid(_) | CoreError::NotFound(_))),
        "{result:?}"
    );
}

mod access;
mod browse;
mod hardening;
mod worktrees;
