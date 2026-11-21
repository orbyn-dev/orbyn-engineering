//! The created check of an escape at rest (slice 4k, PR 4; docs/crew/escapes.md, "Create a check"; migration 52). One row per escape in
//! `crew_escape_checks`: the proposal a person made from a confirmed escape, the crew task of the worker that writes it, its state and
//! its proof (two commit ids and two booleans; never a test body or command output).
//!
//! - **A person's acts** (`propose`, `accept`, `retry`, `withdraw`) are each one immediate transaction in this order (architecture rule 6):
//!   the ledger's audit row, then the state change, then the `crew_escape_events` row where the design lists a kind for it
//!   (`check_proposed`, `check_withdrawn`). Each compares the escape's state and gap and the check's state with what the person saw
//!   (`seen`) inside the transaction, and the `UPDATE` repeats the old state in its `WHERE`, so two acts racing leave one change. The same
//!   act on a check already in that state returns it unchanged and writes nothing. The opposite act is refused in plain words.
//! - **Only `accept` starts a worker**, and only after it has written `writing` and the task's id here. The other three never reach the
//!   engines.
//! - **The core's own steps** (`finish`, `merge`, `note`, `unwind`) record what the proof and the merge queue found. They write the
//!   `check_failed`, `check_proven` and `check_merged` events (cause `core:<task id>:<round>`), never a confirmation of anything.
//! - **Every event has a cause of its own attempt** (a person's own acts name their audit row, the core's name the task and the round
//!   of proposing it is in), and an event insert that meets the same (escape, kind, cause) is `ON CONFLICT DO NOTHING`: already recorded, never a store error.
//! - **Isolation.** Every act names a project; an escape of another project is `not found`.
//! - **Bounds.** A person's act that would pass 20,000 events for the project is refused, as in PR 2.
//!
//! An unreadable row fails the whole read (`CoreError::Unreadable`), never a guess (rule 7).

use orbyn_contracts::{
    EscapeCheckProof, EscapeCheckSeen, EscapeCheckState, EscapeEventKind, EscapeGap, EscapeState,
};
use rusqlite::{OptionalExtension, Row, Transaction, params};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::Store;
use super::crew_escape_person::{audit, load, state_word};
use super::crew_escape_retention::room;
use super::crew_escapes::{EscapeRecord, unreadable};
use crate::error::{CoreError, Result};

/// The most open checks one reconcile reads.
pub const OPEN_CHECKS_MAX: u32 = 200;

/// One created check as stored.
#[derive(Clone, Debug, PartialEq)]
pub struct EscapeCheckRecord {
    pub escape_id: String,
    pub project_id: String,
    pub path: String,
    pub asserts: String,
    pub run: String,
    pub fix_commit: String,
    pub task_id: Option<String>,
    pub state: EscapeCheckState,
    pub proof: Option<EscapeCheckProof>,
    pub note: Option<String>,
    pub attempts: u32,
    /// The worker's change as the core read it (commit, hash over paths and bytes, paths), once the task passed.
    pub diff_commit: Option<String>,
    pub diff_hash: Option<String>,
    pub diff_paths: Vec<String>,
    /// What a person approved: the commit and hash of the change. The proof on a machine that cannot restrict it runs only a change whose
    /// commit and hash equal these.
    pub approved_commit: Option<String>,
    pub approved_hash: Option<String>,
    pub proposed_unix: i64,
    pub updated_unix: i64,
}

/// What a person proposed. The core has already checked the path, the texts and the commit.
#[derive(Clone, Debug)]
pub struct CheckProposal<'a> {
    pub path: &'a str,
    pub asserts: &'a str,
    pub run: &'a str,
    /// A full 40-character commit id.
    pub fix_commit: &'a str,
}

/// How the core records the end of a worker's attempt.
#[derive(Clone, Debug)]
pub enum CheckFinish {
    /// The proof held: the check fails on the escaped merge and passes on the fix or revert.
    Proven(EscapeCheckProof),
    /// The attempt did not give a proven check. `proof` is present when the proof ran, `note` says why in one plain sentence.
    Failed {
        proof: Option<EscapeCheckProof>,
        note: String,
    },
}

