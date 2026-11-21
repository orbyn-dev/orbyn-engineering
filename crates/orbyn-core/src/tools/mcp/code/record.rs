//! Run records, their reads and the per-engine switch (docs/code-mode.md, "Run record" and "Switches").

use super::reply;
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::store::code_runs::{CodeRunEnd, FULL_RESULT_BYTES, MAX_LOG_LINES, StoredCodeRun, head};
use orbyn_code_sandbox::Step;
use orbyn_contracts::{
    CodeModeSetting, CodeRunCall, CodeRunError, CodeRunList, CodeRunPending, CodeRunRecord,
    CodeRunStatus, CodeRunSummary,
};
use serde_json::{Value, json};

/// The result a route returns in full; a bigger one comes as a preview.
const ROUTE_RESULT_BYTES: usize = 64 * 1024;
const LIST_LIMIT: u32 = 100;

/// A run id is a key a model can pass back. Only what the service makes is accepted.
pub(crate) fn valid_run_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn valid_engine_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b':'))
}

fn setting_key(engine: &str) -> String {
    format!("code_mode:{engine}")
}

fn status_of(text: &str) -> CodeRunStatus {
    match text {
        "paused" => CodeRunStatus::Paused,
        "completed" => CodeRunStatus::Completed,
        "error" => CodeRunStatus::Error,
        _ => CodeRunStatus::Running,
    }
}

/// The text a record keeps of a run's result: whole up to 256 KiB, else its size and the start of it.
fn result_text(result: &Value) -> String {
    let text = result.to_string();
    if text.len() <= FULL_RESULT_BYTES {
        return text;
    }
    json!({ "truncated": true, "bytes": text.len(), "preview": head(&text, ROUTE_RESULT_BYTES) })
        .to_string()
}

impl Core {
    /// Whether `engine` may list and call the Code Mode tools: the service flag, then the engine's own switch.
    pub(crate) fn code_mode_on(&self, engine: &str) -> bool {
        self.config.code_mode
            && matches!(self.store.setting(&setting_key(engine)), Ok(Some(v)) if v == "on")
    }

    pub fn code_mode_setting(&self, engine: &str) -> Result<CodeModeSetting> {
        self.code_mode_flag()?;
        if !valid_engine_id(engine) {
            return Err(CoreError::NotFound("engine".into()));
        }
        Ok(CodeModeSetting {
            enabled: self.store.setting(&setting_key(engine))?.as_deref() == Some("on"),
        })
    }

    pub fn set_code_mode(&self, engine: &str, enabled: bool) -> Result<CodeModeSetting> {
        self.code_mode_flag()?;
        if !valid_engine_id(engine) {
            return Err(CoreError::NotFound("engine".into()));
        }
        self.store
            .set_setting(&setting_key(engine), if enabled { "on" } else { "off" })?;
        Ok(CodeModeSetting { enabled })
    }

    fn code_mode_flag(&self) -> Result<()> {
        if self.config.code_mode {
            Ok(())
        } else {
            Err(CoreError::NotFound("code mode".into()))
        }
    }

    /// One run, as the run log draws it. A run id that is not one of ours is not found.
    pub fn code_run_record(&self, run_id: &str) -> Result<CodeRunRecord> {
        self.code_mode_flag()?;
        if !valid_run_id(run_id) {
            return Err(CoreError::NotFound("run".into()));
        }
        let run = self
            .store
            .code_run(run_id)?
            .ok_or_else(|| CoreError::NotFound("run".into()))?;
        let calls = self.store.code_calls(run_id)?;
        let live = matches!(run.status.as_str(), "running" | "paused");
        let pending = calls
            .iter()
            .filter(|c| live && c.status == "running")
            .map(|c| CodeRunPending {
                tool: c.tool.clone(),
                call: c.idx,
            })
            .collect();
        let calls = calls
            .into_iter()
            .map(|c| CodeRunCall {
                index: c.idx,
                tool: c.tool,
                status: c.status,
                preview: c.preview,
                result_bytes: c.result_bytes,
                duration_ms: c.duration_ms,
            })
            .collect();
        Ok(record_of(run, calls, pending))
    }

