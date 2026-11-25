//! Repositories of a project (docs/multi-repo.md): the project's own folder is the first, more are folders
//! the person adds in place, creates or clones under app data. Trust is per repository.

mod scan;
mod scope;
mod source;
mod threads;

pub(crate) use scope::repo_asked;
pub use scope::in_repo;

use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::project::workspace::resolve_project;
use crate::source_control::git::{command, git, text};
use crate::store::AuditRecord;
use crate::store::project_repos::RepoRecord;
use crate::util::{canonicalize, new_id};
use orbyn_contracts::{
    AddRepoFolder, CloneRepo, CreateRepo, RepoClone, RepoCloneState, RepoRow, RepoScan, ReposView,
    ScanRepos,
};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// The most repositories one thread may work in.
pub const MAX_PER_THREAD: u32 = 8;
/// The most extra repositories a project holds.
const MAX_REPOS: usize = 64;
const CLONE_LIMIT: Duration = Duration::from_secs(10 * 60);

/// Clones this process is running, so a cancel can stop one.
#[derive(Default)]
pub(crate) struct RepoDesk {
    clones: Mutex<HashMap<String, CancellationToken>>,
}

impl RepoDesk {
    fn clones(&self) -> std::sync::MutexGuard<'_, HashMap<String, CancellationToken>> {
        self.clones.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A clone's temporary folder: dropping it removes whatever is still there, so a cancel leaves nothing.
struct Staging(PathBuf);

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(super) fn on(core: &Core) -> Result<()> {
    if core.config.multi_repo {
        Ok(())
    } else {
        Err(CoreError::NotFound("repositories".into()))
    }
}

/// The branch, the branches a thread may start from (current first) and whether a remote exists.
pub(super) async fn describe(path: &Path, is_git: bool) -> (Option<String>, Vec<String>, bool) {
    if !is_git || !path.is_dir() {
        return (None, Vec::new(), false);
    }
    let current = git(path, &["branch", "--show-current"])
        .await
        .map(|out| text(&out))
        .ok()
        .filter(|name| !name.is_empty());
    let mut branches: Vec<String> = git(
        path,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
    )
    .await
    .map(|out| {
        String::from_utf8_lossy(&out)
            .lines()
            .map(str::to_owned)
            .collect()
    })
    .unwrap_or_default();
    if let Some(current) = &current {
        branches.retain(|b| b != current);
        branches.insert(0, current.clone());
    }
    let has_remote = git(path, &["remote"])
        .await
        .is_ok_and(|out| !text(&out).is_empty());
    (current, branches, has_remote)
}

impl Core {
    fn clone_directory(&self) -> PathBuf {
        self.config.data_dir.join("repos")
    }

    fn audit_repo(&self, action: &str, target: &str, detail: serde_json::Value) -> Result<()> {
        self.store.append_audit(&AuditRecord {
            actor: "user".into(),
            action: action.to_owned(),
            target: target.to_owned(),
            decision: None,
            source: "user".into(),
            thread_id: None,
            run_id: None,
            detail,
        })?;
        Ok(())
    }

    async fn repo_row(&self, record: &RepoRecord) -> RepoRow {
        let cloning = record.clone_state.is_some();
        let (branch, branches, has_remote) =
            describe(Path::new(&record.path), record.is_git && !cloning).await;
        RepoRow {
            id: record.id.clone(),
            name: record.name.clone(),
            path: record.path.clone(),
            is_git: record.is_git,
            trusted: record.trusted,
            branch,
            branches,
            has_remote,
            primary: false,
            clone: record.clone_state.as_deref().map(|state| RepoClone {
                state: if state == "cloning" {
                    RepoCloneState::Cloning
                } else {
                    RepoCloneState::Failed
                },
                percent: None,
                message: record.clone_message.clone(),
            }),
        }
    }

