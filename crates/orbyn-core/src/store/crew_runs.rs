//! How a Crew task runs: who works on it, where, and the loop guard's counters. The task's
//! state moves only along the lifecycle of SPEC.md section 6 (H13), checked inside the same
//! transaction that writes it, so two callers cannot both win a move.

use orbyn_contracts::{CrewObject, Lease, Report};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};

use super::Store;
use super::crew::{CrewTask, checked, find_task, insert_object};
use crate::crew::lifecycle::{Guard, Next, allowed};
use crate::error::{CoreError, Result};
use crate::util::now;

/// The mutable side of a task. Nothing here can hold the answer key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrewRun {
    pub task_id: String,
    pub lead_thread_id: String,
    pub member: String,
    pub worker_thread_id: Option<String>,
    pub worktree: Option<String>,
    pub base_commit: Option<String>,
    pub attempts: u32,
    pub failures: u32,
    pub guard: Guard,
    /// The run of the engine that last reported, so its end is not read as a missing report.
    pub last_report_run: Option<String>,
    /// The protected files as they were when the task started, as JSON.
    pub protected: Option<String>,
    /// Set while one verification has claimed the task.
    pub verifying_since: Option<String>,
    /// The index entries hiding a change from git when the task started, as JSON.
    pub index_flags: Option<String>,
    /// When the task resumed after a restart, if it ever did. A task resumes once in its life.
    pub resumed_at: Option<String>,
    /// 0 until the one resume after a restart, then 1.
    pub resume_count: u32,
    /// The time the app was off before that resume, which the task's budget does not pay for.
    pub downtime_secs: i64,
}

fn run_row(row: &Row<'_>) -> rusqlite::Result<CrewRun> {
    Ok(CrewRun {
        task_id: row.get("task_id")?,
        lead_thread_id: row.get("lead_thread_id")?,
        member: row.get("member")?,
        worker_thread_id: row.get("worker_thread_id")?,
        worktree: row.get("worktree")?,
        base_commit: row.get("base_commit")?,
        attempts: row.get("attempts")?,
        failures: row.get("failures")?,
        guard: Guard {
            streak: row.get("streak")?,
            fingerprint: row.get("fingerprint")?,
            approach_changed: row.get::<_, i64>("approach_changed")? != 0,
        },
        last_report_run: row.get("last_report_run")?,
        protected: row.get("protected")?,
        verifying_since: row.get("verifying_since")?,
        index_flags: row.get("index_flags")?,
        resumed_at: row.get("resumed_at")?,
        resume_count: row.get("resume_count")?,
        downtime_secs: row.get("downtime_secs")?,
    })
}

/// The brief's turns a task has spent: the worker's runs and its verification attempts (the
/// larger, because a run that just ended is both) plus the approvals reserved against the same
/// budget. The approval reservation and the budget stop read this one count, so they agree.
/// A task with a worker thread or a queued message has been sent its brief, so it counts at least
/// one turn even while that first prompt is still queued and no `runs` row exists yet.
pub(super) fn turns_spent(conn: &Connection, run: &CrewRun) -> Result<u64> {
    let worker: i64 = conn.query_row(
        "SELECT COUNT(*) FROM runs WHERE thread_id = ?1",
        [run.worker_thread_id.as_deref()],
        |row| row.get(0),
    )?;
    let queued: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM queued_messages WHERE thread_id = ?1)",
        [run.worker_thread_id.as_deref()],
        |row| row.get(0),
    )?;
    let worker = if run.worker_thread_id.is_some() || queued {
        worker.max(1)
    } else {
        worker
    };
    let approvals: i64 = conn.query_row(
        "SELECT COUNT(*) FROM crew_autopilot_events WHERE task_id = ?1 AND kind = 'approval_started'",
        [&run.task_id],
        |row| row.get(0),
    )?;
    let turns = i64::from(run.attempts)
        .max(worker)
        .saturating_add(approvals);
    Ok(u64::try_from(turns).unwrap_or(u64::MAX))
}

pub(super) fn find_run(tx: &Transaction<'_>, task_id: &str) -> Result<CrewRun> {
    tx.query_row(
        "SELECT * FROM crew_runs WHERE task_id = ?1",
        [task_id],
        run_row,
    )
    .optional()?
    .ok_or_else(|| CoreError::NotFound(format!("crew run {task_id}")))
}

