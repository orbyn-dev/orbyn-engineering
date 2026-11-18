//! Review capacity at rest: a project's waiting-merge limit and sampling choice, a task's priority and whether
//! Crew began it, the waiting merges that count against the limit, and the record of each sampled batch.
//! Ids, counts and the seed only: no prompt, no path, no secret.

use rusqlite::{OptionalExtension, params};
use serde_json::json;

use super::crew_merges::merge_row;
use super::{AuditRecord, MergeRecord, Store, insert_audit};
use crate::crew::capacity::{CapacitySettings, Priority, Sampling};
use crate::error::{CoreError, Result};

/// One sampled batch as stored: when and with which seed it was drawn, and every task in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SampleBatch {
    pub batch_id: String,
    /// The seed the choice was drawn with.
    pub seed: u64,
    pub chosen_unix: i64,
    /// Every task of the batch, in the order it was recorded, with whether it was chosen.
    pub tasks: Vec<SampledTask>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SampledTask {
    pub task_id: String,
    pub chosen: bool,
}

fn priority_name(priority: Priority) -> &'static str {
    match priority {
        Priority::High => "high",
        Priority::Medium => "medium",
        Priority::Low => "low",
    }
}

fn priority_of(name: &str) -> Result<Priority> {
    match name {
        "high" => Ok(Priority::High),
        "medium" => Ok(Priority::Medium),
        "low" => Ok(Priority::Low),
        other => Err(CoreError::Internal(format!(
            "a stored task priority is unknown: {other}"
        ))),
    }
}

fn sampling_name(sampling: Sampling) -> &'static str {
    match sampling {
        Sampling::Off => "off",
        Sampling::OneInFour => "one_in_four",
    }
}

fn sampling_of(name: &str) -> Result<Sampling> {
    match name {
        "off" => Ok(Sampling::Off),
        "one_in_four" => Ok(Sampling::OneInFour),
        other => Err(CoreError::Internal(format!(
            "a stored sampling choice is unknown: {other}"
        ))),
    }
}

impl Store {
    /// A project's capacity settings. A project that never saved any has the defaults: a limit of 8, sampling off.
    pub fn crew_capacity_settings(&self, project_id: &str) -> Result<CapacitySettings> {
        let stored = self
            .reader()
            .query_row(
                "SELECT limit_n, sampling, pause_enabled FROM crew_capacity_settings WHERE project_id = ?1",
                [project_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((limit, sampling, pause)) = stored else {
            return Ok(CapacitySettings::default());
        };
        Ok(CapacitySettings {
            limit: u32::try_from(limit)
                .ok()
                .filter(|n| *n >= 1)
                .ok_or_else(|| CoreError::Internal("a stored capacity limit is invalid".into()))?,
            sampling: sampling_of(&sampling)?,
            pause_enabled: pause == 1,
        })
    }

    /// Whether a person ever saved capacity settings for the project (otherwise the defaults apply).
    pub fn crew_capacity_settings_saved(&self, project_id: &str) -> Result<bool> {
        Ok(self.reader().query_row(
            "SELECT EXISTS (SELECT 1 FROM crew_capacity_settings WHERE project_id = ?1)",
            [project_id],
            |row| row.get(0),
        )?)
    }

    /// Saves a project's capacity settings, replacing any earlier ones. A limit of zero is refused, and so is a
    /// project that does not exist.
    pub fn set_crew_capacity_settings(
        &self,
        project_id: &str,
        settings: CapacitySettings,
    ) -> Result<()> {
        if settings.limit == 0 {
            return Err(CoreError::Invalid(
                "the capacity limit must be at least 1".into(),
            ));
        }
        self.commit(|tx, _| {
            let known: bool = tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM projects WHERE id = ?1)",
                [project_id],
                |row| row.get(0),
            )?;
            if !known {
                return Err(CoreError::NotFound(format!("project {project_id}")));
            }
            tx.execute(
                "INSERT INTO crew_capacity_settings (project_id, limit_n, sampling, pause_enabled) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(project_id) DO UPDATE SET limit_n = excluded.limit_n, sampling = excluded.sampling,
                     pause_enabled = excluded.pause_enabled",
                params![
                    project_id,
                    settings.limit,
                    sampling_name(settings.sampling),
                    i64::from(settings.pause_enabled)
                ],
            )?;
            insert_audit(
                tx,
                &AuditRecord {
                    actor: "user".into(),
                    action: "crew.capacity.settings".into(),
                    target: project_id.into(),
                    decision: None,
                    source: "user".into(),
                    thread_id: None,
                    run_id: None,
                    detail: json!({
                        "limit": settings.limit,
                        "sampling": sampling_name(settings.sampling),
                        "pause_enabled": settings.pause_enabled,
                    }),
                },
            )?;
            Ok(())
        })
    }

