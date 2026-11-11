//! The bus at rest: a member's messages and facts, how each message was delivered, and the
//! reads the guards count from. A member's object is stored only while its run is live (H7),
//! checked in the same transaction that writes it; the core's own writes (`add_crew_object`)
//! are not, so a check can run again and a gate can be added after a run ends.

use orbyn_contracts::{CrewMessage, CrewObject, Fact};
use rusqlite::{OptionalExtension, params};

use super::Store;
use super::crew::{CrewTask, checked, find_task, insert_object, read_object, task_row};
use super::crew_runs::CrewRun;
use crate::crew::lifecycle::is_final;
use crate::error::{CoreError, Result};
use crate::util::now;

/// How one message reached one recipient, as recorded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrewDelivery {
    pub message_id: String,
    pub recipient: String,
    pub thread_id: String,
    /// `held`, `live`, `next_turn` or `failed`.
    pub mode: String,
    pub note: Option<String>,
    pub at: String,
}

/// What a board write came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FactWrite {
    Stored,
    /// The key was at another revision than the writer saw (H12).
    Stale {
        current: u64,
    },
}

fn facts_of(bodies: Vec<String>) -> Result<Vec<Fact>> {
    bodies
        .iter()
        .map(|text| match read_object(text)? {
            CrewObject::Fact(fact) => Ok(fact),
            other => Err(CoreError::Internal(format!(
                "crew_facts holds a {}",
                other.type_name()
            ))),
        })
        .collect()
}

impl Store {
    /// A member's message or fact, stored while the task's run is live. `RunClosed` when it has
    /// ended. Adding the same object again changes nothing.
    pub fn add_crew_member_object(&self, task_id: &str, object: &CrewObject) -> Result<()> {
        let body = checked(object)?;
        self.commit(|tx, _| {
            if is_final(&find_task(tx, task_id)?.state) {
                return Err(CoreError::RunClosed);
            }
            insert_object(tx, task_id, object, &body)
        })
    }

    /// One board write (H12): stored only when the key stands at the revision the writer
    /// read, `fact.base_rev`, and the run is live. The tuple of crew, key and revision is
    /// unique, so two writes on one revision cannot both land.
    pub fn add_crew_fact(&self, task_id: &str, fact: &Fact) -> Result<FactWrite> {
        let body = checked(&CrewObject::Fact(fact.clone()))?;
        self.commit(|tx, _| {
            if is_final(&find_task(tx, task_id)?.state) {
                return Err(CoreError::RunClosed);
            }
            let current: i64 = tx.query_row(
                "SELECT COALESCE(MAX(rev), 0) FROM crew_facts WHERE crew_id = ?1 AND key = ?2",
                params![fact.crew_id.as_str(), fact.key.as_str()],
                |row| row.get(0),
            )?;
            let current = u64::try_from(current).unwrap_or(0);
            if current != fact.base_rev {
                return Ok(FactWrite::Stale { current });
            }
            insert_object(tx, task_id, &CrewObject::Fact(fact.clone()), &body)?;
            Ok(FactWrite::Stored)
        })
    }

    /// The revision that stands for `key` in a crew, or none.
    pub fn crew_fact(&self, crew_id: &str, key: &str) -> Result<Option<Fact>> {
        let body: Option<String> = self
            .reader()
            .query_row(
                "SELECT body FROM crew_facts WHERE crew_id = ?1 AND key = ?2
                 ORDER BY rev DESC LIMIT 1",
                params![crew_id, key],
                |row| row.get(0),
            )
            .optional()?;
        Ok(facts_of(body.into_iter().collect())?.into_iter().next())
    }