/// Moves a task one step, or refuses with `InvalidTransition`. `stopped` names its reason and
/// nothing else does; a task passes only with Evidence (H8). A move always clears the typed stop
/// detail, which only a stop can set.
pub(super) fn step(
    tx: &Transaction<'_>,
    task: &CrewTask,
    to: &str,
    stop_reason: Option<&str>,
    evidence: &[String],
) -> Result<CrewTask> {
    step_with_detail(tx, task, to, stop_reason, None, evidence)
}

/// [`step`], with the typed stop detail a stop writes in the same UPDATE as its state and reason, so
/// the two cannot diverge. Any other move passes `None` and clears it.
pub(super) fn step_with_detail(
    tx: &Transaction<'_>,
    task: &CrewTask,
    to: &str,
    stop_reason: Option<&str>,
    stop_detail: Option<&str>,
    evidence: &[String],
) -> Result<CrewTask> {
    advance(tx, task, to, stop_reason, stop_detail, evidence, true)
}

/// [`step_with_detail`] for a transient state: the state is written, but the task's eval sample is not finished on it.
/// `fail_crew_verification` goes through `failed` to the state the guard names in one transaction, so only that
/// transaction's last move records the outcome.
fn step_transient(
    tx: &Transaction<'_>,
    task: &CrewTask,
    to: &str,
    stop_reason: Option<&str>,
    evidence: &[String],
) -> Result<CrewTask> {
    advance(tx, task, to, stop_reason, None, evidence, false)
}

fn advance(
    tx: &Transaction<'_>,
    task: &CrewTask,
    to: &str,
    stop_reason: Option<&str>,
    stop_detail: Option<&str>,
    evidence: &[String],
    sample: bool,
) -> Result<CrewTask> {
    if !allowed(&task.state, to) {
        return Err(CoreError::InvalidTransition {
            from: task.state.clone(),
            to: to.to_owned(),
        });
    }
    if (to == "stopped") != stop_reason.is_some() {
        return Err(CoreError::Invalid(
            "a stopped task names its reason, and no other state has one".into(),
        ));
    }
    if stop_detail.is_some() && to != "stopped" {
        return Err(CoreError::Invalid(
            "only a stopped task names a stop detail".into(),
        ));
    }
    if to == "passed" {
        // The Evidence is the attempt's own: every id named is the task's, and there is one.
        let mut found = 0;
        for id in evidence {
            found += tx.query_row(
                "SELECT COUNT(*) FROM crew_evidence WHERE task_id = ?1 AND id = ?2",
                params![task.id, id],
                |row| row.get::<_, i64>(0),
            )?;
        }
        if evidence.is_empty() || found < i64::try_from(evidence.len()).unwrap_or(i64::MAX) {
            return Err(CoreError::Invalid(
                "gate_needs_evidence: a task passes only with Evidence".into(),
            ));
        }
    }
    let at = now();
    tx.execute(
        "UPDATE crew_tasks SET state = ?2, stop_reason = ?3, stop_detail = ?4, updated_at = ?5
         WHERE id = ?1",
        params![task.id, to, stop_reason, stop_detail, at],
    )?;
    // A claim on a verification ends with the state it was for.
    tx.execute(
        "UPDATE crew_runs SET verifying_since = NULL WHERE task_id = ?1",
        [&task.id],
    )?;
    if sample {
        // KC5 slice 4: the sample's outcome is written once, in this same transaction. Its own savepoint logs a
        // failure and rolls only the sample back, so bookkeeping never refuses the state change.
        super::crew_eval_samples::record_finish(tx, &task.id, to, stop_reason, &at);
    }
    find_task(tx, &task.id)
}

impl Store {
    /// Gives a task its lead and the member id its worker will have.
    pub fn add_crew_run(&self, task_id: &str, lead_thread_id: &str, member: &str) -> Result<()> {
        self.commit(|tx, _| {
            find_task(tx, task_id)?;
            tx.execute(
                "INSERT INTO crew_runs (task_id, lead_thread_id, member) VALUES (?1, ?2, ?3)
                 ON CONFLICT(task_id) DO NOTHING",
                params![task_id, lead_thread_id, member],
            )?;
            Ok(())
        })
    }

    /// Records the protected files as they are before the worker exists.
    pub fn set_crew_protected(&self, task_id: &str, snapshot: &str) -> Result<()> {
        self.commit(|tx, _| {
            find_run(tx, task_id)?;
            tx.execute(
                "UPDATE crew_runs SET protected = ?2 WHERE task_id = ?1",
                params![task_id, snapshot],
            )?;
            Ok(())
        })
    }