    async fn primary_row(&self, project: &orbyn_contracts::Project) -> RepoRow {
        let (branch, branches, has_remote) =
            describe(Path::new(&project.path), project.is_git).await;
        RepoRow {
            id: project.id.clone(),
            name: project.name.clone(),
            path: project.path.clone(),
            is_git: project.is_git,
            trusted: project.trusted,
            branch,
            branches,
            has_remote,
            primary: true,
            clone: None,
        }
    }

    /// The row for `repo_id`: the primary one, or one of the project's extra repositories.
    async fn row_of(&self, project_id: &str, repo_id: &str) -> Result<RepoRow> {
        let project = self.store.project(project_id)?;
        if repo_id == project.id {
            return Ok(self.primary_row(&project).await);
        }
        let record = self
            .store
            .repo(project_id, repo_id)?
            .ok_or_else(|| CoreError::NotFound("repository".into()))?;
        Ok(self.repo_row(&record).await)
    }

    pub async fn repos_view(&self, project_id: &str) -> Result<ReposView> {
        on(self)?;
        let project = self.store.project(project_id)?;
        let mut repositories = vec![self.primary_row(&project).await];
        for record in self.store.repos_of(project_id)? {
            repositories.push(self.repo_row(&record).await);
        }
        Ok(ReposView {
            repositories,
            clone_directory: Some(self.clone_directory().to_string_lossy().into_owned()),
            max_per_thread: MAX_PER_THREAD,
        })
    }

    /// Every folder that is a project or a project's repository, by real path.
    fn known_folders(&self) -> Result<HashSet<String>> {
        let mut known = HashSet::new();
        for project in self.store.list_projects()? {
            known.insert(
                canonicalize(Path::new(&project.path)).map_or(project.path, |p| {
                    crate::util::strip_verbatim(&p.to_string_lossy())
                }),
            );
        }
        known.extend(self.store.repo_paths()?);
        Ok(known)
    }

    /// A name no other repository of the project uses: a number is appended when taken.
    fn free_name(&self, project: &orbyn_contracts::Project, wanted: &str) -> Result<String> {
        let mut taken: HashSet<String> = self
            .store
            .repos_of(&project.id)?
            .into_iter()
            .map(|r| r.name)
            .collect();
        taken.insert(
            Path::new(&project.path)
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
        );
        let mut name = wanted.to_owned();
        let mut n = 2;
        while taken.contains(&name) {
            name = format!("{wanted}-{n}");
            n += 1;
        }
        Ok(name)
    }

    fn check_room(&self, project_id: &str) -> Result<()> {
        if self.store.repos_of(project_id)?.len() >= MAX_REPOS {
            return Err(CoreError::RepoLimit(format!(
                "A project holds at most {MAX_REPOS} more repositories."
            )));
        }
        Ok(())
    }

    /// Adds a folder where it is. It starts untrusted; nothing is copied, moved or written.
    pub async fn add_repo_folder(&self, project_id: &str, input: AddRepoFolder) -> Result<RepoRow> {
        on(self)?;
        let project = self.store.project(project_id)?;
        self.check_room(project_id)?;
        let root = self.workspace.root()?;
        let resolved = resolve_project(&root, Path::new(&input.path)).await?;
        let data =
            canonicalize(&self.config.data_dir).unwrap_or_else(|_| self.config.data_dir.clone());
        if resolved.path.starts_with(&data) {
            return Err(CoreError::Invalid(
                "That folder is inside Orbyn's own data.".into(),
            ));
        }
        let path = crate::util::strip_verbatim(&resolved.path.to_string_lossy());
        if self.known_folders()?.contains(&path) {
            return Err(CoreError::RepoExists(
                "That folder is already a repository of a project.".into(),
            ));
        }
        let leaf = resolved
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let name = self.free_name(
            &project,
            &source::folder_name(&leaf).unwrap_or_else(|| "repo".into()),
        )?;
        let record = RepoRecord {
            id: new_id(),
            project_id: project.id.clone(),
            name,
            path: path.clone(),
            trusted: false,
            is_git: resolved.is_git,
            managed: false,
            clone_state: None,
            clone_message: None,
        };
        self.audit_repo("repo.add", &path, json!({ "project": project.id }))?;
        self.store.repo_insert(&record, &crate::util::now())?;
        Ok(self.repo_row(&record).await)
    }

