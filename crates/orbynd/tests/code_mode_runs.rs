//! Code Mode run records against a real `orbynd`: a person's Allow and Deny on a held effect call,
//! the syntax, runtime, call and heap endings, log truncation, and a restart that leaves a run lost.
//!
//! Every run starts the way an engine starts one: the fake engine's `[mcp-call:orbyn_code_run]`
//! prompt calls the tool on the run's own `/mcp/{token}` endpoint, and the service runs the script
//! in its real QuickJS executor. Each run is read back through `GET /api/code-runs/{id}`.

mod support;

use serde_json::{Value, json};
use support::{Options, Service, TempDir, setup, text_of};

const ENGINE: &str = "codex";
const GATE_DENIAL: &str = "Denied in Orbyn.";
const CALL_LIMIT: &str = "A script may make 200 calls.";
const HEAP_LIMIT: &str = "The script used more than its memory limit (heap).";
const SOURCE_LIMIT: &str = "The script is over 65536 bytes.";
const LOST: &str = "This run was lost when Orbyn restarted. Call orbyn_code_run again with the same code and run_id to replay it.";

fn options(desktop: bool, code_mode: bool) -> Options {
    Options {
        args: if desktop {
            vec!["--parent-stdin".into()]
        } else {
            Vec::new()
        },
        env: vec![
            // The real Person gate: a service with a shell has a key, and this turns off the
            // development override `support` leaves on for the other tests.
            ("ORBYN_DEV_DISABLE_PERSON_KEY".into(), "0".into()),
            (
                "ORBYN_CODE_MODE".into(),
                if code_mode { "1" } else { "0" }.into(),
            ),
        ],
    }
}

/// Both this and `support::setup` put the service's data in `<dir>/data`, so a restart here opens
/// the same data dir.
fn service(dir: &TempDir, desktop: bool, code_mode: bool) -> Service {
    Service::start(&dir.path().join("data"), options(desktop, code_mode))
}

/// One raw GET, so a refusal keeps its body.
impl Service {
    async fn get_raw(&self, path: &str) -> (u16, Value) {
        let response = self.get(path).send().await.expect("the read answers");
        let status = response.status().as_u16();
        let text = response.text().await.expect("the body is read");
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    /// `PUT /api/engines/{engine}/code-mode`, with the desktop person key the route needs.
    async fn put_code_mode(&self, engine: &str, body: &str) -> (u16, Value) {
        let key = self
            .person_key
            .as_deref()
            .expect("the desktop service has a person key");
        let response = self
            .client
            .put(format!("{}/api/engines/{engine}/code-mode", self.url))
            .bearer_auth(&self.token)
            .header("x-orbyn-person", key)
            .header("content-type", "application/json")
            .body(body.to_owned())
            .send()
            .await
            .expect("the switch route answers");
        let status = response.status().as_u16();
        let text = response.text().await.expect("the body is read");
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }
}

async fn enable_code_mode(service: &Service) {
    let (status, body) = service.put_code_mode(ENGINE, r#"{"enabled":true}"#).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, json!({ "enabled": true }));
}

fn prompt(script: &str) -> String {
    format!(
        "[mcp-call:orbyn_code_run]{}[/mcp-call]",
        json!({ "code": script })
    )
}

async fn admit_code_run(service: &Service, thread: &str, script: &str) {
    service.admit(thread, &prompt(script)).await;
}

/// Waits for the engine's turn to finish and returns what the engine read from the tool.
async fn finished_text(service: &Service, thread: &str) -> String {
    assert_eq!(service.finished(thread).await, "completed");
    text_of(&service.all_events(thread).await)
}

fn code_reply(text: &str) -> Value {
    assert!(text.contains("tool error=false "), "{text}");
    let raw = text
        .rsplit("tool error=false ")
        .next()
        .unwrap_or_default()
        .trim();
    serde_json::from_str(raw)
        .unwrap_or_else(|error| panic!("the code run reply is JSON: {error}: {text}"))
}

async fn run_record(service: &Service, run_id: &str) -> Value {
    let (status, record) = service.get_raw(&format!("/api/code-runs/{run_id}")).await;
    assert_eq!(status, 200, "{record}");
    record
}

fn reply_run_id(reply: &Value) -> String {
    reply["runId"]
        .as_str()
        .unwrap_or_else(|| panic!("the reply names its run: {reply}"))
        .to_owned()
}

/// The script's first inner call has an effect, so the gate holds the run on a real pending
/// approval. The person's Allow lets the call run; the record comes back with the result, the
/// `ok` call, the recent wait and the `via` that names this run and call.
#[tokio::test]
async fn an_approved_effect_call_completes_the_run_and_records_an_ok_call() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, options(true, true), ENGINE).await;
    enable_code_mode(&service).await;

