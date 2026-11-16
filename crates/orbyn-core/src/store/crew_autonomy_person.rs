//! A person's override of one area at rest (docs/crew.md, "As built (slice 4j, enforcement)"). One transaction does it all, in this
//! order (architecture rule 6): the audit row first, then the `crew_area_events` row naming it (`cause` is `person:<audit id>`),
//! then the mode row. A row the table refuses fails the whole write and the area stays as it was. The audit row says `user`: any
//! local process that holds the service token can post the same request, the limit `set_mode` and the other Crew writes have.

use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::json;

use super::crew_autonomy::mode_row;
use super::{AreaMode, AuditRecord, Store, insert_audit};
use crate::error::{CoreError, Result};

/// What the person saw when they chose: the window's counts and when the breaker lowered the area. Compared **inside** the transaction
/// that writes, so a revert that lands in between makes the choice stale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seen {
    pub merges: u32,
    pub reverts: u32,
    pub tripped_at: Option<i64>,
}

/// One choice of a person. `choice` is `keep_ceiling` or `off`; `None` returns the area to following the record.
#[derive(Clone, Debug)]
pub struct PersonWrite<'a> {
    pub project_id: &'a str,
    pub area: &'a str,
    pub choice: Option<&'a str>,
    /// The record the person saw. A different one is `stale`. `None` skips the check (a follow need not carry one).
    pub seen: Option<Seen>,
    /// The start of the window the counts are read over.
    pub window_since: i64,
    pub reason: Option<&'a str>,
    /// The mode the area was on and will be on, for the trail.
    pub from_mode: &'a str,
    pub to_mode: &'a str,
    pub at: i64,
}

fn current(tx: &Transaction<'_>, project: &str, area: &str) -> Result<Option<AreaMode>> {
    Ok(tx
        .query_row(
            "SELECT * FROM crew_area_modes WHERE project_id = ?1 AND area = ?2",
            params![project, area],
            mode_row,
        )
        .optional()?)
}

/// The merges of the project in the window that touched the area, and how many of them are reverted now. The same count the breaker
/// makes from `crew_merges_of_project`, done in the transaction.
fn window_counts(tx: &Transaction<'_>, write: &PersonWrite<'_>) -> Result<(u32, u32)> {
    Ok(tx.query_row(
        "SELECT COUNT(*), COALESCE(SUM(m.review_state = 'reverted'), 0) FROM crew_merges m
         JOIN crew_runs r ON r.task_id = m.task_id
         JOIN threads t ON t.id = r.lead_thread_id
         WHERE t.project_id = ?1 AND m.merged_unix >= ?2
           AND EXISTS (SELECT 1 FROM json_each(m.areas) j WHERE j.value = ?3)",
        params![write.project_id, write.window_since, write.area],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?)
}

impl Store {
    /// Sets or clears a person's override. Returns whether anything changed: the same choice twice records nothing the second time.
    pub fn record_area_override(&self, write: &PersonWrite<'_>) -> Result<bool> {
        self.commit(|tx, _| {
            let row = current(tx, write.project_id, write.area)?;
            let (merges, reverts) = window_counts(tx, write)?;
            if let Some(seen) = write.seen
                && (
                    seen.merges,
                    seen.reverts,
                    seen.tripped_at,
                ) != (merges, reverts, row.as_ref().and_then(|r| r.tripped_at))
            {
                return Err(CoreError::Stale(
                    "This area changed since you looked. Reload it and choose again.".into(),
                ));
            }
            let record = json!({ "merges": merges, "reverts": reverts, "window_days": 14, "escapes": null }).to_string();
            let held = row.as_ref().and_then(|r| r.person_override.as_deref());
            if held == write.choice {
                return Ok(false);
            }
            if write.choice == Some("keep_ceiling")
                && row.as_ref().is_none_or(|r| r.state != "demoted")
            {
                return Err(CoreError::Invalid(
                    "This area is not lowered, so there is nothing to keep.".into(),
                ));
            }
            let kind = if write.choice.is_some() {
                "override_set"
            } else {
                "override_cleared"
            };
            let audit = insert_audit(
                tx,
                &AuditRecord {
                    actor: "user".to_owned(),
                    action: if write.choice.is_some() {
                        "crew.area.override".to_owned()
                    } else {
                        "crew.area.follow".to_owned()
                    },
                    target: format!("{}:{}", write.project_id, write.area),
                    decision: None,
                    source: "user".to_owned(),
                    thread_id: None,
                    run_id: None,
                    detail: json!({
                        "project": write.project_id,
                        "area": write.area,
                        "choice": write.choice,
                        "was": held,
                        "reason": write.reason,
                    }),
                },
            )?;
            let cause = format!("person:{audit}");
            tx.execute(
                "INSERT INTO crew_area_events (project_id, area, kind, from_mode, to_mode, decided_by, cause, record, at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'person', ?6, ?7, ?8)",
                params![write.project_id, write.area, kind, write.from_mode, write.to_mode, cause, record, write.at],
            )?;
            tx.execute(
                "INSERT INTO crew_area_modes (project_id, area, state, override, override_at, updated_at)
                 VALUES (?1, ?2, 'follows', ?3, ?4, ?5)
                 ON CONFLICT(project_id, area) DO UPDATE SET override = ?3, override_at = ?4, updated_at = ?5",
                params![
                    write.project_id,
                    write.area,
                    write.choice,
                    write.choice.map(|_| write.at),
                    write.at
                ],
            )?;
            Ok(true)
        })
    }
}
