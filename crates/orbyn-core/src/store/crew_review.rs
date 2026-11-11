//! Review lanes, findings and the fix loop at rest. A finding is stored with its lane, round and
//! fingerprint, and only while its reviewer's run is live (H7), in the transaction that checks it.
//! Nothing here decides anything: whether a finding is confirmed is a Gate (`finding_check`),
//! written by the core under the reviewed task.

use orbyn_contracts::{CrewObject, Finding, Gate};
use rusqlite::{OptionalExtension, Row, params};

use super::Store;
use super::crew::{belongs_to, checked, find_task, insert_object, read_object};
use crate::crew::lifecycle::is_final;
use crate::error::{CoreError, Result};
use crate::util::now;

/// A review run: which work it reads, in which lane and round, at which commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrewReview {
    pub task_id: String,
    pub subject_id: String,
    pub lane: String,
    /// `review`, or `finding_check` for the independent second run of a finding check.
    pub role: String,
    pub round: u32,
    pub commit: String,
    /// The finding a second run checks.
    pub check_of: Option<String>,
}

/// A finding as stored.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredFinding {
    pub finding: Finding,
    /// The reviewer's task.
    pub task_id: String,
    pub subject_id: String,
    pub lane: String,
    pub round: u32,
    pub fingerprint: String,
}

/// The lead's repro check that decided a finding, as JSON. It is part of the key for the fix and never shown to
/// a worker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindingRepro {
    pub finding_id: String,
    pub subject_id: String,
    pub repro: String,
}

/// The builder task that answers a confirmed finding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrewFix {
    pub finding_id: String,
    pub subject_id: String,
    pub fix_task_id: String,
    pub fingerprint: String,
}

fn review_row(row: &Row<'_>) -> rusqlite::Result<CrewReview> {
    Ok(CrewReview {
        task_id: row.get("task_id")?,
        subject_id: row.get("subject_id")?,
        lane: row.get("lane")?,
        role: row.get("role")?,
        round: row.get("round")?,
        commit: row.get("commit_id")?,
        check_of: row.get("check_of")?,
    })
}

fn finding_of(row: &Row<'_>) -> rusqlite::Result<(String, String, String, String, u32, String)> {
    Ok((
        row.get("body")?,
        row.get("task_id")?,
        row.get("subject_id")?,
        row.get("lane")?,
        row.get("round")?,
        row.get("fingerprint")?,
    ))
}

fn stored(parts: (String, String, String, String, u32, String)) -> Result<StoredFinding> {
    let (body, task_id, subject_id, lane, round, fingerprint) = parts;
    match read_object(&body)? {
        CrewObject::Finding(finding) => Ok(StoredFinding {
            finding,
            task_id,
            subject_id,
            lane,
            round,
            fingerprint,
        }),
        other => Err(CoreError::Internal(format!(
            "crew_findings holds a {}",
            other.type_name()
        ))),
    }
}

const FINDING_COLUMNS: &str = "body, task_id, subject_id, lane, round, fingerprint";

impl Store {
    /// Records that `task_id` is a review run. Adding the same row again changes nothing.
    pub fn add_crew_review(&self, review: &CrewReview) -> Result<()> {
        self.commit(|tx, _| {
            find_task(tx, &review.task_id)?;
            find_task(tx, &review.subject_id)?;
            tx.execute(
                "INSERT INTO crew_reviews (task_id, subject_id, lane, role, round, commit_id, check_of, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) ON CONFLICT(task_id) DO NOTHING",
                params![
                    review.task_id,
                    review.subject_id,
                    review.lane,
                    review.role,
                    review.round,
                    review.commit,
                    review.check_of,
                    now()
                ],
            )?;
            Ok(())
        })
    }

    pub fn crew_review(&self, task_id: &str) -> Result<Option<CrewReview>> {
        Ok(self
            .reader()
            .query_row(
                "SELECT * FROM crew_reviews WHERE task_id = ?1",
                [task_id],
                review_row,
            )
            .optional()?)
    }

