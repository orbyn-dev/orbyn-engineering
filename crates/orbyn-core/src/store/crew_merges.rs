//! The merge queue and the Review queue at rest. The queue row moves through its states; a merge row is
//! added once, when the merge reaches the branch, and only its review, its watch and its revert change after.

use rusqlite::{OptionalExtension, Row, params};
use serde_json::Value;

use super::Store;
use crate::error::{CoreError, Result};
use crate::util::unix_now;

/// One task asked to merge.
#[derive(Clone, Debug, PartialEq)]
pub struct MergeEntry {
    pub task_id: String,
    pub crew_id: String,
    pub repo: String,
    pub branch: String,
    pub candidate: String,
    pub state: String,
    pub risk: String,
    pub reasons: Value,
    pub areas: Vec<String>,
    /// The sample sizes the risk score read, in words.
    pub samples: Vec<String>,
    pub engine: String,
    pub files: u32,
    pub lines: u32,
    pub mode: String,
    pub note: Option<String>,
    /// A person let it go: it is reviewed by that act and skips the holds.
    pub released: bool,
    pub queued_unix: i64,
}

/// One merge that reached the branch: an item of the Review queue.
#[derive(Clone, Debug, PartialEq)]
pub struct MergeRecord {
    pub id: i64,
    pub task_id: String,
    pub crew_id: String,
    pub repo: String,
    pub branch: String,
    pub before_commit: String,
    pub merge_commit: String,
    pub candidate: String,
    pub risk: String,
    pub reasons: Value,
    pub areas: Vec<String>,
    pub engine: String,
    pub mode: String,
    pub files: u32,
    pub lines: u32,
    pub proof: Value,
    pub not_verified: Value,
    pub merged_unix: i64,
    pub due_unix: Option<i64>,
    pub review_state: String,
    pub comment: Option<String>,
    pub watch: String,
    pub reverted_commit: Option<String>,
    pub needs_person: Option<String>,
}

