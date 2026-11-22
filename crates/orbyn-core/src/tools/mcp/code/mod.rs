//! Code Mode: `orbyn_code_search`, `orbyn_code_describe` and `orbyn_code_run`. A script calls many
//! tools in one model round trip; every call goes back through `Core::call_orbyn_tool`
//! (`docs/code-mode.md`). Behind `CoreConfig::code_mode`, which ships `false`.

mod declare;
#[cfg(test)]
pub(crate) mod fake;
mod host;
mod record;
mod reply;
mod runs;
mod search;

pub(crate) use host::is_inner_tool;
pub(crate) use runs::CodeRuns;

use crate::app::Core;
use crate::store::AuditRecord;
use crate::store::code_runs::{NewCodeRun, StoredCodeCall};
use crate::tools::mcp::McpBinding;
use orbyn_code_sandbox::{ErrorKind, RunError, RunSeed, Step};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::time::Duration;

pub(crate) const SEARCH_TOOL: &str = "orbyn_code_search";
pub(crate) const DESCRIBE_TOOL: &str = "orbyn_code_describe";
pub(crate) const RUN_TOOL: &str = "orbyn_code_run";

const MAX_DESCRIBE: usize = 16;
const MAX_SOURCE_BYTES: usize = 64 * 1024;
const LOST: &str = "This run was lost when Orbyn restarted. Call orbyn_code_run again with the same code and run_id to replay it.";

pub(crate) fn is_tool(name: &str) -> bool {
    [SEARCH_TOOL, DESCRIBE_TOOL, RUN_TOOL].contains(&name)
}

/// The three tools as `tools/list` shows them.
pub(crate) fn specs() -> Vec<Value> {
    vec![
        json!({
            "name": SEARCH_TOOL,
            "description": "Find Orbyn tools a script can call. `query` is a few words; `detail` is name, summary or signature.",
            "inputSchema": { "type": "object", "properties": {
                "query": { "type": "string", "description": "What you want to do, in a few words." },
                "detail": { "type": "string", "enum": ["name", "summary", "signature"] },
                "limit": { "type": "integer", "description": "How many hits, 1 to 50. Default 10." },
            }, "required": ["query"] },
        }),
        json!({
            "name": DESCRIBE_TOOL,
            "description": "TypeScript declarations for the tools in `paths` (up to 16), as a script sees them.",
            "inputSchema": { "type": "object", "properties": {
                "paths": { "type": "array", "items": { "type": "string" }, "description": "Tool names from orbyn_code_search." },
            }, "required": ["paths"] },
        }),
        json!({
            "name": RUN_TOOL,
            "description": "Run a JavaScript or TypeScript script (the body of an async function, ending in `return`) that calls Orbyn tools as `await tools.<name>(args)`. \
                Every call passes the same checks and approvals as a direct call. Replies {status: completed|paused|error, result, logs, runId}. \
                On `paused`, call again with `run_id` to wait for it. Pass `code` to start or `run_id` to resume, not both.",
            "inputSchema": { "type": "object", "properties": {
                "code": { "type": "string", "description": "The script, at most 64 KiB." },
                "run_id": { "type": "string", "description": "A paused run's runId." },
            } },
        }),
    ]
}

impl Core {
    /// Runs a Code Mode tool for the MCP service. The text and whether it is an error.
    pub(crate) async fn code_tool_text(
        self: &Arc<Self>,
        binding: &McpBinding,
        tool: &str,
        args: &Value,
    ) -> (String, bool) {
        if let Err(error) = self.note_tool(binding, tool) {
            tracing::warn!(%error, "could not record a tool call");
        }
        // The same allow list runs here and on every inner call (`call_orbyn_tool`).
        if let Err(error) = self.refuse_for_crew_worker(binding, tool, args) {
            return (error.public_message(), true);
        }
        match tool {
            SEARCH_TOOL => self.code_search(binding, args),
            DESCRIBE_TOOL => self.code_describe(binding, args),
            _ => self.code_run(binding, args).await,
        }
    }

    /// The specs a script of this caller may call: never Code Mode, and for a Crew worker only its
    /// allow list. For a worker every string in a spec is redacted of names it may not use, so search
    /// and describe never name one; for anything else the specs come back unmodified.
    fn code_callable(&self, binding: &McpBinding) -> Vec<Value> {
        let specs = crate::tools::mcp_tools::tool_specs()
            .filter(|spec| spec["name"].as_str().is_some_and(is_inner_tool));
        if !self.is_crew_worker(binding).unwrap_or(true) {
            return specs.collect();
        }
        let own = binding.scope.thread_id.as_str();
        let args = json!({ "thread_id": own });
        let hidden = hidden_names(own);
        specs
            .filter(|spec| {
                let name = spec["name"].as_str().unwrap_or_default();
                crate::crew::worker_refusal(name, &args, own).is_none()
            })
            .map(|mut spec| {
                redact_tool_names(&mut spec, &hidden);
                spec
            })
            .collect()
    }

