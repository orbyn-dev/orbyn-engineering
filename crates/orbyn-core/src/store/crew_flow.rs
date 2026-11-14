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
    /// How the message was delivered, only when every recipient's newest delivery row says the same; else none.
    pub mode: Option<String>,
    pub delivered_at: Option<String>,
    /// How many recipients have a delivery row, so a fan-out still under way can be told from a finished one.
    pub recipients: u32,
}

/// Messages from one sender to one recipient of one kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageEdge {
    pub sender: String,
    pub recipient: String,
    pub kind: String,
    pub count: u32,
    pub mode: Option<String>,
    /// The newest message's time and how many recipients have a delivery row for it.
    pub newest_at: Option<String>,
    pub recipients: u32,
}

/// One member's task of a crew, with the times that say when it could be sent to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberTimes {
    pub member: String,
    pub created_at: String,
    pub updated_at: String,
    pub state: String,
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

/// How many of a crew's newest messages, and of its newest deliveries, the grouped reads look at. The window is the crew's own, so
/// another crew's traffic never cuts it; when it cuts the crew's rows the read says so.
pub const MESSAGE_WINDOW: u32 = 5000;
pub const DELIVERY_WINDOW: u32 = 20000;

/// The row id of the oldest row inside a crew's newest `window` rows, and whether older rows were left out. Finding it sorts all of the
/// crew's rows by id: they sit under several tasks, and no index orders them across tasks (that needs a `(crew, id)` index, so a
/// migration), so the cost grows with the crew's whole history while the rows the grouped reads then scan stay inside the window.
/// Acceptable for Preview; the answer is exact. `sql` selects one row id
/// per row of the crew, newest first, with `?1` the crew and `?2` the number to read.
fn window_floor(
    conn: &rusqlite::Connection,
    sql: &str,
    crew_id: &str,
    window: u32,
) -> Result<(i64, bool)> {
    let mut statement = conn.prepare(sql)?;
    let ids: Vec<i64> = statement
        .query_map(
            rusqlite::params![crew_id, window.saturating_add(1)],
            |row| row.get(0),
        )?
        .collect::<rusqlite::Result<_>>()?;
    let keep = window.max(1) as usize;
    Ok(match ids.get(keep - 1) {
        Some(floor) if ids.len() > keep => (*floor, true),
        _ => (0, false),
    })
}

const MESSAGE_ROWS: &str = "SELECT m.rowid FROM crew_messages m JOIN crew_tasks t ON t.id = m.task_id WHERE t.crew_id = ?1 ORDER BY m.rowid DESC LIMIT ?2";
const DELIVERY_ROWS: &str = "SELECT d.id FROM crew_deliveries d JOIN crew_messages m ON m.id = d.message_id JOIN crew_tasks t ON t.id = m.task_id WHERE t.crew_id = ?1 ORDER BY d.id DESC LIMIT ?2";

