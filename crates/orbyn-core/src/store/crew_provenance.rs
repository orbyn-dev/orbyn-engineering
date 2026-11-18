//! Per-hunk provenance at rest: who wrote each merged line range, and when provenance first began for a
//! repository. Ids, ranges and counts only: no file content, no prompt.

use rusqlite::{OptionalExtension, params};

use super::{AutopilotEvent, Store};
use crate::crew::capacity::{Author, AuthoredHunk, Hunk};
use crate::error::{CoreError, Result};

/// One stored hunk with the note that carries it, if the note was written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredHunk {
    pub hunk: AuthoredHunk,
    pub note_ref: Option<String>,
}

impl Store {
    /// Stores the hunks of one merge and, if this repository has none yet, the time provenance began, in one
    /// transaction: a failed row leaves neither the hunks nor the start behind. The start is set once and never moved.
    /// Hunks stored for this merge by an earlier try are replaced, so a retry after a failed note is safe.
    pub fn record_crew_hunks(
        &self,
        merge_id: i64,
        repo: &str,
        began_unix: i64,
        hunks: &[AuthoredHunk],
    ) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute("DELETE FROM crew_hunks WHERE merge_id = ?1", [merge_id])?;
            for item in hunks {
                let (kind, engine, model) = match &item.author {
                    Author::Agent { engine, model } => ("agent", Some(engine), model.as_ref()),
                    Author::Person => ("person", None, None),
                };
                tx.execute(
                    "INSERT INTO crew_hunks (merge_id, path, start_line, line_count, author_kind, engine, model)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        merge_id,
                        item.hunk.path,
                        item.hunk.start,
                        item.hunk.len,
                        kind,
                        engine,
                        model
                    ],
                )?;
            }
            tx.execute(
                "INSERT OR IGNORE INTO crew_provenance_start (repo, began_unix) VALUES (?1, ?2)",
                params![repo, began_unix],
            )?;
            Ok(())
        })
    }

    /// Marks every hunk of a merge as carried by the named git note.
    pub fn set_crew_hunks_note_ref(&self, merge_id: i64, note_ref: &str) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "UPDATE crew_hunks SET note_ref = ?2 WHERE merge_id = ?1",
                params![merge_id, note_ref],
            )?;
            Ok(())
        })
    }

    /// The oldest provenance failures that no later row marks resolved for the same task, at most `limit`, oldest first.
    /// Resolved rows are filtered out in SQL, so they never crowd the unresolved ones out of the bound.
    pub fn crew_unresolved_provenance_failures(&self, limit: u32) -> Result<Vec<AutopilotEvent>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT id, crew_id, task_id, kind, body, at FROM crew_autopilot_events f
                 WHERE kind = 'provenance_failed'
                   AND json_extract(body, '$.resolved') = 0
                   AND NOT EXISTS (
                       SELECT 1 FROM crew_autopilot_events r
                       WHERE r.kind = 'provenance_failed' AND json_extract(r.body, '$.resolved') = 1
                         AND r.task_id = f.task_id AND r.id > f.id)
                 ORDER BY id LIMIT ?1",
        )?;
        let rows = statement
            .query_map([limit], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(id, crew_id, task_id, kind, body, at)| {
                Ok(AutopilotEvent {
                    id,
                    crew_id,
                    task_id,
                    kind,
                    body: serde_json::from_str(&body)?,
                    at,
                })
            })
            .collect()
    }

    /// The hunks of a merge, by path and line.
    pub fn crew_hunks_of_merge(&self, merge_id: i64) -> Result<Vec<StoredHunk>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT path, start_line, line_count, author_kind, engine, model, note_ref FROM crew_hunks
             WHERE merge_id = ?1 ORDER BY path, start_line",
        )?;
        let rows = statement
            .query_map([merge_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u32>(1)?,
                    row.get::<_, u32>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(path, start, len, kind, engine, model, note_ref)| {
                let author = match (kind.as_str(), engine) {
                    ("agent", Some(engine)) => Author::Agent { engine, model },
                    ("person", _) => Author::Person,
                    _ => {
                        return Err(CoreError::Internal(
                            "a stored hunk has an unknown author".into(),
                        ));
                    }
                };
                Ok(StoredHunk {
                    hunk: AuthoredHunk {
                        hunk: Hunk { path, start, len },
                        author,
                    },
                    note_ref,
                })
            })
            .collect()
    }

    /// When provenance first began for a repository, or nothing if it never has. Merges before it are "Not measured".
    pub fn crew_provenance_began(&self, repo: &str) -> Result<Option<i64>> {
        Ok(self
            .reader()
            .query_row(
                "SELECT began_unix FROM crew_provenance_start WHERE repo = ?1",
                [repo],
                |row| row.get(0),
            )
            .optional()?)
    }
}
