//! Threads in several repositories (docs/multi-repo.md, "Task folders"): one folder under app data, one Git
//! worktree per repository inside it, every worktree on one new branch name. The folder is not a repository.

use super::{MAX_PER_THREAD, describe, on};
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::source_control::git::{git, git_hardened, text};
use crate::store::thread_repos::ThreadRepoRecord;
use crate::util::new_id;
use orbyn_contracts::{
    AddThreadRepo, CreateThread, StartThreadInput, TaskFolder, Thread, ThreadRepo, UpdateThread,
};
use serde_json::json;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

/// A repository a thread will use, checked: its folder is a trusted, finished Git repository.
struct Chosen {
    repo_id: String,
    name: String,
    path: PathBuf,
}

const MAX_BRANCH: usize = 200;

/// The mode a thread starts in must be one the engine offers; nothing is assumed looser than it can honour.
pub(super) fn check_mode(
    offered: &[orbyn_contracts::PermissionMode],
    mode: orbyn_contracts::PermissionMode,
    engine: &str,
) -> Result<()> {
    if offered.contains(&mode) {
        Ok(())
    } else {
        Err(CoreError::PermissionModeUnsupported(format!(
            "{engine} does not offer that permission mode."
        )))
    }
}

async fn branch_exists(repo: &Path, branch: &str) -> bool {
    git(
        repo,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    )
    .await
    .is_ok()
}

/// Removes worktrees that were made for a thread that did not come to be, and the branch each one created.
async fn undo(made: &[(PathBuf, PathBuf)], branch: &str, folder: &Path) {
    for (repo, worktree) in made {
        let target = worktree.to_string_lossy().into_owned();
        let _ = git(repo, &["worktree", "remove", "--force", "--", &target]).await;
        let _ = git(repo, &["branch", "-D", "--", branch]).await;
    }
    let _ = std::fs::remove_dir_all(folder);
}

/// What retiring one worktree of a task folder came to.
#[derive(Debug, PartialEq, Eq)]
enum Retired {
    Removed,
    /// Left where it is: it holds changes or ignored files, or is not inside the thread's own folder.
    Kept(&'static str),
}

impl Core {
    /// The folder of the repository `repo_id` of `project_id`, as the source of its worktrees.
    fn repo_source(&self, project_id: &str, repo_id: &str) -> Result<PathBuf> {
        if repo_id == project_id {
            return Ok(PathBuf::from(self.store.project(project_id)?.path));
        }
        self.store
            .repo(project_id, repo_id)?
            .map(|record| PathBuf::from(record.path))
            .ok_or_else(|| CoreError::NotFound("repository".into()))
    }

    /// Removes one worktree Orbyn made in a thread's task folder, never one with something to lose: a change, an
    /// untracked file or an ignored file (`.env`, build output) keeps it. The branch is never touched.
    async fn retire_worktree(&self, project_id: &str, record: &ThreadRepoRecord) -> Retired {
        let worktree = PathBuf::from(&record.worktree_path);
        let folder = self.config.data_dir.join("threads").join(&record.thread_id);
        if !worktree.starts_with(&folder) || worktree == folder {
            return Retired::Kept("it is not in this thread's own folder");
        }
        let Ok(source) = self.repo_source(project_id, &record.repo_id) else {
            return Retired::Kept("its repository is gone");
        };
        if !worktree.exists() {
            let _ = git(&source, &["worktree", "prune"]).await;
            return Retired::Removed;
        }
        match git_hardened(&worktree, &["status", "--porcelain", "--ignored"]).await {
            Ok(out) if text(&out).trim().is_empty() => {}
            Ok(_) => return Retired::Kept("it holds changes or ignored files"),
            Err(_) => return Retired::Kept("its state cannot be read"),
        }
        let target = worktree.to_string_lossy().into_owned();
        match git(&source, &["worktree", "remove", "--", &target]).await {
            Ok(_) => Retired::Removed,
            Err(_) => Retired::Kept("Git refused to remove it"),
        }
    }

