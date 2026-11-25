//! EvalHistory samples (migration 88, KC5 slice 4): what each crew task's brief carried and how it ended.
//!
//! Recording, and one read back (`Core::crew_task_samples`, ids and enums only). Nothing here routes, scores, verifies
//! or changes a brief, a check or a lease: `EvalHistory` and `score` keep their exact shape, and every caller still
//! passes `EvalHistory::default()`. A row holds ids, enums and counts only, never lesson text. Device only, never
//! synced or exported.
//!
//! Each task records exactly one sample, in the transaction that creates it. Its outcome, engine, project and finish
//! time are written once, in the transaction that first moves the task to a sampled state; a later move never
//! overwrites them. Bookkeeping runs in a savepoint and a failure is logged, never a reason to refuse a start or a
//! state change.

use orbyn_contracts::BriefKind;
use rusqlite::{OptionalExtension, Row, Transaction, params};

use super::Store;
use crate::error::Result;

/// One task's EvalHistory sample. Everything is an id, an enum or a count; no lesson or brief text is kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrewEvalSample {
    pub task_id: String,
    /// The task's project, filled when it becomes known through its run's lead thread.
    pub project_id: Option<String>,
    /// The brief's kind word, such as `implement`.
    pub kind: String,
    /// Whether the brief carried at least one lesson.
    pub had_lessons: bool,
    /// The lesson ids the brief carried.
    pub lesson_ids: Vec<String>,
    /// The lesson ids the brief's answer-key check left out.
    pub left_out_ids: Vec<String>,
    /// `passed`, `failed`, `stopped` or `cancelled`; none until the task reaches one of them.
    pub outcome: Option<String>,
    /// The reason of a `stopped` outcome; none for every other state.
    pub stop_reason: Option<String>,
    /// The task's latest route decision engine, when one exists.
    pub engine: Option<String>,
    pub created_at: String,
    pub finished_at: Option<String>,
}

/// The most samples one project read answers; a project's history is for measuring, not a full export.
const PROJECT_READ_MAX: i64 = 500;

fn ids(row: &Row<'_>, column: &str) -> rusqlite::Result<Vec<String>> {
    let text: String = row.get(column)?;
    serde_json::from_str(&text).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })
}

fn sample_row(row: &Row<'_>) -> rusqlite::Result<CrewEvalSample> {
    Ok(CrewEvalSample {
        task_id: row.get("task_id")?,
        project_id: row.get("project_id")?,
        kind: row.get("kind")?,
        had_lessons: row.get::<_, i64>("had_lessons")? != 0,
        lesson_ids: ids(row, "lesson_ids")?,
        left_out_ids: ids(row, "left_out_ids")?,
        outcome: row.get("outcome")?,
        stop_reason: row.get("stop_reason")?,
        engine: row.get("engine")?,
        created_at: row.get("created_at")?,
        finished_at: row.get("finished_at")?,
    })
}

/// The state a sampled task ends in; any other state records no outcome.
fn outcome_of(state: &str) -> Option<&'static str> {
    match state {
        "passed" => Some("passed"),
        "failed" => Some("failed"),
        "stopped" => Some("stopped"),
        "cancelled" => Some("cancelled"),
        _ => None,
    }
}

/// The project of a task, through the lead thread of its run. None before the run exists, which is the usual case at
/// creation; the finish path fills it once the run is there.
fn project_of(tx: &Transaction<'_>, task_id: &str) -> Result<Option<String>> {
    Ok(tx
        .query_row(
            "SELECT th.project_id FROM crew_runs r JOIN threads th ON th.id = r.lead_thread_id
             WHERE r.task_id = ?1",
            [task_id],
            |row| row.get(0),
        )
        .optional()?)
}

/// Runs bookkeeping in its own savepoint: a failure rolls only the sample back, and the caller logs it and carries on.
fn bookkeeping(
    tx: &Transaction<'_>,
    savepoint: &str,
    work: impl FnOnce() -> Result<()>,
) -> Result<()> {
    tx.execute_batch(&format!("SAVEPOINT {savepoint}"))?;
    if let Err(error) = work() {
        tracing::warn!(%error, "a crew eval sample's bookkeeping failed, so it is rolled back");
        tx.execute_batch(&format!("ROLLBACK TO {savepoint}"))?;
    }
    tx.execute_batch(&format!("RELEASE {savepoint}"))?;
    Ok(())
}

/// Records the one sample of a task that was just created: its kind, whether it carried lessons and which ids. An
/// existing task id never gets a second row.
pub(super) fn record_start(
    tx: &Transaction<'_>,
    task_id: &str,
    kind: BriefKind,
    used: &[String],
    left_out: &[String],
    at: &str,
) {
    let written = bookkeeping(tx, "eval_sample_start", || {
        let kind = super::wire_name(kind)?;
        let lesson_ids = serde_json::to_string(used)?;
        let left_out_ids = serde_json::to_string(left_out)?;
        let project = project_of(tx, task_id)?;
        tx.execute(
            "INSERT INTO crew_eval_samples
                (task_id, project_id, kind, had_lessons, lesson_ids, left_out_ids, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(task_id) DO NOTHING",
            params![
                task_id,
                project,
                kind,
                i64::from(!used.is_empty()),
                lesson_ids,
                left_out_ids,
                at
            ],
        )?;
        Ok(())
    });
    if let Err(error) = written {
        tracing::warn!(%error, task = task_id, "a crew task's eval sample could not be started");
    }
}

/// Finishes a task's sample in the state change's own transaction, only while it has no outcome. A later move never
/// overwrites the first one.
pub(super) fn record_finish(
    tx: &Transaction<'_>,
    task_id: &str,
    state: &str,
    stop_reason: Option<&str>,
    at: &str,
) {
    let Some(outcome) = outcome_of(state) else {
        return;
    };
    let written = bookkeeping(tx, "eval_sample_finish", || {
        let engine: Option<String> = tx
            .query_row(
                "SELECT engine FROM crew_route_decisions WHERE task_id = ?1
                 ORDER BY rowid DESC LIMIT 1",
                [task_id],
                |row| row.get(0),
            )
            .optional()?;
        let project = project_of(tx, task_id)?;
        let stop_reason = (outcome == "stopped").then_some(stop_reason).flatten();
        tx.execute(
            "UPDATE crew_eval_samples
             SET outcome = ?2, stop_reason = ?3, engine = ?4,
                 project_id = COALESCE(project_id, ?5), finished_at = ?6
             WHERE task_id = ?1 AND outcome IS NULL",
            params![task_id, outcome, stop_reason, engine, project, at],
        )?;
        Ok(())
    });
    if let Err(error) = written {
        tracing::warn!(%error, task = task_id, "a crew task's eval sample could not be finished");
    }
}

impl Store {
    /// One project's samples, newest first, at most 500.
    pub fn crew_eval_samples(&self, project_id: &str) -> Result<Vec<CrewEvalSample>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT * FROM crew_eval_samples WHERE project_id = ?1
             ORDER BY created_at DESC, task_id DESC LIMIT ?2",
        )?;
        let rows = statement
            .query_map(params![project_id, PROJECT_READ_MAX], sample_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// One task's sample, if it has one.
    pub fn crew_eval_sample(&self, task_id: &str) -> Result<Option<CrewEvalSample>> {
        Ok(self
            .reader()
            .query_row(
                "SELECT * FROM crew_eval_samples WHERE task_id = ?1",
                [task_id],
                sample_row,
            )
            .optional()?)
    }
}