    let script = format!(
        "await tools.orbyn_thread_organize({});\nreturn \"organized\";",
        json!({ "thread_id": thread, "pinned": true })
    );
    admit_code_run(&service, &thread, &script).await;

    let approval = service.pending().await;
    let approval_id = approval["id"]
        .as_str()
        .expect("a pending approval has an id")
        .to_owned();
    let run_id = approval["via"]["code_run"]
        .as_str()
        .expect("a script's approval names its run")
        .to_owned();
    assert_eq!(approval["kind"], "tool", "{approval}");
    assert_eq!(approval["state"], "pending", "{approval}");
    assert_eq!(approval["via"]["call"], 1, "{approval}");
    assert_eq!(
        approval["via"]["script_sha256"].as_str().map(str::len),
        Some(64),
        "{approval}"
    );

    let (status, decided) = service.decide(&approval_id, true).await;
    assert_eq!(status, 200, "{decided}");

    let reply = code_reply(&finished_text(&service, &thread).await);
    assert_eq!(reply["status"], "completed", "{reply}");
    assert_eq!(reply["result"], "organized", "{reply}");
    assert_eq!(reply_run_id(&reply), run_id);

    let record = run_record(&service, &run_id).await;
    assert_eq!(record["status"], "completed", "{record}");
    assert_eq!(record["result"], "organized", "{record}");
    assert_eq!(record["error"], Value::Null, "{record}");
    // The approval named the script the record holds, not another run's.
    assert_eq!(
        record["script_sha256"], approval["via"]["script_sha256"],
        "{record}"
    );
    // A decision crossed the HTTP round trip, so the wait is recorded. The contract allows any
    // number here; a row stored before waits were recorded is the `null` case.
    assert!(record["waited_ms"].is_number(), "{record}");

    let calls = record["calls"]
        .as_array()
        .expect("the record lists its calls");
    assert_eq!(calls.len(), 1, "{record}");
    assert_eq!(calls[0]["index"], 1, "{record}");
    assert_eq!(calls[0]["tool"], "orbyn_thread_organize", "{record}");
    assert_eq!(calls[0]["status"], "ok", "{record}");
    assert!(calls[0]["duration_ms"].is_number(), "{record}");
}

/// The same effect call, denied by the person: the uncaught refusal ends the run as `denied` with
/// the gate's reason, and the call row keeps the `denied` status.
#[tokio::test]
async fn a_denied_effect_call_ends_the_run_denied_with_the_gate_reason() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, options(true, true), ENGINE).await;
    enable_code_mode(&service).await;

    let script = format!(
        "await tools.orbyn_thread_organize({});\nreturn \"organized\";",
        json!({ "thread_id": thread, "pinned": true })
    );
    admit_code_run(&service, &thread, &script).await;

    let approval = service.pending().await;
    assert_eq!(approval["via"]["call"], 1, "{approval}");
    let approval_id = approval["id"]
        .as_str()
        .expect("a pending approval has an id");
    let run_id = approval["via"]["code_run"]
        .as_str()
        .expect("a script's approval names its run")
        .to_owned();
    let (status, decided) = service.decide(approval_id, false).await;
    assert_eq!(status, 200, "{decided}");

    let reply = code_reply(&finished_text(&service, &thread).await);
    assert_eq!(reply["status"], "error", "{reply}");
    assert_eq!(reply["error"]["kind"], "denied", "{reply}");
    assert!(
        reply["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains(GATE_DENIAL),
        "{reply}"
    );

    let record = run_record(&service, &run_id).await;
    assert_eq!(record["status"], "error", "{record}");
    assert_eq!(record["error"]["kind"], "denied", "{record}");
    assert!(
        record["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains(GATE_DENIAL),
        "{record}"
    );
    let calls = record["calls"]
        .as_array()
        .expect("the record lists its calls");
    assert_eq!(calls.len(), 1, "{record}");
    assert_eq!(calls[0]["index"], 1, "{record}");
    assert_eq!(calls[0]["tool"], "orbyn_thread_organize", "{record}");
    assert_eq!(calls[0]["status"], "denied", "{record}");
}

/// A parse failure is stored as `syntax` and no inner call ran.
#[tokio::test]
async fn a_script_with_a_syntax_error_ends_syntax() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, options(true, true), ENGINE).await;
    enable_code_mode(&service).await;

    admit_code_run(&service, &thread, "const a = 1;\nconst = ;").await;
    let reply = code_reply(&finished_text(&service, &thread).await);
    assert_eq!(reply["status"], "error", "{reply}");
    assert_eq!(reply["error"]["kind"], "syntax", "{reply}");

    let record = run_record(&service, &reply_run_id(&reply)).await;
    assert_eq!(record["status"], "error", "{record}");
    assert_eq!(record["error"]["kind"], "syntax", "{record}");
    assert!(
        !record["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .is_empty(),
        "{record}"
    );
    assert_eq!(record["calls"], json!([]), "{record}");
}

