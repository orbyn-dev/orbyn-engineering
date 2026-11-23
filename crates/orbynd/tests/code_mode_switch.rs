//! The Code Mode routes the web client calls (`web/src/features/codemode/api.ts`) against a real
//! `orbynd`: the per-engine switch with its Person gate, the per-thread run list and one run record.
//!
//! Every success body is checked for its exact key set against the ts-rs generated types the client
//! reads (`web/src/generated/CodeModeSetting.ts`, `CodeRunList.ts`, `CodeRunSummary.ts`,
//! `CodeRunRecord.ts`, `CodeRunCall.ts`). Serde writes an absent `Option` as `null`, so the optional
//! fields in those files are present keys on the wire; a field added to or dropped from a contract
//! fails here even when the client would not notice.
//!
//! A real run is created the way an engine creates one: the fake engine's `[mcp-call:...]` prompt
//! calls `orbyn_code_run` over the run's own `/mcp/{token}` endpoint, and the service runs the
//! script in its real QuickJS executor. The script starts with an inner `orbyn_project_list` read,
//! so the listing, the record, the call row and the `calls`/`waited_ms` counters are all exercised.

mod support;

use std::collections::BTreeSet;

use serde_json::{Value, json};
use support::{Options, Service, TempDir, setup, text_of};

/// A real known engine: `crates/orbyn-core/src/agents/engines/known.rs`.
const ENGINE: &str = "claude";

/// The exact fields `web/src/generated/CodeModeSetting.ts` gives: `{ enabled: boolean }`.
const SETTING_KEYS: &[&str] = &["enabled"];
/// The exact fields `web/src/generated/CodeRunList.ts` gives: `{ runs: CodeRunSummary[] }`.
const LIST_KEYS: &[&str] = &["runs"];
/// `web/src/generated/CodeRunSummary.ts`. The generated `?` fields are `Option` on the wire and
/// serialize as `null`, so all seven keys are present.
const SUMMARY_KEYS: &[&str] = &[
    "run_id",
    "status",
    "started_at",
    "ended_at",
    "calls",
    "script_head",
    "waited_ms",
];
/// `web/src/generated/CodeRunRecord.ts`. Every optional field is `null` here, never absent.
const RECORD_KEYS: &[&str] = &[
    "run_id",
    "status",
    "script",
    "script_sha256",
    "started_at",
    "ended_at",
    "calls",
    "logs",
    "logs_total",
    "result",
    "error",
    "pending",
    "truncated",
    "resumes",
    "waited_ms",
    "crew_worker",
];
/// `web/src/generated/CodeRunCall.ts`.
const CALL_KEYS: &[&str] = &[
    "index",
    "tool",
    "status",
    "preview",
    "result_bytes",
    "duration_ms",
];

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

fn start_service(dir: &TempDir, desktop: bool, code_mode: bool) -> Service {
    Service::start(&dir.path().join("data"), options(desktop, code_mode))
}

