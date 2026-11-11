//! Crew Protocol v0 objects at rest. Each is canonical JSON keyed by its content
//! address and is verified when it goes in: it must read as its type, pass its rules and
//! hash to its own id. The answer key has its own table and its own reader, so the
//! brief a worker reads comes from a table that cannot hold it (H1).

use orbyn_contracts::{
    AnswerKey, Brief, CrewError, CrewMessage, CrewObject, CrewResult, Evidence, Gate, Lease,
    Report, RouteDecision, WorkerObject, canonical_json,
};
use rusqlite::{OptionalExtension, Row, Transaction, params};

use super::Store;
use crate::error::{CoreError, Result};
use crate::util::now;

/// One task: a brief, its key and everything the run produced. Keyed by the brief's id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrewTask {
    pub id: String,
    pub crew_id: String,
    pub run_id: String,
    pub state: String,
    pub stop_reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub(super) fn task_row(row: &Row<'_>) -> rusqlite::Result<CrewTask> {
    Ok(CrewTask {
        id: row.get("id")?,
        crew_id: row.get("crew_id")?,
        run_id: row.get("run_id")?,
        state: row.get("state")?,
        stop_reason: row.get("stop_reason")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

pub(super) fn refuse(error: CrewError) -> CoreError {
    CoreError::Invalid(format!("crew object refused: {error}"))
}

/// Reads a stored body back as the object it must be.
pub(super) fn read_object(text: &str) -> Result<CrewObject> {
    let value = serde_json::from_str(text)?;
    CrewObject::parse(&value).map_err(|error| {
        CoreError::Internal(format!("a stored crew object no longer verifies: {error}"))
    })
}

/// The object as stored: checked, then written as canonical JSON.
pub(super) fn checked(object: &CrewObject) -> Result<String> {
    object.validate().map_err(refuse)?;
    let computed = object.computed_id().map_err(refuse)?;
    if &computed != object.id() {
        return Err(CoreError::Invalid(format!(
            "crew object refused: its id {} is not its hash {computed}",
            object.id()
        )));
    }
    canonical_json(&object.to_value()).map_err(refuse)
}

pub(super) fn find_task(tx: &Transaction<'_>, id: &str) -> Result<CrewTask> {
    tx.query_row("SELECT * FROM crew_tasks WHERE id = ?1", [id], task_row)
        .optional()?
        .ok_or_else(|| CoreError::NotFound(format!("crew task {id}")))
}

/// An object belongs to its task's crew and run, and a result answers its brief.
pub(super) fn belongs_to(task: &CrewTask, object: &CrewObject) -> Result<()> {
    let wrong_run = object
        .run_id()
        .is_some_and(|run| run.as_str() != task.run_id);
    let wrong_brief = match object {
        CrewObject::Result(r) => r.brief.as_str() != task.id,
        CrewObject::RouteDecision(d) => d.brief.as_str() != task.id,
        _ => false,
    };
    if object.crew_id().as_str() != task.crew_id || wrong_run || wrong_brief {
        return Err(CoreError::Invalid(format!(
            "crew object {} is not part of task {}",
            object.id(),
            task.id
        )));
    }
    Ok(())
}

/// Writes an already checked object to its table, inside the caller's transaction. The
/// object must belong to the task's crew and run. Adding the same object again changes nothing.
pub(super) fn insert_object(
    tx: &Transaction<'_>,
    task_id: &str,
    object: &CrewObject,
    body: &str,
) -> Result<()> {
    let task = find_task(tx, task_id)?;
    belongs_to(&task, object)?;
    let id = object.id().as_str();
    match object {
        CrewObject::Message(o) => tx.execute(
            "INSERT INTO crew_messages (id, task_id, recipient, kind, body, sender)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT(id) DO NOTHING",
            params![
                id,
                task_id,
                o.to.as_str(),
                super::wire_name(o.kind)?,
                body,
                o.author.member.as_ref().map_or("", |m| m.as_str())
            ],
        )?,
        CrewObject::Fact(o) => tx.execute(
            "INSERT INTO crew_facts (id, task_id, crew_id, key, rev, author, body)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) ON CONFLICT(id) DO NOTHING",
            params![
                id,
                task_id,
                o.crew_id.as_str(),
                o.key.as_str(),
                i64::try_from(o.rev).unwrap_or(i64::MAX),
                o.author.member.as_ref().map_or("", |m| m.as_str()),
                body
            ],
        )?,
        CrewObject::Lease(o) => tx.execute(
            "INSERT INTO crew_leases (id, task_id, holder, state, expires_at, body)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT(id) DO NOTHING",
            params![
                id,
                task_id,
                o.holder.as_str(),
                super::wire_name(o.state)?,
                o.expires_at.as_str(),
                body
            ],
        )?,
        CrewObject::Evidence(o) => tx.execute(
            "INSERT INTO crew_evidence (id, task_id, check_id, outcome, body)
             VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(id) DO NOTHING",
            params![
                id,
                task_id,
                o.check.as_str(),
                super::wire_name(o.outcome)?,
                body
            ],
        )?,
        CrewObject::Gate(o) => tx.execute(
            "INSERT INTO crew_gates (id, task_id, stage, decision, body)
             VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(id) DO NOTHING",
            params![
                id,
                task_id,
                super::wire_name(o.stage)?,
                super::wire_name(o.decision)?,
                body
            ],
        )?,
        CrewObject::Result(o) => tx.execute(
            "INSERT INTO crew_results (id, task_id, state, final, body)
             VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(id) DO NOTHING",
            params![id, task_id, super::wire_name(o.state)?, o.r#final, body],
        )?,
        CrewObject::RouteDecision(o) => tx.execute(
            "INSERT INTO crew_route_decisions (id, task_id, engine, assigned_by, body)
             VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(id) DO NOTHING",
            params![
                id,
                task_id,
                o.engine.as_str(),
                super::wire_name(o.assigned_by)?,
                body
            ],
        )?,
        CrewObject::Report(_) => tx.execute(
            "INSERT INTO crew_reports (id, task_id, body) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO NOTHING",
            params![id, task_id, body],
        )?,
        other => {
            return Err(CoreError::Invalid(format!(
                "a {} has no crew table yet",
                other.type_name()
            )));
        }
    };
    Ok(())
}

impl Store {
    /// Stores a brief and its answer key together, in the state `briefed`. The key's
    /// `brief` must be this brief's id. Storing the same task again, with the same key and
    /// run, returns it as it is; a different key or run is refused.
    pub fn create_crew_task(
        &self,
        brief: &Brief,
        key: &AnswerKey,
        run_id: &str,
    ) -> Result<CrewTask> {
        let brief_object = CrewObject::Brief(brief.clone());
        let key_object = CrewObject::AnswerKey(key.clone());
        let brief_body = checked(&brief_object)?;
        let key_body = checked(&key_object)?;
        if key.brief != brief.id || key.crew_id != brief.crew_id {
            return Err(CoreError::Invalid(
                "crew answer key refused: it belongs to another brief".into(),
            ));
        }
        self.commit(|tx, _| {
            if let Some(existing) = tx
                .query_row(
                    "SELECT * FROM crew_tasks WHERE id = ?1",
                    [brief.id.as_str()],
                    task_row,
                )
                .optional()?
            {
                let stored_key: String = tx.query_row(
                    "SELECT id FROM crew_answer_keys WHERE task_id = ?1",
                    [brief.id.as_str()],
                    |row| row.get(0),
                )?;
                // Same brief, but a different key or run: refuse, never return the old one.
                if stored_key != key.id.as_str() || existing.run_id != run_id {
                    return Err(CoreError::Invalid(format!(
                        "crew task {} already exists with a different {}",
                        existing.id,
                        if stored_key != key.id.as_str() {
                            "answer key"
                        } else {
                            "run"
                        }
                    )));
                }
                return Ok(existing);
            }
            let stamp = now();
            tx.execute(
                "INSERT INTO crew_tasks (id, crew_id, run_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)",
                params![brief.id.as_str(), brief.crew_id.as_str(), run_id, stamp],
            )?;
            tx.execute(
                "INSERT INTO crew_briefs (id, body) VALUES (?1, ?2)",
                params![brief.id.as_str(), brief_body],
            )?;
            tx.execute(
                "INSERT INTO crew_answer_keys (task_id, id, body) VALUES (?1, ?2, ?3)",
                params![brief.id.as_str(), key.id.as_str(), key_body],
            )?;
            find_task(tx, brief.id.as_str())
        })
    }

    pub fn crew_task(&self, id: &str) -> Result<Option<CrewTask>> {
        Ok(self
            .reader()
            .query_row("SELECT * FROM crew_tasks WHERE id = ?1", [id], task_row)
            .optional()?)
    }

    /// The brief: what a worker may read. It never carries the key.
    pub fn crew_brief(&self, task_id: &str) -> Result<Option<Brief>> {
        let body: Option<String> = self
            .reader()
            .query_row(
                "SELECT body FROM crew_briefs WHERE id = ?1",
                [task_id],
                |row| row.get(0),
            )
            .optional()?;
        body.map(|text| match read_object(&text)? {
            CrewObject::Brief(brief) => Ok(brief),
            other => Err(CoreError::Internal(format!(
                "crew_briefs holds a {}",
                other.type_name()
            ))),
        })
        .transpose()
    }

    /// The answer key. Only the core and the verifier call this; no tool returns it to a member.
    pub fn crew_answer_key(&self, task_id: &str) -> Result<Option<AnswerKey>> {
        let body: Option<String> = self
            .reader()
            .query_row(
                "SELECT body FROM crew_answer_keys WHERE task_id = ?1",
                [task_id],
                |row| row.get(0),
            )
            .optional()?;
        body.map(|text| match read_object(&text)? {
            CrewObject::AnswerKey(key) => Ok(key),
            other => Err(CoreError::Internal(format!(
                "crew_answer_keys holds a {}",
                other.type_name()
            ))),
        })
        .transpose()
    }

    /// Appends a message, lease, evidence, gate or result to a task. The object must be
    /// valid, hash to its id, and belong to the task's crew and run. Adding the same
    /// object again changes nothing.
    pub fn add_crew_object(&self, task_id: &str, object: &CrewObject) -> Result<()> {
        let body = checked(object)?;
        self.commit(|tx, _| insert_object(tx, task_id, object, &body))
    }

    pub(super) fn crew_objects(
        &self,
        table: &'static str,
        task_id: &str,
    ) -> Result<Vec<CrewObject>> {
        let conn = self.reader();
        let mut statement = conn.prepare(&format!(
            "SELECT body FROM {table} WHERE task_id = ?1 ORDER BY rowid"
        ))?;
        let bodies = statement
            .query_map([task_id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        bodies.iter().map(|text| read_object(text)).collect()
    }

    /// What the member `reader` may be shown: the task's brief, the messages in its crew
    /// addressed to `reader` or to `all` (not its own), and the board's facts. A message to
    /// the lead or to another member is left out, and so is the reader's own `all`. Evidence,
    /// gates and results are left out too, since they carry the answer key's check ids and
    /// commands. The H1 leak check must still run on each object before it reaches a worker:
    /// a message or a fact can quote a check.
    pub fn crew_worker_objects(&self, task_id: &str, reader: &str) -> Result<Vec<WorkerObject>> {
        let mut shown: Vec<WorkerObject> = self
            .crew_brief(task_id)?
            .into_iter()
            .map(WorkerObject::Brief)
            .collect();
        let Some(task) = self.crew_task(task_id)? else {
            return Ok(shown);
        };
        let bodies = {
            let conn = self.reader();
            let mut statement = conn.prepare(
                "SELECT m.body FROM crew_messages m JOIN crew_tasks t ON t.id = m.task_id
                 WHERE t.crew_id = ?1
                   AND (m.recipient = ?2 OR (m.recipient = 'all' AND m.sender != ?2))
                 ORDER BY m.rowid",
            )?;
            statement
                .query_map(params![task.crew_id, reader], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        for body in bodies {
            shown.push(WorkerObject::try_from(read_object(&body)?).map_err(refuse)?);
        }
        shown.extend(
            self.crew_facts(&task.crew_id)?
                .into_iter()
                .map(WorkerObject::Fact),
        );
        Ok(shown)
    }

    pub fn crew_messages(&self, task_id: &str) -> Result<Vec<CrewMessage>> {
        Ok(self
            .crew_objects("crew_messages", task_id)?
            .into_iter()
            .filter_map(|o| {
                if let CrewObject::Message(m) = o {
                    Some(m)
                } else {
                    None
                }
            })
            .collect())
    }

    pub fn crew_leases(&self, task_id: &str) -> Result<Vec<Lease>> {
        Ok(self
            .crew_objects("crew_leases", task_id)?
            .into_iter()
            .filter_map(|o| {
                if let CrewObject::Lease(l) = o {
                    Some(l)
                } else {
                    None
                }
            })
            .collect())
    }

    pub fn crew_evidence(&self, task_id: &str) -> Result<Vec<Evidence>> {
        Ok(self
            .crew_objects("crew_evidence", task_id)?
            .into_iter()
            .filter_map(|o| {
                if let CrewObject::Evidence(e) = o {
                    Some(e)
                } else {
                    None
                }
            })
            .collect())
    }

    pub fn crew_gates(&self, task_id: &str) -> Result<Vec<Gate>> {
        Ok(self
            .crew_objects("crew_gates", task_id)?
            .into_iter()
            .filter_map(|o| {
                if let CrewObject::Gate(g) = o {
                    Some(g)
                } else {
                    None
                }
            })
            .collect())
    }

    /// The router's decisions for a task, oldest first.
    pub fn crew_route_decisions(&self, task_id: &str) -> Result<Vec<RouteDecision>> {
        Ok(self
            .crew_objects("crew_route_decisions", task_id)?
            .into_iter()
            .filter_map(|o| {
                if let CrewObject::RouteDecision(d) = o {
                    Some(d)
                } else {
                    None
                }
            })
            .collect())
    }

    pub fn crew_reports(&self, task_id: &str) -> Result<Vec<Report>> {
        Ok(self
            .crew_objects("crew_reports", task_id)?
            .into_iter()
            .filter_map(|o| {
                if let CrewObject::Report(r) = o {
                    Some(r)
                } else {
                    None
                }
            })
            .collect())
    }

    pub fn crew_results(&self, task_id: &str) -> Result<Vec<CrewResult>> {
        Ok(self
            .crew_objects("crew_results", task_id)?
            .into_iter()
            .filter_map(|o| {
                if let CrewObject::Result(r) = o {
                    Some(r)
                } else {
                    None
                }
            })
            .collect())
    }
}
