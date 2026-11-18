//! Project removal wakes resource owners even when there are no filesystem events.

use super::*;

impl Store {
    pub(crate) fn project_changes(&self) -> tokio::sync::watch::Receiver<u64> {
        self.project_changes.subscribe()
    }

    /// Forgets a project and keeps every thread of it, archived and labelled with the
    /// project's name. The row stays hidden, so adding the same folder back restores
    /// them. Rows that only make sense with a project go, as they did when the row
    /// was deleted.
    pub fn remove_project_keeping_threads(&self, id: &str, actor: &str) -> Result<Vec<String>> {
        let result = self.commit(|tx, appended| {
            let project = tx
                .query_row("SELECT * FROM projects WHERE id = ?1", [id], project_row)
                .optional()?
                .ok_or_else(|| CoreError::NotFound("project".into()))?;
            let ids = tombstone_project(tx, appended, &project)?;
            threads::audit(
                tx,
                actor,
                "project.remove",
                &project.path,
                serde_json::json!({ "threads_kept": ids.len() }),
            )?;
            Ok(ids)
        });
        if result.is_ok() {
            self.project_changes
                .send_modify(|revision| *revision = revision.wrapping_add(1));
        }
        result
    }

    pub fn delete_project(&self, id: &str) -> Result<()> {
        let result = self.commit(|tx, _| {
            tx.execute("DELETE FROM projects WHERE id = ?1", [id])?;
            Ok(())
        });
        if result.is_ok() {
            self.project_changes
                .send_modify(|revision| *revision = revision.wrapping_add(1));
        }
        result
    }
}