    /// Deletes a thread. A multi-repository thread's worktrees go with it when they hold nothing to lose; its branch
    /// stays, as a single-repository thread's does. What was kept is in the audit log.
    pub async fn delete_thread_and_worktrees(&self, id: &str, actor: &str) -> Result<()> {
        let records = if self.config.multi_repo {
            self.store.thread_repos(id)?
        } else {
            Vec::new()
        };
        let project_id = self.store.thread(id)?.project_id;
        self.delete_thread(id, actor)?;
        let mut kept = Vec::new();
        for record in &records {
            if let Retired::Kept(why) = self.retire_worktree(&project_id, record).await {
                kept.push(json!({ "repo": record.name, "why": why }));
            }
        }
        if let Some(first) = records.first() {
            let folder = self.config.data_dir.join("threads").join(&first.thread_id);
            // Empty after the worktrees are gone; a folder that still holds one is left alone.
            let _ = std::fs::remove_dir(&folder);
        }
        if !records.is_empty() {
            self.audit_repo(
                "repo.thread.delete",
                id,
                json!({ "repos": records.len(), "kept": kept }),
            )?;
        }
        Ok(())
    }

    /// Removes one repository from a multi-repository thread. The primary one stays, the last one stays, a run in
    /// flight blocks it, and a worktree with changes is refused so nothing is lost; the branch is kept.
    pub async fn remove_thread_repo(&self, thread_id: &str, repo_id: &str) -> Result<()> {
        on(self)?;
        let thread = self.store.thread(thread_id)?;
        let records = self.store.thread_repos(thread_id)?;
        let record = records
            .iter()
            .find(|r| r.repo_id == repo_id)
            .ok_or_else(|| CoreError::NotFound("repository".into()))?;
        if record.position == 0 {
            return Err(CoreError::Invalid(
                "The primary repository stays in the thread.".into(),
            ));
        }
        if thread.active_run.is_some() {
            return Err(CoreError::Invalid(
                "Stop the run before removing a repository.".into(),
            ));
        }
        match self.retire_worktree(&thread.project_id, record).await {
            Retired::Removed => {}
            Retired::Kept(why) => {
                return Err(CoreError::Invalid(format!(
                    "{} stays in the thread: {why}. Commit or discard its changes first.",
                    record.name
                )));
            }
        }
        self.store.thread_repo_delete(thread_id, repo_id)?;
        self.audit_repo(
            "repo.thread.remove",
            thread_id,
            json!({ "repo": repo_id, "branch": record.branch }),
        )?;
        Ok(())
    }