fn bad(error: impl std::error::Error + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

fn word<T: DeserializeOwned>(text: String) -> rusqlite::Result<T> {
    serde_json::from_value(Value::String(text)).map_err(bad)
}

pub(crate) fn check_word(state: EscapeCheckState) -> &'static str {
    match state {
        EscapeCheckState::Proposed => "proposed",
        EscapeCheckState::Writing => "writing",
        EscapeCheckState::Proven => "proven",
        EscapeCheckState::Failed => "failed",
        EscapeCheckState::Withdrawn => "withdrawn",
        EscapeCheckState::Merged => "merged",
    }
}

fn check_row(row: &Row<'_>) -> rusqlite::Result<EscapeCheckRecord> {
    let proof: Option<String> = row.get("proof")?;
    Ok(EscapeCheckRecord {
        escape_id: row.get("escape_id")?,
        project_id: row.get("project_id")?,
        path: row.get("path")?,
        asserts: row.get("asserts")?,
        run: row.get("run")?,
        fix_commit: row.get("fix_commit")?,
        task_id: row.get("task_id")?,
        state: word(row.get("state")?)?,
        proof: proof
            .map(|text| serde_json::from_str(&text).map_err(bad))
            .transpose()?,
        note: row.get("note")?,
        attempts: row.get("attempts")?,
        diff_commit: row.get("diff_commit")?,
        diff_hash: row.get("diff_hash")?,
        diff_paths: row
            .get::<_, Option<String>>("diff_paths")?
            .map(|text| serde_json::from_str(&text).map_err(bad))
            .transpose()?
            .unwrap_or_default(),
        approved_commit: row.get("approved_commit")?,
        approved_hash: row.get("approved_hash")?,
        proposed_unix: row.get("proposed_unix")?,
        updated_unix: row.get("updated_unix")?,
    })
}

fn read(tx: &Transaction<'_>, escape_id: &str) -> Result<Option<EscapeCheckRecord>> {
    tx.query_row(
        "SELECT * FROM crew_escape_checks WHERE escape_id = ?1",
        [escape_id],
        check_row,
    )
    .optional()
    .map_err(unreadable)
}

fn stale(
    escape: &EscapeRecord,
    check: Option<&EscapeCheckRecord>,
    seen: &EscapeCheckSeen,
) -> Result<()> {
    if (escape.state, escape.gap, check.map(|c| c.state)) == (seen.state, seen.gap, seen.check) {
        Ok(())
    } else {
        Err(CoreError::Stale(
            "This check changed since you looked. Reload it and choose again.".into(),
        ))
    }
}

/// An event written by a person's act (`decided_by` person) or by the core (`core`).
#[allow(clippy::too_many_arguments)]
fn event(
    tx: &Transaction<'_>,
    escape: &EscapeRecord,
    kind: EscapeEventKind,
    by: &str,
    cause: &str,
    at: i64,
) -> Result<()> {
    let word = serde_json::to_value(kind)?;
    let state = state_word(escape.state);
    // ON CONFLICT DO NOTHING on the unique key only: the same event again is "already recorded", never an error to the person. Any other
    // refusal (NOT NULL, a CHECK) still errors, so no event is dropped silently. Each cause names its own round, so a different
    // attempt is never mistaken for a repeat.
    tx.execute(
        "INSERT INTO crew_escape_events (escape_id, project_id, kind, from_state, to_state, decided_by, cause, at)
         VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6, ?7)
         ON CONFLICT(escape_id, kind, cause) DO NOTHING",
        params![escape.id, escape.project_id, word.as_str(), state, by, cause, at],
    )?;
    Ok(())
}

