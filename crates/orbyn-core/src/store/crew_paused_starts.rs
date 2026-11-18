//! The starts that review capacity refused, kept so a person can see what is held. One bounded row per trigger: ids,
//! counts and times only, never task text or engine output. Nothing here retries anything.

use rusqlite::{OptionalExtension, Transaction, params};

use super::Store;
use crate::crew::capacity::PausedKind;
use crate::error::{CoreError, Result};

/// The most rows kept for one project. The oldest by last sighting are dropped past it.
pub const PAUSED_STARTS_PER_PROJECT: u32 = 200;

/// One refused start as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PausedStartRow {
    pub id: String,
    pub kind: PausedKind,
    pub key: String,
    /// `held` until the same trigger starts, then `started`.
    pub started: bool,
    /// The waiting count and limit at the last refusal.
    pub waiting: u32,
    pub limit: u32,
    pub first_seen_unix: i64,
    pub last_seen_unix: i64,
    pub started_unix: Option<i64>,
}

impl Store {
    /// Notes a refused start. The same project, kind and key updates its one row (and holds it again if it had started);
    /// a new one is added, and the project's oldest rows past the cap are dropped.
    pub fn record_paused_start(
        &self,
        project_id: &str,
        kind: PausedKind,
        key: &str,
        waiting: u32,
        limit: u32,
        now: i64,
    ) -> Result<()> {
        self.commit(|tx, _| {
            let was_started: Option<bool> = tx
                .query_row(
                    "SELECT state = 'started' FROM crew_paused_starts WHERE project_id = ?1 AND kind = ?2 AND key = ?3",
                    params![project_id, kind.as_str(), key],
                    |row| row.get(0),
                )
                .optional()?;
            match was_started {
                Some(false) => {
                    tx.execute(
                        "UPDATE crew_paused_starts SET waiting = ?4, limit_n = ?5, last_seen_unix = ?6
                         WHERE project_id = ?1 AND kind = ?2 AND key = ?3",
                        params![project_id, kind.as_str(), key, waiting, limit, now],
                    )?;
                }
                Some(true) => {
                    // Refused again after it had started: a new hold.
                    tx.execute(
                        "UPDATE crew_paused_starts SET state = 'held', started_unix = NULL, waiting = ?4, limit_n = ?5,
                             first_seen_unix = ?6, last_seen_unix = ?6
                         WHERE project_id = ?1 AND kind = ?2 AND key = ?3",
                        params![project_id, kind.as_str(), key, waiting, limit, now],
                    )?;
                }
                None => {
                    tx.execute(
                        "INSERT INTO crew_paused_starts
                           (id, project_id, kind, key, state, waiting, limit_n, first_seen_unix, last_seen_unix)
                         VALUES (?1, ?2, ?3, ?4, 'held', ?5, ?6, ?7, ?7)",
                        params![crate::util::new_id(), project_id, kind.as_str(), key, waiting, limit, now],
                    )?;
                    trim(tx, project_id)?;
                }
            }
            Ok(())
        })
    }

    /// The same trigger started: its held row, if any, becomes `started`. Nothing else changes; no row is a no-op.
    pub fn mark_paused_start_started(
        &self,
        project_id: &str,
        kind: PausedKind,
        key: &str,
        now: i64,
    ) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "UPDATE crew_paused_starts SET state = 'started', started_unix = ?4
                 WHERE project_id = ?1 AND kind = ?2 AND key = ?3 AND state = 'held'",
                params![project_id, kind.as_str(), key, now],
            )?;
            Ok(())
        })
    }

    /// A project's rows, the most recently seen first.
    pub fn crew_paused_starts(&self, project_id: &str) -> Result<Vec<PausedStartRow>> {
        let reader = self.reader();
        let mut statement = reader.prepare(
            "SELECT id, kind, key, state, waiting, limit_n, first_seen_unix, last_seen_unix, started_unix
             FROM crew_paused_starts WHERE project_id = ?1 ORDER BY last_seen_unix DESC, id",
        )?;
        let rows = statement.query_map([project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, Option<i64>>(8)?,
            ))
        })?;
        let bad = |what: &str| CoreError::Internal(format!("a stored paused start has {what}"));
        rows.map(|row| {
            let (id, kind, key, state, waiting, limit, first, last, started) = row?;
            Ok(PausedStartRow {
                id,
                kind: PausedKind::parse(&kind).ok_or_else(|| bad("an unknown kind"))?,
                key,
                started: state == "started",
                waiting: u32::try_from(waiting).map_err(|_| bad("a bad count"))?,
                limit: u32::try_from(limit).map_err(|_| bad("a bad limit"))?,
                first_seen_unix: first,
                last_seen_unix: last,
                started_unix: started,
            })
        })
        .collect()
    }
}

/// Drops the project's rows past the cap: `started` rows first (oldest first), then the oldest `held` ones.
fn trim(tx: &Transaction<'_>, project_id: &str) -> Result<()> {
    tx.execute(
        "DELETE FROM crew_paused_starts WHERE project_id = ?1 AND id NOT IN (
             SELECT id FROM crew_paused_starts WHERE project_id = ?1
             ORDER BY (state = 'held') DESC, last_seen_unix DESC, rowid DESC LIMIT ?2)",
        params![project_id, PAUSED_STARTS_PER_PROJECT],
    )?;
    Ok(())
}
