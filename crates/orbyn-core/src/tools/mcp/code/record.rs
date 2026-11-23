//! Run records, their reads and the per-engine switch (docs/code-mode.md, "Run record" and "Switches").

use super::reply;
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::store::AuditRecord;
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

    /// The flag check a route makes before it reads a body, so a service with Code Mode off has no
    /// cancel route at all, even for a malformed request.
    pub fn code_mode_required(&self) -> Result<()> {
        self.code_mode_flag()
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

    /// The person's cancel: ends a live run as `error: cancelled`, denies the approvals it waits on
    /// through the same path a person's denial takes, stops its task and keeps its calls. A second
    /// cancel answers the same record; a run that already ended, or is no longer live, is `409`.
    pub async fn code_run_cancel(&self, run_id: &str) -> Result<CodeRunRecord> {
        self.code_mode_flag()?;
        if !valid_run_id(run_id) {
            return Err(CoreError::NotFound("run".into()));
        }
        let run = self
            .store
            .code_run(run_id)?
            .ok_or_else(|| CoreError::NotFound("run".into()))?;
        let error: Value = run
            .error
            .as_deref()
            .and_then(|text| serde_json::from_str(text).ok())
            .unwrap_or(Value::Null);
        let call_text = json!({ "code": "failed", "message": super::CANCELLED }).to_string();
        if run.status == "error" && error["kind"] == "cancelled" {
            // Cancelling twice is the same answer. Even here, finish any task or queued approval
            // that a concurrent cancel has not yet released.
            self.cancel_finish(run_id, &run.thread_id, &call_text).await;
            return self.code_run_record(run_id);
        }
        if run.status != "running" && run.status != "paused" {
            return Err(run_finished());
        }
        if self.code_runs.begin_cancel(run_id).is_none() {
            // A record that says running or paused but has no live task or preparing start cannot be
            // cancelled: it was never live here (a restart, or a task that outlived its slot).
            return Err(run_finished());
        }
        let error_text = json!({ "kind": "cancelled", "message": super::CANCELLED }).to_string();
        let ended =
            match self
                .store
                .code_run_cancel(run_id, &crate::util::now(), &error_text, &call_text)
            {
                Ok(ended) => ended,
                Err(error) => {
                    self.cancel_finish(run_id, &run.thread_id, &call_text).await;
                    return Err(error);
                }
            };
        if !ended {
            // A finish already in progress may have written its own end first. Release the task and
            // answer that record when it is the cancelled one; a completed or lost end is 409.
            self.cancel_finish(run_id, &run.thread_id, &call_text).await;
            if let Ok(Some(stored)) = self.store.code_run(run_id) {
                let error: Value = stored
                    .error
                    .as_deref()
                    .and_then(|text| serde_json::from_str(text).ok())
                    .unwrap_or(Value::Null);
                if stored.status == "error" && error["kind"] == "cancelled" {
                    return self.code_run_record(run_id);
                }
            }
            return Err(run_finished());
        }
        // The person's denial path: the same write, the same wake-up, with the fixed reason. Doing it
        // before the task stops lets the waiter hear a denial instead of an abandoned expiry.
        self.cancel_finish(run_id, &run.thread_id, &call_text).await;
        self.audit_code_run_cancel(&run);
        self.code_run_record(run_id)
    }

    /// Stops whatever the run has in memory, denies its queued approvals, and fails every call still
    /// running after the task dropped. Safe to call twice and when nothing is live.
    async fn cancel_finish(&self, run_id: &str, thread_id: &str, call_text: &str) {
        self.deny_code_run_approvals(run_id, thread_id);
        self.code_runs.finish_cancel(run_id).await;
        // A call may have reached the store while the task was dropping; fail it and any approval
        // that appeared in the same window.
        self.deny_code_run_approvals(run_id, thread_id);
        if let Err(error) = self.store.code_calls_fail_running(run_id, call_text) {
            tracing::warn!(%error, "could not fail a script call left by a cancel");
        }
    }

    fn deny_code_run_approvals(&self, run_id: &str, thread_id: &str) {
        let Ok(pending) = self.store.pending_for_thread(thread_id) else {
            return;
        };
        for approval in pending {
            if approval
                .via
                .as_ref()
                .is_none_or(|via| via.code_run != run_id)
            {
                continue;
            }
            if let Err(error) = self.gate.decide_with(
                &approval.id,
                false,
                false,
                Some(super::CANCELLED.into()),
                "user",
            ) {
                tracing::warn!(%error, approval = %approval.id, "could not deny an approval of a cancelled run");
            }
        }
    }

    fn audit_code_run_cancel(&self, run: &StoredCodeRun) {
        let record = AuditRecord {
            actor: "user".into(),
            action: "code_run.cancel".into(),
            target: run.run_id.clone(),
            decision: None,
            source: "user".into(),
            thread_id: Some(run.thread_id.clone()),
            run_id: None,
            detail: json!({ "status": run.status }),
        };
        if let Err(error) = self.store.append_audit(&record) {
            tracing::warn!(%error, "could not record a code run cancel");
        }
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
                waited_ms: row.waited_ms,
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

/// A run that ended before the cancel, or is not live here, cannot be cancelled.
fn run_finished() -> CoreError {
    CoreError::Refused {
        status: 409,
        code: "run_finished",
        message: super::RUN_FINISHED.into(),
        detail: Value::Null,
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
        waited_ms: run.waited_ms,
        crew_worker: run.crew_worker,
    }
}
