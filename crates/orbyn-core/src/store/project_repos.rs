//! The repositories of a project beyond its own folder (docs/multi-repo.md).

use rusqlite::{OptionalExtension, params};

use super::Store;
use crate::error::Result;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RepoRecord {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub path: String,
    pub trusted: bool,
    pub is_git: bool,
    pub managed: bool,
    /// `cloning` or `failed`, else none.
    pub clone_state: Option<String>,
    pub clone_message: Option<String>,
}

const COLUMNS: &str =
    "id, project_id, name, path, trusted, is_git, managed, clone_state, clone_message";

fn record(row: &rusqlite::Row<'_>) -> rusqlite::Result<RepoRecord> {
    Ok(RepoRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        name: row.get(2)?,
        path: row.get(3)?,
        trusted: row.get(4)?,
        is_git: row.get(5)?,
        managed: row.get(6)?,
        clone_state: row.get(7)?,
        clone_message: row.get(8)?,
    })
}

impl Store {
    pub(crate) fn repos_of(&self, project_id: &str) -> Result<Vec<RepoRecord>> {
        let conn = self.reader();
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM project_repos WHERE project_id = ?1 ORDER BY created_at, id"
        ))?;
        let rows = stmt.query_map([project_id], record)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub(crate) fn repo(&self, project_id: &str, repo_id: &str) -> Result<Option<RepoRecord>> {
        Ok(self
            .reader()
            .query_row(
                &format!("SELECT {COLUMNS} FROM project_repos WHERE project_id = ?1 AND id = ?2"),
                params![project_id, repo_id],
                record,
            )
            .optional()?)
    }

    pub(crate) fn repo_paths(&self) -> Result<Vec<String>> {
        let conn = self.reader();
        let mut stmt = conn.prepare("SELECT path FROM project_repos")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub(crate) fn repo_insert(&self, repo: &RepoRecord, now: &str) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "INSERT INTO project_repos (id, project_id, name, path, trusted, is_git, managed, clone_state,
                    clone_message, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    repo.id,
                    repo.project_id,
                    repo.name,
                    repo.path,
                    repo.trusted,
                    repo.is_git,
                    repo.managed,
                    repo.clone_state,
                    repo.clone_message,
                    now
                ],
            )?;
            Ok(())
        })
    }

    pub(crate) fn repo_set_clone(
        &self,
        repo_id: &str,
        state: Option<&str>,
        message: Option<&str>,
        is_git: bool,
    ) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "UPDATE project_repos SET clone_state = ?2, clone_message = ?3, is_git = ?4 WHERE id = ?1",
                params![repo_id, state, message, is_git],
            )?;
            Ok(())
        })
    }

    pub(crate) fn repo_trust(&self, repo_id: &str) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "UPDATE project_repos SET trusted = 1 WHERE id = ?1",
                [repo_id],
            )?;
            Ok(())
        })
    }

    pub(crate) fn repo_mark_git(&self, repo_id: &str) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "UPDATE project_repos SET is_git = 1 WHERE id = ?1",
                [repo_id],
            )?;
            Ok(())
        })
    }

    pub(crate) fn repo_delete(&self, repo_id: &str) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute("DELETE FROM project_repos WHERE id = ?1", [repo_id])?;
            Ok(())
        })
    }

    /// A clone that was running when the core stopped is gone.
    pub(crate) fn repos_fail_clones(&self, message: &str) -> Result<usize> {
        self.commit(|tx, _| {
            Ok(tx.execute(
                "UPDATE project_repos SET clone_state = 'failed', clone_message = ?1 WHERE clone_state = 'cloning'",
                [message],
            )?)
        })
    }
}
