//! The possible-escape scan at rest (slice 4k, PR 3; docs/crew/escapes.md). Reads and one write; migration 58 adds retention tombstones.
//!
//! - [`Store::crew_escape_scan_candidates`]: merges that reached a branch, in a project, with **no escape row yet**, newest first. A
//!   merge with a row of any state (a dismissed one included) is never a candidate, so a dismissed escape is not offered again.
//! - [`Store::crew_escape_room`]: how many escapes and events a project holds, for the caps of PR 2.
//! - [`Store::record_possible_escapes`]: one immediate transaction per project that adds `possible` rows and their `detected`
//!   events. State is written as `possible` in the SQL itself, so no scan can confirm; the writer refuses the whole batch, and
//!   writes nothing when protected history prevents admission after eligible retention. Tombstones suppress automatic replay too.

use orbyn_contracts::EscapeSource;
use rusqlite::{OptionalExtension, params};

use super::Store;
use super::crew_escape_retention::room;
use super::crew_escapes::unreadable;
use super::crew_merges::{MergeRecord, merge_row};
use crate::error::{CoreError, Result};

/// One possible escape the scan found, ready to store. `cause` names the evidence (a commit id) and is unique per escape and kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PossibleEscape {
    pub project_id: String,
    pub merge_task_id: String,
    /// `Revert`, `PostMergeCheck` or `FollowUpFix`; the writer refuses `PersonMarked`.
    pub source: EscapeSource,
    pub cause: String,
    /// One plain sentence of at most 200 characters. Never a diff, a commit subject or command output.
    pub reason: String,
}

/// What one write did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EscapesAdded {
    /// New possible escapes.
    pub added: u32,
    /// Merges that already had an escape row (of any state) and were left alone.
    pub already_held: u32,
}

fn source_word(source: EscapeSource) -> Result<&'static str> {
    match source {
        EscapeSource::Revert => Ok("revert"),
        EscapeSource::PostMergeCheck => Ok("post_merge_check"),
        EscapeSource::FollowUpFix => Ok("follow_up_fix"),
        EscapeSource::PersonMarked => Err(CoreError::Invalid(
            "A scan never writes a person's mark.".into(),
        )),
    }
}

impl Store {
    /// Merges with no escape row, made at or after `merged_since`, newest first, at most `limit`, each with its project. With
    /// `after` (the merge time and id of the last merge a scan looked at) only merges older than that in this order are returned,
    /// so successive scans rotate through them. Merges whose lead thread is gone have no project and drop out, as for 4j.
    pub fn crew_escape_scan_candidates(
        &self,
        merged_since: i64,
        limit: u32,
        after: Option<(i64, i64)>,
    ) -> Result<Vec<(MergeRecord, String)>> {
        let conn = self.reader();
        conn.prepare(
            "SELECT m.*, t.project_id AS scan_project FROM crew_merges m
             JOIN crew_runs r ON r.task_id = m.task_id
             JOIN threads t ON t.id = r.lead_thread_id
             WHERE m.merged_unix >= ?1
               AND (?3 IS NULL OR m.merged_unix < ?3 OR (m.merged_unix = ?3 AND m.id < ?4))
               AND NOT EXISTS (SELECT 1 FROM crew_escapes e WHERE e.merge_task_id = m.task_id)
               AND NOT EXISTS (SELECT 1 FROM crew_escape_tombstones x WHERE x.merge_task_id = m.task_id)
             ORDER BY m.merged_unix DESC, m.id DESC LIMIT ?2",
        )
        .map_err(unreadable)?
        .query_map(
            params![
                merged_since,
                limit,
                after.map(|a| a.0),
                after.map_or(0, |a| a.1)
            ],
            |row| Ok((merge_row(row)?, row.get::<_, String>("scan_project")?)),
        )
        .map_err(unreadable)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(unreadable)
    }

    /// Escapes and events a project holds now.
    pub fn crew_escape_room(&self, project_id: &str) -> Result<(u32, u32)> {
        let conn = self.reader();
        let count = |sql: &str| -> Result<u32> {
            conn.query_row(sql, [project_id], |row| row.get(0))
                .map_err(unreadable)
        };
        Ok((
            count("SELECT COUNT(*) FROM crew_escapes WHERE project_id = ?1")?,
            count("SELECT COUNT(*) FROM crew_escape_events WHERE project_id = ?1")?,
        ))
    }