/// Which round of proposing the escape's check is in: how many `check_proposed` events its trail holds. The trail is append-only and
/// every proposal, new or retried, adds one, so the number only grows, survives a withdraw and a new proposal (which replace the check
/// row and its `attempts`) and is read inside the writing transaction. A cause that names it is its own round's.
fn round(tx: &Transaction<'_>, escape_id: &str) -> Result<i64> {
    Ok(tx.query_row(
        "SELECT COUNT(*) FROM crew_escape_events WHERE escape_id = ?1 AND kind = 'check_proposed'",
        [escape_id],
        |row| row.get(0),
    )?)
}

fn proof_json(proof: Option<&EscapeCheckProof>) -> Result<Option<String>> {
    Ok(proof.map(serde_json::to_string).transpose()?)
}

fn changed(count: usize) -> Result<()> {
    if count == 1 {
        Ok(())
    } else {
        Err(CoreError::Stale(
            "This check changed since you looked. Reload it and choose again.".into(),
        ))
    }
}

fn refuse_state(check: &EscapeCheckRecord, cannot: &str) -> CoreError {
    CoreError::Invalid(format!(
        "This check is {}, so it cannot be {cannot} now.",
        check_word(check.state)
    ))
}

impl Store {
    /// The check of one escape, if it has one.
    pub fn crew_escape_check(&self, escape_id: &str) -> Result<Option<EscapeCheckRecord>> {
        self.reader()
            .query_row(
                "SELECT * FROM crew_escape_checks WHERE escape_id = ?1",
                [escape_id],
                check_row,
            )
            .optional()
            .map_err(unreadable)
    }

    /// The round the escape's check is in (see `round`): the brief of an accept names it, so a retry or a new proposal accepted inside
    /// the same clock second is a task of its own, never the earlier attempt's task id again.
    pub fn crew_escape_check_round(&self, escape_id: &str) -> Result<i64> {
        self.reader()
            .query_row(
                "SELECT COUNT(*) FROM crew_escape_events WHERE escape_id = ?1 AND kind = 'check_proposed'",
                [escape_id],
                |row| row.get(0),
            )
            .map_err(unreadable)
    }

    /// The check a crew task writes, if the task is one.
    pub fn crew_escape_check_for_task(&self, task_id: &str) -> Result<Option<EscapeCheckRecord>> {
        self.reader()
            .query_row(
                "SELECT * FROM crew_escape_checks WHERE task_id = ?1",
                [task_id],
                check_row,
            )
            .optional()
            .map_err(unreadable)
    }

