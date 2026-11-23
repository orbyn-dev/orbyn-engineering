//! Resume after a restart (KC8), at rest. A task whose worker died with the process gets one
//! resume in its life: one transaction re-checks it, counts the lost turn and records the audit
//! row before the state change (architecture rule 6). A task that may not resume is stopped by
//! `stop_crew_after_restart`, whose state change and `crew.task.restart_stopped` row share one
//! transaction, like `finish_crew_cancel`. Downtime stays out of the budget: `downtime_secs`
//! records what the app was off, and `crew_task_last_event` says when the task last moved.

use rusqlite::{Connection, OptionalExtension, Transaction, params};

use super::crew::{CrewTask, find_task};
use super::crew_runs::{CrewRun, find_run, step};
use super::{AuditRecord, Store, insert_audit};
use crate::error::Result;

/// What one attempt to resume a task found.
pub enum ResumeOutcome {
    /// The one resume is taken: the task is `running`, its lost turn counted.
    Resumed { task: CrewTask, run: Box<CrewRun> },
    /// The task may not resume; nothing was written. `why` is the row's reason when the caller stops it.
    Refused(&'static str),
    /// The task is no longer `running` or `changed_approach` (a cancel or an end won), so nothing is written.
    NotInFlight,
}

/// The lane a member belongs to. The launch prefixes are the lane kinds: `worker-` is the builder
/// and the fix that answers a finding, `reviewer-` is a review lane. Any other member does not resume.
enum Lane {
    Builder,
    Reviewer,
    Other,
}

fn lane(member: &str) -> Lane {
    if member
        .strip_prefix("worker-")
        .is_some_and(|rest| !rest.is_empty())
    {
        Lane::Builder
    } else if member
        .strip_prefix("reviewer-")
        .is_some_and(|rest| !rest.is_empty())
    {
        Lane::Reviewer
    } else {
        Lane::Other
    }
}

fn audit(tx: &Transaction<'_>, action: &str, task: &str, detail: serde_json::Value) -> Result<()> {
    insert_audit(
        tx,
        &AuditRecord {
            actor: "core".to_owned(),
            action: action.to_owned(),
            target: task.to_owned(),
            decision: None,
            source: "core".to_owned(),
            thread_id: None,
            run_id: None,
            detail,
        },
    )?;
    Ok(())
}

/// The newest time the task moved: its `updated_at`, or the newest audit row for it. Both are
/// RFC 3339 and an unreadable one is ignored, never guessed.
fn last_event(conn: &Connection, task_id: &str) -> Result<Option<i64>> {
    let updated: Option<String> = conn
        .query_row(
            "SELECT updated_at FROM crew_tasks WHERE id = ?1",
            [task_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(updated) = updated else {
        return Ok(None);
    };
    let audited: Option<String> = conn
        .query_row(
            "SELECT ts FROM audit_events WHERE target = ?1 ORDER BY id DESC LIMIT 1",
            [task_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok([
        crate::util::parse_time(&updated),
        audited.as_deref().and_then(crate::util::parse_time),
    ]
    .into_iter()
    .flatten()
    .max())
}

/// The stop a restart leaves: `stopped` with `engine_error`, and its audit row, together.
fn stop(tx: &Transaction<'_>, task: CrewTask, why: &'static str) -> Result<CrewTask> {
    audit(
        tx,
        "crew.task.restart_stopped",
        &task.id,
        serde_json::json!({ "why": why }),
    )?;
    step(tx, &task, "stopped", Some("engine_error"), &[])
}

impl Store {
    /// The newest recorded event of one task: its `updated_at` or its newest audit row, whichever is later.
    pub fn crew_task_last_event(&self, task_id: &str) -> Result<Option<i64>> {
        last_event(&self.reader(), task_id)
    }

    /// Takes the one resume a task gets after a restart, or refuses and writes nothing:
    /// the task must still be `running` (or `changed_approach`, the one retry state), have
    /// `resume_count = 0` and a builder or fix lane. A resume moves `changed_approach` back to
    /// `running`, counts the lost turn, records when it happened and the downtime before it,
    /// and writes `crew.task.resumed` in the same transaction.
    pub fn resume_crew_task(&self, task_id: &str) -> Result<ResumeOutcome> {
        self.commit(|tx, _| {
            let task = find_task(tx, task_id)?;
            if !matches!(task.state.as_str(), "running" | "changed_approach") {
                return Ok(ResumeOutcome::NotInFlight);
            }
            let run = find_run(tx, task_id)?;
            if run.resume_count > 0 {
                return Ok(ResumeOutcome::Refused("already_resumed"));
            }
            match lane(&run.member) {
                Lane::Builder => {}
                Lane::Reviewer => return Ok(ResumeOutcome::Refused("reviewer")),
                Lane::Other => return Ok(ResumeOutcome::Refused("not_resumable")),
            }
            // The lost turn is one more attempt, and the round it resumes is the one the worker is told.
            let round = run.attempts.saturating_add(1);
            let now = crate::util::unix_now();
            let last = last_event(tx, task_id)?.unwrap_or(now);
            let downtime = now.saturating_sub(last).max(0);
            // Rule 6: the row is written before the effect it describes, in this transaction.
            audit(
                tx,
                "crew.task.resumed",
                task_id,
                serde_json::json!({ "round": round, "member": run.member }),
            )?;
            // A report is taken only while `running`, so the one retry state goes back to it.
            let task = if task.state == "changed_approach" {
                step(tx, &task, "running", None, &[])?
            } else {
                task
            };
            tx.execute(
                "UPDATE crew_runs SET resume_count = 1, resumed_at = ?2, attempts = attempts + 1,
                 downtime_secs = ?3 WHERE task_id = ?1 AND resume_count = 0",
                params![task_id, crate::util::format_unix(now), downtime],
            )?;
            let run = find_run(tx, task_id)?;
            Ok(ResumeOutcome::Resumed { task, run: Box::new(run) })
        })
    }

    /// Stops a task the restart may not resume, or whose resume brief could not be sent: the
    /// `stopped`/`engine_error` move and its `crew.task.restart_stopped` row share one transaction.
    /// A task that already ended meanwhile is left alone.
    pub fn stop_crew_after_restart(
        &self,
        task_id: &str,
        why: &'static str,
    ) -> Result<Option<CrewTask>> {
        self.commit(|tx, _| {
            let task = find_task(tx, task_id)?;
            if !matches!(task.state.as_str(), "running" | "changed_approach") {
                return Ok(None);
            }
            Ok(Some(stop(tx, task, why)?))
        })
    }
}