    /// One repository the person chose, or why it cannot be used. An untrusted one is refused here.
    async fn chosen_repo(
        &self,
        project: &orbyn_contracts::Project,
        repo_id: &str,
    ) -> Result<Chosen> {
        if repo_id == project.id {
            if !project.trusted {
                return Err(CoreError::RepoUntrusted(
                    "Trust the project's folder before starting a thread in it.".into(),
                ));
            }
            if !project.is_git {
                return Err(CoreError::NotGit(
                    "The project's folder is not a Git repository.".into(),
                ));
            }
            let leaf = Path::new(&project.path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            return Ok(Chosen {
                repo_id: project.id.clone(),
                name: super::source::folder_name(&leaf).unwrap_or_else(|| "project".into()),
                path: PathBuf::from(&project.path),
            });
        }
        let record = self
            .store
            .repo(&project.id, repo_id)?
            .ok_or_else(|| CoreError::NotFound("repository".into()))?;
        if record.clone_state.is_some() {
            return Err(CoreError::Invalid(format!(
                "{} is still being cloned.",
                record.name
            )));
        }
        if !record.trusted {
            return Err(CoreError::RepoUntrusted(format!(
                "Trust {} before starting a thread in it.",
                record.name
            )));
        }
        if !record.is_git {
            return Err(CoreError::NotGit(format!(
                "{} is not a Git repository.",
                record.name
            )));
        }
        Ok(Chosen {
            repo_id: record.id,
            name: record.name,
            path: PathBuf::from(record.path),
        })
    }

    /// Starts a thread in two or more repositories of a project.
    pub async fn start_repo_thread(
        &self,
        project_id: &str,
        input: StartThreadInput,
    ) -> Result<Thread> {
        on(self)?;
        let project = self.store.project(project_id)?;
        if input.repos.len() > MAX_PER_THREAD as usize {
            return Err(CoreError::RepoLimit(format!(
                "A thread works in at most {MAX_PER_THREAD} repositories."
            )));
        }
        if input.repos.len() < 2 {
            return Err(CoreError::Invalid(
                "Choose two or more repositories; one repository uses the normal thread start."
                    .into(),
            ));
        }
        let mut seen = HashSet::new();
        let mut chosen = Vec::new();
        let mut bases = Vec::new();
        for choice in &input.repos {
            if !seen.insert(choice.repo_id.clone()) {
                return Err(CoreError::Invalid("A repository is chosen twice.".into()));
            }
            let repo = self.chosen_repo(&project, &choice.repo_id).await?;
            if choice.base_branch.is_empty()
                || choice.base_branch.starts_with('-')
                || !branch_exists(&repo.path, &choice.base_branch).await
            {
                return Err(CoreError::Invalid(format!(
                    "{} is not a branch of {}.",
                    choice.base_branch.chars().take(80).collect::<String>(),
                    repo.name
                )));
            }
            bases.push(choice.base_branch.clone());
            chosen.push(repo);
        }
        // The project's own folder is the primary repository and comes first.
        if let Some(at) = chosen.iter().position(|c| c.repo_id == project.id) {
            chosen.swap(0, at);
            bases.swap(0, at);
        }
        let mut names = HashSet::new();
        for repo in &mut chosen {
            let wanted = repo.name.clone();
            let mut n = 2;
            while !names.insert(repo.name.clone()) {
                repo.name = format!("{wanted}-{n}");
                n += 1;
            }
        }
        let driver = self.engines.driver(&input.engine)?;
        self.check_engine_enabled(&input.engine, &project.id)?;
        check_mode(
            &driver.descriptor().capabilities.permission_modes,
            input.permission_mode,
            &input.engine,
        )?;
        let thread_id = new_id();
        let branch = match input.branch_name.as_deref().map(str::trim) {
            Some(name) if !name.is_empty() => name.to_owned(),
            _ => format!("orbyn/{}", thread_id.chars().take(8).collect::<String>()),
        };
        if branch.len() > MAX_BRANCH
            || git(&chosen[0].path, &["check-ref-format", "--branch", &branch])
                .await
                .is_err()
        {
            return Err(CoreError::Invalid(
                "That is not a valid branch name.".into(),
            ));
        }
        for repo in &chosen {
            if branch_exists(&repo.path, &branch).await {
                return Err(CoreError::Invalid(format!(
                    "{} already has a branch named {branch}.",
                    repo.name
                )));
            }
        }
        let folder = self.config.data_dir.join("threads").join(&thread_id);
        std::fs::create_dir_all(&folder)?;
        let mut made: Vec<(PathBuf, PathBuf)> = Vec::new();
        for (repo, base) in chosen.iter().zip(&bases) {
            let worktree = folder.join(&repo.name);
            let target = worktree.to_string_lossy().into_owned();
            let start = format!("refs/heads/{base}");
            if let Err(error) = git(
                &repo.path,
                &["worktree", "add", "-b", &branch, "--", &target, &start],
            )
            .await
            {
                undo(&made, &branch, &folder).await;
                return Err(error);
            }
            made.push((repo.path.clone(), worktree));
        }
        let request = CreateThread {
            project_id: Some(project.id.clone()),
            engine: input.engine.clone(),
            model: input.model.clone(),
            title: None,
            worktree: None,
            skill_profile: None,
            base_branch: None,
            effort: None,
            options: BTreeMap::new(),
        };
        let thread = match self
            .create_thread_in(
                request,
                Some((thread_id.clone(), folder.to_string_lossy().into_owned())),
            )
            .await
        {
            Ok(thread) => thread,
            Err(error) => {
                undo(&made, &branch, &folder).await;
                return Err(error);
            }
        };
        let stamp = crate::util::now();
        for (position, ((repo, base), (_, worktree))) in
            chosen.iter().zip(&bases).zip(&made).enumerate()
        {
            let record = ThreadRepoRecord {
                thread_id: thread.id.clone(),
                repo_id: repo.repo_id.clone(),
                position: u32::try_from(position).unwrap_or(0),
                name: repo.name.clone(),
                branch: branch.clone(),
                base_branch: Some(base.clone()),
                worktree_path: worktree.to_string_lossy().into_owned(),
            };
            if let Err(error) = self.store.thread_repo_insert(&record, &stamp) {
                let _ = self.delete_thread(&thread.id, "repos");
                undo(&made, &branch, &folder).await;
                return Err(error);
            }
        }
        let patch = UpdateThread {
            permission_mode: Some(input.permission_mode),
            ..UpdateThread::default()
        };
        if let Err(error) = self.store.update_thread(&thread.id, &patch, "user") {
            tracing::warn!(%error, "could not set the permission mode of a repository thread");
        }
        self.audit_repo(
            "repo.thread",
            &thread.id,
            json!({ "project": project.id, "repos": chosen.len(), "branch": branch }),
        )?;
        if let Some(message) = input
            .message
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty())
            && let Err(error) = self.runs.enqueue(&thread.id, message, &[])
        {
            let _ = self.delete_thread(&thread.id, "repos");
            undo(&made, &branch, &folder).await;
            return Err(error);
        }
        self.store.thread(&thread.id)
    }