/// The statement behind [`Store::thread_last_events`] for `n` threads: a `VALUES` list of ids, each looked up by the primary key.
pub(crate) fn last_events_sql(n: usize) -> String {
    let ids = vec!["(?)"; n].join(",");
    format!(
        "SELECT v.column1, (SELECT ts FROM thread_events WHERE thread_id = v.column1 ORDER BY seq DESC LIMIT 1) \
         FROM (VALUES {ids}) v"
    )
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

    /// The time of the newest event of each thread, in one query: one seek on the `(thread_id, seq)` key per thread, never a scan.
    pub fn thread_last_events(&self, thread_ids: &[String]) -> Result<Vec<(String, String)>> {
        if thread_ids.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.reader();
        let mut statement = conn.prepare(&last_events_sql(thread_ids.len()))?;
        Ok(statement
            .query_map(rusqlite::params_from_iter(thread_ids), |row| {
                Ok((row.get(0)?, row.get::<_, Option<String>>(1)?))
            })?
            .filter_map(|row| match row {
                Ok((id, Some(ts))) => Some(Ok((id, ts))),
                Ok((_, None)) => None,
                Err(error) => Some(Err(error)),
            })
            .collect::<rusqlite::Result<_>>()?)
    }

    // The grouped reads below scale with the crew's bus traffic. They look at the crew's own newest rows (a window, bounded by the
    // crew id) and say when it cut older ones: enough for Preview, and a long-lived crew cannot make a map read slow.

    /// Messages waiting for each recipient's next turn: the newest delivery row of a message and recipient says `held`. The flag is
    /// set when the crew's delivery window cut older rows.
    pub fn crew_held_counts(
        &self,
        crew_id: &str,
        window: u32,
    ) -> Result<(Vec<(String, u32)>, bool)> {
        let conn = self.reader();
        let (floor, cut) = window_floor(&conn, DELIVERY_ROWS, crew_id, window)?;
        let mut statement = conn.prepare(
            "SELECT d.recipient, COUNT(*) FROM crew_deliveries d
             JOIN crew_messages m ON m.id = d.message_id JOIN crew_tasks t ON t.id = m.task_id
             WHERE t.crew_id = ?1 AND d.mode = 'held' AND d.id >= ?2
               AND d.id = (SELECT MAX(id) FROM crew_deliveries WHERE message_id = d.message_id AND recipient = d.recipient)
             GROUP BY d.recipient LIMIT 200",
        )?;
        let rows = statement
            .query_map(rusqlite::params![crew_id, floor], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok((rows, cut))
    }

    /// The members of a crew with the times of their tasks, newest first: who a message to `all` could have gone to. The flag is set when
    /// the crew has more than `limit` tasks, so the list is not everyone.
    pub fn crew_member_times(&self, crew_id: &str, limit: u32) -> Result<(Vec<MemberTimes>, bool)> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT r.member, t.created_at, t.updated_at, t.state FROM crew_tasks t JOIN crew_runs r ON r.task_id = t.id
             WHERE t.crew_id = ?1 ORDER BY t.created_at DESC, t.rowid DESC LIMIT ?2",
        )?;
        let mut members: Vec<MemberTimes> = statement
            .query_map(rusqlite::params![crew_id, limit.saturating_add(1)], |row| {
                Ok(MemberTimes {
                    member: row.get(0)?,
                    created_at: row.get(1)?,
                    updated_at: row.get(2)?,
                    state: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        let cut = members.len() > limit as usize;
        members.truncate(limit as usize);
        Ok((members, cut))
    }

    /// The newest `limit` message pairs by kind within the crew's message window, newest first, each with how its newest message was
    /// delivered. The flag is set when the window cut older messages, so the counts cover recent traffic only.
    pub fn crew_message_edges(
        &self,
        crew_id: &str,
        limit: u32,
        window: u32,
    ) -> Result<(Vec<MessageEdge>, bool)> {
        let conn = self.reader();
        let (floor, cut) = window_floor(&conn, MESSAGE_ROWS, crew_id, window)?;
        let mut statement = conn.prepare(
            "SELECT m.sender, m.recipient, m.kind, COUNT(*), MAX(m.rowid) FROM crew_messages m
             JOIN crew_tasks t ON t.id = m.task_id WHERE t.crew_id = ?1 AND m.rowid >= ?3
             GROUP BY m.sender, m.recipient, m.kind ORDER BY MAX(m.rowid) DESC LIMIT ?2",
        )?;
        let rows: Vec<(String, String, String, u32, i64)> = statement
            .query_map(rusqlite::params![crew_id, limit, floor], |row| {
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
            "SELECT CASE WHEN COUNT(DISTINCT d.mode) = 1 THEN MIN(d.mode) END, COUNT(DISTINCT d.recipient),
                    (SELECT json_extract(body, '$.created_at') FROM crew_messages WHERE rowid = ?1)
             FROM crew_deliveries d JOIN crew_messages x ON x.id = d.message_id
             WHERE x.rowid = ?1
               AND d.id = (SELECT MAX(id) FROM crew_deliveries WHERE message_id = d.message_id AND recipient = d.recipient)",
        )?;
        let mut edges = Vec::new();
        for (sender, recipient, kind, count, rowid) in rows {
            // The mode only when every recipient's newest row agrees; otherwise none, never one recipient's state.
            let (mode, recipients, newest_at) = newest
                .query_row([rowid], |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, u32>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                })
                .unwrap_or((None, 0, None));
            edges.push(MessageEdge {
                sender,
                recipient,
                kind,
                count,
                mode,
                newest_at,
                recipients,
            });
        }
        Ok((edges, cut))
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
                    (SELECT CASE WHEN COUNT(DISTINCT d.mode) = 1 THEN MIN(d.mode) END FROM crew_deliveries d
                      WHERE d.message_id = m.id
                        AND d.id = (SELECT MAX(id) FROM crew_deliveries WHERE message_id = d.message_id AND recipient = d.recipient)),
                    (SELECT CASE WHEN COUNT(DISTINCT d.mode) = 1 THEN MAX(d.at) END FROM crew_deliveries d
                      WHERE d.message_id = m.id
                        AND d.id = (SELECT MAX(id) FROM crew_deliveries WHERE message_id = d.message_id AND recipient = d.recipient)),
                    (SELECT COUNT(DISTINCT d.recipient) FROM crew_deliveries d WHERE d.message_id = m.id)
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
                    recipients: row.get(8)?,
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

#[cfg(test)]
mod tests {
    use rusqlite::StatementStatus;

    use super::*;
    use crate::testing::TempDir;

    /// A lookup that finds the newest event by its time instead of by its key: `ts` has no index, so it visits every event of the
    /// thread. This is the mutation the test must catch.
    fn walk_every_event_sql(n: usize) -> String {
        let marks = vec!["?"; n].join(",");
        format!(
            "SELECT e.thread_id, e.ts FROM thread_events e WHERE e.thread_id IN ({marks}) \
             AND e.ts = (SELECT MAX(ts) FROM thread_events WHERE thread_id = e.thread_id)"
        )
    }

    /// The correlated lookup the first version used: by the key, so SQLite seeks it too. It is kept to show the measure does not call a
    /// seek a walk.
    fn correlated_seq_sql(n: usize) -> String {
        let marks = vec!["?"; n].join(",");
        format!(
            "SELECT e.thread_id, e.ts FROM thread_events e WHERE e.thread_id IN ({marks}) \
             AND e.seq = (SELECT MAX(seq) FROM thread_events WHERE thread_id = e.thread_id)"
        )
    }

    /// The virtual machine steps SQLite takes to answer `sql` for one thread: work, not time, so the test is the same on every machine.
    fn steps(store: &Store, sql: &str, thread: &str) -> i32 {
        let conn = store.reader();
        let mut statement = conn.prepare(sql).unwrap();
        let rows = statement
            .query_map([thread], |row| row.get::<_, String>(0))
            .unwrap()
            .count();
        assert_eq!(rows, 1, "the thread's newest event is found");
        statement.get_status(StatementStatus::VmStep)
    }

    #[test]
    fn the_last_event_lookup_does_the_same_work_for_five_events_and_for_five_thousand() {
        let dir = TempDir::new();
        let store = Store::open(&dir.path().join("orbyn.sqlite3"), "epoch-a").unwrap();
        {
            let writer = store.writer();
            writer
                .execute_batch("PRAGMA foreign_keys = OFF; BEGIN")
                .unwrap();
            let mut insert = writer
                .prepare("INSERT INTO thread_events (thread_id, seq, run_id, ts, event) VALUES (?1, ?2, NULL, 't' || printf('%07d', ?2), '{}')")
                .unwrap();
            for (thread, count) in [("few", 5), ("many", 5000)] {
                for seq in 1..=count {
                    insert.execute(rusqlite::params![thread, seq]).unwrap();
                }
            }
            drop(insert);
            writer.execute_batch("COMMIT").unwrap();
        }
        let (new, old, keyed) = (
            last_events_sql(1),
            walk_every_event_sql(1),
            correlated_seq_sql(1),
        );
        let (new_few, new_many) = (steps(&store, &new, "few"), steps(&store, &new, "many"));
        let (old_few, old_many) = (steps(&store, &old, "few"), steps(&store, &old, "many"));
        let (keyed_few, keyed_many) = (steps(&store, &keyed, "few"), steps(&store, &keyed, "many"));
        assert!(
            keyed_many <= keyed_few + 10,
            "a lookup by key is a seek: {keyed_few} then {keyed_many}"
        );
        // The measure can tell a walk from a seek: a lookup by time grows with the thread's events (the mutation this test guards against).
        assert!(
            old_many > 50 * old_few,
            "the old walk did not grow: {old_few} then {old_many}"
        );
        // One seek per thread: the work does not depend on how many events the thread has.
        assert!(
            new_many <= new_few + 10,
            "the lookup grew with the thread: {new_few} then {new_many}"
        );
    }
}