/// An uncaught throw is stored as `runtime` with the thrown message.
#[tokio::test]
async fn a_script_that_throws_ends_runtime_with_its_message() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, options(true, true), ENGINE).await;
    enable_code_mode(&service).await;

    admit_code_run(&service, &thread, "throw new Error('the script threw');").await;
    let reply = code_reply(&finished_text(&service, &thread).await);
    assert_eq!(reply["status"], "error", "{reply}");
    assert_eq!(reply["error"]["kind"], "runtime", "{reply}");
    assert!(
        reply["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("the script threw"),
        "{reply}"
    );

    let record = run_record(&service, &reply_run_id(&reply)).await;
    assert_eq!(record["status"], "error", "{record}");
    assert_eq!(record["error"]["kind"], "runtime", "{record}");
    assert!(
        record["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("the script threw"),
        "{record}"
    );
    assert_eq!(record["calls"], json!([]), "{record}");
}

/// Over 64 KiB is refused by the outer call before the script runs, so the tool answers as an
/// error and no run record is written.
#[tokio::test]
async fn a_script_over_64_kib_is_refused_before_it_runs_and_leaves_no_record() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, options(true, true), ENGINE).await;
    enable_code_mode(&service).await;

    let script = "x".repeat(64 * 1024 + 1);
    assert_eq!(script.len(), 65_537);
    admit_code_run(&service, &thread, &script).await;

    let text = finished_text(&service, &thread).await;
    assert!(text.contains("tool error=true "), "{text}");
    let message = text
        .rsplit("tool error=true ")
        .next()
        .unwrap_or_default()
        .trim();
    assert_eq!(message, SOURCE_LIMIT, "{text}");

    // The refusal happens before `code_start_new`, so the thread has no run to read.
    let runs = service
        .json(&format!("/api/threads/{thread}/code-runs"))
        .await;
    assert_eq!(runs, json!({ "runs": [] }), "{runs}");
}

