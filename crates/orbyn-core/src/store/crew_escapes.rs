//! The escape log at rest (slice 4k, PR 1; docs/crew/escapes.md): merges that got past review and checks and were later found
//! defective. This slice only reads. The `revert` source is written by the schema itself: migration 51 back-fills it and a trigger
//! on `crew_merges` adds one possible escape, and its first audit row, in the same transaction as the revert, so a repeat adds
//! nothing and no caller can forget it. Ids, commit ids, enums, areas, engine and model names and one short reason of at most 200
//! characters are stored; never a diff, a test body, command output, a prompt or a secret.
//!
//! **Unreadable fails closed.** Every read maps any storage error, an unknown enum word or a damaged `areas` value to
//! `CoreError::Unreadable`: the whole read fails, never a row left out and never a guess (rule 7).
//!
//! **The stable read API** (what the reviewer replay's `EscapeSource` reads, slice 4n): [`Store::crew_escapes_of_project`],
//! [`Store::crew_escape_of_merge`], [`Store::crew_escape`] and [`Store::crew_escape_events`], over [`EscapeRecord`] and
//! [`EscapeEventRecord`]. Counts of `possible` and `confirmed` are always separate; a rate uses `confirmed` only.

use std::collections::BTreeMap;

use orbyn_contracts::{EscapeActor, EscapeEventKind, EscapeGap, EscapeSource, EscapeState};
use rusqlite::{OptionalExtension, Row, params};
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::Store;
use crate::error::{CoreError, Result};

/// The most escapes one project keeps (docs/crew/escapes.md, "Data model"). Reads never return more.
pub const ESCAPES_PER_PROJECT_MAX: u32 = 5000;
/// The most audit rows one read returns for one escape.
pub const EVENTS_PER_ESCAPE_MAX: u32 = 100;

/// One escape as stored, with when its merge reached the branch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EscapeRecord {
    pub id: String,
    pub project_id: String,
    /// The merge it is about (`crew_merges.task_id`); one escape per merge.
    pub merge_task_id: String,
    pub merged_unix: i64,
    pub source: EscapeSource,
    pub state: EscapeState,
    pub gap: Option<EscapeGap>,
    pub gap_by: Option<EscapeActor>,
    pub engine: String,
    pub model: Option<String>,
    pub areas: Vec<String>,
    pub lane: Option<String>,
    pub reason: Option<String>,
    pub found_unix: i64,
    pub decided_unix: Option<i64>,
    pub decided_by: Option<EscapeActor>,
}

/// One audit row, immutable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EscapeEventRecord {
    pub id: i64,
    pub escape_id: String,
    pub project_id: String,
    pub kind: EscapeEventKind,
    pub from_state: Option<EscapeState>,
    pub to_state: Option<EscapeState>,
    pub decided_by: EscapeActor,
    pub cause: String,
    pub at: i64,
}

/// One engine's merges and escapes among merges made in a window. The three escape counts are never summed here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EscapeTally {
    pub merges: u32,
    pub possible: u32,
    pub confirmed: u32,
    pub dismissed: u32,
}

/// The one error a damaged or unreadable log gives. It names nothing of the damage.
pub(crate) fn unreadable<E>(_: E) -> CoreError {
    CoreError::Unreadable("The escape log could not be read.".into())
}