    /// Every review run of a piece of work, oldest first.
    pub fn crew_reviews_of(&self, subject_id: &str) -> Result<Vec<CrewReview>> {
        let conn = self.reader();
        let mut statement =
            conn.prepare("SELECT * FROM crew_reviews WHERE subject_id = ?1 ORDER BY rowid")?;
        Ok(statement
            .query_map([subject_id], review_row)?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// A reviewer's finding, stored while its run is live. The lane, round and subject come from
    /// the run's review row, never from the reviewer. `Ok(false)` when it was already stored.
    pub fn add_crew_finding(
        &self,
        task_id: &str,
        finding: &Finding,
        fingerprint: &str,
        most: usize,
    ) -> Result<bool> {
        let object = CrewObject::Finding(finding.clone());
        let body = checked(&object)?;
        self.commit(|tx, _| {
            let task = find_task(tx, task_id)?;
            if is_final(&task.state) {
                return Err(CoreError::RunClosed);
            }
            belongs_to(&task, &object)?;
            let review: CrewReview = tx
                .query_row(
                    "SELECT * FROM crew_reviews WHERE task_id = ?1",
                    [task_id],
                    review_row,
                )
                .optional()?
                .ok_or_else(|| CoreError::Invalid("this task is not a review run".into()))?;
            let exists: i64 = tx.query_row(
                "SELECT COUNT(*) FROM crew_findings WHERE id = ?1",
                [finding.id.as_str()],
                |row| row.get(0),
            )?;
            if exists > 0 {
                return Ok(false);
            }
            let filed: i64 = tx.query_row(
                "SELECT COUNT(*) FROM crew_findings WHERE task_id = ?1",
                [task_id],
                |row| row.get(0),
            )?;
            if usize::try_from(filed).unwrap_or(usize::MAX) >= most {
                return Err(CoreError::Invalid(format!(
                    "a review run files at most {most} findings"
                )));
            }
            tx.execute(
                "INSERT INTO crew_findings (id, task_id, subject_id, lane, round, fingerprint, body)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    finding.id.as_str(),
                    task_id,
                    review.subject_id,
                    review.lane,
                    review.round,
                    fingerprint,
                    body
                ],
            )?;
            Ok(true)
        })
    }

    fn findings_where(&self, clause: &str, arg: &str) -> Result<Vec<StoredFinding>> {
        let conn = self.reader();
        let mut statement = conn.prepare(&format!(
            "SELECT {FINDING_COLUMNS} FROM crew_findings WHERE {clause} ORDER BY rowid"
        ))?;
        let rows = statement
            .query_map([arg], finding_of)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter().map(stored).collect()
    }

    /// The findings of every review run of a piece of work, oldest first.
    pub fn crew_findings_of(&self, subject_id: &str) -> Result<Vec<StoredFinding>> {
        self.findings_where("subject_id = ?1", subject_id)
    }

    /// The findings one review run filed.
    pub fn crew_findings_of_task(&self, task_id: &str) -> Result<Vec<StoredFinding>> {
        self.findings_where("task_id = ?1", task_id)
    }

    pub fn crew_finding(&self, id: &str) -> Result<Option<StoredFinding>> {
        Ok(self.findings_where("id = ?1", id)?.into_iter().next())
    }

    /// Keeps the repro check that decided a finding. A finding is decided once.
    pub fn add_finding_repro(&self, repro: &FindingRepro) -> Result<()> {
        self.commit(|tx, _| {
            let changed = tx.execute(
                "INSERT INTO crew_finding_repros (finding_id, subject_id, repro, created_at)
                 VALUES (?1, ?2, ?3, ?4) ON CONFLICT(finding_id) DO NOTHING",
                params![repro.finding_id, repro.subject_id, repro.repro, now()],
            )?;
            if changed == 0 {
                return Err(CoreError::Invalid(
                    "this finding has already been checked".into(),
                ));
            }
            Ok(())
        })
    }

    pub fn crew_finding_repro(&self, finding_id: &str) -> Result<Option<FindingRepro>> {
        Ok(self
            .reader()
            .query_row(
                "SELECT finding_id, subject_id, repro FROM crew_finding_repros WHERE finding_id = ?1",
                [finding_id],
                |row| {
                    Ok(FindingRepro {
                        finding_id: row.get(0)?,
                        subject_id: row.get(1)?,
                        repro: row.get(2)?,
                    })
                },
            )
            .optional()?)
    }

    /// The review runs that check a finding independently, oldest first.
    pub fn crew_second_runs(&self, finding_id: &str) -> Result<Vec<CrewReview>> {
        let conn = self.reader();
        let mut statement =
            conn.prepare("SELECT * FROM crew_reviews WHERE check_of = ?1 ORDER BY rowid")?;
        Ok(statement
            .query_map([finding_id], review_row)?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Ties a confirmed finding to the builder task that answers it. One fix a finding.
    pub fn add_crew_fix(&self, fix: &CrewFix) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "INSERT INTO crew_fixes (finding_id, subject_id, fix_task_id, fingerprint, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(finding_id) DO NOTHING",
                params![
                    fix.finding_id,
                    fix.subject_id,
                    fix.fix_task_id,
                    fix.fingerprint,
                    now()
                ],
            )?;
            Ok(())
        })
    }

    fn fixes_where(&self, clause: &str, arg: &str) -> Result<Vec<CrewFix>> {
        let conn = self.reader();
        let mut statement = conn.prepare(&format!(
            "SELECT finding_id, subject_id, fix_task_id, fingerprint FROM crew_fixes WHERE {clause} ORDER BY rowid"
        ))?;
        Ok(statement
            .query_map([arg], |row| {
                Ok(CrewFix {
                    finding_id: row.get(0)?,
                    subject_id: row.get(1)?,
                    fix_task_id: row.get(2)?,
                    fingerprint: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn crew_fixes_of(&self, subject_id: &str) -> Result<Vec<CrewFix>> {
        self.fixes_where("subject_id = ?1", subject_id)
    }

    pub fn crew_fix_for_task(&self, fix_task_id: &str) -> Result<Option<CrewFix>> {
        Ok(self
            .fixes_where("fix_task_id = ?1", fix_task_id)?
            .into_iter()
            .next())
    }

    /// Stores a core Gate for a finding, only if the finding has no `finding_check` Gate yet, in one transaction, so two checks
    /// racing cannot write a Confirmed and an Unconfirmed one. `Ok(false)` when it already has one.
    pub fn add_finding_gate_once(&self, task_id: &str, gate: &Gate) -> Result<bool> {
        let object = CrewObject::Gate(gate.clone());
        let body = checked(&object)?;
        self.commit(|tx, _| {
            let taken: i64 = tx.query_row(
                "SELECT COUNT(*) FROM crew_gates WHERE task_id = ?1 AND stage = 'finding_check'
                 AND json_extract(body, '$.subject') = ?2",
                params![task_id, gate.subject.as_str()],
                |row| row.get(0),
            )?;
            if taken > 0 {
                return Ok(false);
            }
            insert_object(tx, task_id, &object, &body)?;
            Ok(true)
        })
    }

    /// Takes a fix back when its task never started, so a refused fix does not use up the finding's one.
    pub fn remove_crew_fix(&self, finding_id: &str) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute("DELETE FROM crew_fixes WHERE finding_id = ?1", [finding_id])?;
            Ok(())
        })
    }

    /// The repros kept for the findings of the crew's task `subject_id`.
    pub fn crew_repros_of(&self, subject_id: &str) -> Result<Vec<FindingRepro>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT finding_id, subject_id, repro FROM crew_finding_repros WHERE subject_id = ?1 ORDER BY rowid",
        )?;
        Ok(statement
            .query_map([subject_id], |row| {
                Ok(FindingRepro {
                    finding_id: row.get(0)?,
                    subject_id: row.get(1)?,
                    repro: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }
}