    /// What the merge queue needs to judge a task: whether any accept ever created it for a check (`crew_escape_check_tasks`, never
    /// deleted), and the escape's check as it is now. The check is found through the recorded escape, not through its current task, so a
    /// retry or a new proposal cannot hide an old task.
    pub fn crew_escape_check_task_status(
        &self,
        task_id: &str,
    ) -> Result<(bool, Option<EscapeCheckRecord>)> {
        let conn = self.reader();
        let escape: Option<String> = conn
            .query_row(
                "SELECT escape_id FROM crew_escape_check_tasks WHERE task_id = ?1",
                [task_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(unreadable)?;
        let current = match &escape {
            Some(escape) => conn
                .query_row(
                    "SELECT * FROM crew_escape_checks WHERE escape_id = ?1",
                    [escape],
                    check_row,
                )
                .optional()
                .map_err(unreadable)?,
            None => conn
                .query_row(
                    "SELECT * FROM crew_escape_checks WHERE task_id = ?1",
                    [task_id],
                    check_row,
                )
                .optional()
                .map_err(unreadable)?,
        };
        Ok((escape.is_some() || current.is_some(), current))
    }

    /// Checks in these states, oldest first, at most [`OPEN_CHECKS_MAX`].
    pub fn crew_escape_checks_in(
        &self,
        states: &[EscapeCheckState],
    ) -> Result<Vec<EscapeCheckRecord>> {
        let words: Vec<&str> = states.iter().map(|s| check_word(*s)).collect();
        let marks = vec!["?"; words.len()].join(",");
        let conn = self.reader();
        let mut query = conn
            .prepare(&format!(
                "SELECT * FROM crew_escape_checks WHERE state IN ({marks}) ORDER BY proposed_unix, escape_id LIMIT {OPEN_CHECKS_MAX}"
            ))
            .map_err(unreadable)?;
        query
            .query_map(rusqlite::params_from_iter(words), check_row)
            .map_err(unreadable)?
            .collect::<rusqlite::Result<_>>()
            .map_err(unreadable)
    }

    /// A person proposes a check for a confirmed escape of `project`. Nothing runs and no task exists. A tooling gap has no proposal.
    /// The same proposal again returns the check as it is; a withdrawn check may be proposed again.
    pub fn propose_escape_check(
        &self,
        project: &str,
        id: &str,
        seen: &EscapeCheckSeen,
        proposal: &CheckProposal<'_>,
        at: i64,
    ) -> Result<(EscapeRecord, EscapeCheckRecord)> {
        self.commit(|tx, _| {
            let escape = load(tx, project, id)?;
            let held = read(tx, id)?;
            if let Some(check) = &held
                && check.state == EscapeCheckState::Proposed
                && (&check.path[..], &check.asserts[..], &check.run[..], &check.fix_commit[..])
                    == (proposal.path, proposal.asserts, proposal.run, proposal.fix_commit)
            {
                return Ok((escape, check.clone()));
            }
            stale(&escape, held.as_ref(), seen)?;
            if escape.state != EscapeState::Confirmed {
                return Err(CoreError::Invalid(
                    "Only a confirmed escape can have a check proposed.".into(),
                ));
            }
            if escape.gap == Some(EscapeGap::ToolingGap) {
                return Err(CoreError::Invalid(
                    "A tooling gap has no proposal in this version. Change the gap to a test gap to propose a check.".into(),
                ));
            }
            if let Some(check) = &held
                && check.state != EscapeCheckState::Withdrawn
            {
                return Err(CoreError::Invalid(format!(
                    "This escape already has a check ({}).",
                    check_word(check.state)
                )));
            }
            room(tx, project, 0, 1, Some(id), at)?;
            let audit_id = audit(
                tx,
                "crew.escape.check.propose",
                project,
                id,
                json!({ "project": project, "escape": id, "path": proposal.path, "run": proposal.run, "fix_commit": proposal.fix_commit }),
            )?;
            tx.execute("DELETE FROM crew_escape_checks WHERE escape_id = ?1", [id])?;
            tx.execute(
                "INSERT INTO crew_escape_checks (escape_id, project_id, path, asserts, run, fix_commit, state, proposed_unix, updated_unix)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'proposed', ?7, ?7)",
                params![id, project, proposal.path, proposal.asserts, proposal.run, proposal.fix_commit, at],
            )?;
            event(
                tx,
                &escape,
                EscapeEventKind::CheckProposed,
                "person",
                &format!("person:{audit_id}"),
                at,
            )?;
            let check = read(tx, id)?.ok_or_else(|| unreadable(()))?;
            Ok((escape, check))
        })
    }

    /// A person accepts a proposal: `proposed` becomes `writing`, with the id of the crew task the caller is about to start. The
    /// caller starts the worker only after this returns. Accepting a check already `writing` returns it unchanged.
    pub fn accept_escape_check(
        &self,
        project: &str,
        id: &str,
        seen: &EscapeCheckSeen,
        expect: &CheckProposal<'_>,
        task_id: &str,
        at: i64,
    ) -> Result<(EscapeRecord, EscapeCheckRecord, bool)> {
        self.commit(|tx, _| {
            let escape = load(tx, project, id)?;
            let check = read(tx, id)?.ok_or_else(|| CoreError::NotFound("check".into()))?;
            if check.state == EscapeCheckState::Writing {
                return Ok((escape, check, false));
            }
            stale(&escape, Some(&check), seen)?;
            if check.state != EscapeCheckState::Proposed {
                return Err(refuse_state(&check, "accepted"));
            }
            if escape.state != EscapeState::Confirmed {
                return Err(CoreError::Invalid(
                    "Only a confirmed escape can have a check written.".into(),
                ));
            }
            if escape.gap == Some(EscapeGap::ToolingGap) {
                return Err(CoreError::Invalid(
                    "This escape is now a tooling gap, which has no proposal. Change the gap to a test gap first.".into(),
                ));
            }
            // The brief the caller wrote was made from the proposal it read; accept only that proposal.
            if (&check.path[..], &check.asserts[..], &check.run[..], &check.fix_commit[..])
                != (expect.path, expect.asserts, expect.run, expect.fix_commit)
            {
                return Err(CoreError::Stale(
                    "This proposal changed since the brief was written. Reload it and accept again.".into(),
                ));
            }
            room(tx, project, 0, 3, Some(id), at)?;
            audit(
                tx,
                "crew.escape.check.accept",
                project,
                id,
                json!({ "project": project, "escape": id, "task": task_id, "path": check.path, "attempt": check.attempts + 1 }),
            )?;
            tx.execute(
                "INSERT OR IGNORE INTO crew_escape_check_tasks (task_id, escape_id, created_unix) VALUES (?1, ?2, ?3)",
                params![task_id, id, at],
            )?;
            changed(tx.execute(
                "UPDATE crew_escape_checks SET state = 'writing', task_id = ?1, proof = NULL, note = NULL,
                        diff_commit = NULL, diff_hash = NULL, diff_paths = NULL, approved_commit = NULL, approved_hash = NULL,
                        attempts = attempts + 1, updated_unix = ?2
                 WHERE escape_id = ?3 AND state = 'proposed'",
                params![task_id, at, id],
            )?)?;
            let check = read(tx, id)?.ok_or_else(|| unreadable(()))?;
            Ok((escape, check, true))
        })
    }

    /// The start failed before any task existed: `writing` goes back to `proposed`, with an audit row that says why, so the person can
    /// accept again.
    pub fn unwind_escape_check_accept(
        &self,
        project: &str,
        id: &str,
        task_id: &str,
        why: &str,
        at: i64,
    ) -> Result<EscapeCheckRecord> {
        self.commit(|tx, _| {
            let check = read(tx, id)?.ok_or_else(|| CoreError::NotFound("check".into()))?;
            if check.state != EscapeCheckState::Writing
                || check.task_id.as_deref() != Some(task_id)
                || check.project_id != project
            {
                return Ok(check);
            }
            audit(
                tx,
                "crew.escape.check.start_failed",
                project,
                id,
                json!({ "project": project, "escape": id, "task": task_id, "why": why }),
            )?;
            changed(tx.execute(
                "UPDATE crew_escape_checks SET state = 'proposed', task_id = NULL, note = ?1, diff_commit = NULL, diff_hash = NULL, diff_paths = NULL, approved_commit = NULL, approved_hash = NULL, updated_unix = ?2
                 WHERE escape_id = ?3 AND state = 'writing' AND task_id = ?4",
                params![why.chars().take(200).collect::<String>(), at, id, task_id],
            )?)?;
            read(tx, id)?.ok_or_else(|| unreadable(()))
        })
    }

    /// A person sends a failed check back to `proposed` (a new proposal round); it starts nothing, and only accept starts a worker.
    pub fn retry_escape_check(
        &self,
        project: &str,
        id: &str,
        seen: &EscapeCheckSeen,
        at: i64,
    ) -> Result<(EscapeRecord, EscapeCheckRecord)> {
        self.commit(|tx, _| {
            let escape = load(tx, project, id)?;
            let check = read(tx, id)?.ok_or_else(|| CoreError::NotFound("check".into()))?;
            if check.state == EscapeCheckState::Proposed {
                return Ok((escape, check));
            }
            stale(&escape, Some(&check), seen)?;
            if check.state != EscapeCheckState::Failed {
                return Err(refuse_state(&check, "retried"));
            }
            room(tx, project, 0, 1, Some(id), at)?;
            let audit_id = audit(
                tx,
                "crew.escape.check.retry",
                project,
                id,
                json!({ "project": project, "escape": id, "was_task": check.task_id, "attempts": check.attempts }),
            )?;
            changed(tx.execute(
                "UPDATE crew_escape_checks SET state = 'proposed', task_id = NULL, proof = NULL, note = NULL, diff_commit = NULL, diff_hash = NULL, diff_paths = NULL, approved_commit = NULL, approved_hash = NULL, updated_unix = ?1
                 WHERE escape_id = ?2 AND state = 'failed'",
                params![at, id],
            )?)?;
            event(
                tx,
                &escape,
                EscapeEventKind::CheckProposed,
                "person",
                &format!("retry:{audit_id}"),
                at,
            )?;
            let check = read(tx, id)?.ok_or_else(|| unreadable(()))?;
            Ok((escape, check))
        })
    }

    /// A person withdraws a check that is not merged. Returns the check as it now stands and the task it had, when one may still
    /// be running or queued, so the caller can stop it.
    pub fn withdraw_escape_check(
        &self,
        project: &str,
        id: &str,
        seen: &EscapeCheckSeen,
        at: i64,
    ) -> Result<(EscapeRecord, EscapeCheckRecord, Option<EscapeCheckState>)> {
        self.commit(|tx, _| {
            let escape = load(tx, project, id)?;
            let check = read(tx, id)?.ok_or_else(|| CoreError::NotFound("check".into()))?;
            if check.state == EscapeCheckState::Withdrawn {
                return Ok((escape, check, None));
            }
            stale(&escape, Some(&check), seen)?;
            let merged = match &check.task_id {
                Some(task) => tx
                    .query_row("SELECT 1 FROM crew_merges WHERE task_id = ?1", [task], |_| Ok(()))
                    .optional()?
                    .is_some(),
                None => false,
            };
            if check.state == EscapeCheckState::Merged || merged {
                return Err(CoreError::Invalid(
                    "This check is already merged, so it cannot be withdrawn now.".into(),
                ));
            }
            // A merge past its point of no return cannot be stopped by a withdraw: refuse, with no state change. The merge side re-reads the
            // check after it marks the entry `merging` (or saves the forge intent as requested), and both sides run in a transaction on this
            // database, so exactly one of the two orders happens and a withdrawn check never lands.
            if let Some(task) = &check.task_id {
                let landing: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM crew_merge_queue WHERE task_id = ?1 AND state = 'merging')
                         OR EXISTS(SELECT 1 FROM crew_forge_merges WHERE task_id = ?1
                                   AND json_extract(body, '$.phase') IN ('merge_requested', 'merged'))",
                    [task],
                    |row| row.get(0),
                )?;
                if landing {
                    return Err(CoreError::Invalid(
                        "The merge is landing right now. Withdraw the check after it ends.".into(),
                    ));
                }
            }
            room(tx, project, 0, 1, Some(id), at)?;
            let audit_id = audit(
                tx,
                "crew.escape.check.withdraw",
                project,
                id,
                json!({ "project": project, "escape": id, "task": check.task_id, "from": check_word(check.state) }),
            )?;
            changed(tx.execute(
                "UPDATE crew_escape_checks SET state = 'withdrawn', updated_unix = ?1
                 WHERE escape_id = ?2 AND state = ?3",
                params![at, id, check_word(check.state)],
            )?)?;
            event(
                tx,
                &escape,
                EscapeEventKind::CheckWithdrawn,
                "person",
                &format!("person:{audit_id}"),
                at,
            )?;
            let was = check.state;
            let now = read(tx, id)?.ok_or_else(|| unreadable(()))?;
            Ok((escape, now, Some(was)))
        })
    }