    /// A task's priority and whether Crew began it itself. A task stored before this existed is Medium and
    /// person-started.
    pub fn crew_task_capacity(&self, task_id: &str) -> Result<(Priority, bool)> {
        let (priority, crew_started) = self
            .reader()
            .query_row(
                "SELECT priority, crew_started FROM crew_tasks WHERE id = ?1",
                [task_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?
            .ok_or_else(|| CoreError::NotFound(format!("crew task {task_id}")))?;
        Ok((priority_of(&priority)?, crew_started == 1))
    }

    /// Sets a task's priority.
    pub fn set_crew_task_priority(&self, task_id: &str, priority: Priority) -> Result<()> {
        self.set_task_column(
            task_id,
            "UPDATE crew_tasks SET priority = ?2 WHERE id = ?1",
            priority_name(priority),
        )
    }

    /// Marks whether Crew began the task itself (`true`) or a person did (`false`, the default).
    pub fn set_crew_task_crew_started(&self, task_id: &str, crew_started: bool) -> Result<()> {
        self.set_task_column(
            task_id,
            "UPDATE crew_tasks SET crew_started = ?2 WHERE id = ?1",
            i64::from(crew_started),
        )
    }

    /// Sets a task's priority and/or Crew-started flag in one transaction with an audit row, and returns what is stored
    /// now. A fieldless call, an unknown task or a failed write changes nothing.
    pub fn set_crew_task_capacity(
        &self,
        task_id: &str,
        priority: Option<Priority>,
        crew_started: Option<bool>,
    ) -> Result<(Priority, bool)> {
        if priority.is_none() && crew_started.is_none() {
            return Err(CoreError::Invalid(
                "send a priority, crew_started or both".into(),
            ));
        }
        self.commit(|tx, _| {
            let known: bool = tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM crew_tasks WHERE id = ?1)",
                [task_id],
                |row| row.get(0),
            )?;
            if !known {
                return Err(CoreError::NotFound(format!("crew task {task_id}")));
            }
            if let Some(priority) = priority {
                tx.execute(
                    "UPDATE crew_tasks SET priority = ?2 WHERE id = ?1",
                    params![task_id, priority_name(priority)],
                )?;
            }
            if let Some(started) = crew_started {
                tx.execute(
                    "UPDATE crew_tasks SET crew_started = ?2 WHERE id = ?1",
                    params![task_id, i64::from(started)],
                )?;
            }
            insert_audit(
                tx,
                &AuditRecord {
                    actor: "user".into(),
                    action: "crew.capacity.task".into(),
                    target: task_id.into(),
                    decision: None,
                    source: "user".into(),
                    thread_id: None,
                    run_id: None,
                    detail: json!({
                        "priority": priority.map(priority_name),
                        "crew_started": crew_started,
                    }),
                },
            )?;
            let (p, s): (String, i64) = tx.query_row(
                "SELECT priority, crew_started FROM crew_tasks WHERE id = ?1",
                [task_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            Ok((priority_of(&p)?, s == 1))
        })
    }

    fn set_task_column(&self, task_id: &str, sql: &str, value: impl rusqlite::ToSql) -> Result<()> {
        self.commit(|tx, _| {
            if tx.execute(sql, params![task_id, value])? == 0 {
                return Err(CoreError::NotFound(format!("crew task {task_id}")));
            }
            Ok(())
        })
    }

    /// The merges of a project that still wait for a person and carry High or Medium risk, oldest first: what
    /// `capacity::waiting_count` counts.
    pub fn crew_waiting_merges_of_project(&self, project_id: &str) -> Result<Vec<MergeRecord>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT m.* FROM crew_merges m
             JOIN crew_runs r ON r.task_id = m.task_id
             JOIN threads t ON t.id = r.lead_thread_id
             WHERE t.project_id = ?1 AND m.review_state = 'waiting' AND m.risk IN ('high', 'medium')
             ORDER BY m.id",
        )?;
        Ok(statement
            .query_map([project_id], merge_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// The project a task belongs to, through its run's lead thread. `None` for a task with no run.
    pub fn crew_project_of_task(&self, task_id: &str) -> Result<Option<String>> {
        Ok(self
            .reader()
            .query_row(
                "SELECT t.project_id FROM crew_runs r JOIN threads t ON t.id = r.lead_thread_id WHERE r.task_id = ?1",
                [task_id],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// When sampling was last turned on for the project, in unix seconds: the time of the latest settings audit row
    /// that set sampling on while the row before it (or the default) had it off. Saving other settings with sampling
    /// still on does not move it; turning sampling off clears it, so off then on again starts a new boundary. `None`
    /// while sampling is off. The audit log is append-only, so no migration or extra column is needed.
    pub fn crew_sampling_since(&self, project_id: &str) -> Result<Option<i64>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT ts, detail FROM audit_events WHERE action = 'crew.capacity.settings' AND target = ?1 ORDER BY id",
        )?;
        let rows = statement
            .query_map([project_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut since = None;
        for (ts, detail) in rows {
            let detail: serde_json::Value = serde_json::from_str(&detail)?;
            let on = detail["sampling"].as_str() == Some(sampling_name(Sampling::OneInFour));
            match (on, since) {
                (true, None) => {
                    since = Some(crate::util::parse_time(&ts).ok_or_else(|| {
                        CoreError::Internal("a settings audit time is invalid".into())
                    })?);
                }
                (false, _) => since = None,
                (true, Some(_)) => {}
            }
        }
        Ok(since)
    }

    /// The Low-risk merges of a project up to merge `up_to` that no batch has taken yet and that landed at or after
    /// `since_unix`, oldest first: the open sampling batch. Merges from before are never part of it.
    pub fn crew_open_low_merges(
        &self,
        project_id: &str,
        up_to: i64,
        since_unix: i64,
    ) -> Result<Vec<MergeRecord>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT m.* FROM crew_merges m
             JOIN crew_runs r ON r.task_id = m.task_id
             JOIN threads t ON t.id = r.lead_thread_id
             WHERE t.project_id = ?1 AND m.risk = 'low' AND m.id <= ?2 AND m.merged_unix >= ?3
               AND NOT EXISTS (SELECT 1 FROM crew_review_samples s WHERE s.task_id = m.task_id)
             ORDER BY m.id",
        )?;
        Ok(statement
            .query_map(params![project_id, up_to, since_unix], merge_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// The tasks of a project picked for review whose merge still waits for a person, oldest merge first.
    pub fn crew_picked_for_review(&self, project_id: &str) -> Result<Vec<String>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT m.task_id FROM crew_merges m
             JOIN crew_review_samples s ON s.task_id = m.task_id AND s.chosen = 1
             JOIN crew_runs r ON r.task_id = m.task_id
             JOIN threads t ON t.id = r.lead_thread_id
             WHERE t.project_id = ?1 AND m.review_state = 'waiting'
             ORDER BY m.id",
        )?;
        Ok(statement
            .query_map([project_id], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Records a sampled batch once: each task with whether it was chosen, the seed and the time. A batch id that
    /// was recorded before, a batch with no task or the same task twice is refused, so a record never changes.
    pub fn record_crew_review_sample(
        &self,
        batch_id: &str,
        seed: u64,
        chosen_unix: i64,
        tasks: &[SampledTask],
    ) -> Result<()> {
        if tasks.is_empty() {
            return Err(CoreError::Invalid("a sampled batch needs a task".into()));
        }
        self.commit(|tx, _| {
            let known: bool = tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM crew_review_samples WHERE batch_id = ?1)",
                [batch_id],
                |row| row.get(0),
            )?;
            if known {
                return Err(CoreError::Invalid(format!(
                    "batch {batch_id} was already recorded"
                )));
            }
            // The seed is a u64; SQLite keeps the same 64 bits in a signed integer.
            let stored_seed = i64::from_ne_bytes(seed.to_ne_bytes());
            for task in tasks {
                let added = tx.execute(
                    "INSERT OR IGNORE INTO crew_review_samples (batch_id, task_id, chosen, seed, chosen_unix)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        batch_id,
                        task.task_id,
                        i64::from(task.chosen),
                        stored_seed,
                        chosen_unix
                    ],
                )?;
                if added == 0 {
                    return Err(CoreError::Invalid(format!(
                        "task {} is in batch {batch_id} twice",
                        task.task_id
                    )));
                }
            }
            Ok(())
        })
    }

    /// A recorded batch, or nothing for an id never recorded.
    pub fn crew_review_sample(&self, batch_id: &str) -> Result<Option<SampleBatch>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT task_id, chosen, seed, chosen_unix FROM crew_review_samples
             WHERE batch_id = ?1 ORDER BY rowid",
        )?;
        let rows = statement
            .query_map([batch_id], |row| {
                Ok((
                    SampledTask {
                        task_id: row.get(0)?,
                        chosen: row.get::<_, i64>(1)? == 1,
                    },
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let Some(&(_, seed, chosen_unix)) = rows.first() else {
            return Ok(None);
        };
        Ok(Some(SampleBatch {
            batch_id: batch_id.to_owned(),
            seed: u64::from_ne_bytes(seed.to_ne_bytes()),
            chosen_unix,
            tasks: rows.into_iter().map(|(task, _, _)| task).collect(),
        }))
    }
}