/// The 201st inner call ends the run at the service's default call cap; the first 200 are
/// recorded as ordinary `ok` calls.
///
/// The 10 s CPU limit is not tested here: `CodeRuns` passes `Limits::default()` and the service
/// has no env or option to shorten it, so the sandbox's own test with a small `Limits` covers it.
#[tokio::test]
async fn more_than_200_inner_calls_end_the_run_at_the_call_limit() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, options(true, true), ENGINE).await;
    enable_code_mode(&service).await;

    let script = "for (let i = 0; i < 201; i++) {\n\
        await tools.orbyn_project_list({});\n\
        }\n\
        return 'done';";
    admit_code_run(&service, &thread, script).await;

    let reply = code_reply(&finished_text(&service, &thread).await);
    assert_eq!(reply["status"], "error", "{reply}");
    assert_eq!(reply["error"]["kind"], "limit", "{reply}");
    assert!(
        reply["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains(CALL_LIMIT),
        "{reply}"
    );

    let record = run_record(&service, &reply_run_id(&reply)).await;
    assert_eq!(record["status"], "error", "{record}");
    assert_eq!(record["error"]["kind"], "limit", "{record}");
    assert!(
        record["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains(CALL_LIMIT),
        "{record}"
    );
    let calls = record["calls"]
        .as_array()
        .expect("the record lists its calls");
    assert_eq!(calls.len(), 200, "{record}");
    assert!(
        calls
            .iter()
            .all(|call| call["status"].as_str() == Some("ok")),
        "{record}"
    );
}

/// A heap bomb ends at the real 64 MiB heap limit, naming the heap rather than the CPU.
#[tokio::test]
async fn a_heap_bomb_ends_the_run_at_the_heap_limit() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, options(true, true), ENGINE).await;
    enable_code_mode(&service).await;

    let script = "const a = []; while (true) a.push(\"x\".repeat(1e6));";
    admit_code_run(&service, &thread, script).await;

    let reply = code_reply(&finished_text(&service, &thread).await);
    assert_eq!(reply["status"], "error", "{reply}");
    assert_eq!(reply["error"]["kind"], "limit", "{reply}");

    let record = run_record(&service, &reply_run_id(&reply)).await;
    assert_eq!(record["status"], "error", "{record}");
    assert_eq!(record["error"]["kind"], "limit", "{record}");
    assert_eq!(record["error"]["message"], HEAP_LIMIT, "{record}");
}

/// A run that logs 501 lines keeps the newest 500 and reports `truncated`.
#[tokio::test]
async fn more_than_500_log_lines_are_truncated_and_keep_the_newest() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, options(true, true), ENGINE).await;
    enable_code_mode(&service).await;

    let script = "for (let i = 1; i <= 501; i++) console.log('line-' + i);\n\
        return 'logged';";
    admit_code_run(&service, &thread, script).await;

    let reply = code_reply(&finished_text(&service, &thread).await);
    assert_eq!(reply["status"], "completed", "{reply}");
    assert_eq!(reply["truncated"], true, "{reply}");
    assert_eq!(
        reply["logs"].as_array().map(|logs| logs.len()),
        Some(500),
        "{reply}"
    );

    let record = run_record(&service, &reply_run_id(&reply)).await;
    assert_eq!(record["status"], "completed", "{record}");
    assert_eq!(record["truncated"], true, "{record}");
    let logs = record["logs"]
        .as_array()
        .expect("the record lists its logs");
    assert_eq!(logs.len(), 500, "{record}");
    assert_eq!(logs[0], json!("line-2"), "{record}");
    assert_eq!(logs[499], json!("line-501"), "{record}");
}

/// A live run held on an approval is gone once the process does. A new service on the same data
/// dir marks it `lost`, keeps its running call row, and answers the documented replay message.
#[tokio::test]
async fn a_restart_marks_a_run_held_on_an_approval_as_lost() {
    let dir = TempDir::new();
    let (mut first, thread) = setup(&dir, options(true, true), ENGINE).await;
    enable_code_mode(&first).await;

    let script = format!(
        "await tools.orbyn_thread_organize({});\nreturn \"organized\";",
        json!({ "thread_id": thread, "pinned": true })
    );
    admit_code_run(&first, &thread, &script).await;
    let approval = first.pending().await;
    let run_id = approval["via"]["code_run"]
        .as_str()
        .expect("a script's approval names its run")
        .to_owned();

    // The crash path, as the restart recovery covers it: a graceful stop would wait on the outer
    // MCP call that the pending approval is holding. `setup` and `service` share `<dir>/data`.
    first.kill();
    drop(first);

    let restarted = service(&dir, true, true);
    let (status, record) = restarted.get_raw(&format!("/api/code-runs/{run_id}")).await;
    assert_eq!(status, 200, "{record}");
    assert_eq!(record["run_id"], run_id, "{record}");
    assert_eq!(record["status"], "error", "{record}");
    assert_eq!(record["error"]["kind"], "lost", "{record}");
    assert_eq!(record["error"]["message"], LOST, "{record}");
    assert_eq!(record["pending"], json!([]), "{record}");
    let calls = record["calls"]
        .as_array()
        .expect("the record keeps its calls");
    assert_eq!(calls.len(), 1, "{record}");
    assert_eq!(calls[0]["status"], "running", "{record}");
}