    /// The core records how the worker's attempt ended. Only a `writing` check of this task changes; anything else answers `false`
    /// and writes nothing (a withdrawn check stays withdrawn).
    pub fn finish_escape_check(
        &self,
        escape_id: &str,
        task_id: &str,
        finish: &CheckFinish,
        at: i64,
    ) -> Result<bool> {
        self.commit(|tx, _| {
            let Some(check) = read(tx, escape_id)? else {
                return Ok(false);
            };
            if check.state != EscapeCheckState::Writing || check.task_id.as_deref() != Some(task_id)
            {
                return Ok(false);
            }
            let (state, proof, note, kind) = match finish {
                CheckFinish::Proven(proof) => (
                    "proven",
                    proof_json(Some(proof))?,
                    None,
                    EscapeEventKind::CheckProven,
                ),
                CheckFinish::Failed { proof, note } => (
                    "failed",
                    proof_json(proof.as_ref())?,
                    Some(note.chars().take(200).collect::<String>()),
                    EscapeEventKind::CheckFailed,
                ),
            };
            let escape = load(tx, &check.project_id, escape_id)?;
            changed(tx.execute(
                "UPDATE crew_escape_checks SET state = ?1, proof = ?2, note = ?3, updated_unix = ?4
                 WHERE escape_id = ?5 AND state = 'writing' AND task_id = ?6",
                params![state, proof, note, at, escape_id, task_id],
            )?)?;
            event(
                tx,
                &escape,
                kind,
                "core",
                &format!("core:{task_id}:{}", round(tx, escape_id)?),
                at,
            )?;
            Ok(true)
        })
    }

