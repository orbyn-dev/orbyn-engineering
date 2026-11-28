//! The repositories of a multi-repository thread (docs/multi-repo.md).

use rusqlite::params;

use super::Store;
use crate::error::Result;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ThreadRepoRecord {
    pub thread_id: String,
    pub repo_id: String,
    pub position: u32,
    pub name: String,
    pub branch: String,
    /// The branch the thread's branch was cut from, from migration 90 on. None for a row written before it:
    /// the base was not stored then, so it cannot be known rather than defaulted.
    pub base_branch: Option<String>,
    pub worktree_path: String,
}

impl Store {
    /// The thread's repositories, primary first. Empty for an ordinary thread.
    pub(crate) fn thread_repos(&self, thread_id: &str) -> Result<Vec<ThreadRepoRecord>> {
        let conn = self.reader();
        let mut stmt = conn.prepare(
            "SELECT thread_id, repo_id, position, name, branch, base_branch, worktree_path FROM thread_repos
             WHERE thread_id = ?1 ORDER BY position",
        )?;
        let rows = stmt.query_map([thread_id], |row| {
            Ok(ThreadRepoRecord {
                thread_id: row.get(0)?,
                repo_id: row.get(1)?,
                position: u32::try_from(row.get::<_, i64>(2)?).unwrap_or(0),
                name: row.get(3)?,
                branch: row.get(4)?,
                base_branch: row.get(5)?,
                worktree_path: row.get(6)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Drops one repository from a thread, and the pull request links and watches it had. The thread's other rows stay.
    pub(crate) fn thread_repo_delete(&self, thread_id: &str, repo_id: &str) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "DELETE FROM thread_pull_requests WHERE thread_id = ?1 AND repo_id = ?2",
                params![thread_id, repo_id],
            )?;
            tx.execute(
                "DELETE FROM pull_watches WHERE thread_id = ?1 AND repo_id = ?2",
                params![thread_id, repo_id],
            )?;
            tx.execute(
                "DELETE FROM thread_repos WHERE thread_id = ?1 AND repo_id = ?2",
                params![thread_id, repo_id],
            )?;
            Ok(())
        })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn thread_repo_insert(&self, record: &ThreadRepoRecord, now: &str) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "INSERT INTO thread_repos (thread_id, repo_id, position, name, branch, base_branch, worktree_path, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    record.thread_id,
                    record.repo_id,
                    record.position,
                    record.name,
                    record.branch,
                    record.base_branch,
                    record.worktree_path,
                    now
                ],
            )?;
            Ok(())
        })
    }
}
