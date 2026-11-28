//! Queued messages and attachment records. A queued message becomes a run in one
//! transaction, so it can neither start twice nor vanish between the two steps.

use std::collections::HashSet;

use orbyn_contracts::{
    Attachment, AttachmentSource, ModelSelection, PullGuide, PullRequest, PullWatch, QueuedMessage,
    Run, RunState, SteerDelivery, SteerFailed, SteerStateEvent,
};
use rusqlite::{OptionalExtension, Row, Transaction, params};

use super::runs::admit_in;
use super::threads::apply_selection;
use super::{Admission, Store, insert_event, parse_json, parse_wire};
use crate::error::{CoreError, Result};
use crate::util::{new_id, now};

/// What a guide write stores; the stops are already checked and serialized.
pub struct PullGuideWrite<'a> {
    pub head_sha: Option<&'a str>,
    pub stops: &'a str,
    pub thread_id: &'a str,
}

fn queued_row(row: &Row<'_>) -> rusqlite::Result<QueuedMessage> {
    let ids: String = row.get("attachment_ids")?;
    Ok(QueuedMessage {
        id: row.get("id")?,
        thread_id: row.get("thread_id")?,
        prompt: row.get("prompt")?,
        attachment_ids: parse_json(3, &ids)?,
        held: row.get("held")?,
        created_at: row.get("created_at")?,
        model: row.get("model")?,
        effort: row.get("effort")?,
        options: parse_json(0, &row.get::<_, String>("options")?)?,
    })
}

fn enqueue_in(
    tx: &Transaction<'_>,
    thread_id: &str,
    prompt: &str,
    attachments: &[String],
    selection: &ModelSelection,
    force_hold: bool,
    crew_note: bool,
) -> Result<String> {
    let id = new_id();
    let stamp = now();
    let ids = serde_json::to_string(attachments)?;
    let options = serde_json::to_string(&selection.options)?;
    // A new message joins any held queue rather than jumping past it.
    let held: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM queued_messages WHERE thread_id = ?1 AND held = 1)",
        [thread_id],
        |row| row.get(0),
    )?;
    let preparation = super::setup::hold_owner(tx, thread_id)?;
    let unrelated: bool = tx.query_row("SELECT EXISTS (SELECT 1 FROM queued_messages WHERE thread_id = ?1 AND held = 1 AND preparation_run_id IS NULL)", [thread_id], |row| row.get(0))?;
    let preparation = preparation.filter(|_| !unrelated && !force_hold);
    let held = force_hold || held || super::setup::failed(tx, thread_id)?;
    tx.execute(
        "INSERT INTO queued_messages (id, thread_id, prompt, attachment_ids, held,
            created_at, model, effort, options, preparation_run_id, crew_note)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            id,
            thread_id,
            prompt,
            ids,
            held,
            stamp,
            selection.model,
            selection.effort,
            options,
            preparation,
            crew_note,
        ],
    )?;
    Ok(id)
}

/// A stored watch, with what its last pass saw.
#[derive(Clone, Debug)]
pub struct WatchRow {
    pub thread_id: String,
    pub repo_id: String,
    pub number: u64,
    pub snapshot: Option<String>,
    pub comment_wakes: i64,
    pub failures: i64,
    pub created_at: String,
}

#[derive(Clone, Debug, Default)]
pub struct WatchUpdate {
    pub snapshot: Option<String>,
    pub comment_wakes: i64,
    pub failures: i64,
}

impl WatchUpdate {
    /// A failed read: the wake count and snapshot stand, so only this rises.
    pub fn failed(failures: i64) -> Self {
        Self {
            failures,
            ..Self::default()
        }
    }
}

fn pull_row(row: &Row<'_>) -> rusqlite::Result<PullRequest> {
    Ok(PullRequest {
        number: row.get::<_, i64>("number")? as u64,
        title: row.get("title")?,
        url: row.get("url")?,
        state: row.get("state")?,
        head: row.get("head")?,
        base: row.get("base")?,
        author: row.get("author")?,
        draft: false,
        updated_at: None,
        labels: Vec::new(),
        review_state: None,
        checks_state: None,
        checks: None,
    })
}

