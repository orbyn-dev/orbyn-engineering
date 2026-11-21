//! A thread's transcript at rest: the read the export needs and the one write the import makes. The import is one transaction
//! that adds the thread row (marked with `source` and `imported_at`), its messages as ordinary log events, and the audit row.
//! It writes no file and stores no engine session id.

use orbyn_contracts::{AgUiEvent, Thread, TranscriptMessage};
use rusqlite::params;
use serde_json::json;

use super::{AuditRecord, NewThread, Store, insert_audit};
use crate::error::Result;
use crate::util::{new_id, now};

/// What the import stores for the transcript's origin.
pub const TRANSCRIPT_SOURCE: &str = "orbyn-transcript";

impl Store {
    /// The newest `limit` user and assistant messages of a thread, oldest first, and how many such messages it has.
    pub fn transcript_messages(
        &self,
        thread_id: &str,
        limit: usize,
    ) -> Result<(Vec<TranscriptMessage>, usize)> {
        self.thread(thread_id)?;
        let conn = self.reader();
        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM thread_messages
             WHERE thread_id = ?1 AND role IN ('user', 'assistant') AND text <> ''",
            [thread_id],
            |row| row.get(0),
        )?;
        let mut statement = conn.prepare(
            "SELECT role, ts, text FROM thread_messages
             WHERE thread_id = ?1 AND role IN ('user', 'assistant') AND text <> ''
             ORDER BY seq DESC LIMIT ?2",
        )?;
        let rows = statement.query_map(
            params![thread_id, i64::try_from(limit).unwrap_or(i64::MAX)],
            |row| {
                Ok(TranscriptMessage {
                    role: row.get(0)?,
                    ts: row.get(1)?,
                    text: row.get(2)?,
                })
            },
        )?;
        let mut messages = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        messages.reverse();
        Ok((messages, usize::try_from(total).unwrap_or(0)))
    }

    /// Creates the imported thread, its messages and the `thread.imported` audit row together. `messages` are already admitted.
    pub fn import_transcript_thread(
        &self,
        new: NewThread,
        messages: &[TranscriptMessage],
        dropped: usize,
    ) -> Result<Thread> {
        let id = new_id();
        let stamp = now();
        self.commit(|tx, _| {
            tx.execute(
                "INSERT INTO threads (id, project_id, title, engine, model, cwd,
                    created_at, updated_at, position, source, imported_at)
                 VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?6,
                    (SELECT COALESCE(MIN(position), 0) - 1 FROM threads), ?7, ?6)",
                params![
                    id,
                    new.project_id,
                    new.title,
                    new.engine,
                    new.cwd,
                    stamp,
                    TRANSCRIPT_SOURCE
                ],
            )?;
            let mut seq: i64 = 1;
            for (index, message) in messages.iter().enumerate() {
                let message_id = format!("imported-{index}");
                let first = seq;
                for event in [
                    AgUiEvent::TextMessageStart {
                        message_id: message_id.clone(),
                        role: message.role.clone(),
                    },
                    AgUiEvent::TextMessageContent {
                        message_id: message_id.clone(),
                        delta: message.text.clone(),
                    },
                    AgUiEvent::TextMessageEnd {
                        message_id: message_id.clone(),
                    },
                ] {
                    tx.execute(
                        "INSERT INTO thread_events (thread_id, seq, run_id, ts, event)
                         VALUES (?1, ?2, NULL, ?3, ?4)",
                        params![id, seq, message.ts, serde_json::to_string(&event)?],
                    )?;
                    seq += 1;
                }
                tx.execute(
                    "INSERT INTO thread_messages (thread_id, run_key, message_id, seq, role, ts, text)
                     VALUES (?1, '', ?2, ?3, ?4, ?5, ?6)",
                    params![id, message_id, first, message.role, message.ts, message.text],
                )?;
            }
            tx.execute(
                "UPDATE threads SET next_seq = ?2 WHERE id = ?1",
                params![id, seq],
            )?;
            // Counts and the origin only: no title, no text.
            insert_audit(
                tx,
                &AuditRecord {
                    actor: "user".into(),
                    action: "thread.imported".into(),
                    target: id.clone(),
                    decision: None,
                    source: "user".into(),
                    thread_id: Some(id.clone()),
                    run_id: None,
                    detail: json!({
                        "source": TRANSCRIPT_SOURCE,
                        "messages": messages.len(),
                        "dropped": dropped,
                    }),
                },
            )?;
            Ok(())
        })?;
        self.thread(&id)
    }
}