fn bad(error: impl std::error::Error + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

fn word<T: DeserializeOwned>(text: String) -> rusqlite::Result<T> {
    serde_json::from_value(Value::String(text)).map_err(bad)
}

fn maybe<T: DeserializeOwned>(row: &Row<'_>, column: &str) -> rusqlite::Result<Option<T>> {
    row.get::<_, Option<String>>(column)?.map(word).transpose()
}

fn areas(text: String) -> rusqlite::Result<Vec<String>> {
    serde_json::from_str(&text).map_err(bad)
}

fn escape_row(row: &Row<'_>) -> rusqlite::Result<EscapeRecord> {
    Ok(EscapeRecord {
        id: row.get("id")?,
        project_id: row.get("project_id")?,
        merge_task_id: row.get("merge_task_id")?,
        merged_unix: row.get("merged_unix")?,
        source: word(row.get("source")?)?,
        state: word(row.get("state")?)?,
        gap: maybe(row, "gap")?,
        gap_by: maybe(row, "gap_by")?,
        engine: row.get("engine")?,
        model: row.get("model")?,
        areas: areas(row.get("areas")?)?,
        lane: row.get("lane")?,
        reason: row.get("reason")?,
        found_unix: row.get("found_unix")?,
        decided_unix: row.get("decided_unix")?,
        decided_by: maybe(row, "decided_by")?,
    })
}

fn event_row(row: &Row<'_>) -> rusqlite::Result<EscapeEventRecord> {
    Ok(EscapeEventRecord {
        id: row.get("id")?,
        escape_id: row.get("escape_id")?,
        project_id: row.get("project_id")?,
        kind: word(row.get("kind")?)?,
        from_state: maybe(row, "from_state")?,
        to_state: maybe(row, "to_state")?,
        decided_by: word(row.get("decided_by")?)?,
        cause: row.get("cause")?,
        at: row.get("at")?,
    })
}

const SELECT: &str = "SELECT e.*, m.merged_unix FROM crew_escapes e JOIN crew_merges m ON m.task_id = e.merge_task_id";

fn state_name(state: EscapeState) -> &'static str {
    match state {
        EscapeState::Possible => "possible",
        EscapeState::Confirmed => "confirmed",
        EscapeState::Dismissed => "dismissed",
    }
}

impl Store {
    /// One page of a project's escapes, newest found first, optionally of one state, and the number matching in all.
    pub fn crew_escape_page(
        &self,
        project_id: &str,
        state: Option<EscapeState>,
        limit: u32,
        offset: u32,
    ) -> Result<(Vec<EscapeRecord>, u32)> {
        let filter = state.map(state_name);
        let mut conn = self.reader();
        let tx = conn.transaction().map_err(unreadable)?;
        let total: u32 = tx
            .query_row(
                "SELECT COUNT(*) FROM crew_escapes WHERE project_id = ?1 AND (?2 IS NULL OR state = ?2)",
                params![project_id, filter],
                |row| row.get(0),
            )
            .map_err(unreadable)?;
        let rows = tx
            .prepare(&format!(
                "{SELECT} WHERE e.project_id = ?1 AND (?2 IS NULL OR e.state = ?2)
                 ORDER BY e.found_unix DESC, e.id DESC LIMIT ?3 OFFSET ?4"
            ))
            .map_err(unreadable)?
            .query_map(params![project_id, filter, limit, offset], escape_row)
            .map_err(unreadable)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(unreadable)?;
        Ok((rows, total))
    }

    /// Every escape of a project whose merge reached the branch at or after `merged_since`, oldest merge first, at most
    /// [`ESCAPES_PER_PROJECT_MAX`]. All of it or an error: one unreadable row fails the whole read. This is the read the reviewer replay's
    /// escape source builds on; count `confirmed` as escapes and keep `possible` apart.
    pub fn crew_escapes_of_project(
        &self,
        project_id: &str,
        merged_since: i64,
    ) -> Result<Vec<EscapeRecord>> {
        let conn = self.reader();
        conn.prepare(&format!(
            "{SELECT} WHERE e.project_id = ?1 AND m.merged_unix >= ?2
             ORDER BY m.merged_unix, e.id LIMIT ?3"
        ))
        .map_err(unreadable)?
        .query_map(
            params![project_id, merged_since, ESCAPES_PER_PROJECT_MAX],
            escape_row,
        )
        .map_err(unreadable)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(unreadable)
    }

    /// The escape of one merge, if the log holds one.
    pub fn crew_escape_of_merge(&self, task_id: &str) -> Result<Option<EscapeRecord>> {
        self.reader()
            .query_row(
                &format!("{SELECT} WHERE e.merge_task_id = ?1"),
                [task_id],
                escape_row,
            )
            .optional()
            .map_err(unreadable)
    }

    /// One escape by id.
    pub fn crew_escape(&self, id: &str) -> Result<Option<EscapeRecord>> {
        self.reader()
            .query_row(&format!("{SELECT} WHERE e.id = ?1"), [id], escape_row)
            .optional()
            .map_err(unreadable)
    }