    pub fn code_runs_list(&self, thread_id: &str) -> Result<CodeRunList> {
        self.code_mode_flag()?;
        let runs = self
            .store
            .code_runs_of(thread_id, LIST_LIMIT)?
            .into_iter()
            .map(|row| CodeRunSummary {
                run_id: row.run_id,
                status: status_of(&row.status),
                started_at: row.started_at,
                ended_at: row.ended_at,
                calls: row.calls,
                script_head: Some(row.script_head),
            })
            .collect();
        Ok(CodeRunList { runs })
    }

    /// The end of a run, kept. A `Yielded` run is still alive and keeps nothing.
    pub(super) fn code_run_finished(&self, run_id: &str, step: &Step) {
        let now = crate::util::now();
        let end = match step {
            Step::Done {
                result,
                logs,
                truncated,
            } => CodeRunEnd {
                status: "completed",
                ended_at: now,
                error: None,
                result: Some(result_text(result)),
                logs: newest(logs),
                logs_total: logs.len() as u64,
                truncated: *truncated,
            },
            Step::Failed {
                error,
                logs,
                truncated,
            } => CodeRunEnd {
                status: "error",
                ended_at: now,
                error: Some(reply::error_json(error).to_string()),
                result: None,
                logs: newest(logs),
                logs_total: logs.len() as u64,
                truncated: *truncated,
            },
            Step::Yielded => return,
        };
        if let Err(error) = self.store.code_run_end(run_id, &end) {
            tracing::warn!(%error, "could not record the end of a script run");
        }
    }

    /// At start: a run that was alive when the core stopped is gone, and a record older than 7 days is deleted.
    pub(crate) fn code_runs_recover(&self) {
        let lost = serde_json::json!({ "kind": "lost", "message": super::LOST }).to_string();
        if let Err(error) = self.store.code_runs_lose(&crate::util::now(), &lost) {
            tracing::warn!(%error, "could not mark lost script runs");
        }
        self.code_runs_prune();
    }

    pub(super) fn code_runs_prune(&self) {
        if let Err(error) = self.store.code_runs_prune(crate::util::unix_now()) {
            tracing::warn!(%error, "could not delete old script runs");
        }
    }
}

fn newest(logs: &[String]) -> Vec<String> {
    logs[logs.len().saturating_sub(MAX_LOG_LINES)..].to_vec()
}

fn record_of(
    run: StoredCodeRun,
    calls: Vec<CodeRunCall>,
    pending: Vec<CodeRunPending>,
) -> CodeRunRecord {
    let error = run
        .error
        .as_deref()
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .map(|v| CodeRunError {
            kind: v["kind"].as_str().unwrap_or("runtime").to_owned(),
            message: v["message"].as_str().unwrap_or_default().to_owned(),
            line: v["line"].as_u64().and_then(|n| u32::try_from(n).ok()),
        });
    let result = run.result.as_deref().map(|text| {
        if text.len() > ROUTE_RESULT_BYTES {
            json!({ "truncated": true, "bytes": text.len(), "preview": head(text, ROUTE_RESULT_BYTES) })
        } else {
            serde_json::from_str(text).unwrap_or(Value::Null)
        }
    });
    CodeRunRecord {
        run_id: run.run_id,
        status: status_of(&run.status),
        script: Some(run.script),
        script_sha256: Some(run.script_sha256),
        started_at: run.started_at,
        ended_at: run.ended_at,
        calls,
        logs: run.logs,
        logs_total: run.logs_total.filter(|total| *total > MAX_LOG_LINES as u64),
        result,
        error,
        pending,
        truncated: run.truncated,
        resumes: run.resumes,
        waited_ms: None,
        crew_worker: run.crew_worker,
    }
}