fn attachment_row(row: &Row<'_>) -> rusqlite::Result<(Attachment, String)> {
    let source: Option<String> = row.get("source")?;
    let source = source
        .as_deref()
        .map(|text| parse_json(0, text))
        .transpose()?;
    Ok((
        Attachment {
            id: row.get("id")?,
            thread_id: row.get("thread_id")?,
            name: row.get("name")?,
            mime: row.get("mime")?,
            size: row.get::<_, i64>("size")? as u64,
            created_at: row.get("created_at")?,
            source,
        },
        row.get("path")?,
    ))
}

impl Store {
    pub fn enqueue(
        &self,
        thread_id: &str,
        prompt: &str,
        attachments: &[String],
        selection: &ModelSelection,
    ) -> Result<QueuedMessage> {
        self.enqueue_asked(thread_id, prompt, attachments, selection, false)
    }

    /// Queues a message Crew itself asks the lead to read: a task's result, a review, or a start
    /// it refused. The run that starts from it is judged by what the lead leaves behind, not by
    /// what it says, because the person did not send it (docs/engine-run-contract.md).
    pub fn enqueue_crew_note(
        &self,
        thread_id: &str,
        prompt: &str,
        attachments: &[String],
        selection: &ModelSelection,
    ) -> Result<QueuedMessage> {
        self.enqueue_asked(thread_id, prompt, attachments, selection, true)
    }

    fn enqueue_asked(
        &self,
        thread_id: &str,
        prompt: &str,
        attachments: &[String],
        selection: &ModelSelection,
        crew_note: bool,
    ) -> Result<QueuedMessage> {
        let id = self.commit(|tx, _| {
            enqueue_in(
                tx,
                thread_id,
                prompt,
                attachments,
                selection,
                false,
                crew_note,
            )
        })?;
        self.queued(&id)
    }