    /// A new empty repository under the clone directory. Orbyn made it empty, so it starts trusted.
    pub async fn create_repo(&self, project_id: &str, input: CreateRepo) -> Result<RepoRow> {
        on(self)?;
        let project = self.store.project(project_id)?;
        self.check_room(project_id)?;
        let wanted = source::folder_name(&input.name)
            .ok_or_else(|| CoreError::Invalid("Choose another repository name.".into()))?;
        let (name, path) = self.free_slot(&project, &wanted)?;
        let parent = path
            .parent()
            .ok_or_else(|| CoreError::Invalid("Choose another repository name.".into()))?;
        std::fs::create_dir_all(parent)?;
        let target = path.to_string_lossy().into_owned();
        git(parent, &["init", "--quiet", "-b", "main", "--", &target]).await?;
        let record = RepoRecord {
            id: new_id(),
            project_id: project.id.clone(),
            name,
            path: crate::util::strip_verbatim(&path.to_string_lossy()),
            trusted: true,
            is_git: true,
            managed: true,
            clone_state: None,
            clone_message: None,
        };
        self.audit_repo(
            "repo.create",
            &record.path,
            json!({ "project": project.id }),
        )?;
        self.store.repo_insert(&record, &crate::util::now())?;
        Ok(self.repo_row(&record).await)
    }

    /// The name and folder for a new managed repository: under the clone directory, a number appended when taken,
    /// and the folder never anywhere else.
    fn free_slot(
        &self,
        project: &orbyn_contracts::Project,
        wanted: &str,
    ) -> Result<(String, PathBuf)> {
        let directory = self.clone_directory();
        std::fs::create_dir_all(&directory)?;
        let directory = canonicalize(&directory)?;
        let mut candidate = self.free_name(project, wanted)?;
        let mut n = 2;
        loop {
            let path = directory.join(&candidate);
            if path.parent() != Some(directory.as_path()) {
                return Err(CoreError::Invalid("Choose another repository name.".into()));
            }
            if std::fs::symlink_metadata(&path).is_err() {
                return Ok((candidate, path));
            }
            candidate = format!("{wanted}-{n}");
            n += 1;
            if n > 1000 {
                return Err(CoreError::Invalid("Choose another repository name.".into()));
            }
        }
    }

    /// Starts a clone and answers at once with a row that is `cloning`.
    pub async fn clone_repo(
        self: &std::sync::Arc<Self>,
        project_id: &str,
        input: CloneRepo,
    ) -> Result<RepoRow> {
        on(self)?;
        let project = self.store.project(project_id)?;
        self.check_room(project_id)?;
        let source = source::check_source(&input.source)?.to_owned();
        let wanted = source::name_of_source(&source).ok_or_else(|| {
            CoreError::CloneRefused("That source has no name to clone into.".into())
        })?;
        let (name, path) = self.free_slot(&project, &wanted)?;
        let record = RepoRecord {
            id: new_id(),
            project_id: project.id.clone(),
            name,
            path: crate::util::strip_verbatim(&path.to_string_lossy()),
            trusted: false,
            is_git: false,
            managed: true,
            clone_state: Some("cloning".into()),
            clone_message: None,
        };
        self.audit_repo("repo.clone", &source, json!({ "project": project.id }))?;
        self.store.repo_insert(&record, &crate::util::now())?;
        let stop = CancellationToken::new();
        self.repos.clones().insert(record.id.clone(), stop.clone());
        let core = self.clone();
        let (id, destination) = (record.id.clone(), path);
        tokio::spawn(async move {
            let outcome = tokio::select! {
                () = stop.cancelled() => Err("Cancelled.".to_owned()),
                done = core.run_repo_clone(&source, &destination) => done.map_err(|e| e.public_message()),
            };
            core.repos.clones().remove(&id);
            let result = match outcome {
                Ok(()) => core.store.repo_set_clone(&id, None, None, true),
                Err(message) => {
                    let message: String = message.chars().take(300).collect();
                    core.store
                        .repo_set_clone(&id, Some("failed"), Some(&message), false)
                }
            };
            if let Err(error) = result {
                tracing::warn!(%error, "could not record a clone");
            }
        });
        Ok(self.repo_row(&record).await)
    }

