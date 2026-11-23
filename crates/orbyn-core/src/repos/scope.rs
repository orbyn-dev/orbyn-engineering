//! Which repository of a multi-repository thread a Git or file call is about (docs/multi-repo.md, "Per-repository
//! Git routes"). A route says it with `repo_id`; the one resolver every thread Git and file call goes through then
//! reads the thread with its `cwd` set to that repository's worktree. Absent means the primary repository, so a
//! thread with one repository, and every older client, are unchanged.

use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::source_control::git::{git_hardened, text};
use orbyn_contracts::{Thread, ThreadRepo};
use std::future::Future;
use std::path::Path;

tokio::task_local! {
    static REPO_SCOPE: Option<String>;
}

/// Runs `work` with `repo_id` as the repository its thread Git and file calls act on. Work that is spawned from
/// inside does not inherit it, so a spawned task names its repository itself.
pub async fn in_repo<F: Future>(repo_id: Option<String>, work: F) -> F::Output {
    REPO_SCOPE.scope(repo_id, work).await
}

fn wanted() -> Option<String> {
    REPO_SCOPE.try_with(Clone::clone).ok().flatten()
}

/// The most changed files a repository reports.
const MAX_FILES: usize = 200;

impl Core {
    /// The repository a pull request link of this thread belongs to: the one asked for, else the thread's own
    /// project, which is its primary repository. `scoped_thread` is what checks the repository exists.
    pub fn pull_repo(&self, thread_id: &str) -> Result<String> {
        match wanted() {
            Some(repo_id) => Ok(repo_id),
            None => Ok(self.store.thread(thread_id)?.project_id),
        }
    }

    /// The repository id a diff of this thread names on its files: the one asked for, else the primary. `None` for
    /// an ordinary thread, whose files name no repository.
    pub fn diff_repo(&self, thread_id: &str, asked: Option<&str>) -> Result<Option<String>> {
        if !self.config.multi_repo || self.store.thread_repos(thread_id)?.is_empty() {
            return Ok(None);
        }
        Ok(Some(match asked {
            Some(id) => id.to_owned(),
            None => self.store.thread(thread_id)?.project_id,
        }))
    }

    /// The thread as stored, with `cwd` set to the worktree of the repository asked for. An untrusted repository
    /// is refused here, so no new entry point can reach one.
    pub fn scoped_thread(&self, id: &str) -> Result<Thread> {
        let mut thread = self.store.thread(id)?;
        let asked = wanted();
        if asked.is_some() && !self.config.multi_repo {
            return Err(CoreError::NotFound("repository".into()));
        }
        let repos = self.store.thread_repos(id)?;
        if repos.is_empty() {
            return match asked {
                None => Ok(thread),
                Some(_) => Err(CoreError::NotFound("repository".into())),
            };
        }
        let chosen = match &asked {
            None => repos.first(),
            Some(repo_id) => repos.iter().find(|repo| &repo.repo_id == repo_id),
        }
        .ok_or_else(|| CoreError::NotFound("repository".into()))?;
        // A repository that was untrusted since, or never trusted, does nothing. One removed from the project
        // keeps working in a thread that already has its worktree.
        if chosen.repo_id != thread.project_id
            && self
                .store
                .repo(&thread.project_id, &chosen.repo_id)?
                .is_some_and(|row| !row.trusted)
        {
            return Err(CoreError::RepoUntrusted(
                "Trust this repository before using it.".into(),
            ));
        }
        thread.cwd.clone_from(&chosen.worktree_path);
        Ok(thread)
    }

    /// The repositories a multi-repository thread works in, with what changed in each. `NotFound` for an ordinary thread.
    pub async fn thread_repos(&self, thread_id: &str) -> Result<Vec<ThreadRepo>> {
        if !self.config.multi_repo {
            return Err(CoreError::NotFound("repositories".into()));
        }
        let thread = self.store.thread(thread_id)?;
        let records = self.store.thread_repos(thread_id)?;
        if records.is_empty() {
            return Err(CoreError::NotFound("repositories".into()));
        }
        let mut out = Vec::new();
        for record in records {
            let trusted = record.repo_id == thread.project_id
                || self
                    .store
                    .repo(&thread.project_id, &record.repo_id)?
                    .is_none_or(|row| row.trusted);
            let (added, deleted, files) = if trusted {
                changes(Path::new(&record.worktree_path)).await
            } else {
                (0, 0, None)
            };
            let pull_request = self
                .store
                .linked_pulls(thread_id, &record.repo_id)?
                .into_iter()
                .max_by_key(|pull| (pull.state == "open", pull.number));
            out.push(ThreadRepo {
                repo_id: record.repo_id,
                name: record.name,
                branch: record.branch,
                added,
                deleted,
                trusted,
                files,
                pull_request,
            });
        }
        Ok(out)
    }
}

/// Lines added and deleted, and the changed files, of one worktree against its HEAD. Anything that cannot be read is
/// no change: the answer is for display.
async fn changes(worktree: &Path) -> (u32, u32, Option<Vec<String>>) {
    let Ok(out) = git_hardened(worktree, &["diff", "--numstat", "HEAD"]).await else {
        return (0, 0, None);
    };
    let (mut added, mut deleted) = (0u32, 0u32);
    let mut files = Vec::new();
    for line in text(&out).lines() {
        let mut parts = line.splitn(3, '\t');
        let (Some(a), Some(d), Some(path)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        added = added.saturating_add(a.parse().unwrap_or(0));
        deleted = deleted.saturating_add(d.parse().unwrap_or(0));
        if files.len() < MAX_FILES {
            files.push(path.to_owned());
        }
    }
    (added, deleted, Some(files))
}