/// One raw GET, so a refusal keeps its body instead of panicking in `Service::json`.
impl Service {
    async fn get_raw(&self, path: &str) -> (u16, Value) {
        let response = self.get(path).send().await.unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    /// `PUT /api/engines/{id}/code-mode` with a body as sent, so malformed JSON and an over-limit
    /// body reach the service unchanged. `with_key` false sends no `x-orbyn-person` header.
    async fn put_code_mode(&self, engine: &str, body: &str, with_key: bool) -> (u16, Value) {
        let mut request = self
            .client
            .put(format!("{}/api/engines/{engine}/code-mode", self.url))
            .bearer_auth(&self.token)
            .header("content-type", "application/json")
            .body(body.to_owned());
        if with_key && let Some(key) = &self.person_key {
            request = request.header("x-orbyn-person", key);
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    async fn code_mode(&self, engine: &str) -> Value {
        let (status, body) = self
            .get_raw(&format!("/api/engines/{engine}/code-mode"))
            .await;
        assert_eq!(status, 200, "{body}");
        body
    }
}

/// A response object with exactly `expected` as its keys.
fn assert_keys(value: &Value, expected: &[&str], what: &str) {
    let object = value
        .as_object()
        .unwrap_or_else(|| panic!("{what} is not an object: {value}"));
    let actual: BTreeSet<&str> = object.keys().map(String::as_str).collect();
    let expected: BTreeSet<&str> = expected.iter().copied().collect();
    assert_eq!(actual, expected, "{what} fields: {value}");
}

fn not_found(message: &str) -> Value {
    json!({ "code": "not_found", "message": message })
}

fn invalid_input(message: &str) -> Value {
    json!({ "code": "invalid_input", "message": message })
}

#[tokio::test]
async fn the_switch_is_off_by_default_for_a_known_engine_and_an_unknown_one() {
    let dir = TempDir::new();
    let service = start_service(&dir, true, true);

    // A regression that reads the setting wrong (or writes `enabled` under another name) fails:
    // the known engine's default is exactly `{ "enabled": false }`.
    let (status, body) = service
        .get_raw(&format!("/api/engines/{ENGINE}/code-mode"))
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, json!({ "enabled": false }));
    assert_keys(&body, SETTING_KEYS, "CodeModeSetting");

    // The route validates the id's shape, not that the engine exists. An unknown engine id with a
    // valid shape answers 200 and off, exactly like a known engine: no lookup happens. A regression
    // that starts checking the engine catalogue would answer 404 here. docs/code-mode.md records it.
    for unknown in ["no-such-engine", "unknown.engine:v2"] {
        let (status, body) = service
            .get_raw(&format!("/api/engines/{unknown}/code-mode"))
            .await;
        assert_eq!(status, 200, "{unknown}: {body}");
        assert_eq!(body, json!({ "enabled": false }), "{unknown}");
        assert_keys(&body, SETTING_KEYS, "CodeModeSetting");
    }

    // A key-shaped wrong id is the one refusal: 404 `not_found`, not a 500 from the store.
    let (status, body) = service
        .get_raw("/api/engines/bad%20engine%21/code-mode")
        .await;
    assert_eq!(status, 404, "{body}");
    assert_eq!(body, not_found("engine not found"));
}

#[tokio::test]
async fn the_switch_writes_what_it_stored_and_survives_a_restart() {
    let dir = TempDir::new();
    let service = start_service(&dir, true, true);
    assert_eq!(service.code_mode(ENGINE).await, json!({ "enabled": false }));

    // The write answers with what was stored, not with the request echoed: a regression that returns
    // the body before writing, or writes the flip of it, fails the following GET.
    let (status, body) = service
        .put_code_mode(ENGINE, r#"{"enabled":true}"#, true)
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, json!({ "enabled": true }));
    assert_keys(&body, SETTING_KEYS, "CodeModeSetting");
    assert_eq!(service.code_mode(ENGINE).await, json!({ "enabled": true }));

    let (status, body) = service
        .put_code_mode(ENGINE, r#"{"enabled":false}"#, true)
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, json!({ "enabled": false }));
    assert_eq!(service.code_mode(ENGINE).await, json!({ "enabled": false }));

    // The setting is in the settings table, not in memory: a second Service on the same data dir
    // reads it back. A regression that keeps the switch in a process map fails here.
    let (status, body) = service
        .put_code_mode(ENGINE, r#"{"enabled":true}"#, true)
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, json!({ "enabled": true }));
    service.stop().await;

    let restarted = start_service(&dir, true, true);
    assert_eq!(
        restarted.code_mode(ENGINE).await,
        json!({ "enabled": true })
    );
}

#[tokio::test]
async fn a_refused_switch_write_is_not_stored() {
    let dir = TempDir::new();
    let service = start_service(&dir, true, true);
    let (status, body) = service
        .put_code_mode(ENGINE, r#"{"enabled":true}"#, true)
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(service.code_mode(ENGINE).await, json!({ "enabled": true }));

    // No `x-orbyn-person`: the Person gate refuses before the handler. A regression that drops the
    // route from ENFORCED/PERSON_ROUTES would store the false value and answer 200.
    let (status, body) = service
        .put_code_mode(ENGINE, r#"{"enabled":false}"#, false)
        .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(
        body,
        json!({
            "code": "person_required",
            "message": "This action needs the person at the desktop app."
        })
    );
    assert_eq!(service.code_mode(ENGINE).await, json!({ "enabled": true }));

    // Invalid JSON is refused by the JSON extractor with its one fixed sentence, and the handler
    // never runs. A regression that deserializes with serde's default rejection leaks its prose.
    let (status, body) = service.put_code_mode(ENGINE, r#"{"enabled":"#, true).await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body, invalid_input("The request body is not valid JSON."));
    assert_eq!(service.code_mode(ENGINE).await, json!({ "enabled": true }));

    // A non-bool `enabled` names the field but never the value (`true` here would be valid; the
    // point is the sentence). A regression that accepts "false"/1 would write a wrong setting.
    let (status, body) = service
        .put_code_mode(ENGINE, r#"{"enabled":"off"}"#, true)
        .await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(
        body,
        invalid_input(
            "The request body is not valid: the field `enabled` has a value this route does not accept."
        )
    );
    assert_eq!(service.code_mode(ENGINE).await, json!({ "enabled": true }));

    // A body with no `enabled` is refused too, so a later `#[serde(default)]` cannot silently turn
    // the switch off. The extractor's sentence is not pinned; the status and code are fixed.
    let (status, body) = service.put_code_mode(ENGINE, r#"{}"#, true).await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["code"], "invalid_input", "{body}");
    assert_eq!(service.code_mode(ENGINE).await, json!({ "enabled": true }));

    // The router's 4 MiB DefaultBodyLimit refuses before the handler. A regression that removes the
    // limit, or streams the body into the store, would not answer this fixed sentence.
    let over = format!(
        r#"{{"enabled":false,"pad":"{}"}}"#,
        "x".repeat(4 * 1024 * 1024)
    );
    let (status, body) = service.put_code_mode(ENGINE, &over, true).await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body, invalid_input("The request body could not be read."));
    assert_eq!(service.code_mode(ENGINE).await, json!({ "enabled": true }));
}

#[tokio::test]
async fn an_extra_field_is_ignored_by_the_switch_write() {
    let dir = TempDir::new();
    let service = start_service(&dir, true, true);
    assert_eq!(service.code_mode(ENGINE).await, json!({ "enabled": false }));

    // The required refusal is not what the service does: `CodeModeSetting` has no
    // `deny_unknown_fields`, so serde ignores the unknown key and the write succeeds. This test
    // pins that real behaviour. Making the extra field a 400 needs one attribute on the contract
    // (`#[serde(deny_unknown_fields)]` on `CodeModeSetting` in
    // `crates/orbyn-contracts/src/code_mode.rs`); that is production code, so this branch reports
    // it instead of changing it.
    let (status, body) = service
        .put_code_mode(ENGINE, r#"{"enabled":true,"not_a_field":1}"#, true)
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, json!({ "enabled": true }));
    assert_keys(&body, SETTING_KEYS, "CodeModeSetting");
    assert_eq!(service.code_mode(ENGINE).await, json!({ "enabled": true }));
}

#[tokio::test]
async fn a_shell_less_service_answers_desktop_required_and_writes_nothing() {
    let dir = TempDir::new();
    // No `--parent-stdin`: there is no shell, so no person key exists to present, not even the
    // development override (the options turn it off).
    let service = start_service(&dir, false, true);
    assert!(service.person_key.is_none());

    let (status, body) = service
        .put_code_mode(ENGINE, r#"{"enabled":true}"#, false)
        .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(
        body,
        json!({
            "code": "desktop_required",
            "message": "This action is only available in the desktop app."
        })
    );
    assert_eq!(service.code_mode(ENGINE).await, json!({ "enabled": false }));
}

#[tokio::test]
async fn with_code_mode_off_every_client_route_is_not_offered() {
    let dir = TempDir::new();
    let service = start_service(&dir, true, false);
    assert!(service.person_key.is_some());

    // Every well-formed request the web client can make is `404 code mode not found`: the client
    // turns exactly that into `null` ("Code Mode is not offered"). A regression that leaves a route
    // registered or answers another code shows up here.
    for path in [
        format!("/api/engines/{ENGINE}/code-mode"),
        "/api/threads/no-such-thread/code-runs".to_owned(),
        "/api/code-runs/no-such-run".to_owned(),
    ] {
        let (status, body) = service.get_raw(&path).await;
        assert_eq!(status, 404, "{path}: {body}");
        assert_eq!(body, not_found("code mode not found"), "{path}");
    }

    let (status, body) = service
        .put_code_mode(ENGINE, r#"{"enabled":true}"#, true)
        .await;
    assert_eq!(status, 404, "{body}");
    assert_eq!(body, not_found("code mode not found"));

    // The Person gate still runs first: the flag is checked in the handler, after the route layer.
    // A keyless PUT is `403 person_required` even with Code Mode off; the client always sends its
    // key, so its request sees the 404 above.
    let (status, body) = service
        .put_code_mode(ENGINE, r#"{"enabled":true}"#, false)
        .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["code"], "person_required", "{body}");
}

#[tokio::test]
async fn a_thread_with_no_runs_and_an_unknown_thread_both_answer_an_empty_list() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, options(true, true), "codex").await;

    // The empty list is exactly `{ "runs": [] }`: a regression that adds a total or omits the
    // wrapper fails the key set, and one that seeds a row for a new thread fails the value.
    let (status, body) = service
        .get_raw(&format!("/api/threads/{thread}/code-runs"))
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, json!({ "runs": [] }));
    assert_keys(&body, LIST_KEYS, "CodeRunList");

    // The read does not look the thread up: an unknown (or malformed) thread id answers the same
    // 200 empty list as a thread with no runs. docs/code-mode.md records this. A regression that
    // starts validating threads would answer 404 here.
    for unknown in ["no-such-thread", "bad%20thread%21"] {
        let (status, body) = service
            .get_raw(&format!("/api/threads/{unknown}/code-runs"))
            .await;
        assert_eq!(status, 200, "{unknown}: {body}");
        assert_eq!(body, json!({ "runs": [] }), "{unknown}");
        assert_keys(&body, LIST_KEYS, "CodeRunList");
    }
}

#[tokio::test]
async fn unknown_and_malformed_run_ids_are_404_not_500() {
    let dir = TempDir::new();
    let (service, _) = setup(&dir, options(true, true), "codex").await;

    // The guard is `valid_run_id`: only letters, digits, `_` and `-` up to 64 characters reach the
    // store. A regression that drops it sends `../x`/a 65-byte key to SQLite; one that returns a
    // 500 for the guard's `NotFound` fails the status. The body is the reader's one 404.
    let long = "x".repeat(65);
    for id in ["no-such-run", "bad%20run%21", long.as_str()] {
        let (status, body) = service.get_raw(&format!("/api/code-runs/{id}")).await;
        assert_eq!(status, 404, "{id}: {body}");
        assert_eq!(body, not_found("run not found"), "{id}");
    }
}

#[tokio::test]
async fn a_real_run_is_listed_and_read_with_the_contract_shape() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, options(true, true), "codex").await;
    assert_eq!(
        service.code_mode("codex").await,
        json!({ "enabled": false })
    );
    let (status, body) = service
        .put_code_mode("codex", r#"{"enabled":true}"#, true)
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, json!({ "enabled": true }));

    // The engine reaches `orbyn_code_run` over the run's own MCP endpoint, with the binding token
    // the driver gave it. The service runs the real QuickJS executor. The script reads a project
    // (a no-approval tool) so the record has a real call row, then logs and returns a value.
    let script = "const projects = await tools.orbyn_project_list({});\nconsole.log(projects.length);\nreturn projects.length;";
    let prompt = format!(
        "[mcp-call:orbyn_code_run]{}[/mcp-call]",
        json!({ "code": script })
    );
    service.admit(&thread, &prompt).await;
    assert_eq!(service.finished(&thread).await, "completed");
    let text = text_of(&service.all_events(&thread).await);
    let reply: Value = serde_json::from_str(
        text.rsplit("tool error=false ")
            .next()
            .unwrap_or_default()
            .trim(),
    )
    .unwrap();
    assert_eq!(reply["status"], "completed", "{reply}");
    assert_eq!(reply["result"], 1, "{reply}");
    assert_eq!(reply["logs"], json!(["1"]), "{reply}");
    let run_id = reply["runId"].as_str().unwrap().to_owned();

    // The listing: one summary, newest first still means this run, and the counters the web row
    // draws are present. A regression that turns `calls` into an array here, drops `waited_ms`, or
    // returns a record instead of a summary fails the key set and the values.
    let (status, list) = service
        .get_raw(&format!("/api/threads/{thread}/code-runs"))
        .await;
    assert_eq!(status, 200, "{list}");
    assert_keys(&list, LIST_KEYS, "CodeRunList");
    let runs = list["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 1, "{list}");
    let summary = &runs[0];
    assert_keys(summary, SUMMARY_KEYS, "CodeRunSummary");
    assert_eq!(summary["run_id"], run_id);
    assert_eq!(summary["status"], "completed");
    assert_eq!(summary["calls"], 1);
    assert_eq!(summary["script_head"], script);
    assert_eq!(summary["waited_ms"], 0);
    assert!(summary["started_at"].is_string(), "{summary}");
    assert!(summary["ended_at"].is_string(), "{summary}");

    // The record: the same run, the script and its hash, the logs and result, no pending call and
    // one finished call with every `CodeRunCall` field. A regression that writes the preview or the
    // counters under other names, or leaves `calls` out of the record, fails here.
    let (status, record) = service.get_raw(&format!("/api/code-runs/{run_id}")).await;
    assert_eq!(status, 200, "{record}");
    assert_keys(&record, RECORD_KEYS, "CodeRunRecord");
    assert_eq!(record["run_id"], run_id);
    assert_eq!(record["status"], "completed");
    assert_eq!(record["script"], script);
    assert_eq!(
        record["script_sha256"].as_str().unwrap().len(),
        64,
        "{record}"
    );
    assert!(record["started_at"].is_string(), "{record}");
    assert!(record["ended_at"].is_string(), "{record}");
    assert_eq!(record["logs"], json!(["1"]));
    assert_eq!(record["logs_total"], Value::Null);
    assert_eq!(record["result"], 1);
    assert_eq!(record["error"], Value::Null);
    assert_eq!(record["pending"], json!([]));
    assert_eq!(record["truncated"], false);
    assert_eq!(record["resumes"], 0);
    assert_eq!(record["waited_ms"], 0);
    assert_eq!(record["crew_worker"], false);

    let calls = record["calls"].as_array().unwrap();
    assert_eq!(calls.len(), 1, "{record}");
    let call = &calls[0];
    assert_keys(call, CALL_KEYS, "CodeRunCall");
    // Script calls are 1-based (`RunHost::next` starts at 1); the first call is index 1.
    assert_eq!(call["index"], 1);
    assert_eq!(call["tool"], "orbyn_project_list");
    assert_eq!(call["status"], "ok");
    assert!(call["preview"].is_string(), "{call}");
    // The whole result is small, so its size is recorded and its duration is a number: a
    // regression that leaves either counter absent or null fails here.
    assert!(call["result_bytes"].is_number(), "{call}");
    assert!(call["duration_ms"].is_number(), "{call}");
}