    /// The core notes what happened to a proven check in the merge queue (queued, or why not), or to a withdrawn one (its entry could not be
    /// stopped). Only a `proven` or `withdrawn` check of this task.
    pub fn note_escape_check(
        &self,
        escape_id: &str,
        task_id: &str,
        note: &str,
        at: i64,
    ) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "UPDATE crew_escape_checks SET note = ?1, updated_unix = ?2
                 WHERE escape_id = ?3 AND state IN ('proven', 'withdrawn') AND task_id = ?4",
                params![
                    note.chars().take(200).collect::<String>(),
                    at,
                    escape_id,
                    task_id
                ],
            )?;
            Ok(())
        })
    }

    /// The merge queue merged the check's task: `proven` becomes `merged`, with its event. Anything else changes nothing.
    pub fn merge_escape_check(&self, escape_id: &str, task_id: &str, at: i64) -> Result<bool> {
        self.commit(|tx, _| {
            let Some(check) = read(tx, escape_id)? else {
                return Ok(false);
            };
            if check.state != EscapeCheckState::Proven || check.task_id.as_deref() != Some(task_id)
            {
                return Ok(false);
            }
            let escape = load(tx, &check.project_id, escape_id)?;
            changed(tx.execute(
                "UPDATE crew_escape_checks SET state = 'merged', note = NULL, updated_unix = ?1
                 WHERE escape_id = ?2 AND state = 'proven' AND task_id = ?3",
                params![at, escape_id, task_id],
            )?)?;
            event(
                tx,
                &escape,
                EscapeEventKind::CheckMerged,
                "core",
                &format!("core:{task_id}:{}", round(tx, escape_id)?),
                at,
            )?;
            Ok(true)
        })
    }

    /// The core records the worker's change as it read it from git. Only a `writing` check of this task; the same commit and hash
    /// again changes nothing, and a different commit clears any approval of an earlier one.
    pub fn record_escape_check_diff(
        &self,
        escape_id: &str,
        task_id: &str,
        commit: &str,
        hash: &str,
        paths: &[String],
        at: i64,
    ) -> Result<bool> {
        self.commit(|tx, _| {
            let Some(check) = read(tx, escape_id)? else {
                return Ok(false);
            };
            if check.state != EscapeCheckState::Writing
                || check.task_id.as_deref() != Some(task_id)
                || (check.diff_commit.as_deref(), check.diff_hash.as_deref())
                    == (Some(commit), Some(hash))
            {
                return Ok(false);
            }
            tx.execute(
                "UPDATE crew_escape_checks SET diff_commit = ?1, diff_hash = ?2, diff_paths = ?3,
                        approved_commit = NULL, approved_hash = NULL, updated_unix = ?4
                 WHERE escape_id = ?5 AND state = 'writing' AND task_id = ?6",
                params![
                    commit,
                    hash,
                    serde_json::to_string(paths)?,
                    at,
                    escape_id,
                    task_id
                ],
            )?;
            Ok(true)
        })
    }

    /// A person approves exactly the change the core read: its commit and hash. Audit row first. The same approval again changes
    /// nothing; a commit or hash that is not the current change is `stale`.
    pub fn approve_escape_check(
        &self,
        project: &str,
        id: &str,
        seen: &EscapeCheckSeen,
        commit: &str,
        hash: &str,
        at: i64,
    ) -> Result<(EscapeRecord, EscapeCheckRecord, bool)> {
        self.commit(|tx, _| {
            let escape = load(tx, project, id)?;
            let check = read(tx, id)?.ok_or_else(|| CoreError::NotFound("check".into()))?;
            if (check.approved_commit.as_deref(), check.approved_hash.as_deref())
                == (Some(commit), Some(hash))
            {
                return Ok((escape, check, false));
            }
            stale(&escape, Some(&check), seen)?;
            if check.state != EscapeCheckState::Writing || check.diff_commit.is_none() {
                return Err(CoreError::Invalid(
                    "There is no change waiting for approval on this check.".into(),
                ));
            }
            if (check.diff_commit.as_deref(), check.diff_hash.as_deref())
                != (Some(commit), Some(hash))
            {
                return Err(CoreError::Stale(
                    "The change differs from the one you saw. Reload it and approve again.".into(),
                ));
            }
            room(tx, project, 0, 1, Some(id), at)?;
            audit(
                tx,
                "crew.escape.check.approve",
                project,
                id,
                json!({ "project": project, "escape": id, "task": check.task_id, "commit": commit,
                        "diff_hash": hash, "paths": check.diff_paths }),
            )?;
            changed(tx.execute(
                "UPDATE crew_escape_checks SET approved_commit = ?1, approved_hash = ?2, updated_unix = ?3
                 WHERE escape_id = ?4 AND state = 'writing' AND diff_commit = ?1 AND diff_hash = ?2",
                params![commit, hash, at, id],
            )?)?;
            let check = read(tx, id)?.ok_or_else(|| unreadable(()))?;
            Ok((escape, check, true))
        })
    }

    /// Open merge-queue entries of a task that an accept created for a check whose escape's current check is not proven for that very
    /// task (withdrawn, failed, proposed again, writing again, or gone): `(escape id, task id)`. Such an entry never merges.
    pub fn crew_escape_checks_unproven_queued(&self) -> Result<Vec<(String, String)>> {
        let conn = self.reader();
        let mut query = conn
            .prepare(
                "SELECT t.escape_id, t.task_id FROM crew_escape_check_tasks t
                 JOIN crew_merge_queue q ON q.task_id = t.task_id
                 LEFT JOIN crew_escape_checks c ON c.escape_id = t.escape_id
                 WHERE q.state IN ('waiting', 'queued', 'blocked')
                   AND NOT (c.task_id = t.task_id AND c.state IN ('proven', 'merged')) LIMIT 200",
            )
            .map_err(unreadable)?;
        query
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(unreadable)?
            .collect::<rusqlite::Result<_>>()
            .map_err(unreadable)
    }

    /// `writing` checks whose task does not exist: the process ended between the accept and the start of the task.
    pub fn crew_escape_checks_without_task(&self) -> Result<Vec<EscapeCheckRecord>> {
        let conn = self.reader();
        let mut query = conn
            .prepare(
                "SELECT * FROM crew_escape_checks c WHERE c.state = 'writing'
                 AND NOT EXISTS (SELECT 1 FROM crew_tasks t WHERE t.id = c.task_id) LIMIT 200",
            )
            .map_err(unreadable)?;
        query
            .query_map([], check_row)
            .map_err(unreadable)?
            .collect::<rusqlite::Result<_>>()
            .map_err(unreadable)
    }
}