    fn code_search(&self, binding: &McpBinding, args: &Value) -> (String, bool) {
        let Some(query) = args.get("query").and_then(Value::as_str) else {
            return ("query is required.".into(), true);
        };
        let detail = args
            .get("detail")
            .and_then(Value::as_str)
            .unwrap_or("summary");
        if !["name", "summary", "signature"].contains(&detail) {
            return ("detail is name, summary or signature.".into(), true);
        }
        let limit = args
            .get("limit")
            .and_then(Value::as_u64)
            .map_or(search::DEFAULT_LIMIT, |n| n as usize);
        let hits = search::search(&self.code_callable(binding), query, detail, limit);
        (json!(hits).to_string(), false)
    }

    fn code_describe(&self, binding: &McpBinding, args: &Value) -> (String, bool) {
        let Some(paths) = args.get("paths").and_then(Value::as_array) else {
            return ("paths is required.".into(), true);
        };
        if paths.len() > MAX_DESCRIBE {
            return (
                format!("Describe at most {MAX_DESCRIBE} tools at a time."),
                true,
            );
        }
        let callable = self.code_callable(binding);
        let (mut known, mut unknown) = (Vec::new(), Vec::new());
        for path in paths.iter().filter_map(Value::as_str) {
            match callable.iter().find(|spec| spec["name"] == path) {
                Some(spec) => known.push(declare::declaration(spec)),
                // The caller's own input: describe never adds a name the caller did not send.
                None => unknown.push(path.to_owned()),
            }
        }
        (
            json!({ "declarations": known.join("\n\n"), "unknown": unknown }).to_string(),
            false,
        )
    }

    async fn code_run(self: &Arc<Self>, binding: &McpBinding, args: &Value) -> (String, bool) {
        let thread = &binding.scope.thread_id;
        let code = args.get("code").and_then(Value::as_str);
        let resume = args.get("run_id").and_then(Value::as_str);
        if let Some(id) = resume
            && !record::valid_run_id(id)
        {
            return ("That run_id is not a run of this thread.".into(), true);
        }
        let (run_id, live) = match (code, resume) {
            (None, None) => {
                return (
                    "Pass `code` to start a run or `run_id` to resume one.".into(),
                    true,
                );
            }
            (None, Some(id)) => match self.code_runs.get(id, thread) {
                Some(live) => {
                    if let Err(error) = self.store.code_run_set_status(id, "running", true) {
                        tracing::warn!(%error, "could not record a resume");
                    }
                    (id.to_owned(), live)
                }
                None => return (self.code_resume_without_live(id, thread), false),
            },
            (Some(code), resume) => {
                if code.len() > MAX_SOURCE_BYTES {
                    return (
                        format!("The script is over {MAX_SOURCE_BYTES} bytes."),
                        true,
                    );
                }
                let started = match resume {
                    Some(id) => self.code_replay(binding, id, code).await,
                    None => self.code_start_new(binding, code).await,
                };
                match started {
                    Ok(started) => started,
                    Err(answer) => return answer,
                }
            }
        };
        let slice = slice_for(self.code_runs.slice());
        let answer = match live.wait(slice).await {
            Some(step) => reply::finished(&run_id, &step, super::MAX_RESULT_BYTES),
            None => {
                if let Err(error) = self.store.code_run_set_status(&run_id, "paused", false) {
                    tracing::warn!(%error, "could not record a pause");
                }
                reply::paused(&run_id, &live.pending())
            }
        };
        (answer.to_string(), false)
    }

    /// A resume of a run that is not in memory: a finished one answers from its record, any other is lost.
    fn code_resume_without_live(&self, run_id: &str, thread: &str) -> String {
        let stored = self
            .store
            .code_run(run_id)
            .ok()
            .flatten()
            .filter(|run| run.thread_id == thread);
        let Some(run) = stored else {
            return reply::lost(run_id, LOST).to_string();
        };
        let error: Value = run
            .error
            .as_deref()
            .and_then(|text| serde_json::from_str(text).ok())
            .unwrap_or(Value::Null);
        match run.status.as_str() {
            "completed" => {
                let result = run
                    .result
                    .as_deref()
                    .and_then(|text| serde_json::from_str(text).ok())
                    .unwrap_or(Value::Null);
                reply::finished(
                    run_id,
                    &Step::Done {
                        result,
                        logs: run.logs,
                        truncated: run.truncated,
                    },
                    super::MAX_RESULT_BYTES,
                )
                .to_string()
            }
            "error" if error["kind"] != "lost" => json!({
                "status": "error", "runId": run_id, "error": error, "logs": run.logs, "truncated": run.truncated })
            .to_string(),
            _ => reply::lost(run_id, LOST).to_string(),
        }
    }

