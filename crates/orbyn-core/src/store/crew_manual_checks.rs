//! The check a person makes (issue #794; migration 74). A `manual` check of an answer key is not something code can run: it waits for a
//! person to look and decide. One row per (task, check) in `crew_manual_checks`: `awaiting` while the task waits, then `passed` or `failed`
//! with the person's note and when they decided. The table holds no command, no output and no key text.
//!
//! - **The wait.** The verifier parks a `manual` check that no person has decided yet (`park_manual_check`), and reads the row back on every
//!   later verification (`crew_manual_check`): a decided check is no longer a wait, it is one of the person's answers.
//! - **The decision.** `decide_manual_check` is a person's act and one immediate transaction, in the order architecture rule 6 asks for: the
//!   ledger's audit row first, then the state change, whose `WHERE` repeats `awaiting`, so two decisions racing leave one change and the
//!   other is `stale`. The same decision twice returns `false` and writes nothing (a retry after a lost answer is not an error); the
//!   opposite decision on a decided check is refused in plain words.
//! - **Bounds.** One task's waits are read together, oldest first, and a person's note is at most 200 characters, already redacted by the
//!   caller.
//!
//! An unreadable row fails the whole read (`CoreError::Unreadable`), never a guess (rule 7).

use rusqlite::{OptionalExtension, Row, Transaction, params};

use super::{AuditRecord, Store, insert_audit};
use crate::error::{CoreError, Result};

/// The most of a person's note this table keeps.
pub const NOTE_MAX: usize = 200;

/// What a person said about a `manual` check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManualCheckState {
    /// Nobody has decided it yet: the task waits.
    Awaiting,
    /// A person looked and it holds.
    Passed,
    /// A person looked and it does not hold.
    Failed,
}

impl ManualCheckState {
    pub fn word(self) -> &'static str {
        match self {
            Self::Awaiting => "awaiting",
            Self::Passed => "passed",
            Self::Failed => "failed",
        }
    }

    fn of(word: &str) -> Option<Self> {
        match word {
            "awaiting" => Some(Self::Awaiting),
            "passed" => Some(Self::Passed),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

/// One `manual` check of a task as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManualCheck {
    pub task_id: String,
    pub check_id: String,
    pub state: ManualCheckState,
    pub note: Option<String>,
    pub waiting_unix: i64,
    pub decided_unix: Option<i64>,
}

fn unreadable(error: rusqlite::Error) -> CoreError {
    CoreError::Unreadable(format!("a manual check row cannot be read: {error}"))
}

fn row_of(row: &Row<'_>) -> rusqlite::Result<ManualCheck> {
    let word: String = row.get("state")?;
    let state = ManualCheckState::of(&word).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("a manual check state is {word}"),
            )),
        )
    })?;
    Ok(ManualCheck {
        task_id: row.get("task_id")?,
        check_id: row.get("check_id")?,
        state,
        note: row.get("note")?,
        waiting_unix: row.get("waiting_unix")?,
        decided_unix: row.get("decided_unix")?,
    })
}

fn read(tx: &Transaction<'_>, task_id: &str, check_id: &str) -> Result<Option<ManualCheck>> {
    tx.query_row(
        "SELECT * FROM crew_manual_checks WHERE task_id = ?1 AND check_id = ?2",
        params![task_id, check_id],
        row_of,
    )
    .optional()
    .map_err(unreadable)
}

fn decided_elsewhere(was: ManualCheckState) -> CoreError {
    let word = match was {
        ManualCheckState::Passed => "passed",
        ManualCheckState::Failed => "denied",
        ManualCheckState::Awaiting => "waiting",
    };
    CoreError::Invalid(format!(
        "A person already {word} this check, so it cannot be decided again now."
    ))
}