    /// The clone itself: into a staging folder beside the destination, then renamed in. Git is hardened: only
    /// `https` and `ssh`, no prompts, no helpers, no submodules, no template hooks.
    async fn run_repo_clone(&self, source: &str, destination: &Path) -> Result<()> {
        let directory = destination
            .parent()
            .ok_or_else(|| CoreError::Invalid("Choose another repository name.".into()))?
            .to_path_buf();
        let staging = Staging(directory.join(format!(".incoming-{}", new_id())));
        let target = staging.0.to_string_lossy().into_owned();
        let mut clone = command(&directory);
        clone
            .env("GIT_ALLOW_PROTOCOL", "https:ssh")
            .env("GIT_TERMINAL_PROMPT", "0")
            .args([
                "-c",
                "protocol.ext.allow=never",
                "-c",
                "protocol.file.allow=never",
                "-c",
                "core.symlinks=true",
                "-c",
                "init.templateDir=",
                "clone",
                "--no-recurse-submodules",
                "--",
                source,
                &target,
            ]);
        let output = tokio::time::timeout(CLONE_LIMIT, async {
            crate::host::process::ManagedChild::spawn(clone)?
                .output()
                .await
        })
        .await
        .map_err(|_| CoreError::Git("The clone took longer than 10 minutes.".into()))?
        .map_err(|error| CoreError::Git(format!("Could not run git: {error}")))?;
        if !output.status.success() {
            let why = String::from_utf8_lossy(&output.stderr);
            return Err(CoreError::Git(format!("The clone failed: {}", why.trim())));
        }
        if std::fs::symlink_metadata(destination).is_ok() {
            return Err(CoreError::Invalid("That folder appeared meanwhile.".into()));
        }
        std::fs::rename(&staging.0, destination)?;
        Ok(())
    }

    pub fn cancel_repo_clone(&self, project_id: &str, repo_id: &str) -> Result<RepoRow> {
        on(self)?;
        let record = self
            .store
            .repo(project_id, repo_id)?
            .ok_or_else(|| CoreError::NotFound("repository".into()))?;
        let Some(stop) = self.repos.clones().get(repo_id).cloned() else {
            return Err(CoreError::Invalid(
                "That repository is not being cloned.".into(),
            ));
        };
        stop.cancel();
        Ok(RepoRow {
            id: record.id,
            name: record.name,
            path: record.path,
            is_git: false,
            trusted: false,
            branch: None,
            branches: Vec::new(),
            has_remote: false,
            primary: false,
            clone: Some(RepoClone {
                state: RepoCloneState::Failed,
                percent: None,
                message: Some("Cancelled.".into()),
            }),
        })
    }

    /// Only the person's word makes a folder usable: nothing runs in it before this.
    pub async fn trust_repo(&self, project_id: &str, repo_id: &str) -> Result<RepoRow> {
        on(self)?;
        let project = self.store.project(project_id)?;
        if repo_id == project.id {
            self.store.trust_project(&project.id, "user")?;
        } else {
            let record = self
                .store
                .repo(project_id, repo_id)?
                .ok_or_else(|| CoreError::NotFound("repository".into()))?;
            if record.clone_state.is_some() {
                return Err(CoreError::Invalid("Wait for the clone to finish.".into()));
            }
            self.audit_repo("repo.trust", &record.path, json!({ "project": project_id }))?;
            self.store.repo_trust(repo_id)?;
        }
        self.row_of(project_id, repo_id).await
    }

