//! Autonomy by area at rest (docs/crew.md, "As built (slice 4j, shadow)"): where the circuit breaker has put an area and the audit
//! trail of why. The record the breaker reads (merges and reverts) is never stored here: it is counted from `crew_merges`. A
//! breaker write adds its `crew_area_events` row and changes `crew_area_modes` in one transaction, event first, so a row that
//! cannot be written leaves the mode as it was. Events are never changed or removed. The merge gate reads a mode through
//! `crew::autopilot::areas`; a person's override is written by `crew_autonomy_person.rs`.

use rusqlite::{OptionalExtension, Row, params};
use serde_json::Value;

use super::Store;
use crate::error::Result;

/// The most areas one project lists.
const AREAS_MAX: u32 = 5000;

/// Where one area stands.
#[derive(Clone, Debug, PartialEq)]
pub struct AreaMode {
    pub project_id: String,
    pub area: String,
    /// `follows` its ceiling or `demoted`.
    pub state: String,
    pub tripped_at: Option<i64>,
    pub trip_reason: Option<Value>,
    /// When the recovery clock started: the demotion, or the latest revert since.
    pub recovery_since: Option<i64>,
    /// A person's override: `keep_ceiling` or `off`.
    pub person_override: Option<String>,
    pub override_at: Option<i64>,
    pub updated_at: i64,
}

/// One row of the audit trail.
#[derive(Clone, Debug, PartialEq)]
pub struct AreaEvent {
    pub id: i64,
    pub project_id: String,
    pub area: String,
    pub kind: String,
    pub from_mode: Option<String>,
    pub to_mode: Option<String>,
    pub decided_by: String,
    pub cause: String,
    pub record: Value,
    pub at: i64,
}

/// What a breaker write does to the area's mode row, after its event is stored.
#[derive(Clone, Debug, PartialEq)]
pub enum AreaChange {
    /// The area is lowered; the recovery clock starts at `recovery_since`.
    Demote {
        trip_reason: Value,
        recovery_since: i64,
    },
    /// A revert while lowered: the recovery clock restarts.
    ResetRecovery { since: i64 },
    /// The area follows its ceiling again. A `keep_ceiling` override ends with the lowering it ignored; `off` stays.
    Restore,
    /// A new trip while a person's `keep_ceiling` override stands: the override ends and the recovery clock restarts.
    ClearKeepCeiling { since: i64 },
}

/// One breaker write: the event and the change it makes.
#[derive(Clone, Debug)]
pub struct AreaWrite<'a> {
    pub project_id: &'a str,
    pub area: &'a str,
    pub kind: &'a str,
    pub from_mode: Option<&'a str>,
    pub to_mode: Option<&'a str>,
    pub decided_by: &'a str,
    pub cause: &'a str,
    pub record: &'a Value,
    pub at: i64,
    pub change: AreaChange,
}

fn json(text: String) -> rusqlite::Result<Value> {
    serde_json::from_str(&text).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}

pub(super) fn mode_row(row: &Row<'_>) -> rusqlite::Result<AreaMode> {
    Ok(AreaMode {
        project_id: row.get("project_id")?,
        area: row.get("area")?,
        state: row.get("state")?,
        tripped_at: row.get("tripped_at")?,
        trip_reason: row
            .get::<_, Option<String>>("trip_reason")?
            .map(json)
            .transpose()?,
        recovery_since: row.get("recovery_since")?,
        person_override: row.get("override")?,
        override_at: row.get("override_at")?,
        updated_at: row.get("updated_at")?,
    })
}

pub(super) fn event_row(row: &Row<'_>) -> rusqlite::Result<AreaEvent> {
    Ok(AreaEvent {
        id: row.get("id")?,
        project_id: row.get("project_id")?,
        area: row.get("area")?,
        kind: row.get("kind")?,
        from_mode: row.get("from_mode")?,
        to_mode: row.get("to_mode")?,
        decided_by: row.get("decided_by")?,
        cause: row.get("cause")?,
        record: json(row.get("record")?)?,
        at: row.get("at")?,
    })
}

impl Store {
    pub fn area_mode(&self, project_id: &str, area: &str) -> Result<Option<AreaMode>> {
        Ok(self
            .reader()
            .query_row(
                "SELECT * FROM crew_area_modes WHERE project_id = ?1 AND area = ?2",
                params![project_id, area],
                mode_row,
            )
            .optional()?)
    }

    /// Makes every read of the area modes fail, as a damaged table would. Tests only.
    #[cfg(any(test, feature = "test-support"))]
    pub fn break_area_modes_for_test(&self) {
        self.writer()
            .execute_batch("ALTER TABLE crew_area_modes RENAME TO crew_area_modes_broken")
            .unwrap();
    }

