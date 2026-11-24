//! The issue tracker a project's issues come from (docs/issues.md, "Create, comment, react, templates and project
//! binding"): two names and no secret. A removed project takes its binding with it through the foreign key.

use rusqlite::{OptionalExtension, params};

use super::Store;
use crate::error::Result;

/// The tracker and scope a project is bound to, as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IssueTrackerBinding {
    pub tracker: String,
    pub scope: String,
}

impl Store {
    pub(crate) fn project_issue_tracker(
        &self,
        project_id: &str,
    ) -> Result<Option<IssueTrackerBinding>> {
        Ok(self
            .reader()
            .query_row(
                "SELECT tracker, scope FROM project_issue_trackers WHERE project_id = ?1",
                [project_id],
                |row| {
                    Ok(IssueTrackerBinding {
                        tracker: row.get(0)?,
                        scope: row.get(1)?,
                    })
                },
            )
            .optional()?)
    }

    /// Stores the binding, replacing a previous one for the project.
    pub(crate) fn set_project_issue_tracker(
        &self,
        project_id: &str,
        tracker: &str,
        scope: &str,
        now: &str,
    ) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "INSERT INTO project_issue_trackers (project_id, tracker, scope, created_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (project_id) DO UPDATE SET
                    tracker = excluded.tracker,
                    scope = excluded.scope,
                    created_at = excluded.created_at",
                params![project_id, tracker, scope, now],
            )?;
            Ok(())
        })
    }

    /// Removes the binding; a project with none is already unbound.
    pub(crate) fn clear_project_issue_tracker(&self, project_id: &str) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "DELETE FROM project_issue_trackers WHERE project_id = ?1",
                [project_id],
            )?;
            Ok(())
        })
    }
}