    /// `git init` in a trusted folder that has none. Only `.git` is written.
    pub async fn setup_repo_git(&self, project_id: &str, repo_id: &str) -> Result<RepoRow> {
        on(self)?;
        let project = self.store.project(project_id)?;
        if repo_id == project.id {
            self.init_project_git(project_id).await?;
            return self.row_of(project_id, repo_id).await;
        }
        let record = self
            .store
            .repo(project_id, repo_id)?
            .ok_or_else(|| CoreError::NotFound("repository".into()))?;
        if !record.trusted {
            return Err(CoreError::RepoUntrusted(
                "Trust this folder before starting Git in it.".into(),
            ));
        }
        let path = PathBuf::from(&record.path);
        let root = self.workspace.root()?;
        let resolved = resolve_project(&root, &path).await?;
        if resolved.is_git || record.is_git || record.clone_state.is_some() {
            return Err(CoreError::Invalid(
                "That folder is already a Git repository.".into(),
            ));
        }
        self.audit_repo(
            "repo.git-init",
            &record.path,
            json!({ "project": project_id }),
        )?;
        let parent = path
            .parent()
            .ok_or_else(|| CoreError::Invalid("Choose a folder.".into()))?;
        let target = path.to_string_lossy().into_owned();
        git(parent, &["init", "--quiet", "-b", "main", "--", &target]).await?;
        self.store.repo_mark_git(repo_id)?;
        self.row_of(project_id, repo_id).await
    }

    /// Stops tracking a repository. Its files, branches and threads stay. The project's own folder is the project.
    pub fn remove_repo(&self, project_id: &str, repo_id: &str) -> Result<()> {
        on(self)?;
        let project = self.store.project(project_id)?;
        if repo_id == project.id {
            return Err(CoreError::Invalid(
                "The project's own folder is the project; remove the project instead.".into(),
            ));
        }
        let record = self
            .store
            .repo(project_id, repo_id)?
            .ok_or_else(|| CoreError::NotFound("repository".into()))?;
        if let Some(stop) = self.repos.clones().get(repo_id).cloned() {
            stop.cancel();
        }
        self.audit_repo(
            "repo.remove",
            &record.path,
            json!({ "project": project_id }),
        )?;
        self.store.repo_delete(repo_id)
    }

    /// Publishing to a host is not built yet.
    pub async fn publish_repo(&self, project_id: &str, repo_id: &str) -> Result<RepoRow> {
        on(self)?;
        let row = self.row_of(project_id, repo_id).await?;
        if !row.trusted {
            return Err(CoreError::RepoUntrusted(
                "Trust this repository before publishing it.".into(),
            ));
        }
        Err(CoreError::Unsupported(
            "Publishing a repository arrives in a later update.".into(),
        ))
    }

    /// A folder or a `.code-workspace` file the person picked: what repositories it holds. Reads only.
    pub async fn scan_repos(&self, project_id: &str, input: ScanRepos) -> Result<RepoScan> {
        on(self)?;
        self.store.project(project_id)?;
        let given = Path::new(&input.path);
        if !given.is_absolute() {
            return Err(CoreError::Invalid("Use an absolute path.".into()));
        }
        let real = canonicalize(given)
            .map_err(|_| CoreError::Invalid(format!("{} does not exist.", given.display())))?;
        let data =
            canonicalize(&self.config.data_dir).unwrap_or_else(|_| self.config.data_dir.clone());
        if real.starts_with(&data) {
            return Err(CoreError::Invalid(
                "That folder is inside Orbyn's own data.".into(),
            ));
        }
        let known = self.known_folders()?;
        tokio::task::spawn_blocking(move || scan::scan(&real, &known))
            .await
            .map_err(|error| CoreError::Internal(error.to_string()))?
    }

    /// A clone that was running when the core stopped is gone.
    pub(crate) fn repos_recover(&self) {
        if let Err(error) = self
            .store
            .repos_fail_clones("Orbyn restarted during the clone.")
        {
            tracing::warn!(%error, "could not mark interrupted clones");
        }
    }
}

#[cfg(test)]
mod tests;