    /// Undoes [`Self::break_area_modes_for_test`]. Tests only.
    #[cfg(any(test, feature = "test-support"))]
    pub fn fix_area_modes_for_test(&self) {
        self.writer()
            .execute_batch("ALTER TABLE crew_area_modes_broken RENAME TO crew_area_modes")
            .unwrap();
    }

    /// Every area of a project that has a row, by name.
    pub fn area_modes(&self, project_id: &str) -> Result<Vec<AreaMode>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT * FROM crew_area_modes WHERE project_id = ?1 ORDER BY area LIMIT ?2",
        )?;
        Ok(statement
            .query_map(params![project_id, AREAS_MAX], mode_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Every area's events at or after `since`, newest first, at most `limit` rows, for the digest.
    pub fn area_events_since(&self, since: i64, limit: u32) -> Result<Vec<AreaEvent>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT * FROM crew_area_events WHERE at >= ?1 AND kind != 'recovery_reset' ORDER BY id DESC LIMIT ?2",
        )?;
        Ok(statement
            .query_map(params![since, limit], event_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// The trail of one area, newest first, at most `limit` rows.
    pub fn area_events(&self, project_id: &str, area: &str, limit: u32) -> Result<Vec<AreaEvent>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT * FROM crew_area_events WHERE project_id = ?1 AND area = ?2 ORDER BY id DESC LIMIT ?3",
        )?;
        Ok(statement
            .query_map(params![project_id, area, limit], event_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Adds the event, then changes the area's mode row, in one transaction. A write that was already recorded changes nothing and
    /// returns `false`: the same cause is recorded once per kind, and a revert that has tripped or reset the breaker never does
    /// either again. An event the table refuses fails the whole write, and the mode row stays as it was.
    pub fn record_area_change(&self, write: &AreaWrite<'_>) -> Result<bool> {
        let record = serde_json::to_string(write.record)?;
        let trip_reason = match &write.change {
            AreaChange::Demote { trip_reason, .. } => Some(serde_json::to_string(trip_reason)?),
            _ => None,
        };
        self.commit(|tx, _| {
            let revert_seen: bool = tx.query_row(
                "SELECT COUNT(*) FROM crew_area_events WHERE project_id = ?1 AND area = ?2 AND cause = ?3
                    AND kind IN ('demoted', 'recovery_reset')",
                params![write.project_id, write.area, write.cause],
                |row| row.get::<_, i64>(0).map(|n| n > 0),
            )?;
            if revert_seen && matches!(write.kind, "demoted" | "recovery_reset") {
                return Ok(false);
            }
            let added = tx.execute(
                "INSERT INTO crew_area_events (project_id, area, kind, from_mode, to_mode, decided_by, cause, record, at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(project_id, area, kind, cause) DO NOTHING",
                params![write.project_id, write.area, write.kind, write.from_mode, write.to_mode, write.decided_by, write.cause, record, write.at],
            )?;
            if added == 0 {
                return Ok(false);
            }
            match &write.change {
                AreaChange::Demote { recovery_since, .. } => {
                    tx.execute(
                        "INSERT INTO crew_area_modes (project_id, area, state, tripped_at, trip_reason, recovery_since, updated_at)
                         VALUES (?1, ?2, 'demoted', ?3, ?4, ?5, ?3)
                         ON CONFLICT(project_id, area) DO UPDATE SET state = 'demoted', tripped_at = ?3, trip_reason = ?4,
                             recovery_since = ?5, updated_at = ?3",
                        params![write.project_id, write.area, write.at, trip_reason, recovery_since],
                    )?;
                }
                AreaChange::ResetRecovery { since } => {
                    tx.execute(
                        "UPDATE crew_area_modes SET recovery_since = ?3, updated_at = ?4
                         WHERE project_id = ?1 AND area = ?2 AND state = 'demoted'",
                        params![write.project_id, write.area, since, write.at],
                    )?;
                }
                AreaChange::Restore => {
                    tx.execute(
                        "UPDATE crew_area_modes SET state = 'follows', tripped_at = NULL, trip_reason = NULL, recovery_since = NULL,
                             override = CASE WHEN override = 'keep_ceiling' THEN NULL ELSE override END,
                             override_at = CASE WHEN override = 'keep_ceiling' THEN NULL ELSE override_at END,
                             updated_at = ?3 WHERE project_id = ?1 AND area = ?2",
                        params![write.project_id, write.area, write.at],
                    )?;
                }
                AreaChange::ClearKeepCeiling { since } => {
                    tx.execute(
                        "UPDATE crew_area_modes SET override = NULL, override_at = NULL, recovery_since = ?3, updated_at = ?4
                         WHERE project_id = ?1 AND area = ?2 AND override = 'keep_ceiling'",
                        params![write.project_id, write.area, since, write.at],
                    )?;
                }
            }
            Ok(true)
        })
    }
}
