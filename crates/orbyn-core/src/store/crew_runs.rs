//! How a Crew task runs: who works on it, where, and the loop guard's counters. The task's
//! state moves only along the lifecycle of SPEC.md section 6 (H13), checked inside the same
//! transaction that writes it, so two callers cannot both win a move.

use orbyn_contracts::{CrewObject, Lease, Report};
use rusqlite::{OptionalExtension, Row, Transaction, params};

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
    })
}

fn find_run(tx: &Transaction<'_>, task_id: &str) -> Result<CrewRun> {
    tx.query_row(
        "SELECT * FROM crew_runs WHERE task_id = ?1",
        [task_id],
        run_row,
    )
    .optional()?
    .ok_or_else(|| CoreError::NotFound(format!("crew run {task_id}")))
}

/// Moves a task one step, or refuses with `InvalidTransition`. `stopped` names its reason and
/// nothing else does; a task passes only with Evidence (H8).
fn step(
    tx: &Transaction<'_>,
    task: &CrewTask,
    to: &str,
    stop_reason: Option<&str>,
    evidence: &[String],
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
    tx.execute(
        "UPDATE crew_tasks SET state = ?2, stop_reason = ?3, updated_at = ?4 WHERE id = ?1",
        params![task.id, to, stop_reason, now()],
    )?;
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
            let failed = step(tx, &task, "failed", None, &[])?;
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
