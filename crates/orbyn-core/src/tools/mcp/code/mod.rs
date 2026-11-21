//! Code Mode: `orbyn_code_search`, `orbyn_code_describe` and `orbyn_code_run`. A script calls many
//! tools in one model round trip; every call goes back through `Core::call_orbyn_tool`
//! (`docs/code-mode.md`). Behind `CoreConfig::code_mode`, which ships `false`.

mod declare;
#[cfg(test)]
pub(crate) mod fake;
mod host;
mod reply;
mod runs;
mod search;

pub(crate) use runs::CodeRuns;

use crate::app::Core;
use crate::store::AuditRecord;
use crate::tools::mcp::McpBinding;
use orbyn_code_sandbox::RunSeed;
use serde_json::{Value, json};
use std::sync::Arc;
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
        // A crew worker gets these only once the allow list names them.
        if let Err(error) = self.refuse_for_crew_worker(binding, tool, args) {
            return (error.public_message(), true);
        }
        match tool {
            SEARCH_TOOL => self.code_search(binding, args),
            DESCRIBE_TOOL => self.code_describe(binding, args),
            _ => self.code_run(binding, args).await,
        }
    }

    /// The specs a script of this caller may call: never Code Mode, and for a Crew worker only its allow list.
    fn code_callable(&self, binding: &McpBinding) -> Vec<Value> {
        let own = json!({ "thread_id": binding.scope.thread_id });
        crate::tools::mcp_tools::tool_specs()
            .filter(|spec| {
                let name = spec["name"].as_str().unwrap_or_default();
                host::is_inner_tool(name)
                    && self.refuse_for_crew_worker(binding, name, &own).is_ok()
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
        let (run_id, live) = match (code, resume) {
            (Some(_), Some(_)) => {
                return (
                    "Pass `code` to start a run or `run_id` to resume one, not both.".into(),
                    true,
                );
            }
            (None, None) => {
                return (
                    "Pass `code` to start a run or `run_id` to resume one.".into(),
                    true,
                );
            }
            (None, Some(id)) => match self.code_runs.get(id, thread) {
                Some(live) => (id.to_owned(), live),
                None => return (reply::lost(id, LOST).to_string(), false),
            },
            (Some(code), None) => {
                if code.len() > MAX_SOURCE_BYTES {
                    return (
                        format!("The script is over {MAX_SOURCE_BYTES} bytes."),
                        true,
                    );
                }
                let run_id = ulid::Ulid::generate().to_string();
                self.note_code_run(binding, &run_id, code);
                let host_core = self.clone();
                let host_binding = binding.clone();
                let seed = RunSeed {
                    start_ms: now_ms(),
                    seed: (ulid::Ulid::generate().random() & 0xffff_ffff) as u32,
                };
                let started = self
                    .code_runs
                    .start(&run_id, thread, code, seed, move |pending| {
                        Arc::new(host::RunHost::new(&host_core, &host_binding, pending))
                    })
                    .await;
                match started {
                    Ok(live) => (run_id, live),
                    Err(runs::Refused::Limit(message)) => return (message, true),
                    Err(runs::Refused::Script(error)) => {
                        return (reply::failed(&run_id, &error).to_string(), false);
                    }
                }
            }
        };
        let slice = slice_for(self.code_runs.slice());
        let answer = match live.wait(slice).await {
            Some(step) => reply::finished(&run_id, &step, super::MAX_RESULT_BYTES),
            None => reply::paused(&run_id, &live.pending()),
        };
        (answer.to_string(), false)
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