    async fn code_start_new(
        self: &Arc<Self>,
        binding: &McpBinding,
        code: &str,
    ) -> Result<(String, Arc<runs::Live>), (String, bool)> {
        let thread = &binding.scope.thread_id;
        let run_id = ulid::Ulid::generate().to_string();
        self.note_code_run(binding, &run_id, code);
        let seed = RunSeed {
            start_ms: now_ms(),
            seed: (ulid::Ulid::generate().random() & 0xffff_ffff) as u32,
        };
        self.code_runs_prune();
        let crew_worker = self.is_crew_worker(binding).unwrap_or(true);
        let record = NewCodeRun {
            run_id: run_id.clone(),
            thread_id: thread.clone(),
            script: code.to_owned(),
            script_sha256: crate::util::sha256_hex(code.as_bytes()),
            seed_start_ms: seed.start_ms,
            seed: seed.seed,
            started_at: crate::util::now(),
            crew_worker,
        };
        if let Err(error) = self.store.code_run_begin(&record) {
            tracing::warn!(%error, "could not record a script run");
        }
        self.code_begin_task(binding, &run_id, code, seed, HashMap::new(), crew_worker)
            .await
            .map(|live| (run_id, live))
    }

    /// A lost run picked up again with the same code: its recorded calls answer again and nothing that
    /// was recorded runs twice.
    async fn code_replay(
        self: &Arc<Self>,
        binding: &McpBinding,
        run_id: &str,
        code: &str,
    ) -> Result<(String, Arc<runs::Live>), (String, bool)> {
        let thread = &binding.scope.thread_id;
        // Filter by owner first: another thread's live run answers like an unknown id, not with the
        // news that it exists.
        if self.code_runs.get(run_id, thread).is_some() && self.code_runs.is_running(run_id) {
            return Err((
                "That run is still running. Resume it with run_id alone.".into(),
                true,
            ));
        }
        let stored = self
            .store
            .code_run(run_id)
            .ok()
            .flatten()
            .filter(|run| run.thread_id == *thread);
        let Some(run) = stored else {
            return Err((reply::lost(run_id, LOST).to_string(), false));
        };
        let lost = run
            .error
            .as_deref()
            .and_then(|text| serde_json::from_str::<Value>(text).ok())
            .is_some_and(|error| error["kind"] == "lost");
        if run.status != "error" || !lost {
            return Err((
                "That run did not end as lost. Start a new run with `code`, or resume it with run_id alone."
                    .into(),
                true,
            ));
        }
        if run.script_sha256 != crate::util::sha256_hex(code.as_bytes()) {
            return Err((reply::lost(run_id, host::NOT_REPEATABLE).to_string(), false));
        }
        let calls: HashMap<u32, StoredCodeCall> = match self.store.code_calls(run_id) {
            Ok(calls) => calls.into_iter().map(|c| (c.idx, c)).collect(),
            Err(error) => return Err((error.public_message(), true)),
        };
        if let Err(error) = self.store.code_run_restart(run_id) {
            tracing::warn!(%error, "could not record a replay");
        }
        let seed = RunSeed {
            start_ms: run.seed_start_ms,
            seed: run.seed,
        };
        let crew_worker = self.is_crew_worker(binding).unwrap_or(true);
        self.code_begin_task(binding, run_id, code, seed, calls, crew_worker)
            .await
            .map(|live| (run_id.to_owned(), live))
    }

