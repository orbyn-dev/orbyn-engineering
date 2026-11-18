//! A person's acts on the escape log at rest (slice 4k, PR 2; docs/crew/escapes.md): mark a merge, confirm, dismiss, set the gap.
//! Each is one immediate transaction, in this order (architecture rule 6): the ledger's audit row first, then the state change, then
//! the `crew_escape_events` row that names the audit row (`cause` is `person:<audit id>`), so a failure at any step leaves the escape
//! as it was.
//!
//! - **Transitions.** `possible` becomes `confirmed` or `dismissed`; those two are final in v1. The same act on an escape already in
//!   that state returns it unchanged and writes nothing (idempotent, and a retry after a lost answer is not an error). The opposite
//!   act on a decided escape is refused in plain words. Only a person's write ever sets `confirmed`; nothing here is called by the
//!   core's own detection, the breaker or the router.
//! - **Race safety.** The state is read and compared to what the person saw (`seen`) inside the immediate transaction, and the
//!   `UPDATE` repeats the old state in its `WHERE`; a change that landed in between is `stale`, never a double write.
//! - **Isolation.** Every act names a project; an escape or merge of another project is `not found`.
//! - **Bounds.** A mark that would pass 5,000 escapes, or any act that would pass 20,000 events, for the project is refused: pruning is
//!   not built yet, and a silent drop of a `confirmed` row is never allowed.

use orbyn_contracts::{
    EscapeActor, EscapeEventKind, EscapeGap, EscapeSeen, EscapeSource, EscapeState,
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::json;

use super::Store;
use super::crew_escapes::{ESCAPES_PER_PROJECT_MAX, EscapeRecord, SELECT, escape_row, unreadable};
use super::{AuditRecord, insert_audit};
use crate::error::{CoreError, Result};

/// The most audit rows one project's escapes keep (docs/crew/escapes.md, "Data model").
pub const EVENTS_PER_PROJECT_MAX: u32 = 20_000;

/// What a person decides about an escape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EscapeDecision {
    Confirm,
    Dismiss,
}

impl EscapeDecision {
    fn to(self) -> EscapeState {
        match self {
            Self::Confirm => EscapeState::Confirmed,
            Self::Dismiss => EscapeState::Dismissed,
        }
    }
    fn word(self) -> &'static str {
        match self {
            Self::Confirm => "confirm",
            Self::Dismiss => "dismiss",
        }
    }
}

pub(super) fn state_word(state: EscapeState) -> &'static str {
    match state {
        EscapeState::Possible => "possible",
        EscapeState::Confirmed => "confirmed",
        EscapeState::Dismissed => "dismissed",
    }
}

pub(super) fn load(tx: &Transaction<'_>, project: &str, id: &str) -> Result<EscapeRecord> {
    tx.query_row(&format!("{SELECT} WHERE e.id = ?1"), [id], escape_row)
        .optional()
        .map_err(unreadable)?
        .filter(|record| record.project_id == project)
        .ok_or_else(|| CoreError::NotFound("escape".into()))
}

pub(super) fn stale(record: &EscapeRecord, seen: &EscapeSeen) -> Result<()> {
    if (record.state, record.gap) == (seen.state, seen.gap) {
        Ok(())
    } else {
        Err(CoreError::Stale(
            "This escape changed since you looked. Reload it and choose again.".into(),
        ))
    }
}

pub(super) fn room(tx: &Transaction<'_>, project: &str, escapes: u32, events: u32) -> Result<()> {
    let count = |sql: &str| -> Result<u32> {
        tx.query_row(sql, [project], |row| row.get(0))
            .map_err(unreadable)
    };
    let held_events = count("SELECT COUNT(*) FROM crew_escape_events WHERE project_id = ?1")?;
    let held_escapes = if escapes > 0 {
        count("SELECT COUNT(*) FROM crew_escapes WHERE project_id = ?1")?
    } else {
        0
    };
    if held_events + events > EVENTS_PER_PROJECT_MAX
        || held_escapes + escapes > ESCAPES_PER_PROJECT_MAX
    {
        return Err(CoreError::Invalid(
            "This project's escape log is full. Nothing more can be added until old entries are cleared.".into(),
        ));
    }
    Ok(())
}