    /// Adds a trusted repository of the project to a multi-repository thread, on the thread's branch.
    pub async fn add_thread_repo(
        &self,
        thread_id: &str,
        input: AddThreadRepo,
    ) -> Result<ThreadRepo> {
        on(self)?;
        let thread = self.store.thread(thread_id)?;
        let records = self.store.thread_repos(thread_id)?;
        let Some(first) = records.first() else {
            return Err(CoreError::NotFound("repositories".into()));
        };
        if records.len() >= MAX_PER_THREAD as usize {
            return Err(CoreError::RepoLimit(format!(
                "A thread works in at most {MAX_PER_THREAD} repositories."
            )));
        }
        if records.iter().any(|r| r.repo_id == input.repo_id) {
            return Err(CoreError::Invalid(
                "That repository is already in this thread.".into(),
            ));
        }
        let project = self.store.project(&thread.project_id)?;
        let repo = self.chosen_repo(&project, &input.repo_id).await?;
        let branch = first.branch.clone();
        if branch_exists(&repo.path, &branch).await {
            return Err(CoreError::Invalid(format!(
                "{} already has a branch named {branch}.",
                repo.name
            )));
        }
        let (current, _, _) = describe(&repo.path, true).await;
        let base = current.ok_or_else(|| {
            CoreError::Invalid(format!("Check out a branch in {} first.", repo.name))
        })?;
        let mut name = repo.name.clone();
        let taken: HashSet<_> = records.iter().map(|r| r.name.clone()).collect();
        let mut n = 2;
        while taken.contains(&name) {
            name = format!("{}-{n}", repo.name);
            n += 1;
        }
        let worktree = Path::new(&thread.cwd).join(&name);
        let target = worktree.to_string_lossy().into_owned();
        git(
            &repo.path,
            &[
                "worktree",
                "add",
                "-b",
                &branch,
                "--",
                &target,
                &format!("refs/heads/{base}"),
            ],
        )
        .await?;
        let record = ThreadRepoRecord {
            thread_id: thread_id.to_owned(),
            repo_id: repo.repo_id.clone(),
            position: u32::try_from(records.len()).unwrap_or(0),
            name,
            branch,
            // The branch the worktree above was cut from: the one checked out in that repository.
            base_branch: Some(base),
            worktree_path: target,
        };
        if let Err(error) = self.store.thread_repo_insert(&record, &crate::util::now()) {
            undo(&[(repo.path, worktree)], &record.branch, Path::new("")).await;
            return Err(error);
        }
        self.audit_repo(
            "repo.thread-add",
            thread_id,
            json!({ "repo": record.repo_id }),
        )?;
        let listed = self.thread_repos(thread_id).await?;
        listed
            .into_iter()
            .find(|r| r.repo_id == record.repo_id)
            .ok_or_else(|| CoreError::NotFound("repository".into()))
    }

    /// The thread's folder and what is in it. `NotFound` for an ordinary thread.
    pub async fn task_folder(&self, thread_id: &str) -> Result<TaskFolder> {
        on(self)?;
        let thread = self.store.thread(thread_id)?;
        let records = self.store.thread_repos(thread_id)?;
        let branch = records
            .first()
            .map(|r| r.branch.clone())
            .ok_or_else(|| CoreError::NotFound("task folder".into()))?;
        Ok(TaskFolder {
            root: thread.cwd,
            branch,
            repos: self.thread_repos(thread_id).await?,
            max_per_thread: MAX_PER_THREAD,
        })
    }
}