    async fn code_begin_task(
        self: &Arc<Self>,
        binding: &McpBinding,
        run_id: &str,
        code: &str,
        seed: RunSeed,
        replay: HashMap<u32, StoredCodeCall>,
        crew_worker: bool,
    ) -> Result<Arc<runs::Live>, (String, bool)> {
        let failure: host::ReplayFailure = Arc::default();
        let host_core = self.clone();
        let host_binding = binding.clone();
        let host_run = run_id.to_owned();
        let host_script = crate::util::sha256_hex(code.as_bytes());
        let host_failure = failure.clone();
        let weak: Weak<Core> = Arc::downgrade(self);
        let finish_run = run_id.to_owned();
        let started = self
            .code_runs
            .start(
                run_id,
                &binding.scope.thread_id,
                crew_worker,
                code,
                seed,
                move |pending| {
                    Arc::new(host::RunHost::new(
                        &host_core,
                        &host_binding,
                        pending,
                        &host_run,
                        &host_script,
                        replay,
                        host_failure,
                    ))
                },
                move |step| {
                    let message = failure
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .clone();
                    let step = match (message, step) {
                        (
                            Some(message),
                            Step::Done {
                                logs, truncated, ..
                            }
                            | Step::Failed {
                                logs, truncated, ..
                            },
                        ) => Step::Failed {
                            error: RunError {
                                kind: ErrorKind::Runtime,
                                message,
                                line: None,
                                code: Some("lost".into()),
                            },
                            logs,
                            truncated,
                        },
                        (_, step) => step,
                    };
                    if let Some(core) = weak.upgrade() {
                        core.code_run_finished(&finish_run, &step);
                    }
                    step
                },
            )
            .await;
        match started {
            Ok(live) => Ok(live),
            Err(runs::Refused::Limit(message)) => {
                if let Err(error) = self.store.code_run_forget(run_id) {
                    tracing::warn!(%error, "could not drop a refused script run");
                }
                Err((message, true))
            }
            Err(runs::Refused::Duplicate(message)) => Err((message, true)),
            Err(runs::Refused::Script(error)) => {
                self.code_run_finished(
                    run_id,
                    &Step::Failed {
                        error: error.clone(),
                        logs: vec![],
                        truncated: false,
                    },
                );
                Err((reply::failed(run_id, &error).to_string(), false))
            }
        }
    }

    /// One `code.run` audit record when a run starts: its size and SHA-256, never the script.
    fn note_code_run(&self, binding: &McpBinding, run_id: &str, code: &str) {
        let record = AuditRecord {
            actor: format!("engine:{}", binding.scope.engine),
            action: "code.run".into(),
            target: run_id.to_owned(),
            decision: None,
            source: "engine".into(),
            thread_id: Some(binding.scope.thread_id.clone()),
            run_id: Some(binding.scope.run_id.clone()),
            detail: json!({ "bytes": code.len(), "sha256": crate::util::sha256_hex(code.as_bytes()) }),
        };
        if let Err(error) = self.store.append_audit(&record) {
            tracing::warn!(%error, "could not record a code run");
        }
    }
}

/// Every name a worker may not use in a script, from the whole tool universe, longest first.
fn hidden_names(own: &str) -> Vec<String> {
    let args = json!({ "thread_id": own });
    let mut names: Vec<String> = super::tool_list(false)
        .into_iter()
        .chain(crate::tools::mcp::code::specs())
        .chain(crate::crew::tool_specs())
        .filter_map(|spec| spec["name"].as_str().map(str::to_owned))
        .collect();
    names.push("approve".to_owned());
    names.retain(|name| {
        !is_inner_tool(name) || crate::crew::worker_refusal(name, &args, own).is_some()
    });
    names.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    names.dedup();
    names
}

/// Replaces hidden tool names in every string value of a spec, including nested schema strings.
/// Object keys are untouched: they are schema structure, not text a reader sees.
fn redact_tool_names(value: &mut Value, hidden: &[String]) {
    let mut hidden: Vec<&str> = hidden.iter().map(String::as_str).collect();
    hidden.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    redact_value(value, &hidden);
}

fn redact_value(value: &mut Value, hidden: &[&str]) {
    match value {
        Value::String(text) => {
            let mut redacted = text.clone();
            for name in hidden {
                redacted = replace_identifier(&redacted, name, "another tool");
            }
            *text = redacted;
        }
        Value::Array(items) => {
            for item in items {
                redact_value(item, hidden);
            }
        }
        Value::Object(map) => {
            for item in map.values_mut() {
                redact_value(item, hidden);
            }
        }
        _ => {}
    }
}

/// Replaces `name` only when both neighbours are outside `[A-Za-z0-9_]`, so a name that is a prefix
/// of a longer allowed name does not corrupt it.
fn replace_identifier(text: &str, name: &str, replacement: &str) -> String {
    if name.is_empty() {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(name) {
        let after = at + name.len();
        let before_edge = rest[..at].chars().next_back().is_none_or(|c| !is_ident(c));
        let after_edge = rest[after..].chars().next().is_none_or(|c| !is_ident(c));
        if before_edge && after_edge {
            out.push_str(&rest[..at]);
            out.push_str(replacement);
        } else {
            out.push_str(&rest[..after]);
        }
        rest = &rest[after..];
    }
    out.push_str(rest);
    out
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// The slice, kept under the outer call's own deadline when it has one.
fn slice_for(slice: Duration) -> Duration {
    match super::call_deadline() {
        Some(deadline) => {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            slice.min(left.saturating_sub(Duration::from_secs(5)))
        }
        None => slice,
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(0))
}

#[cfg(test)]
mod tests;