pub(super) fn audit(
    tx: &Transaction<'_>,
    action: &str,
    project: &str,
    target: &str,
    detail: serde_json::Value,
) -> Result<i64> {
    insert_audit(
        tx,
        &AuditRecord {
            actor: "user".to_owned(),
            action: action.to_owned(),
            target: format!("{project}:{target}"),
            decision: None,
            source: "user".to_owned(),
            thread_id: None,
            run_id: None,
            detail,
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn event(
    tx: &Transaction<'_>,
    escape_id: &str,
    project: &str,
    kind: EscapeEventKind,
    from: Option<EscapeState>,
    to: Option<EscapeState>,
    cause: &str,
    at: i64,
) -> Result<()> {
    let word = serde_json::to_value(kind)?;
    tx.execute(
        "INSERT INTO crew_escape_events (escape_id, project_id, kind, from_state, to_state, decided_by, cause, at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'person', ?6, ?7)",
        params![escape_id, project, word.as_str(), from.map(state_word), to.map(state_word), cause, at],
    )?;
    Ok(())
}

/// Moves a possible escape to `to` as a person, with its audit row and event. The caller has checked the transition.
fn decide(
    tx: &Transaction<'_>,
    record: &EscapeRecord,
    to: EscapeState,
    action: &str,
    note: Option<&str>,
    at: i64,
) -> Result<()> {
    room(tx, &record.project_id, 0, 1)?;
    let audit_id = audit(
        tx,
        action,
        &record.project_id,
        &record.id,
        json!({ "project": record.project_id, "escape": record.id, "merge": record.merge_task_id,
                "from": state_word(record.state), "to": state_word(to), "note": note }),
    )?;
    let changed = tx.execute(
        "UPDATE crew_escapes SET state = ?1, decided_unix = ?2, decided_by = 'person'
         WHERE id = ?3 AND project_id = ?4 AND state = 'possible'",
        params![state_word(to), at, record.id, record.project_id],
    )?;
    if changed != 1 {
        return Err(CoreError::Stale(
            "This escape changed since you looked. Reload it and choose again.".into(),
        ));
    }
    let kind = if to == EscapeState::Confirmed {
        EscapeEventKind::Confirmed
    } else {
        EscapeEventKind::Dismissed
    };
    event(
        tx,
        &record.id,
        &record.project_id,
        kind,
        Some(record.state),
        Some(to),
        &format!("person:{audit_id}"),
        at,
    )
}

impl Store {
    /// A person confirms or dismisses one escape of `project`. Returns the escape as it now stands.
    pub fn decide_escape(
        &self,
        project: &str,
        id: &str,
        decision: EscapeDecision,
        seen: &EscapeSeen,
        note: Option<&str>,
        at: i64,
    ) -> Result<EscapeRecord> {
        self.commit(|tx, _| {
            let record = load(tx, project, id)?;
            let to = decision.to();
            if record.state == to {
                return Ok(record);
            }
            stale(&record, seen)?;
            if record.state != EscapeState::Possible {
                return Err(CoreError::Invalid(format!(
                    "This escape is already {}, so it cannot be {} now.",
                    state_word(record.state),
                    match decision {
                        EscapeDecision::Confirm => "confirmed",
                        EscapeDecision::Dismiss => "dismissed",
                    }
                )));
            }
            decide(
                tx,
                &record,
                to,
                &format!("crew.escape.{}", decision.word()),
                note,
                at,
            )?;
            load(tx, project, id)
        })
    }

    /// A person marks a merge of `project` as an escape; the mark is the confirm. A merge with no escape gets one, `person_marked`
    /// and confirmed at once; a possible one is confirmed; a confirmed one is returned as it is; a dismissed one is refused.
    pub fn mark_escape(
        &self,
        project: &str,
        merge_task_id: &str,
        seen: Option<&EscapeSeen>,
        note: Option<&str>,
        at: i64,
    ) -> Result<EscapeRecord> {
        self.commit(|tx, _| {
            let merge: Option<String> = tx
                .query_row(
                    "SELECT t.project_id FROM crew_merges m
                     JOIN crew_runs r ON r.task_id = m.task_id
                     JOIN threads t ON t.id = r.lead_thread_id WHERE m.task_id = ?1",
                    [merge_task_id],
                    |row| row.get(0),
                )
                .optional()?;
            if merge.as_deref() != Some(project) {
                return Err(CoreError::NotFound("merge".into()));
            }
            let held = tx
                .query_row(
                    &format!("{SELECT} WHERE e.merge_task_id = ?1"),
                    [merge_task_id],
                    escape_row,
                )
                .optional()
                .map_err(unreadable)?;
            if let Some(record) = held {
                if record.state == EscapeState::Confirmed {
                    return Ok(record);
                }
                if let Some(seen) = seen {
                    stale(&record, seen)?;
                }
                if record.state == EscapeState::Dismissed {
                    return Err(CoreError::Invalid(
                        "This escape was dismissed, so the merge cannot be marked now.".into(),
                    ));
                }
                decide(tx, &record, EscapeState::Confirmed, "crew.escape.mark", note, at)?;
                return load(tx, project, &record.id);
            }
            if seen.is_some() {
                return Err(CoreError::Stale(
                    "This escape changed since you looked. Reload it and choose again.".into(),
                ));
            }
            room(tx, project, 1, 2)?;
            let audit_id = audit(
                tx,
                "crew.escape.mark",
                project,
                merge_task_id,
                json!({ "project": project, "merge": merge_task_id, "from": null, "to": "confirmed", "note": note }),
            )?;
            let reason = note.unwrap_or("A person marked this merge as an escape.");
            let source = serde_json::to_value(EscapeSource::PersonMarked)?;
            tx.execute(
                "INSERT INTO crew_escapes (id, project_id, merge_task_id, source, state, engine, areas, reason, found_unix, decided_unix, decided_by)
                 SELECT 'esc_' || lower(hex(randomblob(12))), ?1, m.task_id, ?2, 'confirmed', m.engine, m.areas, ?3, ?4, ?4, 'person'
                 FROM crew_merges m WHERE m.task_id = ?5",
                params![project, source.as_str(), reason, at, merge_task_id],
            )?;
            let record = tx
                .query_row(
                    &format!("{SELECT} WHERE e.merge_task_id = ?1"),
                    [merge_task_id],
                    escape_row,
                )
                .map_err(unreadable)?;
            let cause = format!("person:{audit_id}");
            event(tx, &record.id, project, EscapeEventKind::Detected, None, Some(EscapeState::Possible), &cause, at)?;
            event(
                tx,
                &record.id,
                project,
                EscapeEventKind::Confirmed,
                Some(EscapeState::Possible),
                Some(EscapeState::Confirmed),
                &cause,
                at,
            )?;
            Ok(record)
        })
    }

    /// A person sets the gap of a possible or confirmed escape of `project`. The same gap again changes nothing.
    pub fn set_escape_gap(
        &self,
        project: &str,
        id: &str,
        seen: &EscapeSeen,
        gap: EscapeGap,
        at: i64,
    ) -> Result<EscapeRecord> {
        self.commit(|tx, _| {
            let record = load(tx, project, id)?;
            if record.gap == Some(gap) && record.gap_by == Some(EscapeActor::Person) {
                return Ok(record);
            }
            stale(&record, seen)?;
            if record.state == EscapeState::Dismissed {
                return Err(CoreError::Invalid(
                    "This escape was dismissed, so its gap cannot be set.".into(),
                ));
            }
            room(tx, project, 0, 1)?;
            let word = serde_json::to_value(gap)?;
            let was: Option<String> = record
                .gap
                .and_then(|g| serde_json::to_value(g).ok())
                .and_then(|v| v.as_str().map(str::to_owned));
            let audit_id = audit(
                tx,
                "crew.escape.gap",
                project,
                id,
                json!({ "project": project, "escape": id, "gap": word, "was": was }),
            )?;
            let changed = tx.execute(
                "UPDATE crew_escapes SET gap = ?1, gap_by = 'person'
                 WHERE id = ?2 AND project_id = ?3 AND state = ?4 AND gap IS ?5",
                params![word.as_str(), id, project, state_word(record.state), was],
            )?;
            if changed != 1 {
                return Err(CoreError::Stale(
                    "This escape changed since you looked. Reload it and choose again.".into(),
                ));
            }
            event(
                tx,
                id,
                project,
                EscapeEventKind::Reclassified,
                Some(record.state),
                Some(record.state),
                &format!("person:{audit_id}"),
                at,
            )?;
            load(tx, project, id)
        })
    }
}
