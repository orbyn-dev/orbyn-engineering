//! Code Mode run records: what a run was, what each call returned, and what a replay needs
//! (docs/code-mode.md). Kept 7 days.

use rusqlite::{OptionalExtension, params};

use super::Store;
use crate::error::Result;

/// A run's own result is kept whole up to this size; a bigger one keeps its size only.
pub(crate) const FULL_RESULT_BYTES: usize = 256 * 1024;
/// The first part of a call's result, for the run log.
pub(crate) const PREVIEW_BYTES: usize = 4 * 1024;
/// The newest log lines a record keeps.
pub(crate) const MAX_LOG_LINES: usize = 500;
const KEEP_SECONDS: i64 = 7 * 24 * 60 * 60;

#[derive(Clone, Debug)]
pub(crate) struct NewCodeRun {
    pub run_id: String,
    pub thread_id: String,
    pub script: String,
    pub script_sha256: String,
    pub seed_start_ms: i64,
    pub seed: u32,
    pub started_at: String,
    pub crew_worker: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct CodeRunEnd {
    /// `completed` or `error`.
    pub status: &'static str,
    pub ended_at: String,
    pub error: Option<String>,
    pub result: Option<String>,
    pub logs: Vec<String>,
    pub logs_total: u64,
    pub truncated: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct StoredCodeRun {
    pub run_id: String,
    pub thread_id: String,
    pub status: String,
    pub script: String,
    pub script_sha256: String,
    pub seed_start_ms: i64,
    pub seed: u32,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub error: Option<String>,
    pub result: Option<String>,
    pub logs: Vec<String>,
    pub logs_total: Option<u64>,
    pub truncated: bool,
    pub resumes: u32,
    pub crew_worker: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StoredCodeCall {
    pub idx: u32,
    pub tool: String,
    pub args_digest: String,
    pub effect: bool,
    pub status: String,
    pub preview: Option<String>,
    pub result_bytes: Option<u64>,
    pub result: Option<String>,
    pub duration_ms: Option<u64>,
}

#[derive(Clone, Debug)]
pub(crate) struct CodeRunRow {
    pub run_id: String,
    pub status: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub calls: u32,
    pub script_head: String,
}

/// The first `max` bytes of `text`, cut on a character boundary.
pub(crate) fn head(text: &str, max: usize) -> &str {
    let mut end = max.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

impl Store {
    pub(crate) fn code_run_begin(&self, run: &NewCodeRun) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "INSERT INTO code_runs (run_id, thread_id, status, script, script_sha256, seed_start_ms, seed,
                    started_at, crew_worker) VALUES (?1, ?2, 'running', ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    run.run_id,
                    run.thread_id,
                    run.script,
                    run.script_sha256,
                    run.seed_start_ms,
                    i64::from(run.seed),
                    run.started_at,
                    run.crew_worker
                ],
            )?;
            Ok(())
        })
    }

    /// A lost run picked up again by a replay: running, nothing of its end kept, one more resume.
    pub(crate) fn code_run_restart(&self, run_id: &str) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "UPDATE code_runs SET status = 'running', ended_at = NULL, error = NULL, result = NULL,
                    logs = '[]', logs_total = NULL, truncated = 0, resumes = resumes + 1 WHERE run_id = ?1",
                [run_id],
            )?;
            Ok(())
        })
    }

    /// `running` or `paused`, never over a run that has ended.
    pub(crate) fn code_run_set_status(
        &self,
        run_id: &str,
        status: &str,
        count_resume: bool,
    ) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "UPDATE code_runs SET status = ?2, resumes = resumes + ?3
                 WHERE run_id = ?1 AND status IN ('running', 'paused')",
                params![run_id, status, i64::from(count_resume)],
            )?;
            Ok(())
        })
    }

    pub(crate) fn code_run_end(&self, run_id: &str, end: &CodeRunEnd) -> Result<()> {
        let logs = serde_json::to_string(&end.logs)?;
        self.commit(|tx, _| {
            tx.execute(
                "UPDATE code_runs SET status = ?2, ended_at = ?3, error = ?4, result = ?5, logs = ?6,
                    logs_total = ?7, truncated = ?8 WHERE run_id = ?1",
                params![
                    run_id,
                    end.status,
                    end.ended_at,
                    end.error,
                    end.result,
                    logs,
                    i64::try_from(end.logs_total).unwrap_or(i64::MAX),
                    end.truncated
                ],
            )?;
            Ok(())
        })
    }

    pub(crate) fn code_call_begin(
        &self,
        run_id: &str,
        idx: u32,
        tool: &str,
        args_digest: &str,
        effect: bool,
    ) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "INSERT OR REPLACE INTO code_run_calls (run_id, idx, tool, args_digest, effect, status)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'running')",
                params![run_id, idx, tool, args_digest, effect],
            )?;
            Ok(())
        })
    }

    /// `result` is the call's JSON text (a value, or `{"code", "message"}` for a failure); the preview and the
    /// size are made here.
    pub(crate) fn code_call_end(
        &self,
        run_id: &str,
        idx: u32,
        status: &str,
        result: &str,
        duration_ms: u64,
    ) -> Result<()> {
        let preview = head(result, PREVIEW_BYTES).to_owned();
        let bytes = i64::try_from(result.len()).unwrap_or(i64::MAX);
        let full = (result.len() <= FULL_RESULT_BYTES).then_some(result);
        self.commit(|tx, _| {
            tx.execute(
                "UPDATE code_run_calls SET status = ?3, preview = ?4, result_bytes = ?5, result = ?6,
                    duration_ms = ?7 WHERE run_id = ?1 AND idx = ?2",
                params![
                    run_id,
                    idx,
                    status,
                    preview,
                    bytes,
                    full,
                    i64::try_from(duration_ms).unwrap_or(i64::MAX)
                ],
            )?;
            Ok(())
        })
    }

    /// Edits a stored call, as a crash at a bad moment or a changed script would leave it (tests).
    #[cfg(test)]
    pub(crate) fn code_call_tamper(
        &self,
        run_id: &str,
        idx: u32,
        tool: Option<&str>,
        status: Option<&str>,
        effect: Option<bool>,
    ) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute(
                "UPDATE code_run_calls SET tool = COALESCE(?3, tool), status = COALESCE(?4, status),
                    effect = COALESCE(?5, effect) WHERE run_id = ?1 AND idx = ?2",
                params![run_id, idx, tool, status, effect],
            )?;
            Ok(())
        })
    }

    /// A run whose start was refused leaves no record.
    pub(crate) fn code_run_forget(&self, run_id: &str) -> Result<()> {
        self.commit(|tx, _| {
            tx.execute("DELETE FROM code_runs WHERE run_id = ?1", [run_id])?;
            Ok(())
        })
    }

    pub(crate) fn code_run(&self, run_id: &str) -> Result<Option<StoredCodeRun>> {
        Ok(self
            .reader()
            .query_row(
                "SELECT run_id, thread_id, status, script, script_sha256, seed_start_ms, seed, started_at,
                    ended_at, error, result, logs, logs_total, truncated, resumes, crew_worker
                 FROM code_runs WHERE run_id = ?1",
                [run_id],
                |row| {
                    let logs: String = row.get(11)?;
                    Ok(StoredCodeRun {
                        run_id: row.get(0)?,
                        thread_id: row.get(1)?,
                        status: row.get(2)?,
                        script: row.get(3)?,
                        script_sha256: row.get(4)?,
                        seed_start_ms: row.get(5)?,
                        seed: u32::try_from(row.get::<_, i64>(6)?).unwrap_or(0),
                        started_at: row.get(7)?,
                        ended_at: row.get(8)?,
                        error: row.get(9)?,
                        result: row.get(10)?,
                        logs: serde_json::from_str(&logs).unwrap_or_default(),
                        logs_total: row
                            .get::<_, Option<i64>>(12)?
                            .and_then(|n| u64::try_from(n).ok()),
                        truncated: row.get(13)?,
                        resumes: u32::try_from(row.get::<_, i64>(14)?).unwrap_or(0),
                        crew_worker: row.get(15)?,
                    })
                },
            )
            .optional()?)
    }

    pub(crate) fn code_calls(&self, run_id: &str) -> Result<Vec<StoredCodeCall>> {
        let conn = self.reader();
        let mut stmt = conn.prepare(
            "SELECT idx, tool, args_digest, effect, status, preview, result_bytes, result, duration_ms
             FROM code_run_calls WHERE run_id = ?1 ORDER BY idx",
        )?;
        let rows = stmt.query_map([run_id], |row| {
            Ok(StoredCodeCall {
                idx: u32::try_from(row.get::<_, i64>(0)?).unwrap_or(0),
                tool: row.get(1)?,
                args_digest: row.get(2)?,
                effect: row.get(3)?,
                status: row.get(4)?,
                preview: row.get(5)?,
                result_bytes: row
                    .get::<_, Option<i64>>(6)?
                    .and_then(|n| u64::try_from(n).ok()),
                result: row.get(7)?,
                duration_ms: row
                    .get::<_, Option<i64>>(8)?
                    .and_then(|n| u64::try_from(n).ok()),
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// A thread's runs, newest first.
    pub(crate) fn code_runs_of(&self, thread_id: &str, limit: u32) -> Result<Vec<CodeRunRow>> {
        let conn = self.reader();
        let mut stmt = conn.prepare(
            "SELECT r.run_id, r.status, r.started_at, r.ended_at,
                    (SELECT COUNT(*) FROM code_run_calls c WHERE c.run_id = r.run_id), substr(r.script, 1, 120)
             FROM code_runs r WHERE r.thread_id = ?1 ORDER BY r.started_at DESC, r.run_id DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![thread_id, limit], |row| {
            Ok(CodeRunRow {
                run_id: row.get(0)?,
                status: row.get(1)?,
                started_at: row.get(2)?,
                ended_at: row.get(3)?,
                calls: u32::try_from(row.get::<_, i64>(4)?).unwrap_or(0),
                script_head: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Every run that was alive when the core stopped is lost: its task is gone. The calls stay as they were,
    /// so a call that was running shows as running and a replay can tell.
    pub(crate) fn code_runs_lose(&self, ended_at: &str, error: &str) -> Result<usize> {
        self.commit(|tx, _| {
            Ok(tx.execute(
                "UPDATE code_runs SET status = 'error', ended_at = ?1, error = ?2
                 WHERE status IN ('running', 'paused')",
                params![ended_at, error],
            )?)
        })
    }

    /// Deletes runs that started more than 7 days before `now` (unix seconds). Their calls go with them.
    pub(crate) fn code_runs_prune(&self, now: i64) -> Result<usize> {
        let cutoff = crate::util::format_unix(now - KEEP_SECONDS);
        self.commit(
            |tx, _| Ok(tx.execute("DELETE FROM code_runs WHERE started_at < ?1", [cutoff])?),
        )
    }
}
