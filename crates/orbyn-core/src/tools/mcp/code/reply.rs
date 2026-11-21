//! The reply of `orbyn_code_run`, kept under the size every tool reply has.

use super::runs::Pending;
use orbyn_code_sandbox::{ErrorKind, RunError, Step};
use serde_json::{Value, json};

/// Room left under `MAX_RESULT_BYTES` for the JSON around the result.
const HEADROOM: usize = 1024;
const PREVIEW_BYTES: usize = 64 * 1024;

pub(super) fn error_json(error: &RunError) -> Value {
    let kind = match error.kind {
        ErrorKind::Syntax => "syntax",
        ErrorKind::Limit => "limit",
        ErrorKind::Runtime if error.code.as_deref() == Some("denied") => "denied",
        ErrorKind::Runtime if error.code.as_deref() == Some("lost") => "lost",
        ErrorKind::Runtime => "runtime",
    };
    let mut out = json!({ "kind": kind, "message": error.message });
    if let Some(line) = error.line {
        out["line"] = json!(line);
    }
    out
}

pub(super) fn lost(run_id: &str, message: &str) -> Value {
    json!({ "status": "error", "runId": run_id, "logs": [],
        "error": { "kind": "lost", "message": message } })
}

pub(super) fn failed(run_id: &str, error: &RunError) -> Value {
    json!({ "status": "error", "runId": run_id, "logs": [], "error": error_json(error) })
}

pub(super) fn paused(run_id: &str, pending: &[Pending]) -> Value {
    let calls: Vec<Value> = pending
        .iter()
        .map(|p| json!({ "tool": p.tool, "call": p.call }))
        .collect();
    json!({ "status": "paused", "runId": run_id, "pending": calls })
}

/// The reply for a finished run, trimmed to fit `limit` bytes.
pub(super) fn finished(run_id: &str, step: &Step, limit: usize) -> Value {
    let mut reply = match step {
        Step::Done {
            result,
            logs,
            truncated,
        } => json!({
            "status": "completed", "runId": run_id, "result": result, "logs": logs, "truncated": truncated }),
        Step::Failed {
            error,
            logs,
            truncated,
        } => json!({
            "status": "error", "runId": run_id, "error": error_json(error), "logs": logs, "truncated": truncated }),
        Step::Yielded => return paused(run_id, &[]),
    };
    let budget = limit.saturating_sub(HEADROOM);
    if reply.to_string().len() > budget {
        reply["logs"] = json!([]);
        reply["truncated"] = json!(true);
    }
    if reply.to_string().len() > budget && reply.get("result").is_some() {
        let text = reply["result"].to_string();
        let mut end = PREVIEW_BYTES.min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        reply["result"] =
            json!({ "truncated": true, "bytes": text.len(), "preview": &text[..end] });
    }
    reply
}

#[cfg(test)]
mod tests {
    use super::*;

    fn done(result: Value, logs: Vec<String>) -> Step {
        Step::Done {
            result,
            logs,
            truncated: false,
        }
    }

    #[test]
    fn a_small_result_is_returned_as_it_is() {
        let reply = finished(
            "r1",
            &done(json!({ "a": 1 }), vec!["hi".into()]),
            256 * 1024,
        );
        assert_eq!(reply["status"], "completed");
        assert_eq!(reply["result"], json!({ "a": 1 }));
        assert_eq!(reply["logs"], json!(["hi"]));
        assert_eq!(reply["runId"], "r1");
    }

    #[test]
    fn a_big_result_drops_the_logs_first_and_then_becomes_a_preview() {
        let logs = vec!["x".repeat(600)];
        let reply = finished(
            "r",
            &done(json!("y".repeat(900)), logs.clone()),
            2048 - 1024 + 1200,
        );
        assert_eq!(reply["logs"], json!([]), "logs went first");
        assert_eq!(reply["truncated"], true);
        assert_eq!(
            reply["result"].as_str().map(str::len),
            Some(900),
            "the result still fits"
        );
        let big = finished("r", &done(json!("é".repeat(200_000)), vec![]), 256 * 1024);
        assert_eq!(big["result"]["truncated"], true);
        assert!(big["result"]["bytes"].as_u64().unwrap() > 256 * 1024);
        assert!(
            big.to_string().len() < 256 * 1024,
            "valid JSON under the cap"
        );
    }

    #[test]
    fn errors_name_their_kind_and_a_denial_is_denied() {
        let run = |kind, code: Option<&str>| {
            let error = RunError {
                kind,
                message: "m".into(),
                line: Some(3),
                code: code.map(str::to_owned),
            };
            failed("r", &error)["error"].clone()
        };
        assert_eq!(
            run(ErrorKind::Syntax, None),
            json!({ "kind": "syntax", "message": "m", "line": 3 })
        );
        assert_eq!(run(ErrorKind::Limit, None)["kind"], "limit");
        assert_eq!(run(ErrorKind::Runtime, None)["kind"], "runtime");
        assert_eq!(run(ErrorKind::Runtime, Some("denied"))["kind"], "denied");
    }
}