    /// An escape's audit trail, oldest first, at most `limit` rows.
    pub fn crew_escape_events(
        &self,
        escape_id: &str,
        limit: u32,
    ) -> Result<Vec<EscapeEventRecord>> {
        let conn = self.reader();
        conn.prepare("SELECT * FROM crew_escape_events WHERE escape_id = ?1 ORDER BY id LIMIT ?2")
            .map_err(unreadable)?
            .query_map(params![escape_id, limit], event_row)
            .map_err(unreadable)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(unreadable)
    }

    /// Per engine: the project's merges that reached the branch at or after `merged_since`, and the escapes of those merges by
    /// state. An engine with escapes always has a merge row, since an escape belongs to a merge.
    pub fn crew_escape_tallies(
        &self,
        project_id: &str,
        merged_since: i64,
    ) -> Result<BTreeMap<String, EscapeTally>> {
        let mut conn = self.reader();
        let tx = conn.transaction().map_err(unreadable)?;
        let mut tallies: BTreeMap<String, EscapeTally> = BTreeMap::new();
        let merges = tx
            .prepare(
                "SELECT m.engine, COUNT(*) FROM crew_merges m
                 JOIN crew_runs r ON r.task_id = m.task_id
                 JOIN threads t ON t.id = r.lead_thread_id
                 WHERE t.project_id = ?1 AND m.merged_unix >= ?2 GROUP BY m.engine",
            )
            .map_err(unreadable)?
            .query_map(params![project_id, merged_since], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
            })
            .map_err(unreadable)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(unreadable)?;
        for (engine, count) in merges {
            tallies.entry(engine).or_default().merges = count;
        }
        let escapes = tx
            .prepare(
                "SELECT e.engine, e.state, COUNT(*) FROM crew_escapes e
                 JOIN crew_merges m ON m.task_id = e.merge_task_id
                 WHERE e.project_id = ?1 AND m.merged_unix >= ?2 GROUP BY e.engine, e.state",
            )
            .map_err(unreadable)?
            .query_map(params![project_id, merged_since], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    word::<EscapeState>(row.get(1)?)?,
                    row.get::<_, u32>(2)?,
                ))
            })
            .map_err(unreadable)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(unreadable)?;
        for (engine, state, count) in escapes {
            let tally = tallies.entry(engine).or_default();
            match state {
                EscapeState::Possible => tally.possible = count,
                EscapeState::Confirmed => tally.confirmed = count,
                EscapeState::Dismissed => tally.dismissed = count,
            }
        }
        Ok(tallies)
    }
}

/// Rows for tests of other crates, which cannot reach the writer. Never built into the service.
#[cfg(any(test, feature = "test-support"))]
impl Store {
    /// Adds one escape and its `detected` audit row for a merge that already exists, as PR 2's writer will.
    pub fn insert_escape_for_test(
        &self,
        id: &str,
        project_id: &str,
        merge_task_id: &str,
        source: &str,
        state: &str,
        reason: &str,
    ) -> Result<()> {
        self.commit(|tx, _| {
            let (decided, by) = if state == "possible" {
                (None, None)
            } else {
                (Some(2_i64), Some("person"))
            };
            tx.execute(
                "INSERT INTO crew_escapes (id, project_id, merge_task_id, source, state, engine, areas, reason, found_unix, decided_unix, decided_by)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'claude', '[\"src/app\"]', ?6, 1, ?7, ?8)",
                params![id, project_id, merge_task_id, source, state, reason, decided, by],
            )?;
            tx.execute(
                "INSERT INTO crew_escape_events (escape_id, project_id, kind, from_state, to_state, decided_by, cause, at)
                 VALUES (?1, ?2, 'detected', NULL, 'possible', 'core', 'test', 1)",
                params![id, project_id],
            )?;
            Ok(())
        })
    }

    /// Damages one stored escape so that every read of the log must fail closed, as a damaged table would.
    pub fn damage_escapes_for_test(&self) -> Result<()> {
        let conn = self.writer();
        conn.execute_batch(
            "PRAGMA ignore_check_constraints = ON; UPDATE crew_escapes SET state = 'bogus'; PRAGMA ignore_check_constraints = OFF;",
        )?;
        Ok(())
    }
}