    /// Keep a rejected in-flight prompt with the run that received it. A late
    /// callback cannot evade a completed stop's queue hold or a later retry fence.
    pub(crate) fn requeue_failed_steer(
        &self,
        run: &str,
        thread_id: &str,
        message_id: &str,
        prompt: &str,
    ) -> Result<QueuedMessage> {
        let id = self.commit(|tx, appended| {
            let (owner_thread, state, epoch, stopped): (String, String, String, bool) = tx
                .query_row(
                    "SELECT thread_id, state, owner_epoch, stop_requested FROM runs WHERE id = ?1",
                    [run],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()?
                .ok_or_else(|| CoreError::NotFound("run".into()))?;
            if epoch != self.epoch || owner_thread != thread_id {
                return Err(CoreError::StaleOwner);
            }
            let state: RunState = parse_wire(1, state)?;
            let id = enqueue_in(
                tx,
                thread_id,
                prompt,
                &[],
                &ModelSelection::default(),
                stopped || state.is_terminal(),
                false,
            )?;
            insert_event(
                tx,
                appended,
                thread_id,
                None,
                &SteerFailed {
                    message_id: message_id.to_owned(),
                    prompt: prompt.to_owned(),
                }
                .into_event(),
            )?;
            // The refusal and the message's failed mark commit with the queue row.
            insert_event(
                tx,
                appended,
                thread_id,
                None,
                &SteerStateEvent {
                    message_id: message_id.to_owned(),
                    state: SteerDelivery::Failed,
                }
                .into_event(),
            )?;
            Ok(id)
        })?;
        self.queued(&id)
    }

    pub fn queued(&self, id: &str) -> Result<QueuedMessage> {
        self.reader()
            .query_row(
                "SELECT * FROM queued_messages WHERE id = ?1",
                [id],
                queued_row,
            )
            .optional()?
            .ok_or_else(|| CoreError::NotFound("queued message".into()))
    }

    pub fn list_queue(&self, thread_id: &str) -> Result<Vec<QueuedMessage>> {
        let conn = self.reader();
        let mut statement = conn.prepare_cached(
            "SELECT * FROM queued_messages WHERE thread_id = ?1 ORDER BY urgent DESC, rowid",
        )?;
        let rows = statement.query_map([thread_id], queued_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn edit_queued(&self, id: &str, prompt: &str) -> Result<QueuedMessage> {
        self.commit(|tx, _| {
            let changed = tx.execute(
                "UPDATE queued_messages SET prompt = ?2 WHERE id = ?1",
                params![id, prompt],
            )?;
            if changed == 0 {
                return Err(CoreError::NotFound("queued message".into()));
            }
            Ok(())
        })?;
        self.queued(id)
    }

    pub fn remove_queued(&self, id: &str) -> Result<()> {
        self.commit(|tx, _| {
            let changed = tx.execute("DELETE FROM queued_messages WHERE id = ?1", [id])?;
            if changed == 0 {
                return Err(CoreError::NotFound("queued message".into()));
            }
            Ok(())
        })
    }

    /// Puts one message next in line, past any hold.
    pub fn mark_urgent(&self, id: &str) -> Result<QueuedMessage> {
        self.commit(|tx, _| {
            let changed = tx.execute(
                "UPDATE queued_messages SET urgent = 1, held = 0, preparation_run_id = NULL WHERE id = ?1",
                [id],
            )?;
            if changed == 0 {
                return Err(CoreError::NotFound("queued message".into()));
            }
            Ok(())
        })?;
        self.queued(id)
    }

    pub fn release_queue(&self, thread_id: &str) -> Result<usize> {
        self.commit(|tx, _| {
            Ok(tx.execute(
                "UPDATE queued_messages SET held = 0, preparation_run_id = NULL WHERE thread_id = ?1",
                [thread_id],
            )?)
        })
    }

    /// Turns the next message into a run, or does nothing if there is none, it is
    /// held, or the thread is busy. Losing the race leaves the message queued.
    pub fn admit_next(&self, thread_id: &str, actor: &str) -> Result<Option<(Run, QueuedMessage)>> {
        let outcome = self.commit(|tx, appended| {
            let next = tx
                .query_row(
                    "SELECT * FROM queued_messages WHERE thread_id = ?1
                     ORDER BY urgent DESC, rowid LIMIT 1",
                    [thread_id],
                    |row| Ok((queued_row(row)?, row.get::<_, bool>("crew_note")?)),
                )
                .optional()?;
            let Some((message, crew_note)) = next else {
                return Ok(None);
            };
            if super::setup::failed(tx, thread_id)? {
                tx.execute(
                    "UPDATE queued_messages SET preparation_run_id = CASE WHEN held = 0 THEN ?2 ELSE preparation_run_id END, held = 1 WHERE thread_id = ?1",
                    params![thread_id, super::setup::hold_owner(tx, thread_id)?],
                )?;
                return Ok(None);
            }
            if message.held {
                return Ok(None);
            }
            let run = admit_in(
                tx,
                appended,
                &self.epoch,
                &Admission {
                    thread_id,
                    prompt: &message.prompt,
                    actor,
                },
            )?;
            tx.execute("DELETE FROM queued_messages WHERE id = ?1", [&message.id])?;
            tx.execute(
                "UPDATE runs SET queued_message_id = ?2, crew_note = ?3 WHERE id = ?1",
                params![run.id, message.id, crew_note],
            )?;
            apply_selection(
                tx,
                appended,
                thread_id,
                message.model.as_deref(),
                message.effort.as_deref(),
                Some(&message.options),
            )?;
            super::setup::capture_input(tx, &run, &message.prompt, &message.attachment_ids)?;
            Ok(Some((run, message)))
        });
        match outcome {
            Err(CoreError::ThreadBusy) => Ok(None),
            other => self.run_changed(other),
        }
    }

    pub fn add_attachment(&self, attachment: &Attachment, path: &str) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "INSERT INTO attachments (id, thread_id, name, mime, size, path, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    attachment.id,
                    attachment.thread_id,
                    attachment.name,
                    attachment.mime,
                    attachment.size as i64,
                    path,
                    attachment.created_at
                ],
            )?;
            Ok(())
        })
    }

    pub fn list_attachments(&self, thread_id: &str) -> Result<Vec<(Attachment, String)>> {
        let conn = self.reader();
        let mut statement = conn.prepare_cached(
            "SELECT attachments.*, attachment_sources.source AS source
             FROM attachments
             LEFT JOIN attachment_sources ON attachment_sources.attachment_id = attachments.id
             WHERE attachments.thread_id = ?1 ORDER BY attachments.rowid",
        )?;
        let rows = statement.query_map([thread_id], attachment_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn attachment(&self, id: &str) -> Result<(Attachment, String)> {
        let conn = self.reader();
        let mut statement = conn.prepare_cached(
            "SELECT attachments.*, attachment_sources.source AS source
             FROM attachments
             LEFT JOIN attachment_sources ON attachment_sources.attachment_id = attachments.id
             WHERE attachments.id = ?1",
        )?;
        statement
            .query_row([id], attachment_row)
            .optional()?
            .ok_or_else(|| CoreError::NotFound("attachment".into()))
    }

    /// Replaces the source kept beside an attachment. The attachment must exist.
    pub fn set_attachment_source(
        &self,
        attachment_id: &str,
        source: &AttachmentSource,
    ) -> Result<()> {
        let source = serde_json::to_string(source)?;
        let stamp = now();
        self.commit(|tx, _| {
            let exists: bool = tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM attachments WHERE id = ?1)",
                [attachment_id],
                |row| row.get(0),
            )?;
            if !exists {
                return Err(CoreError::NotFound("attachment".into()));
            }
            tx.execute(
                "INSERT INTO attachment_sources (attachment_id, source, created_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT (attachment_id) DO UPDATE SET source = excluded.source,
                    created_at = excluded.created_at",
                params![attachment_id, source, stamp],
            )?;
            Ok(())
        })
    }

    /// The source `set_attachment_source` kept for an attachment, if any.
    pub fn attachment_source(&self, attachment_id: &str) -> Result<Option<AttachmentSource>> {
        let conn = self.reader();
        let source: Option<String> = conn
            .query_row(
                "SELECT source FROM attachment_sources WHERE attachment_id = ?1",
                [attachment_id],
                |row| row.get(0),
            )
            .optional()?;
        source
            .as_deref()
            .map(|text| parse_json(0, text))
            .transpose()
            .map_err(CoreError::from)
    }

    /// Whether the thread's queued messages or a run it admitted already name this attachment.
    pub fn attachment_is_sent(&self, attachment_id: &str, thread_id: &str) -> Result<bool> {
        Ok(self.reader().query_row(
            "SELECT EXISTS (
                 SELECT 1 FROM queued_messages q, json_each(q.attachment_ids) item
                 WHERE q.thread_id = ?1 AND item.value = ?2
             ) OR EXISTS (
                 SELECT 1 FROM thread_setup s
                 WHERE s.thread_id = ?1 AND s.attachment_ids IS NOT NULL
                   AND EXISTS (
                       SELECT 1 FROM json_each(s.attachment_ids) item WHERE item.value = ?2
                   )
             ) OR EXISTS (
                 SELECT 1 FROM thread_events e
                 WHERE e.thread_id = ?1
                   AND json_extract(e.event, '$.type') = 'CUSTOM'
                   AND json_extract(e.event, '$.name') = 'user.attachments'
                   AND json_type(e.event, '$.value.attachments') = 'array'
                   AND EXISTS (
                       SELECT 1 FROM json_each(e.event, '$.value.attachments') item
                       WHERE json_extract(item.value, '$.id') = ?2
                   )
             )",
            params![thread_id, attachment_id],
            |row| row.get(0),
        )?)
    }

    /// Links a pull request of one repository of the thread; the thread's own project is its primary repository.
    pub fn link_pull(&self, thread_id: &str, repo_id: &str, pull: &PullRequest) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "INSERT INTO thread_pull_requests
                    (thread_id, repo_id, number, url, title, state, head, base, author)
                 VALUES (?1, ?9, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT (thread_id, repo_id, number) DO UPDATE SET url = excluded.url,
                    title = excluded.title, state = excluded.state, head = excluded.head,
                    base = excluded.base, author = excluded.author",
                params![
                    thread_id,
                    pull.number as i64,
                    pull.url,
                    pull.title,
                    pull.state,
                    pull.head,
                    pull.base,
                    pull.author,
                    repo_id
                ],
            )?;
            Ok(())
        })
    }

    /// The pull requests a thread linked in one repository.
    pub fn linked_pulls(&self, thread_id: &str, repo_id: &str) -> Result<Vec<PullRequest>> {
        let conn = self.reader();
        let mut statement = conn.prepare_cached(
            "SELECT * FROM thread_pull_requests WHERE thread_id = ?1 AND repo_id = ?2 ORDER BY number",
        )?;
        let rows = statement.query_map(params![thread_id, repo_id], pull_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The (thread, repository) pairs with a pull request not yet known to be finished.
    pub fn threads_with_open_pulls(&self) -> Result<Vec<(String, String)>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT DISTINCT thread_id, repo_id FROM thread_pull_requests WHERE state = 'open'",
        )?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Starts watching one repository's pull request. A pull request watched before keeps what it last reported.
    pub fn watch_pull(&self, thread_id: &str, repo_id: &str, number: u64) -> Result<PullWatch> {
        let created_at = now();
        self.commit(|tx, _| {
            tx.execute(
                "INSERT INTO pull_watches (thread_id, repo_id, number, created_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (thread_id, repo_id, number) DO NOTHING",
                params![thread_id, repo_id, number as i64, created_at],
            )?;
            Ok(())
        })?;
        Ok(PullWatch {
            thread_id: thread_id.to_owned(),
            repo_id: repo_id.to_owned(),
            number,
            created_at,
        })
    }

    pub fn unwatch_pull(&self, thread_id: &str, repo_id: &str, number: u64) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "DELETE FROM pull_watches WHERE thread_id = ?1 AND repo_id = ?2 AND number = ?3",
                params![thread_id, repo_id, number as i64],
            )?;
            Ok(())
        })
    }

    pub fn update_watch(
        &self,
        thread_id: &str,
        repo_id: &str,
        number: u64,
        update: WatchUpdate,
    ) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "UPDATE pull_watches SET snapshot = COALESCE(?4, snapshot), comment_wakes = ?5,
                    failures = ?6 WHERE thread_id = ?1 AND repo_id = ?2 AND number = ?3",
                params![
                    thread_id,
                    repo_id,
                    number as i64,
                    update.snapshot,
                    update.comment_wakes,
                    update.failures
                ],
            )?;
            Ok(())
        })
    }

    /// One thread's watches, or every watch when `None`.
    pub fn pull_watches(&self, thread_id: Option<&str>) -> Result<Vec<WatchRow>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT * FROM pull_watches WHERE ?1 IS NULL OR thread_id = ?1 ORDER BY created_at",
        )?;
        let rows = statement.query_map([thread_id], |row| {
            Ok(WatchRow {
                thread_id: row.get("thread_id")?,
                repo_id: row.get("repo_id")?,
                number: row.get::<_, i64>("number")? as u64,
                snapshot: row.get("snapshot")?,
                comment_wakes: row.get("comment_wakes")?,
                failures: row.get("failures")?,
                created_at: row.get("created_at")?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn viewed_files(&self, project_id: &str, number: u64) -> Result<HashSet<String>> {
        let conn = self.reader();
        let mut statement = conn
            .prepare("SELECT path FROM pull_files_viewed WHERE project_id = ?1 AND number = ?2")?;
        let rows = statement.query_map(params![project_id, number as i64], |row| row.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn set_files_viewed(
        &self,
        project_id: &str,
        number: u64,
        paths: &[String],
        viewed: bool,
    ) -> Result<()> {
        self.commit(|tx, _| {
            for path in paths {
                let sql = if viewed {
                    "INSERT OR IGNORE INTO pull_files_viewed (project_id, number, path)
                     VALUES (?1, ?2, ?3)"
                } else {
                    "DELETE FROM pull_files_viewed
                     WHERE project_id = ?1 AND number = ?2 AND path = ?3"
                };
                tx.execute(sql, params![project_id, number as i64, path])?;
            }
            Ok(())
        })
    }

    /// Replaces the project's review guide for one pull request. `stops` is the
    /// already checked JSON array of stops.
    pub fn write_pull_guide(
        &self,
        project_id: &str,
        number: u64,
        guide: &PullGuideWrite<'_>,
    ) -> Result<String> {
        let written_at = now();
        self.commit(|tx, _| {
            tx.execute(
                "INSERT INTO pull_guides (project_id, number, head_sha, stops, thread_id, written_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT (project_id, number) DO UPDATE SET head_sha = excluded.head_sha,
                    stops = excluded.stops, thread_id = excluded.thread_id,
                    written_at = excluded.written_at",
                params![
                    project_id,
                    number as i64,
                    guide.head_sha,
                    guide.stops,
                    guide.thread_id,
                    written_at
                ],
            )?;
            Ok(())
        })?;
        Ok(written_at)
    }

    /// The project's review guide for one pull request, if one was written.
    pub fn pull_guide(&self, project_id: &str, number: u64) -> Result<Option<PullGuide>> {
        let conn = self.reader();
        let mut statement =
            conn.prepare_cached("SELECT * FROM pull_guides WHERE project_id = ?1 AND number = ?2")?;
        Ok(statement
            .query_row(params![project_id, number as i64], |row| {
                Ok(PullGuide {
                    number: row.get::<_, i64>("number")? as u64,
                    head_sha: row.get("head_sha")?,
                    stops: parse_json(3, &row.get::<_, String>("stops")?)?,
                    thread_id: row.get("thread_id")?,
                    written_at: row.get("written_at")?,
                })
            })
            .optional()?)
    }

    /// Drops a link and the watch on the same repository's pull request, if there is one.
    pub fn unlink_pull(&self, thread_id: &str, repo_id: &str, number: u64) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "DELETE FROM pull_watches WHERE thread_id = ?1 AND repo_id = ?2 AND number = ?3",
                params![thread_id, repo_id, number as i64],
            )?;
            let changed = tx.execute(
                "DELETE FROM thread_pull_requests WHERE thread_id = ?1 AND repo_id = ?2 AND number = ?3",
                params![thread_id, repo_id, number as i64],
            )?;
            if changed == 0 {
                return Err(CoreError::NotFound("pull request".into()));
            }
            Ok(())
        })
    }

    /// Stored files of a thread and its descendants, for cleanup after deletion.
    pub fn family_attachment_paths(&self, thread_id: &str) -> Result<Vec<String>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "WITH RECURSIVE family(id) AS (
                SELECT id FROM threads WHERE id = ?1
                UNION SELECT threads.id FROM threads
                    JOIN family ON threads.parent_thread_id = family.id
             ) SELECT path FROM attachments WHERE thread_id IN (SELECT id FROM family)",
        )?;
        let rows = statement.query_map([thread_id], |row| row.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Deletes the record and returns the stored file's path.
    pub fn remove_attachment(&self, id: &str) -> Result<String> {
        self.commit(|tx, _| {
            let path: String = tx
                .query_row("SELECT path FROM attachments WHERE id = ?1", [id], |row| {
                    row.get(0)
                })
                .optional()?
                .ok_or_else(|| CoreError::NotFound("attachment".into()))?;
            tx.execute(
                "DELETE FROM attachment_sources WHERE attachment_id = ?1",
                [id],
            )?;
            tx.execute("DELETE FROM attachments WHERE id = ?1", [id])?;
            Ok(path)
        })
    }
}