fn json(text: String) -> rusqlite::Result<Value> {
    serde_json::from_str(&text).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn entry_row(row: &Row<'_>) -> rusqlite::Result<MergeEntry> {
    Ok(MergeEntry {
        task_id: row.get("task_id")?,
        crew_id: row.get("crew_id")?,
        repo: row.get("repo")?,
        branch: row.get("branch")?,
        candidate: row.get("candidate")?,
        state: row.get("state")?,
        risk: row.get("risk")?,
        reasons: json(row.get("reasons")?)?,
        areas: strings(&json(row.get("areas")?)?),
        samples: strings(&json(row.get("samples")?)?),
        engine: row.get("engine")?,
        files: row.get("files")?,
        lines: row.get("lines")?,
        mode: row.get("mode")?,
        note: row.get("note")?,
        released: row.get::<_, i64>("released")? == 1,
        queued_unix: row.get("queued_unix")?,
    })
}

fn merge_row(row: &Row<'_>) -> rusqlite::Result<MergeRecord> {
    let areas = strings(&json(row.get("areas")?)?);
    Ok(MergeRecord {
        id: row.get("id")?,
        task_id: row.get("task_id")?,
        crew_id: row.get("crew_id")?,
        repo: row.get("repo")?,
        branch: row.get("branch")?,
        before_commit: row.get("before_commit")?,
        merge_commit: row.get("merge_commit")?,
        candidate: row.get("candidate")?,
        risk: row.get("risk")?,
        reasons: json(row.get("reasons")?)?,
        areas,
        engine: row.get("engine")?,
        mode: row.get("mode")?,
        files: row.get("files")?,
        lines: row.get("lines")?,
        proof: json(row.get("proof")?)?,
        not_verified: json(row.get("not_verified")?)?,
        merged_unix: row.get("merged_unix")?,
        due_unix: row.get("due_unix")?,
        review_state: row.get("review_state")?,
        comment: row.get("comment")?,
        watch: row.get("watch")?,
        reverted_commit: row.get("reverted_commit")?,
        needs_person: row.get("needs_person")?,
    })
}

impl Store {
    /// Puts a task in the queue. A task already merged is refused; one still waiting is replaced.
    pub fn add_merge_entry(&self, entry: &MergeEntry) -> Result<()> {
        let reasons = serde_json::to_string(&entry.reasons)?;
        let areas = serde_json::to_string(&entry.areas)?;
        let samples = serde_json::to_string(&entry.samples)?;
        self.commit(|tx, _| {
            let existing: Option<String> = tx
                .query_row("SELECT state FROM crew_merge_queue WHERE task_id = ?1", [&entry.task_id], |r| r.get(0))
                .optional()?;
            if matches!(existing.as_deref(), Some("merged" | "merging")) {
                return Err(CoreError::Invalid("this task is already merged or merging".into()));
            }
            tx.execute(
                "INSERT INTO crew_merge_queue (task_id, crew_id, repo, branch, candidate, state, risk, reasons, areas, samples, engine, files, lines, mode, note, queued_unix, updated_unix)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?16)
                 ON CONFLICT(task_id) DO UPDATE SET repo = ?3, branch = ?4, candidate = ?5, state = ?6, risk = ?7, reasons = ?8,
                     areas = ?9, samples = ?10, engine = ?11, files = ?12, lines = ?13, mode = ?14, note = ?15, updated_unix = ?16",
                params![entry.task_id, entry.crew_id, entry.repo, entry.branch, entry.candidate, entry.state, entry.risk, reasons, areas, samples, entry.engine, entry.files, entry.lines, entry.mode, entry.note, entry.queued_unix],
            )?;
            Ok(())
        })
    }

    pub fn merge_entry(&self, task_id: &str) -> Result<Option<MergeEntry>> {
        Ok(self
            .reader()
            .query_row(
                "SELECT * FROM crew_merge_queue WHERE task_id = ?1",
                [task_id],
                entry_row,
            )
            .optional()?)
    }

    /// The entries of a target in the given states, in the order they were queued.
    pub fn merge_entries(
        &self,
        repo: &str,
        branch: &str,
        states: &[&str],
    ) -> Result<Vec<MergeEntry>> {
        let conn = self.reader();
        let mut statement = conn.prepare("SELECT * FROM crew_merge_queue WHERE repo = ?1 AND branch = ?2 ORDER BY queued_unix, rowid")?;
        let rows = statement
            .query_map(params![repo, branch], entry_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows
            .into_iter()
            .filter(|e| states.contains(&e.state.as_str()))
            .collect())
    }

    /// One page of the queue across targets, in the order queued, with the number of entries in the given states.
    pub fn merge_entries_page(
        &self,
        states: &[&str],
        limit: u32,
        offset: u32,
    ) -> Result<(Vec<MergeEntry>, u32)> {
        let marks = vec!["?"; states.len()].join(",");
        let filter = if states.is_empty() {
            String::new()
        } else {
            format!("WHERE state IN ({marks})")
        };
        let mut args: Vec<rusqlite::types::Value> = states
            .iter()
            .map(|s| rusqlite::types::Value::from((*s).to_owned()))
            .collect();
        let conn = self.reader();
        let total: u32 = conn.query_row(
            &format!("SELECT COUNT(*) FROM crew_merge_queue {filter}"),
            rusqlite::params_from_iter(args.iter()),
            |row| row.get(0),
        )?;
        args.push(i64::from(limit).into());
        args.push(i64::from(offset).into());
        let mut statement = conn.prepare(&format!(
            "SELECT * FROM crew_merge_queue {filter} ORDER BY queued_unix, rowid LIMIT ? OFFSET ?"
        ))?;
        let rows = statement
            .query_map(rusqlite::params_from_iter(args.iter()), entry_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok((rows, total))
    }

    /// A person releases a waiting entry: it is queued again, and every later move keeps the release.
    pub fn release_merge_entry(&self, task_id: &str) -> Result<()> {
        self.commit(|tx, _| {
            let changed = tx.execute(
                "UPDATE crew_merge_queue SET released = 1, state = 'queued', note = 'released by a person', updated_unix = ?2
                 WHERE task_id = ?1 AND state = 'waiting'",
                params![task_id, unix_now()],
            )?;
            if changed == 0 {
                return Err(CoreError::Invalid("only a waiting merge can be released".into()));
            }
            Ok(())
        })
    }

    /// Moves an entry to a state, with a note. `merged` has its own writer; this refuses it.
    pub fn set_merge_entry_state(
        &self,
        task_id: &str,
        state: &str,
        note: Option<&str>,
    ) -> Result<()> {
        if state == "merged" {
            return Err(CoreError::Invalid(
                "a merge is recorded with add_crew_merge".into(),
            ));
        }
        self.commit(|tx, _| {
            let changed = tx.execute(
                "UPDATE crew_merge_queue SET state = ?2, note = ?3, updated_unix = ?4 WHERE task_id = ?1 AND state != 'merged'",
                params![task_id, state, note, unix_now()],
            )?;
            if changed == 0 {
                return Err(CoreError::NotFound(format!("queued merge {task_id}")));
            }
            Ok(())
        })
    }

    /// Records a merge that reached the branch and marks its queue entry `merged`, in one transaction.
    pub fn add_crew_merge(&self, record: &MergeRecord) -> Result<i64> {
        let (reasons, areas) = (
            serde_json::to_string(&record.reasons)?,
            serde_json::to_string(&record.areas)?,
        );
        let (proof, not_verified) = (
            serde_json::to_string(&record.proof)?,
            serde_json::to_string(&record.not_verified)?,
        );
        self.commit(|tx, _| {
            tx.execute(
                "INSERT INTO crew_merges (task_id, crew_id, repo, branch, before_commit, merge_commit, candidate, risk, reasons, areas, engine, mode,
                     files, lines, proof, not_verified, merged_unix, due_unix)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
                params![record.task_id, record.crew_id, record.repo, record.branch, record.before_commit, record.merge_commit, record.candidate,
                    record.risk, reasons, areas, record.engine, record.mode, record.files, record.lines, proof, not_verified, record.merged_unix, record.due_unix],
            )?;
            let id = tx.last_insert_rowid();
            tx.execute("UPDATE crew_merge_queue SET state = 'merged', updated_unix = ?2 WHERE task_id = ?1", params![record.task_id, unix_now()])?;
            Ok(id)
        })
    }

    /// Every merge to a target, oldest first.
    pub fn crew_merges(&self, repo: &str, branch: &str) -> Result<Vec<MergeRecord>> {
        let conn = self.reader();
        let mut statement =
            conn.prepare("SELECT * FROM crew_merges WHERE repo = ?1 AND branch = ?2 ORDER BY id")?;
        Ok(statement
            .query_map(params![repo, branch], merge_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Every merge in every target, oldest first.
    pub fn crew_all_merges(&self) -> Result<Vec<MergeRecord>> {
        let conn = self.reader();
        let mut statement = conn.prepare("SELECT * FROM crew_merges ORDER BY id")?;
        Ok(statement
            .query_map([], merge_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// One page of merges, newest first, and the number of merges in all.
    pub fn crew_merges_page(&self, limit: u32, offset: u32) -> Result<(Vec<MergeRecord>, u32)> {
        let conn = self.reader();
        let total: u32 =
            conn.query_row("SELECT COUNT(*) FROM crew_merges", [], |row| row.get(0))?;
        let mut statement =
            conn.prepare("SELECT * FROM crew_merges ORDER BY id DESC LIMIT ?1 OFFSET ?2")?;
        let rows = statement
            .query_map(params![limit, offset], merge_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok((rows, total))
    }

    /// Merges still standing, by what the watch found: `(watch, count)`.
    pub fn crew_watch_counts(&self) -> Result<Vec<(String, u32)>> {
        let conn = self.reader();
        let mut statement = conn.prepare(
            "SELECT watch, COUNT(*) FROM crew_merges WHERE review_state != 'reverted' GROUP BY watch",
        )?;
        Ok(statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn crew_merge(&self, task_id: &str) -> Result<Option<MergeRecord>> {
        Ok(self
            .reader()
            .query_row(
                "SELECT * FROM crew_merges WHERE task_id = ?1",
                [task_id],
                merge_row,
            )
            .optional()?)
    }

    /// The person's answer to a merge in the Review queue. A merge that was reverted keeps that: the answer is refused.
    pub fn set_merge_review(
        &self,
        task_id: &str,
        state: &str,
        comment: Option<&str>,
    ) -> Result<()> {
        self.commit(|tx, _| {
            let changed = tx.execute(
                "UPDATE crew_merges SET review_state = ?2, comment = COALESCE(?3, comment), reviewed_unix = ?4
                 WHERE task_id = ?1 AND review_state != 'reverted'",
                params![task_id, state, comment, unix_now()],
            )?;
            if changed == 0 {
                let exists: bool = tx.query_row(
                    "SELECT COUNT(*) FROM crew_merges WHERE task_id = ?1",
                    [task_id],
                    |row| row.get::<_, i64>(0).map(|n| n > 0),
                )?;
                return Err(if exists {
                    CoreError::Invalid("this merge was already reverted".into())
                } else {
                    CoreError::NotFound(format!("merge of task {task_id}"))
                });
            }
            Ok(())
        })
    }

    /// What the watch on main found for a merge: `green` or `broken`.
    pub fn set_merge_watch(&self, task_id: &str, watch: &str) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "UPDATE crew_merges SET watch = ?2 WHERE task_id = ?1",
                params![task_id, watch],
            )?;
            Ok(())
        })
    }

    /// A merge that a person must decide about (a break the crew is not certain of), or `None` to clear it.
    pub fn set_merge_needs_person(&self, task_id: &str, why: Option<&str>) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "UPDATE crew_merges SET needs_person = ?2 WHERE task_id = ?1",
                params![task_id, why],
            )?;
            Ok(())
        })
    }

    /// The merge was reverted by `commit` on the branch.
    pub fn set_merge_reverted(&self, task_id: &str, commit: &str) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "UPDATE crew_merges SET review_state = 'reverted', reverted_commit = ?2, needs_person = NULL, reviewed_unix = ?3 WHERE task_id = ?1",
                params![task_id, commit, unix_now()],
            )?;
            Ok(())
        })
    }
}