    /// The revision that stands for each key of a crew, oldest key first.
    pub fn crew_facts(&self, crew_id: &str) -> Result<Vec<Fact>> {
        let bodies = {
            let conn = self.reader();
            let mut statement = conn.prepare(
                "SELECT f.body FROM crew_facts f
                 WHERE f.crew_id = ?1
                   AND f.rev = (SELECT MAX(rev) FROM crew_facts g
                                WHERE g.crew_id = f.crew_id AND g.key = f.key)
                 ORDER BY f.rowid",
            )?;
            statement
                .query_map([crew_id], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        facts_of(bodies)
    }

    /// Every message of a crew's tasks, oldest first, with the task each was stored under.
    pub fn crew_messages_in_crew(&self, crew_id: &str) -> Result<Vec<(String, CrewMessage)>> {
        let rows = {
            let conn = self.reader();
            let mut statement = conn.prepare(
                "SELECT m.task_id, m.body FROM crew_messages m
                 JOIN crew_tasks t ON t.id = m.task_id
                 WHERE t.crew_id = ?1 ORDER BY m.rowid",
            )?;
            statement
                .query_map([crew_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        rows.into_iter()
            .map(|(task, body)| match read_object(&body)? {
                CrewObject::Message(message) => Ok((task, message)),
                other => Err(CoreError::Internal(format!(
                    "crew_messages holds a {}",
                    other.type_name()
                ))),
            })
            .collect()
    }

    /// How many messages `sender` has posted in a task's run.
    pub fn count_crew_posts(&self, task_id: &str, sender: &str) -> Result<u32> {
        Ok(self.reader().query_row(
            "SELECT COUNT(*) FROM crew_messages WHERE task_id = ?1 AND sender = ?2",
            params![task_id, sender],
            |row| row.get(0),
        )?)
    }

    /// Every task of a crew with its run, oldest first. Its members are the runs' members.
    pub fn crew_tasks_in_crew(&self, crew_id: &str) -> Result<Vec<(CrewTask, CrewRun)>> {
        let tasks: Vec<CrewTask> = {
            let conn = self.reader();
            let mut statement = conn.prepare(
                "SELECT * FROM crew_tasks WHERE crew_id = ?1 ORDER BY created_at, rowid",
            )?;
            statement
                .query_map([crew_id], task_row)?
                .collect::<rusqlite::Result<_>>()?
        };
        let mut found = Vec::new();
        for task in tasks {
            if let Some(run) = self.crew_run(&task.id)? {
                found.push((task, run));
            }
        }
        Ok(found)
    }

    /// Whether every id names Evidence stored in one of the crew's tasks.
    pub fn crew_has_evidence(&self, crew_id: &str, ids: &[String]) -> Result<bool> {
        let conn = self.reader();
        for id in ids {
            let found: i64 = conn.query_row(
                "SELECT COUNT(*) FROM crew_evidence e JOIN crew_tasks t ON t.id = e.task_id
                 WHERE t.crew_id = ?1 AND e.id = ?2",
                params![crew_id, id],
                |row| row.get(0),
            )?;
            if found == 0 {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Records how `message_id` reached `recipient`, or that it is held for the recipient's
    /// next prompt. Rows are only added; the newest one for the pair is where it stands.
    pub fn record_crew_delivery(
        &self,
        message_id: &str,
        recipient: &str,
        thread_id: &str,
        mode: &str,
        note: Option<&str>,
    ) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "INSERT INTO crew_deliveries (message_id, recipient, thread_id, mode, note, at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![message_id, recipient, thread_id, mode, note, now()],
            )?;
            Ok(())
        })
    }

    /// The delivery rows of the messages stored under a task, oldest first.
    pub fn crew_deliveries(&self, task_id: &str) -> Result<Vec<CrewDelivery>> {
        self.delivery_rows("m.task_id = ?1", task_id)
    }

    /// The delivery rows of every message of a crew, oldest first.
    pub fn crew_deliveries_in_crew(&self, crew_id: &str) -> Result<Vec<CrewDelivery>> {
        self.delivery_rows(
            "m.task_id IN (SELECT id FROM crew_tasks WHERE crew_id = ?1)",
            crew_id,
        )
    }

    fn delivery_rows(&self, scope: &str, arg: &str) -> Result<Vec<CrewDelivery>> {
        let conn = self.reader();
        let mut statement = conn.prepare(&format!(
            "SELECT d.message_id, d.recipient, d.thread_id, d.mode, d.note, d.at
             FROM crew_deliveries d JOIN crew_messages m ON m.id = d.message_id
             WHERE {scope} ORDER BY d.id"
        ))?;
        Ok(statement
            .query_map([arg], |row| {
                Ok(CrewDelivery {
                    message_id: row.get(0)?,
                    recipient: row.get(1)?,
                    thread_id: row.get(2)?,
                    mode: row.get(3)?,
                    note: row.get(4)?,
                    at: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// The messages of a crew still held for `recipient`: its newest delivery row says `held`.
    pub fn held_crew_messages(&self, crew_id: &str, recipient: &str) -> Result<Vec<CrewMessage>> {
        let bodies = {
            let conn = self.reader();
            let mut statement = conn.prepare(
                "SELECT m.body FROM crew_deliveries d
                 JOIN crew_messages m ON m.id = d.message_id
                 JOIN crew_tasks t ON t.id = m.task_id
                 WHERE t.crew_id = ?1 AND d.recipient = ?2 AND d.mode = 'held'
                   AND NOT EXISTS (SELECT 1 FROM crew_deliveries e
                                   WHERE e.message_id = d.message_id
                                     AND e.recipient = d.recipient AND e.id > d.id)
                 ORDER BY m.rowid",
            )?;
            statement
                .query_map(params![crew_id, recipient], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        bodies
            .iter()
            .map(|text| match read_object(text)? {
                CrewObject::Message(message) => Ok(message),
                other => Err(CoreError::Internal(format!(
                    "crew_messages holds a {}",
                    other.type_name()
                ))),
            })
            .collect()
    }
}