    /// Records which index entries hid a change from git when the task started.
    pub fn set_crew_index_flags(&self, task_id: &str, flags: &str) -> Result<()> {
        self.commit(|tx, _| {
            find_run(tx, task_id)?;
            tx.execute(
                "UPDATE crew_runs SET index_flags = ?2 WHERE task_id = ?1",
                params![task_id, flags],
            )?;
            Ok(())
        })
    }

    /// Claims the verification of a task that is `verifying`. True for the one caller that
    /// gets it; any other, at the same time or later, gets false and verifies nothing.
    pub fn claim_crew_verification(&self, task_id: &str) -> Result<bool> {
        self.commit(|tx, _| {
            let task = find_task(tx, task_id)?;
            if task.state != "verifying" {
                return Ok(false);
            }
            let claimed = tx.execute(
                "UPDATE crew_runs SET verifying_since = ?2
                 WHERE task_id = ?1 AND verifying_since IS NULL",
                params![task_id, now()],
            )?;
            Ok(claimed == 1)
        })
    }

    /// At a start no verification is alive, so any claim left over is from one that died.
    pub fn clear_crew_claims(&self) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute("UPDATE crew_runs SET verifying_since = NULL", [])?;
            Ok(())
        })
    }

    /// The turns a task has spent of its brief's budget: worker turns and reserved approvals.
    /// The budget stop and a result's usage read it.
    pub fn crew_turns_spent(&self, task_id: &str) -> Result<u64> {
        let run = self
            .crew_run(task_id)?
            .ok_or_else(|| CoreError::NotFound(format!("crew run {task_id}")))?;
        turns_spent(&self.reader(), &run)
    }

    /// The turns every task in a crew has spent, summed from the same per-task tally
    /// (`crew_turns_spent`), so the crew-wide budget and a task's own budget cannot disagree. A
    /// task whose run is not stored yet adds nothing.
    pub fn crew_turns_spent_in_crew(&self, crew_id: &str) -> Result<u64> {
        let conn = self.reader();
        let runs: Vec<CrewRun> = {
            let mut statement = conn.prepare(
                "SELECT r.* FROM crew_runs r JOIN crew_tasks t ON t.id = r.task_id
                 WHERE t.crew_id = ?1",
            )?;
            statement
                .query_map([crew_id], run_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut total: u64 = 0;
        for run in &runs {
            total = total.saturating_add(turns_spent(&conn, run)?);
        }
        Ok(total)
    }

    pub fn crew_run(&self, task_id: &str) -> Result<Option<CrewRun>> {
        Ok(self
            .reader()
            .query_row(
                "SELECT * FROM crew_runs WHERE task_id = ?1",
                [task_id],
                run_row,
            )
            .optional()?)
    }

    /// Whether a thread is a Crew lead or worker: its runs are judged by what they leave behind.
    pub fn is_crew_thread(&self, thread_id: &str) -> Result<bool> {
        Ok(self.reader().query_row(
            "SELECT EXISTS(SELECT 1 FROM crew_runs WHERE worker_thread_id = ?1 OR lead_thread_id = ?1)",
            [thread_id],
            |row| row.get(0),
        )?)
    }

    /// The task a thread works on, if it is a Crew worker.
    pub fn crew_task_for_worker(&self, thread_id: &str) -> Result<Option<(CrewTask, CrewRun)>> {
        let Some(run) = self
            .reader()
            .query_row(
                "SELECT * FROM crew_runs WHERE worker_thread_id = ?1",
                [thread_id],
                run_row,
            )
            .optional()?
        else {
            return Ok(None);
        };
        Ok(self.crew_task(&run.task_id)?.map(|task| (task, run)))
    }

    /// Records the thread that works the task, the checkout the verifier runs in and the
    /// commit the worker started from. Before the thread's first message is sent, so a
    /// report cannot arrive before it is known.
    pub fn set_crew_worker(
        &self,
        task_id: &str,
        thread_id: &str,
        worktree: &str,
        base_commit: Option<&str>,
    ) -> Result<()> {
        self.commit(|tx, _| {
            find_run(tx, task_id)?;
            // A task a person cancelled (or that ended) while the worker was being made gets no worker.
            if crate::crew::lifecycle::is_final(&find_task(tx, task_id)?.state) {
                return Err(CoreError::TaskEnded);
            }
            tx.execute(
                "UPDATE crew_runs SET worker_thread_id = ?2, worktree = ?3, base_commit = ?4
                 WHERE task_id = ?1",
                params![task_id, thread_id, worktree, base_commit],
            )?;
            Ok(())
        })
    }

    /// One lifecycle step. `InvalidTransition` when the table does not allow it.
    pub fn move_crew_task(
        &self,
        task_id: &str,
        to: &str,
        stop_reason: Option<&str>,
    ) -> Result<CrewTask> {
        self.commit(|tx, _| {
            let task = find_task(tx, task_id)?;
            step(tx, &task, to, stop_reason, &[])
        })
    }

    /// `verifying` to `passed`, with the ids of the Evidence this attempt made (H8).
    pub fn pass_crew_task(&self, task_id: &str, evidence: &[String]) -> Result<CrewTask> {
        self.commit(|tx, _| {
            let task = find_task(tx, task_id)?;
            step(tx, &task, "passed", None, evidence)
        })
    }

    /// The worker reported (or its turn ended with no report, `report` empty): the attempt
    /// counts, and the task goes `running` to `verifying`.
    pub fn begin_crew_verification(
        &self,
        task_id: &str,
        engine_run: &str,
        report: Option<&Report>,
    ) -> Result<CrewTask> {
        let stored = report
            .map(|r| checked(&CrewObject::Report(r.clone())).map(|body| (r, body)))
            .transpose()?;
        self.commit(|tx, _| {
            let task = find_task(tx, task_id)?;
            let moved = step(tx, &task, "verifying", None, &[])?;
            if let Some((report, body)) = &stored {
                insert_object(tx, task_id, &CrewObject::Report((*report).clone()), body)?;
            }
            tx.execute(
                "UPDATE crew_runs SET attempts = attempts + 1, last_report_run = ?2
                 WHERE task_id = ?1",
                params![task_id, engine_run],
            )?;
            Ok(moved)
        })
    }

    /// A failed verification: the guard counts it and names the next state, and the task goes
    /// through `failed` to that state in one transaction, so `failed` is never seen at rest.
    pub fn fail_crew_verification(
        &self,
        task_id: &str,
        fingerprint: &str,
    ) -> Result<(CrewTask, Next)> {
        self.commit(|tx, _| {
            let task = find_task(tx, task_id)?;
            let run = find_run(tx, task_id)?;
            let failed = step_transient(tx, &task, "failed", None, &[])?;
            let (guard, next) = run.guard.after_failure(fingerprint);
            tx.execute(
                "UPDATE crew_runs SET failures = failures + 1, streak = ?2, fingerprint = ?3,
                 approach_changed = ?4 WHERE task_id = ?1",
                params![
                    task_id,
                    guard.streak,
                    guard.fingerprint,
                    i64::from(guard.approach_changed)
                ],
            )?;
            let reason = (next == Next::Stop).then_some("repeated_failure");
            let moved = step(tx, &failed, next.state(), reason, &[])?;
            Ok((moved, next))
        })
    }

    /// Every lease of every task in a crew, oldest first, with the task each belongs to.
    pub fn crew_leases_in_crew(&self, crew_id: &str) -> Result<Vec<(String, Lease)>> {
        let rows = {
            let conn = self.reader();
            let mut statement = conn.prepare(
                "SELECT l.task_id, l.body FROM crew_leases l
                 JOIN crew_tasks t ON t.id = l.task_id
                 WHERE t.crew_id = ?1 ORDER BY l.rowid",
            )?;
            statement
                .query_map([crew_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        rows.into_iter()
            .map(|(task, body)| match super::crew::read_object(&body)? {
                CrewObject::Lease(lease) => Ok((task, lease)),
                other => Err(CoreError::Internal(format!(
                    "crew_leases holds a {}",
                    other.type_name()
                ))),
            })
            .collect()
    }

    /// Tasks a worker is on or that are being verified, for a restart to pick up.
    pub fn crew_tasks_in_flight(&self) -> Result<Vec<(CrewTask, CrewRun)>> {
        let ids: Vec<String> = {
            let conn = self.reader();
            let mut statement = conn.prepare(
                "SELECT t.id FROM crew_tasks t JOIN crew_runs r ON r.task_id = t.id
                 WHERE t.state IN ('running', 'verifying', 'changed_approach')",
            )?;
            statement
                .query_map([], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        let mut found = Vec::new();
        for id in ids {
            if let (Some(task), Some(run)) = (self.crew_task(&id)?, self.crew_run(&id)?) {
                found.push((task, run));
            }
        }
        Ok(found)
    }
}