    /// Adds the possible escapes of one project in one transaction. All of it or nothing: a batch that would pass the project's
    /// 5,000 escapes or 20,000 events is refused whole, in plain words, and nothing is written. A merge that already has an escape
    /// (any state) or that is not a merge of `project_id` is skipped; the rest are added as `possible`, decided by nobody, with one
    /// `detected` event whose `cause` is the evidence.
    pub fn record_possible_escapes(
        &self,
        project_id: &str,
        found: &[PossibleEscape],
        at: i64,
    ) -> Result<EscapesAdded> {
        for item in found {
            source_word(item.source)?;
            if item.project_id != project_id {
                return Err(CoreError::Invalid(
                    "A scan writes one project at a time.".into(),
                ));
            }
        }
        self.commit(|tx, _| {
            let mut new: Vec<&PossibleEscape> = Vec::new();
            let mut held = 0_u32;
            for item in found {
                // Only actual admissions may cause retention. Missing/foreign merges must not clear history before INSERT SELECT skips them.
                let insertable: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM crew_merges m JOIN crew_runs r ON r.task_id=m.task_id
                     JOIN threads t ON t.id=r.lead_thread_id WHERE m.task_id=?1 AND t.project_id=?2)",
                    params![item.merge_task_id, project_id], |row| row.get(0),
                ).map_err(unreadable)?;
                let exists: Option<i64> = tx
                    .query_row(
                        "SELECT 1 FROM crew_escapes WHERE merge_task_id = ?1 UNION ALL SELECT 1 FROM crew_escape_tombstones WHERE merge_task_id = ?1 LIMIT 1",
                        [&item.merge_task_id],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(unreadable)?;
                if !insertable || exists.is_some() || new.iter().any(|n| n.merge_task_id == item.merge_task_id) {
                    held += 1;
                } else {
                    new.push(item);
                }
            }
            let n = u32::try_from(new.len()).unwrap_or(u32::MAX);
            room(tx, project_id, n, n, None, at)?;
            let mut added = 0_u32;
            for item in new {
                let source = source_word(item.source)?;
                let reason: String = item.reason.chars().take(200).collect();
                let cause: String = item.cause.chars().take(200).collect();
                let inserted = tx.execute(
                    "INSERT OR IGNORE INTO crew_escapes (id, project_id, merge_task_id, source, state, engine, areas, reason, found_unix)
                     SELECT 'esc_' || lower(hex(randomblob(12))), ?1, m.task_id, ?2, 'possible', m.engine, m.areas, ?3, ?4
                     FROM crew_merges m JOIN crew_runs r ON r.task_id = m.task_id JOIN threads t ON t.id = r.lead_thread_id
                     WHERE m.task_id = ?5 AND t.project_id = ?1",
                    params![project_id, source, reason, at, item.merge_task_id],
                )?;
                if inserted != 1 {
                    held += 1;
                    continue;
                }
                tx.execute(
                    "INSERT INTO crew_escape_events (escape_id, project_id, kind, from_state, to_state, decided_by, cause, at)
                     SELECT e.id, e.project_id, 'detected', NULL, 'possible', 'core', ?2, ?3
                     FROM crew_escapes e WHERE e.merge_task_id = ?1",
                    params![item.merge_task_id, cause, at],
                )?;
                added += 1;
            }
            Ok(EscapesAdded {
                added,
                already_held: held,
            })
        })
    }
}

/// Rows for tests, which cannot reach the writer. Never built into the service.
#[cfg(any(test, feature = "test-support"))]
impl Store {
    /// Fills a project's log with `escapes` protected confirmed ghost escapes and `events` unclassified ghost events, so a full log can
    /// be tested without writing thousands of real merges.
    pub fn fill_escape_log_for_test(
        &self,
        project_id: &str,
        escapes: u32,
        events: u32,
    ) -> Result<()> {
        let conn = self.writer();
        conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
        conn.execute(
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < ?2)
             INSERT INTO crew_escapes (id, project_id, merge_task_id, source, state, engine, areas, found_unix, decided_unix, decided_by)
             SELECT 'ghost' || i, ?1, 'ghost-merge' || i, 'revert', 'confirmed', 'alpha', '[]', 1, 1, 'person' FROM n WHERE ?2 > 0",
            params![project_id, escapes],
        )?;
        conn.execute(
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < ?2)
             INSERT INTO crew_escape_events (escape_id, project_id, kind, decided_by, cause, at)
             SELECT 'ghost', ?1, 'detected', 'core', 'c' || i, 1 FROM n WHERE ?2 > 0",
            params![project_id, events],
        )?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(())
    }
}
