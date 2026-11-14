//! The reads behind the Flow screens (Crew Preview): which crews have work and which tasks are theirs. Reads only, on the
//! store's read connection. A work task is a task the lead started to build something: a reviewer's run and the fix
//! that answers a finding are rounds of another task, not rows of their own.

use super::Store;
use super::crew::{CrewTask, task_row};
use crate::error::Result;

/// The condition that leaves out reviewer runs and fix tasks.
const WORK: &str = "id NOT IN (SELECT task_id FROM crew_reviews) AND id NOT IN (SELECT fix_task_id FROM crew_fixes)";

/// One crew's count of work tasks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrewTotals {
    pub crew_id: String,
    pub tasks: u32,
    pub finished: u32,
    pub updated_at: String,
}

impl Store {
    /// Crews with work tasks, the most recently moved first, and how many there are.
    pub fn crew_totals_page(&self, limit: u32, offset: u32) -> Result<(Vec<CrewTotals>, u32)> {
        let conn = self.reader();
        let total: u32 = conn.query_row(
            &format!("SELECT COUNT(DISTINCT crew_id) FROM crew_tasks WHERE {WORK}"),
            [],
            |row| row.get(0),
        )?;
        let mut statement = conn.prepare(&format!(
            "SELECT crew_id, COUNT(*), SUM(state IN ('passed', 'stopped', 'cancelled')), MAX(updated_at) \
             FROM crew_tasks WHERE {WORK} GROUP BY crew_id ORDER BY MAX(updated_at) DESC, crew_id LIMIT ?1 OFFSET ?2"
        ))?;
        let rows = statement
            .query_map([limit, offset], |row| {
                Ok(CrewTotals {
                    crew_id: row.get(0)?,
                    tasks: row.get(1)?,
                    finished: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok((rows, total))
    }

    /// The newest `limit` work tasks of a crew, newest first.
    pub fn crew_work_tasks(&self, crew_id: &str, limit: u32) -> Result<Vec<CrewTask>> {
        let conn = self.reader();
        let mut statement = conn.prepare(&format!(
            "SELECT * FROM crew_tasks WHERE crew_id = ?1 AND {WORK} ORDER BY created_at DESC, rowid DESC LIMIT ?2"
        ))?;
        Ok(statement
            .query_map(rusqlite::params![crew_id, limit], task_row)?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Whether a task is a work task (not a reviewer's run and not a fix).
    pub fn crew_task_is_work(&self, task_id: &str) -> Result<bool> {
        Ok(self.reader().query_row(
            &format!("SELECT COUNT(*) FROM crew_tasks WHERE id = ?1 AND {WORK}"),
            [task_id],
            |row| row.get::<_, u32>(0),
        )? == 1)
    }
}
