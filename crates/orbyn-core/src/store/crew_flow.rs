//! The reads behind the Flow screens (Crew Preview): which crews have work and which tasks are theirs. Reads only, on the
//! store's read connection. A work task is a task the lead started to build something: a reviewer's run and the fix
//! that answers a finding are rounds of another task, not rows of their own.

use super::Store;
use super::crew::{CrewTask, task_row};
use crate::error::Result;

/// The condition that leaves out reviewer runs and fix tasks.
const WORK: &str = "id NOT IN (SELECT task_id FROM crew_reviews) AND id NOT IN (SELECT fix_task_id FROM crew_fixes)";

/// One crew's count of work tasks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrewTotals {
    pub crew_id: String,
    pub tasks: u32,
    pub finished: u32,
    pub updated_at: String,
}

impl Store {
    /// Crews with work tasks, the most recently moved first, and how many there are.
    pub fn crew_totals_page(&self, limit: u32, offset: u32) -> Result<(Vec<CrewTotals>, u32)> {
        let conn = self.reader();
        let total: u32 = conn.query_row(
            &format!("SELECT COUNT(DISTINCT crew_id) FROM crew_tasks WHERE {WORK}"),
            [],
            |row| row.get(0),
        )?;
        let mut statement = conn.prepare(&format!(
            "SELECT crew_id, COUNT(*), SUM(state IN ('passed', 'stopped', 'cancelled')), MAX(updated_at) \
             FROM crew_tasks WHERE {WORK} GROUP BY crew_id ORDER BY MAX(updated_at) DESC, crew_id LIMIT ?1 OFFSET ?2"
        ))?;
        let rows = statement
            .query_map([limit, offset], |row| {
                Ok(CrewTotals {
                    crew_id: row.get(0)?,
                    tasks: row.get(1)?,
                    finished: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok((rows, total))
    }

    /// The newest `limit` work tasks of a crew, newest first.
    pub fn crew_work_tasks(&self, crew_id: &str, limit: u32) -> Result<Vec<CrewTask>> {
        let conn = self.reader();
        let mut statement = conn.prepare(&format!(
            "SELECT * FROM crew_tasks WHERE crew_id = ?1 AND {WORK} ORDER BY created_at DESC, rowid DESC LIMIT ?2"
        ))?;
        Ok(statement
            .query_map(rusqlite::params![crew_id, limit], task_row)?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Whether a task is a work task (not a reviewer's run and not a fix).
    pub fn crew_task_is_work(&self, task_id: &str) -> Result<bool> {
        Ok(self.reader().query_row(
            &format!("SELECT COUNT(*) FROM crew_tasks WHERE id = ?1 AND {WORK}"),
            [task_id],
            |row| row.get::<_, u32>(0),
        )? == 1)
    }
}

/// One message by its labels. The body is read inside SQL only for its time and reply link; it never reaches Rust.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageLabels {
    pub id: String,
    pub sender: String,
    pub recipient: String,
    pub kind: String,
    pub at: Option<String>,
    pub in_reply_to: Option<String>,
    /// How the newest delivery row of the message went.
    pub mode: Option<String>,
    pub delivered_at: Option<String>,
}

/// Messages from one sender to one recipient of one kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageEdge {
    pub sender: String,
    pub recipient: String,
    pub kind: String,
    pub count: u32,
    pub mode: Option<String>,
}

/// The newest revision of a board key, without its value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FactLabels {
    pub key: String,
    pub class: Option<String>,
    pub author: String,
    pub rev: u32,
    pub at: Option<String>,
    pub verified: bool,
}

impl Store {
    /// The newest `limit` tasks of a crew of any kind, newest first: the nodes of the map.
    pub fn crew_map_tasks(&self, crew_id: &str, limit: u32) -> Result<Vec<CrewTask>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT * FROM crew_tasks WHERE crew_id = ?1 ORDER BY created_at DESC, rowid DESC LIMIT ?2",
        )?;
        Ok(statement
            .query_map(rusqlite::params![crew_id, limit], task_row)?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// The time of the newest event of each thread, in one query: a seek on `(thread_id, seq)` per thread.
    pub fn thread_last_events(&self, thread_ids: &[String]) -> Result<Vec<(String, String)>> {
        if thread_ids.is_empty() {
            return Ok(Vec::new());
        }
        let marks = vec!["?"; thread_ids.len()].join(",");
        let conn = self.reader();
        let mut statement = conn.prepare(&format!(
            "SELECT e.thread_id, e.ts FROM thread_events e WHERE e.thread_id IN ({marks}) \
             AND e.seq = (SELECT MAX(seq) FROM thread_events WHERE thread_id = e.thread_id)"
        ))?;
        Ok(statement
            .query_map(rusqlite::params_from_iter(thread_ids), |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Messages waiting for each recipient's next turn: the newest delivery row of a message and recipient says `held`.
    pub fn crew_held_counts(&self, crew_id: &str) -> Result<Vec<(String, u32)>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT d.recipient, COUNT(*) FROM crew_deliveries d
             JOIN crew_messages m ON m.id = d.message_id JOIN crew_tasks t ON t.id = m.task_id
             WHERE t.crew_id = ?1 AND d.mode = 'held'
               AND d.id = (SELECT MAX(id) FROM crew_deliveries WHERE message_id = d.message_id AND recipient = d.recipient)
             GROUP BY d.recipient",
        )?;
        Ok(statement
            .query_map([crew_id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// The newest `limit` message pairs by kind, newest first, each with how its newest message was delivered.
    pub fn crew_message_edges(&self, crew_id: &str, limit: u32) -> Result<Vec<MessageEdge>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT m.sender, m.recipient, m.kind, COUNT(*), MAX(m.rowid) FROM crew_messages m
             JOIN crew_tasks t ON t.id = m.task_id WHERE t.crew_id = ?1
             GROUP BY m.sender, m.recipient, m.kind ORDER BY MAX(m.rowid) DESC LIMIT ?2",
        )?;
        let rows: Vec<(String, String, String, u32, i64)> = statement
            .query_map(rusqlite::params![crew_id, limit], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })?
            .collect::<rusqlite::Result<_>>()?;
        let mut newest = conn.prepare(
            "SELECT d.mode FROM crew_deliveries d JOIN crew_messages x ON x.id = d.message_id
             WHERE x.rowid = ?1 ORDER BY d.id DESC LIMIT 1",
        )?;
        let mut edges = Vec::new();
        for (sender, recipient, kind, count, rowid) in rows {
            let mode = newest
                .query_row([rowid], |row| row.get::<_, String>(0))
                .ok();
            edges.push(MessageEdge {
                sender,
                recipient,
                kind,
                count,
                mode,
            });
        }
        Ok(edges)
    }

    /// A page of a crew's messages by labels, newest first, and how many there are. `member` keeps those it sent or received.
    pub fn crew_message_labels(
        &self,
        crew_id: &str,
        member: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<(Vec<MessageLabels>, u32)> {
        let conn = self.reader();
        let total: u32 = conn.query_row(
            "SELECT COUNT(*) FROM crew_messages m JOIN crew_tasks t ON t.id = m.task_id
             WHERE t.crew_id = ?1 AND (?2 IS NULL OR m.sender = ?2 OR m.recipient = ?2)",
            rusqlite::params![crew_id, member],
            |row| row.get(0),
        )?;
        let mut statement = conn.prepare(
            "SELECT m.id, m.sender, m.recipient, m.kind,
                    json_extract(m.body, '$.created_at'), json_extract(m.body, '$.in_reply_to'),
                    (SELECT d.mode FROM crew_deliveries d WHERE d.message_id = m.id ORDER BY d.id DESC LIMIT 1),
                    (SELECT d.at FROM crew_deliveries d WHERE d.message_id = m.id ORDER BY d.id DESC LIMIT 1)
             FROM crew_messages m JOIN crew_tasks t ON t.id = m.task_id
             WHERE t.crew_id = ?1 AND (?2 IS NULL OR m.sender = ?2 OR m.recipient = ?2)
             ORDER BY m.rowid DESC LIMIT ?3 OFFSET ?4",
        )?;
        let rows = statement
            .query_map(rusqlite::params![crew_id, member, limit, offset], |row| {
                Ok(MessageLabels {
                    id: row.get(0)?,
                    sender: row.get(1)?,
                    recipient: row.get(2)?,
                    kind: row.get(3)?,
                    at: row.get(4)?,
                    in_reply_to: row.get(5)?,
                    mode: row.get(6)?,
                    delivered_at: row.get(7)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok((rows, total))
    }

    /// The newest `limit` board keys of a crew, newest first, without their values, and how many keys there are.
    pub fn crew_fact_labels(&self, crew_id: &str, limit: u32) -> Result<(Vec<FactLabels>, u32)> {
        const NEWEST: &str = "f.crew_id = ?1 AND f.rev = (SELECT MAX(rev) FROM crew_facts g WHERE g.crew_id = f.crew_id AND g.key = f.key)";
        let conn = self.reader();
        let total: u32 = conn.query_row(
            &format!("SELECT COUNT(*) FROM crew_facts f WHERE {NEWEST}"),
            [crew_id],
            |row| row.get(0),
        )?;
        let mut statement = conn.prepare(&format!(
            "SELECT f.key, json_extract(f.body, '$.class'), f.author, f.rev, json_extract(f.body, '$.created_at'),
                    COALESCE(json_array_length(json_extract(f.body, '$.evidence')), 0) > 0
             FROM crew_facts f WHERE {NEWEST} ORDER BY f.rowid DESC LIMIT ?2"
        ))?;
        let rows = statement
            .query_map(rusqlite::params![crew_id, limit], |row| {
                Ok(FactLabels {
                    key: row.get(0)?,
                    class: row.get(1)?,
                    author: row.get(2)?,
                    rev: row.get(3)?,
                    at: row.get(4)?,
                    verified: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok((rows, total))
    }
}
