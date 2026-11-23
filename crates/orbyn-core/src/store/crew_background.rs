//! Background crews at rest (docs/crew.md, "Background crews"): the crew the person defined, and its runs with what each spent against
//! its budget. The caller validates and redacts; this layer keeps the rules that must hold whatever the caller does: an audit row is
//! written in the same transaction as every change (rule 6), the budget never changes after creation, at most one run per crew is
//! running, and a run that reaches its budget stops with the reason `budget` and waits for the person's review. The person
//! settles a waiting review by acknowledging it: the wait clears, and the run keeps its reason.

use rusqlite::{OptionalExtension, Row, Transaction, params};
use serde_json::{Value, json};

use super::{AuditRecord, CrewTask, Store, insert_audit};
use crate::error::{CoreError, Result};
use crate::util::{new_id, now};

/// The most background crews one project may have, so the list stays readable and a loop cannot fill the table.
pub const PER_PROJECT_MAX: u32 = 50;
/// The most watches one project may have: a watch runs a command on a timer, so a project gets a small, fixed handful.
pub const WATCHES_PER_PROJECT_MAX: u32 = 10;

/// One background crew as stored.
#[derive(Clone, Debug, PartialEq)]
pub struct BackgroundRow {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub template: String,
    pub trigger_kind: String,
    pub trigger_settings: Value,
    pub budget_turns: u32,
    pub budget_minutes: u32,
    /// What each engine may spend on the crew in a day; 0 and 0 is a crew from before the daily budget.
    pub daily_runs: u32,
    pub daily_turns: u32,
    /// The person's clock in minutes east of UTC, so the daily budget resets at their midnight.
    pub utc_offset_minutes: i32,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// What a new background crew is made of. The text has been redacted.
pub struct NewBackground<'a> {
    pub project_id: &'a str,
    pub name: &'a str,
    pub template: &'a str,
    pub trigger_kind: &'a str,
    pub trigger_settings: &'a Value,
    pub budget_turns: u32,
    pub budget_minutes: u32,
    pub daily_runs: u32,
    pub daily_turns: u32,
    pub utc_offset_minutes: i32,
}

/// One run as stored.
#[derive(Clone, Debug, PartialEq)]
pub struct BackgroundRunRow {
    pub id: String,
    pub background_id: String,
    pub state: String,
    pub stop_reason: Option<String>,
    /// Why the run never started (`disk_low`), when it is a start a guard refused rather than a run that stopped.
    pub reason_code: Option<String>,
    pub turns_spent: u32,
    pub minutes_spent: u32,
    pub budget_turns: u32,
    pub budget_minutes: u32,
    pub review_waiting: bool,
    pub review_reason: Option<String>,
    pub started_at: String,
    pub updated_at: String,
    pub ended_at: Option<String>,
}

/// One run that waits for the person, with the crew it belongs to: the Review queue joins a run to the name and template of
/// its crew, which are stored on the crew row, not the run.
#[derive(Clone, Debug, PartialEq)]
pub struct BackgroundReviewRow {
    pub run: BackgroundRunRow,
    pub crew_name: String,
    pub project_id: String,
    pub template: String,
}