impl Store {
    /// Records that a `manual` check waits for a person. The same check parked again keeps the row it has, including a decision a person
    /// already made: the wait is not restarted by a later verification.
    pub fn park_manual_check(&self, task_id: &str, check_id: &str, at: i64) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "INSERT INTO crew_manual_checks (task_id, check_id, state, waiting_unix)
                 VALUES (?1, ?2, 'awaiting', ?3)
                 ON CONFLICT(task_id, check_id) DO NOTHING",
                params![task_id, check_id, at],
            )?;
            Ok(())
        })
    }

    /// One `manual` check of a task, if the task has one by that id.
    pub fn crew_manual_check(&self, task_id: &str, check_id: &str) -> Result<Option<ManualCheck>> {
        self.reader()
            .query_row(
                "SELECT * FROM crew_manual_checks WHERE task_id = ?1 AND check_id = ?2",
                params![task_id, check_id],
                row_of,
            )
            .optional()
            .map_err(unreadable)
    }

    /// The `manual` checks of a task still waiting for a person, by id, in the order they were parked.
    pub fn crew_manual_checks_waiting(&self, task_id: &str) -> Result<Vec<String>> {
        let conn = self.reader();
        let mut query = conn
            .prepare(
                "SELECT check_id FROM crew_manual_checks
                 WHERE task_id = ?1 AND state = 'awaiting' ORDER BY waiting_unix, check_id",
            )
            .map_err(unreadable)?;
        query
            .query_map([task_id], |row| row.get(0))
            .map_err(unreadable)?
            .collect::<rusqlite::Result<Vec<String>>>()
            .map_err(unreadable)
    }

    /// Every `manual` check of a task, oldest first.
    pub fn crew_manual_checks(&self, task_id: &str) -> Result<Vec<ManualCheck>> {
        let conn = self.reader();
        let mut query = conn
            .prepare(
                "SELECT * FROM crew_manual_checks WHERE task_id = ?1 ORDER BY waiting_unix, check_id",
            )
            .map_err(unreadable)?;
        query
            .query_map([task_id], row_of)
            .map_err(unreadable)?
            .collect::<rusqlite::Result<Vec<ManualCheck>>>()
            .map_err(unreadable)
    }

    /// A person passes or denies one `manual` check of `task_id`. Returns whether this call changed it: the same decision again returns
    /// `false` and writes nothing. The audit row comes first, then the state change, in one transaction.
    pub fn decide_manual_check(
        &self,
        task_id: &str,
        check_id: &str,
        pass: bool,
        note: Option<&str>,
        at: i64,
    ) -> Result<bool> {
        self.commit(|tx, _| {
            let held = read(tx, task_id, check_id)?
                .ok_or_else(|| CoreError::NotFound("manual check".into()))?;
            let wanted = if pass {
                ManualCheckState::Passed
            } else {
                ManualCheckState::Failed
            };
            if held.state == wanted {
                return Ok(false);
            }
            if held.state != ManualCheckState::Awaiting {
                return Err(decided_elsewhere(held.state));
            }
            let note = note
                .map(|text| text.chars().take(NOTE_MAX).collect::<String>())
                .filter(|text| !text.trim().is_empty());
            insert_audit(
                tx,
                &AuditRecord {
                    actor: "user".to_owned(),
                    action: "crew.task.decide_manual_check".to_owned(),
                    target: format!("{task_id}:{check_id}"),
                    decision: None,
                    source: "user".to_owned(),
                    thread_id: None,
                    run_id: None,
                    detail: serde_json::json!({
                        "task": task_id,
                        "check": check_id,
                        "decision": wanted.word(),
                        "note": note,
                    }),
                },
            )?;
            let changed = tx.execute(
                "UPDATE crew_manual_checks SET state = ?1, note = ?2, decided_unix = ?3
                 WHERE task_id = ?4 AND check_id = ?5 AND state = 'awaiting'",
                params![wanted.word(), note, at, task_id, check_id],
            )?;
            if changed != 1 {
                return Err(CoreError::Stale(
                    "This check changed since you looked. Reload it and decide again.".into(),
                ));
            }
            Ok(true)
        })
    }
}