/// How a run ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunEnd<'a> {
    /// Stopped for a reason, with no review: the run left nothing to look at.
    Stopped(&'a str),
    /// Finished with work for the person to look at.
    NeedsReview(&'a str),
}

fn row(row: &Row<'_>) -> rusqlite::Result<BackgroundRow> {
    let settings: String = row.get("trigger_settings")?;
    Ok(BackgroundRow {
        id: row.get("id")?,
        project_id: row.get("project_id")?,
        name: row.get("name")?,
        template: row.get("template")?,
        trigger_kind: row.get("trigger_kind")?,
        trigger_settings: serde_json::from_str(&settings).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        budget_turns: row.get("budget_turns")?,
        budget_minutes: row.get("budget_minutes")?,
        daily_runs: row.get("daily_runs")?,
        daily_turns: row.get("daily_turns")?,
        utc_offset_minutes: row.get("utc_offset_minutes")?,
        enabled: row.get::<_, i64>("enabled")? == 1,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn run_row(row: &Row<'_>) -> rusqlite::Result<BackgroundRunRow> {
    Ok(BackgroundRunRow {
        id: row.get("id")?,
        background_id: row.get("background_id")?,
        state: row.get("state")?,
        stop_reason: row.get("stop_reason")?,
        reason_code: row.get("reason_code")?,
        turns_spent: row.get("turns_spent")?,
        minutes_spent: row.get("minutes_spent")?,
        budget_turns: row.get("budget_turns")?,
        budget_minutes: row.get("budget_minutes")?,
        review_waiting: row.get::<_, i64>("review_waiting")? == 1,
        review_reason: row.get("review_reason")?,
        started_at: row.get("started_at")?,
        updated_at: row.get("updated_at")?,
        ended_at: row.get("ended_at")?,
    })
}

fn audit(
    tx: &Transaction<'_>,
    actor: &str,
    action: &str,
    target: &str,
    detail: Value,
) -> Result<()> {
    insert_audit(
        tx,
        &AuditRecord {
            actor: actor.to_owned(),
            action: action.to_owned(),
            target: target.to_owned(),
            decision: None,
            source: if actor == "user" { "user" } else { "core" }.to_owned(),
            thread_id: None,
            run_id: None,
            detail,
        },
    )?;
    Ok(())
}

fn crew_in(tx: &Transaction<'_>, id: &str) -> Result<BackgroundRow> {
    tx.query_row("SELECT * FROM crew_background WHERE id = ?1", [id], row)
        .optional()?
        .ok_or_else(|| CoreError::NotFound("background crew".into()))
}

fn daily_in(tx: &Transaction<'_>, crew: &str, day: &str, engine: &str) -> Result<(u32, u32)> {
    Ok(tx.query_row(
        "SELECT COUNT(*), COALESCE(SUM(turns_spent), 0) FROM crew_background_daily
         WHERE background_id = ?1 AND day = ?2 AND engine = ?3",
        params![crew, day, engine],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?)
}

fn running_in(tx: &Transaction<'_>, crew: &str) -> Result<Option<BackgroundRunRow>> {
    Ok(tx
        .query_row(
            "SELECT * FROM crew_background_runs WHERE background_id = ?1 AND state = 'running'",
            [crew],
            run_row,
        )
        .optional()?)
}

/// How many watches one project has, read inside the transaction that would add another.
fn watches_in(tx: &Transaction<'_>, project: &str) -> Result<u32> {
    Ok(tx.query_row(
        "SELECT COUNT(*) FROM crew_background WHERE project_id = ?1 AND trigger_kind = 'watch'",
        [project],
        |row| row.get(0),
    )?)
}

/// Whether `name` is taken by another crew of the project: compared without regard to case for any script, which the unique index
/// does for ASCII only. A project holds at most `PER_PROJECT_MAX` crews, so reading their names is cheap.
fn name_taken(
    tx: &Transaction<'_>,
    project: &str,
    name: &str,
    except: Option<&str>,
) -> Result<bool> {
    let folded = name.to_lowercase();
    let mut statement = tx.prepare("SELECT id, name FROM crew_background WHERE project_id = ?1")?;
    let names = statement
        .query_map([project], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(names
        .iter()
        .any(|(id, other)| Some(id.as_str()) != except && other.to_lowercase() == folded))
}

/// What `Store::unwind_background_run` did.
#[derive(Debug, Default)]
pub struct Unwound {
    /// The run and its lead thread were taken back: no task existed.
    pub discarded: bool,
    /// Every task of the retained run that now waits for a person: the ones moved here and the ones already waiting (as they stood
    /// before the move). The caller releases their leases and gives each a result if it has none.
    pub held: Vec<CrewTask>,
    deleted_threads: Vec<String>,
}

/// What `Store::recover_background_starts` did.
#[derive(Debug, Default)]
pub(crate) struct Recovered {
    /// How many runs were ended.
    pub runs: usize,
    /// The tasks of those runs that now wait for a person, each with the reason the run was ended for.
    pub held: Vec<(CrewTask, &'static str)>,
}

/// Deletes a run's lead thread, in the caller's transaction. Returns the deleted thread ids; the caller
/// removes them from the hub after the commit.
fn discard_lead_in(tx: &Transaction<'_>, thread_id: &str) -> Result<Vec<String>> {
    let present: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM threads WHERE id = ?1)",
        [thread_id],
        |row| row.get(0),
    )?;
    if present {
        super::delete_thread_in(tx, thread_id, "crew")
    } else {
        Ok(Vec::new())
    }
}

/// Holds every task of the run's crew for a person and ends the run `needs_review` with `why`, inside the caller's transaction.
/// `None` when the crew has no task at all (nothing was held and the run is untouched). Shared by every path that must retain a
/// run whose start did not finish: the task hold and the run end commit together or not at all. `actor` is written on the run's end
/// audit. Returns the tasks that now wait for a person: the ones moved here and the ones already waiting.
pub(crate) fn hold_tasks_and_end_run(
    tx: &Transaction<'_>,
    run: &BackgroundRunRow,
    why: &str,
    actor: &str,
) -> Result<Option<Vec<CrewTask>>> {
    let tasks = {
        let mut statement = tx.prepare("SELECT * FROM crew_tasks WHERE crew_id = ?1")?;
        statement
            .query_map([&run.id], super::crew::task_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    if tasks.is_empty() {
        return Ok(None);
    }
    let mut held = Vec::new();
    for task in tasks {
        if crate::crew::lifecycle::allowed(&task.state, "needs_person") {
            super::crew_runs::step(tx, &task, "needs_person", None, &[])?;
            held.push(task);
        } else if task.state == "needs_person" {
            held.push(task);
        }
    }
    end_run(tx, run, RunEnd::NeedsReview(why), Some(why), actor)?;
    Ok(Some(held))
}

/// Ends a run in the transaction, with the audit row first. The caller has checked that it is running. When this stop ends a spend, the
/// run has spent: `run` carries the totals, written in the same statement as the state.
fn end_run(
    tx: &Transaction<'_>,
    run: &BackgroundRunRow,
    end: RunEnd<'_>,
    review: Option<&str>,
    actor: &str,
) -> Result<()> {
    let (state, reason) = match end {
        RunEnd::Stopped(reason) => ("stopped", Some(reason)),
        RunEnd::NeedsReview(_) => ("needs_review", None),
    };
    audit(
        tx,
        actor,
        if state == "stopped" {
            "crew.background.run.stop"
        } else {
            "crew.background.run.review"
        },
        &run.background_id,
        json!({
            "run": run.id,
            "state": state,
            "reason": reason,
            "turns_spent": run.turns_spent,
            "minutes_spent": run.minutes_spent,
            "review": review.is_some(),
        }),
    )?;
    let stamp = now();
    tx.execute(
        "UPDATE crew_background_runs SET state = ?2, stop_reason = ?3, review_waiting = ?4, review_reason = ?5,
             turns_spent = ?7, minutes_spent = ?8, updated_at = ?6, ended_at = ?6 WHERE id = ?1 AND state = 'running'",
        params![
            run.id,
            state,
            reason,
            i64::from(review.is_some()),
            review,
            stamp,
            run.turns_spent,
            run.minutes_spent
        ],
    )?;
    Ok(())
}

impl Store {
    /// Adds a background crew, off. Audited in the same transaction.
    pub fn create_background_crew(
        &self,
        new: &NewBackground<'_>,
        actor: &str,
    ) -> Result<BackgroundRow> {
        let id = new_id();
        let stamp = now();
        let settings = serde_json::to_string(new.trigger_settings)?;
        self.commit(|tx, _| {
            let held: u32 = tx.query_row(
                "SELECT COUNT(*) FROM crew_background WHERE project_id = ?1",
                [new.project_id],
                |row| row.get(0),
            )?;
            if held >= PER_PROJECT_MAX {
                return Err(CoreError::Invalid(format!(
                    "a project has at most {PER_PROJECT_MAX} background crews"
                )));
            }
            if new.trigger_kind == "watch" && watches_in(tx, new.project_id)? >= WATCHES_PER_PROJECT_MAX
            {
                return Err(CoreError::Invalid(format!(
                    "A project can have {WATCHES_PER_PROJECT_MAX} watches."
                )));
            }
            if name_taken(tx, new.project_id, new.name, None)? {
                return Err(CoreError::Invalid(
                    "this project already has a background crew with that name".into(),
                ));
            }
            audit(
                tx,
                actor,
                "crew.background.create",
                &id,
                json!({
                    "project": new.project_id,
                    "name": new.name,
                    "template": new.template,
                    "trigger": new.trigger_kind,
                    "budget_turns": new.budget_turns,
                    "budget_minutes": new.budget_minutes,
                    "daily_runs": new.daily_runs,
                    "daily_turns": new.daily_turns,
                    "utc_offset_minutes": new.utc_offset_minutes,
                }),
            )?;
            tx.execute(
                "INSERT INTO crew_background
                 (id, project_id, name, template, trigger_kind, trigger_settings, budget_turns, budget_minutes,
                  daily_runs, daily_turns, utc_offset_minutes, enabled, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0, ?12, ?12)",
                params![
                    id,
                    new.project_id,
                    new.name,
                    new.template,
                    new.trigger_kind,
                    settings,
                    new.budget_turns,
                    new.budget_minutes,
                    new.daily_runs,
                    new.daily_turns,
                    new.utc_offset_minutes,
                    stamp
                ],
            )?;
            crew_in(tx, &id)
        })
    }

    /// Changes the name and, or, the trigger. Nothing else of a crew can change here. Audited in the same transaction.
    pub fn update_background_crew(
        &self,
        id: &str,
        name: Option<&str>,
        trigger: Option<(&str, &Value)>,
        actor: &str,
    ) -> Result<BackgroundRow> {
        self.commit(|tx, _| {
            let current = crew_in(tx, id)?;
            if let Some(name) = name
                && name_taken(tx, &current.project_id, name, Some(id))?
            {
                return Err(CoreError::Invalid(
                    "this project already has a background crew with that name".into(),
                ));
            }
            let (kind, settings) = match trigger {
                Some((kind, settings)) => (kind.to_owned(), serde_json::to_string(settings)?),
                None => (
                    current.trigger_kind.clone(),
                    serde_json::to_string(&current.trigger_settings)?,
                ),
            };
            // A trigger that turns a crew into a watch takes one of the project's ten places, checked in the same transaction.
            if kind == "watch"
                && current.trigger_kind != "watch"
                && watches_in(tx, &current.project_id)? >= WATCHES_PER_PROJECT_MAX
            {
                return Err(CoreError::Invalid(format!(
                    "A project can have {WATCHES_PER_PROJECT_MAX} watches."
                )));
            }
            audit(
                tx,
                actor,
                "crew.background.update",
                id,
                json!({
                    "name": name,
                    "trigger": trigger.map(|(kind, _)| kind),
                    "was_enabled": current.enabled,
                }),
            )?;
            let name = name.unwrap_or(&current.name);
            tx.execute(
                "UPDATE crew_background SET name = ?2, trigger_kind = ?3, trigger_settings = ?4, updated_at = ?5 WHERE id = ?1",
                params![id, name, kind, settings, now()],
            )?;
            crew_in(tx, id)
        })
    }

    /// Turns a crew on or off. Turning it off stops a run in progress with the reason `disabled`, its stop row written as `actor` (the person who turned it off).
    /// Audited in the same transaction.
    pub fn set_background_enabled(
        &self,
        id: &str,
        enabled: bool,
        actor: &str,
    ) -> Result<BackgroundRow> {
        self.commit(|tx, _| {
            let current = crew_in(tx, id)?;
            audit(
                tx,
                actor,
                if enabled {
                    "crew.background.enable"
                } else {
                    "crew.background.disable"
                },
                id,
                json!({ "name": current.name, "was_enabled": current.enabled }),
            )?;
            if !enabled && let Some(run) = running_in(tx, id)? {
                end_run(tx, &run, RunEnd::Stopped("disabled"), None, actor)?;
            }
            tx.execute(
                "UPDATE crew_background SET enabled = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, i64::from(enabled), now()],
            )?;
            crew_in(tx, id)
        })
    }

    /// Deletes a crew and its runs. Refused only while a run is in progress. Waiting reviews go with the crew; the audit row stays and
    /// records how many runs, and how many waiting reviews, the delete removed.
    pub fn delete_background_crew(&self, id: &str, actor: &str) -> Result<()> {
        self.commit(|tx, _| {
            let current = crew_in(tx, id)?;
            if running_in(tx, id)?.is_some() {
                return Err(CoreError::RunsActive);
            }
            // A run that ended while its task still runs (the crew was turned off) is not gone: the worker is still spending.
            let live: u32 = tx.query_row(
                "SELECT COUNT(*) FROM crew_tasks WHERE state NOT IN ('passed', 'stopped', 'cancelled')
                   AND crew_id IN (SELECT id FROM crew_background_runs WHERE background_id = ?1)",
                [id],
                |row| row.get(0),
            )?;
            if live > 0 {
                return Err(CoreError::RunsActive);
            }
            let (runs, waiting): (u32, u32) = tx.query_row(
                "SELECT COUNT(*), COALESCE(SUM(review_waiting), 0) FROM crew_background_runs WHERE background_id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            audit(
                tx,
                actor,
                "crew.background.delete",
                id,
                json!({
                    "name": current.name,
                    "project": current.project_id,
                    "runs_removed": runs,
                    "reviews_waiting_removed": waiting,
                }),
            )?;
            tx.execute("DELETE FROM crew_background WHERE id = ?1", [id])?;
            Ok(())
        })
    }

    pub fn background_crew(&self, id: &str) -> Result<Option<BackgroundRow>> {
        Ok(self
            .reader()
            .query_row("SELECT * FROM crew_background WHERE id = ?1", [id], row)
            .optional()?)
    }

    /// One page of crews, by name, and how many there are. `project` narrows it to one project.
    pub fn background_crews_page(
        &self,
        project: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<(Vec<BackgroundRow>, u32)> {
        let conn = self.reader();
        let total: u32 = conn.query_row(
            "SELECT COUNT(*) FROM crew_background WHERE (?1 IS NULL OR project_id = ?1)",
            [project],
            |row| row.get(0),
        )?;
        let mut statement = conn.prepare(
            "SELECT * FROM crew_background WHERE (?1 IS NULL OR project_id = ?1)
             ORDER BY name COLLATE NOCASE, id LIMIT ?2 OFFSET ?3",
        )?;
        let rows = statement
            .query_map(params![project, limit, offset], row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok((rows, total))
    }

    /// One page of a crew's runs, newest first, and how many there are.
    pub fn background_runs_page(
        &self,
        crew: &str,
        limit: u32,
        offset: u32,
    ) -> Result<(Vec<BackgroundRunRow>, u32)> {
        let conn = self.reader();
        let total: u32 = conn.query_row(
            "SELECT COUNT(*) FROM crew_background_runs WHERE background_id = ?1",
            [crew],
            |row| row.get(0),
        )?;
        let mut statement = conn.prepare(
            "SELECT * FROM crew_background_runs WHERE background_id = ?1
             ORDER BY started_at DESC, rowid DESC LIMIT ?2 OFFSET ?3",
        )?;
        let rows = statement
            .query_map(params![crew, limit, offset], run_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok((rows, total))
    }

    /// The runs that wait for the person, oldest first, each with the crew it belongs to: every run whose review is waiting
    /// (`review_waiting`, whether it finished with work, stopped for its budget or waited on a person), and a run that stopped for
    /// a failure, which has nothing to review but still tells the person what happened. A review the person acknowledged clears
    /// `review_waiting` and drops the run from this list, while the run keeps why it waited. A run stopped by the person's own
    /// switch (`disabled`) needs nothing from them, and a running run is not waiting.
    pub fn background_reviews(&self) -> Result<Vec<BackgroundReviewRow>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT r.*, c.name AS crew_name, c.project_id AS crew_project, c.template AS crew_template
             FROM crew_background_runs r JOIN crew_background c ON c.id = r.background_id
             WHERE r.review_waiting = 1
                OR (r.state = 'stopped' AND COALESCE(r.stop_reason, '') IN ('repeated_failure', 'engine_error'))
             ORDER BY r.started_at ASC, r.rowid ASC",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok(BackgroundReviewRow {
                    run: run_row(row)?,
                    crew_name: row.get("crew_name")?,
                    project_id: row.get("crew_project")?,
                    template: row.get("crew_template")?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// The newest run of a crew, and how many of its runs wait for review.
    pub fn background_standing(&self, crew: &str) -> Result<(Option<BackgroundRunRow>, u32)> {
        let conn = self.reader();
        let newest = conn
            .query_row(
                "SELECT * FROM crew_background_runs WHERE background_id = ?1
                 ORDER BY started_at DESC, rowid DESC LIMIT 1",
                [crew],
                run_row,
            )
            .optional()?;
        let waiting: u32 = conn.query_row(
            "SELECT COUNT(*) FROM crew_background_runs WHERE background_id = ?1 AND review_waiting = 1",
            [crew],
            |row| row.get(0),
        )?;
        Ok((newest, waiting))
    }

    /// Opens a run for an enabled crew, with the crew's budget. A crew that is off, or already running, is refused. The trigger
    /// that calls this is slice 4h.
    pub fn start_background_run(&self, crew: &str) -> Result<BackgroundRunRow> {
        self.start_background_run_as(crew, &new_id(), "crew")
    }

    /// `start_background_run` with the run's id given. A run that has a lead thread of its own takes that thread's id, so the run, the
    /// thread and the Crew tasks of the thread (whose crew id is the thread's id) find each other with no extra column.
    pub fn start_background_run_as(
        &self,
        crew: &str,
        id: &str,
        actor: &str,
    ) -> Result<BackgroundRunRow> {
        let id = id.to_owned();
        self.commit(|tx, _| {
            let current = crew_in(tx, crew)?;
            if !current.enabled {
                return Err(CoreError::PolicyDenied(
                    "this background crew is off".into(),
                ));
            }
            if running_in(tx, crew)?.is_some() {
                return Err(CoreError::RunsActive);
            }
            audit(
                tx,
                actor,
                "crew.background.run.start",
                crew,
                json!({
                    "run": id,
                    "budget_turns": current.budget_turns,
                    "budget_minutes": current.budget_minutes,
                }),
            )?;
            let stamp = now();
            tx.execute(
                "INSERT INTO crew_background_runs (id, background_id, state, budget_turns, budget_minutes, started_at, updated_at)
                 VALUES (?1, ?2, 'running', ?3, ?4, ?5, ?5)",
                params![id, crew, current.budget_turns, current.budget_minutes, stamp],
            )?;
            Ok(tx.query_row(
                "SELECT * FROM crew_background_runs WHERE id = ?1",
                [&id],
                run_row,
            )?)
        })
    }

    /// Adds what a running run spent. Reaching either number of its budget stops it with the reason `budget` and puts it in the review
    /// queue. A spend writes its audit row (and a stop its own) before the run row changes, in the same transaction. A run that is not running is refused.
    pub fn add_background_spend(
        &self,
        run: &str,
        turns: u32,
        minutes: u32,
    ) -> Result<BackgroundRunRow> {
        self.commit(|tx, _| {
            let current = tx
                .query_row(
                    "SELECT * FROM crew_background_runs WHERE id = ?1",
                    [run],
                    run_row,
                )
                .optional()?
                .ok_or_else(|| CoreError::NotFound("background run".into()))?;
            if current.state != "running" {
                return Err(CoreError::RunClosed);
            }
            let turns_spent = current.turns_spent.saturating_add(turns);
            let minutes_spent = current.minutes_spent.saturating_add(minutes);
            // The spend's audit row comes before the run row changes; a stop it causes writes its own audit row before the one update
            // that records both the spend and the stop.
            audit(
                tx,
                "crew",
                "crew.background.run.spend",
                &current.background_id,
                json!({
                    "run": run,
                    "turns": turns,
                    "minutes": minutes,
                    "turns_spent": turns_spent,
                    "minutes_spent": minutes_spent,
                }),
            )?;
            let spent = BackgroundRunRow {
                turns_spent,
                minutes_spent,
                ..current
            };
            if turns_spent >= spent.budget_turns || minutes_spent >= spent.budget_minutes {
                end_run(
                    tx,
                    &spent,
                    RunEnd::Stopped("budget"),
                    Some("The run reached its budget and stopped."),
                    "crew",
                )?;
            } else {
                tx.execute(
                    "UPDATE crew_background_runs SET turns_spent = ?2, minutes_spent = ?3, updated_at = ?4 WHERE id = ?1",
                    params![run, turns_spent, minutes_spent, now()],
                )?;
            }
            Ok(tx.query_row(
                "SELECT * FROM crew_background_runs WHERE id = ?1",
                [run],
                run_row,
            )?)
        })
    }

    /// The background crews of one trigger kind, by name. The listeners' observation set; a kind nothing can start
    /// (issue labels, dependency updates, watches) has no listener and is never listed by one.
    pub fn background_crews_of_kind(&self, kind: &str) -> Result<Vec<BackgroundRow>> {
        let conn = self.reader();
        let mut statement =
            conn.prepare("SELECT * FROM crew_background WHERE trigger_kind = ?1 ORDER BY name COLLATE NOCASE, id")?;
        let rows = statement
            .query_map([kind], row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// First-write-wins claim of one background event: true when this firing is new, false when the same event has
    /// already fired. The row is never removed, so a repeat of the same event starts nothing new, and a firing that
    /// was refused keeps its row: nothing is retried on a timer (docs/crew/background-engine-start.md, section 2).
    pub fn claim_background_event(&self, identity: &str) -> Result<bool> {
        self.commit(|tx, _| {
            let n = tx.execute(
                "INSERT OR IGNORE INTO crew_background_events (identity, claimed_unix) VALUES (?1, ?2)",
                params![identity, now()],
            )?;
            Ok(n == 1)
        })
    }

    /// The newest schedule bucket this crew has fired (claimed), or none. A bucket at or before it never fires again, so a clock
    /// that moves back cannot re-fire a bucket the crew already had, and a bucket that was only claimed counts as fired.
    pub fn latest_schedule_bucket(&self, crew: &str) -> Result<Option<i64>> {
        let low = format!("schedule:{crew}:");
        // ':' is followed by ';' in byte order, so this range is every identity that starts with `low`.
        let high = format!("schedule:{crew};");
        let conn = self.reader();
        Ok(conn.query_row(
            "SELECT MAX(CAST(substr(identity, ?3) AS INTEGER)) FROM crew_background_events WHERE identity >= ?1 AND identity < ?2",
            params![low, high, low.chars().count() as i64 + 1],
            |row| row.get::<_, Option<i64>>(0),
        )?)
    }

    /// A listener firing the start refused, as one audit row beside the refusal: which listener fired and a fixed category of the
    /// refusal, never the refusal's text and never any event text. `kind` is `schedule` or `ci`, and `reason` is one of the
    /// listener's own words.
    pub fn audit_background_listener_refusal(
        &self,
        crew: &str,
        kind: &str,
        reason: &str,
    ) -> Result<()> {
        self.commit(|tx, _| {
            audit(
                tx,
                "crew",
                "crew.background.listener.refused",
                crew,
                json!({ "listener": kind, "reason": reason }),
            )
        })
    }

    /// One run by id.
    pub fn background_run(&self, id: &str) -> Result<Option<BackgroundRunRow>> {
        Ok(self
            .reader()
            .query_row(
                "SELECT * FROM crew_background_runs WHERE id = ?1",
                [id],
                run_row,
            )
            .optional()?)
    }

    /// Settles the background runs a crash left `running`. Call it only from `Core::open`, after the data-directory owner lock is
    /// taken and before any starter or listener exists, so no live starter can still be working. A run whose task has a worker in
    /// `running`, `verifying` or `changed_approach` is left to Crew task recovery, which ends it through the task's own end (with
    /// Crew off at startup such a run stays `running`). Every other running run is ended `needs_review` in one transaction with its
    /// audit: a run with no task; a run whose task is held for a person (`briefed`, `routed`, and worker-less `running` or
    /// `verifying` move to `needs_person`, compare-and-set, no engine; a worker-less `changed_approach`, `failed` or
    /// already waiting task stays as it is); and a run whose task already ended. Nothing is brought back and no engine work starts.
    /// Terminal runs never change and a repeat finds nothing. The held tasks come back with the reason, for the caller to release
    /// their leases and give each a result once a `Core` exists. A failing transaction fails `Core::open`: startup is fail-closed,
    /// as `Store::recover` is.
    pub(crate) fn recover_background_starts(&self) -> Result<Recovered> {
        const NO_TASK: &str = "The background run was interrupted before its task was created. Start another run when you are ready.";
        const HELD: &str =
            "The app stopped before this run's worker started. Its task waits for a person.";
        const ENDED: &str = "The run's task ended, but the app stopped before the run was closed.";
        const LEFT: &str =
            "The app stopped before this run was closed. Its task is left as it was.";
        self.commit(|tx, _| {
            let mut held_all = Vec::new();
            let runs = {
                let mut statement = tx.prepare(
                    "SELECT * FROM crew_background_runs b WHERE b.state = 'running'
                     AND NOT EXISTS (SELECT 1 FROM crew_tasks t JOIN crew_runs r ON r.task_id = t.id
                                     WHERE t.crew_id = b.id AND t.state IN ('running', 'verifying', 'changed_approach')
                                     AND r.worker_thread_id IS NOT NULL)",
                )?;
                statement
                    .query_map([], run_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?
            };
            for run in &runs {
                let tasks = {
                    let mut statement = tx.prepare("SELECT * FROM crew_tasks WHERE crew_id = ?1")?;
                    statement
                        .query_map([&run.id], super::crew::task_row)?
                        .collect::<rusqlite::Result<Vec<_>>>()?
                };
                let why = if tasks.is_empty() {
                    NO_TASK
                } else if tasks
                    .iter()
                    .any(|task| crate::crew::lifecycle::allowed(&task.state, "needs_person"))
                {
                    HELD
                } else if tasks
                    .iter()
                    .all(|task| crate::crew::lifecycle::is_final(&task.state))
                {
                    ENDED
                } else {
                    LEFT
                };
                audit(
                    tx,
                    "core",
                    "crew.background.run.recover",
                    &run.background_id,
                    json!({
                        "run": run.id,
                        "why": why,
                        "tasks": tasks.iter().map(|task| json!({ "id": task.id, "state": task.state })).collect::<Vec<_>>(),
                    }),
                )?;
                match hold_tasks_and_end_run(tx, run, why, "core")? {
                    Some(held) => held_all.extend(held.into_iter().map(|task| (task, why))),
                    None => end_run(tx, run, RunEnd::NeedsReview(why), Some(why), "core")?,
                }
            }
            Ok(Recovered {
                runs: runs.len(),
                held: held_all,
            })
        })
    }

    /// Whether `thread_id` is the worker of a task whose lead thread is a background run's (a run's id is its lead thread's id).
    pub fn is_background_worker(&self, thread_id: &str) -> Result<bool> {
        Ok(self.reader().query_row(
            "SELECT EXISTS(SELECT 1 FROM crew_runs r JOIN crew_background_runs b ON b.id = r.lead_thread_id
             WHERE r.worker_thread_id = ?1)",
            [thread_id],
            |row| row.get(0),
        )?)
    }

    /// How many turns a task's worker has had: the runs of its worker thread. A task with no worker has had none.
    pub fn crew_worker_turns(&self, task_id: &str) -> Result<u32> {
        Ok(self
            .reader()
            .query_row(
                "SELECT COUNT(*) FROM runs WHERE thread_id = (SELECT worker_thread_id FROM crew_runs WHERE task_id = ?1)",
                [task_id],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }

    /// Settles a refused start in one commit. With no task yet, the run and its lead thread are all taken
    /// back, so a crash leaves nothing half done. Once a task exists the run is retained for review instead: the tasks are held for
    /// a person and the run ends `needs_review` (`hold_tasks_and_end_run`). The refusal audit is in the same commit either way.
    /// Repeating this is harmless: a missing or already ended run changes nothing.
    pub fn unwind_background_run(&self, run: &str, why: &str) -> Result<Unwound> {
        let unwound = self.commit(|tx, _| {
            let Some(current) = tx
                .query_row(
                    "SELECT * FROM crew_background_runs WHERE id = ?1",
                    [run],
                    run_row,
                )
                .optional()?
            else {
                return Ok(Unwound::default());
            };
            if current.state != "running" {
                return Ok(Unwound::default());
            }
            audit(
                tx,
                "crew",
                "crew.background.run.refused",
                &current.background_id,
                json!({ "run": run, "why": why }),
            )?;
            if let Some(held) = hold_tasks_and_end_run(tx, &current, why, "crew")? {
                return Ok(Unwound {
                    discarded: false,
                    held,
                    deleted_threads: Vec::new(),
                });
            }
            tx.execute("DELETE FROM crew_background_runs WHERE id = ?1", [run])?;
            let deleted_threads = discard_lead_in(tx, run)?;
            Ok(Unwound {
                discarded: true,
                held: Vec::new(),
                deleted_threads,
            })
        })?;
        for id in &unwound.deleted_threads {
            self.hub.remove(id);
        }
        Ok(unwound)
    }

    /// Takes back the lead thread of a start that failed before its run row existed, in one commit. A thread that has a
    /// run or a task is left alone, and so is one that is already gone, so repeating this is harmless.
    pub fn discard_unopened_background_lead(&self, thread_id: &str) -> Result<()> {
        let deleted = self.commit(|tx, _| {
            let used: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM crew_background_runs WHERE id = ?1)
                    OR EXISTS(SELECT 1 FROM crew_tasks WHERE crew_id = ?1)",
                [thread_id],
                |row| row.get(0),
            )?;
            let present: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM threads WHERE id = ?1)",
                [thread_id],
                |row| row.get(0),
            )?;
            if used || !present {
                return Ok(Vec::new());
            }
            discard_lead_in(tx, thread_id)
        })?;
        for id in &deleted {
            self.hub.remove(id);
        }
        Ok(())
    }

    /// Ends a run whose start a guard refused before any task existed, keeping its row as the crew's history: the run
    /// is left `stopped`, with no stop reason, `reason_code` naming the refusal and the one sentence in
    /// `review_reason`, and it waits for nobody (`review_waiting` is false). Nothing ran, so any lead thread of the
    /// run is removed in the same transaction and the person has only the history row to read; the row keeps the
    /// thread's id. Audited as the refusal in the same transaction. A missing or already-ended run changes nothing and
    /// returns `None`.
    pub fn end_background_run_not_started(
        &self,
        run: &str,
        code: &str,
        why: &str,
        actor: &str,
    ) -> Result<Option<BackgroundRunRow>> {
        let (kept, deleted_threads) = self.commit(|tx, _| {
            let Some(current) = tx
                .query_row(
                    "SELECT * FROM crew_background_runs WHERE id = ?1",
                    [run],
                    run_row,
                )
                .optional()?
            else {
                return Ok((None, Vec::new()));
            };
            if current.state != "running" {
                return Ok((None, Vec::new()));
            }
            audit(
                tx,
                actor,
                "crew.background.run.refused",
                &current.background_id,
                json!({ "run": run, "reason_code": code, "why": why }),
            )?;
            let stamp = now();
            tx.execute(
                "UPDATE crew_background_runs SET state = 'stopped', stop_reason = NULL, reason_code = ?2,
                     review_waiting = 0, review_reason = ?3, updated_at = ?4, ended_at = ?4
                 WHERE id = ?1 AND state = 'running'",
                params![run, code, why, stamp],
            )?;
            let deleted = discard_lead_in(tx, run)?;
            let row = tx
                .query_row(
                    "SELECT * FROM crew_background_runs WHERE id = ?1",
                    [run],
                    run_row,
                )
                .optional()?;
            Ok((row, deleted))
        })?;
        for id in &deleted_threads {
            self.hub.remove(id);
        }
        Ok(kept)
    }

    /// Ends a running run: stopped for a reason, or finished with work for review. Audited in the same transaction.
    pub fn end_background_run(&self, run: &str, end: RunEnd<'_>) -> Result<BackgroundRunRow> {
        self.end_background_run_spent(run, end, None)
    }

    /// What one engine has spent on a crew in a day: `(runs, turns)`. The runs are the ledger's rows, one per run the engine
    /// worked that day, so an engine's count never touches another's.
    pub fn background_daily_spent(
        &self,
        crew: &str,
        day: &str,
        engine: &str,
    ) -> Result<(u32, u32)> {
        Ok(self.reader().query_row(
            "SELECT COUNT(*), COALESCE(SUM(turns_spent), 0) FROM crew_background_daily
             WHERE background_id = ?1 AND day = ?2 AND engine = ?3",
            params![crew, day, engine],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    }

    /// One engine turn of one run of the crew: the run's row for `(day, engine)` gains a turn, made when it does not exist yet.
    /// Audited in the same transaction. Returns what the engine has spent on the crew that day: `(runs, turns)`.
    pub fn add_background_daily_turn(
        &self,
        crew: &str,
        run: &str,
        day: &str,
        engine: &str,
    ) -> Result<(u32, u32)> {
        self.commit(|tx, _| {
            tx.execute(
                "INSERT INTO crew_background_daily (background_id, run_id, day, engine, turns_spent)
                 VALUES (?1, ?2, ?3, ?4, 1)
                 ON CONFLICT (run_id, day, engine) DO UPDATE SET turns_spent = turns_spent + 1",
                params![crew, run, day, engine],
            )?;
            let spent = daily_in(tx, crew, day, engine)?;
            audit(
                tx,
                "crew",
                "crew.background.daily.spend",
                crew,
                json!({
                    "run": run,
                    "day": day,
                    "engine": engine,
                    "runs": spent.0,
                    "turns": spent.1,
                }),
            )?;
            Ok(spent)
        })
    }

    /// A new event of a crew refused because an engine's daily budget is spent: the audit row that reads beside the refusal, with
    /// the engine and when the person's day turns.
    pub fn audit_background_daily_refusal(
        &self,
        crew: &str,
        engine: &str,
        day: &str,
        resets_at: &str,
    ) -> Result<()> {
        self.commit(|tx, _| {
            audit(
                tx,
                "crew",
                "crew.background.daily.refused",
                crew,
                json!({
                    "engine": engine,
                    "day": day,
                    "reason": format!("daily budget spent: {engine}"),
                    "resets_at": resets_at,
                }),
            )
        })
    }

    /// One engine turn of a running run, counted as it happens: the run's `turns_spent` gains one and its `minutes_spent` becomes
    /// the wall minutes given. A run that is not running is left alone (a late turn after its end counts for the day, not the run).
    /// Audited in the same transaction.
    pub fn count_background_run_turn(&self, run: &str, minutes: u32) -> Result<()> {
        self.commit(|tx, _| {
            let Some(current) = tx
                .query_row(
                    "SELECT * FROM crew_background_runs WHERE id = ?1",
                    [run],
                    run_row,
                )
                .optional()?
            else {
                return Ok(());
            };
            if current.state != "running" {
                return Ok(());
            }
            audit(
                tx,
                "crew",
                "crew.background.run.turn",
                &current.background_id,
                json!({
                    "run": run,
                    "turns_spent": current.turns_spent + 1,
                    "minutes_spent": minutes,
                }),
            )?;
            tx.execute(
                "UPDATE crew_background_runs SET turns_spent = ?2, minutes_spent = ?3, updated_at = ?4
                 WHERE id = ?1 AND state = 'running'",
                params![run, current.turns_spent + 1, minutes, now()],
            )?;
            Ok(())
        })
    }

    /// Ends a running run at its crew's daily cap, stopped `budget` and waiting for the person with the sentence given, which carries
    /// the engine, what it spent that day (an overshoot is shown, never clamped) and when the day turns. `spent` replaces the run's
    /// totals in the same write. Audited in the same transaction.
    pub fn end_background_run_daily(
        &self,
        run: &str,
        review: &str,
        spent: (u32, u32),
    ) -> Result<BackgroundRunRow> {
        self.commit(|tx, _| {
            let current = tx
                .query_row(
                    "SELECT * FROM crew_background_runs WHERE id = ?1",
                    [run],
                    run_row,
                )
                .optional()?
                .ok_or_else(|| CoreError::NotFound("background run".into()))?;
            if current.state != "running" {
                return Err(CoreError::RunClosed);
            }
            let current = BackgroundRunRow {
                turns_spent: spent.0,
                minutes_spent: spent.1,
                ..current
            };
            end_run(
                tx,
                &current,
                RunEnd::Stopped("budget"),
                Some(review),
                "crew",
            )?;
            Ok(tx.query_row(
                "SELECT * FROM crew_background_runs WHERE id = ?1",
                [run],
                run_row,
            )?)
        })
    }

    /// `end_background_run`, recording what the run spent in the same write: `(turns, minutes)` replace the totals. Unlike
    /// `add_background_spend` it never stops the run by itself, so a run that passed on its last turn is not recorded as over budget.
    pub fn end_background_run_spent(
        &self,
        run: &str,
        end: RunEnd<'_>,
        spent: Option<(u32, u32)>,
    ) -> Result<BackgroundRunRow> {
        self.commit(|tx, _| {
            let current = tx
                .query_row(
                    "SELECT * FROM crew_background_runs WHERE id = ?1",
                    [run],
                    run_row,
                )
                .optional()?
                .ok_or_else(|| CoreError::NotFound("background run".into()))?;
            if current.state != "running" {
                return Err(CoreError::RunClosed);
            }
            let current = match spent {
                Some((turns_spent, minutes_spent)) => BackgroundRunRow {
                    turns_spent,
                    minutes_spent,
                    ..current
                },
                None => current,
            };
            let review = match end {
                RunEnd::NeedsReview(reason) => Some(reason),
                // A run that stopped for its budget always waits for the person, whoever ended it.
                RunEnd::Stopped("budget") => Some("The run reached its budget and stopped."),
                RunEnd::Stopped(_) => None,
            };
            end_run(tx, &current, end, review, "crew")?;
            Ok(tx.query_row(
                "SELECT * FROM crew_background_runs WHERE id = ?1",
                [run],
                run_row,
            )?)
        })
    }

    /// The person's acknowledgement of one waiting review: the wait is cleared in the same transaction as its audit row, and
    /// nothing else changes. The run keeps why it waited (`review_reason`), its state, its stop reason and its spend, so the
    /// record still says what the person settled. A review that is not waiting is refused plainly and nothing is written.
    pub fn acknowledge_background_review(
        &self,
        run: &str,
        actor: &str,
    ) -> Result<BackgroundRunRow> {
        self.commit(|tx, _| {
            let current = tx
                .query_row(
                    "SELECT * FROM crew_background_runs WHERE id = ?1",
                    [run],
                    run_row,
                )
                .optional()?
                .ok_or_else(|| CoreError::NotFound("background run".into()))?;
            if !current.review_waiting {
                return Err(CoreError::Invalid(
                    "this run is not waiting for the person's review".into(),
                ));
            }
            audit(
                tx,
                actor,
                "crew.background.run.acknowledge",
                &current.background_id,
                json!({
                    "run": run,
                    "state": current.state,
                    "stop_reason": current.stop_reason,
                    "review_reason": current.review_reason,
                }),
            )?;
            tx.execute(
                "UPDATE crew_background_runs SET review_waiting = 0, updated_at = ?2 WHERE id = ?1 AND review_waiting = 1",
                params![run, now()],
            )?;
            Ok(tx.query_row(
                "SELECT * FROM crew_background_runs WHERE id = ?1",
                [run],
                run_row,
            )?)
        })
    }
}
