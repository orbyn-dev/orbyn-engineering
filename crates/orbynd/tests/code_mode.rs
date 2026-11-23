//! `POST /api/code-runs/{id}/cancel` through the real service: the Person gate, the Code Mode
//! switch, the body-less shape, the fixed answers and a real live run cancelled while its approval
//! is pending. The run's own end with a script a test can pause is also played in the core.

mod support;

use serde_json::{Value, json};
use support::{Options, Service, TempDir, setup};

const CANCEL: &str = "/api/code-runs/no-such-run/cancel";

fn options(desktop: bool, code_mode: bool) -> Options {
    Options {
        args: if desktop {
            vec!["--parent-stdin".into()]
        } else {
            Vec::new()
        },
        env: vec![
            ("ORBYN_DEV_DISABLE_PERSON_KEY".into(), "0".into()),
            (
                "ORBYN_CODE_MODE".into(),
                if code_mode { "1" } else { "0" }.into(),
            ),
        ],
    }
}

fn service(dir: &TempDir, desktop: bool, code_mode: bool) -> Service {
    Service::start(&dir.path().join("data"), options(desktop, code_mode))
}

impl Service {
    /// One cancel with the state as sent, so a refusal keeps its body.
    async fn cancel_at(&self, path: &str, with_key: bool, body: Option<String>) -> (u16, Value) {
        let mut request = self.post(path);
        if with_key && let Some(key) = &self.person_key {
            request = request.header("x-orbyn-person", key);
        }
        if let Some(body) = body {
            request = request
                .header("content-type", "application/json")
                .body(body);
        }
        let response = request.send().await.expect("the cancel route answers");
        let status = response.status().as_u16();
        (status, response.json().await.unwrap_or(Value::Null))
    }

    async fn cancel(&self, with_key: bool, body: Option<String>) -> (u16, Value) {
        self.cancel_at(CANCEL, with_key, body).await
    }

    /// One raw GET, so a refusal keeps its body.
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

#[tokio::test]
async fn cancel_needs_the_person_key_like_every_person_route() {
    let dir = TempDir::new();
    let service = service(&dir, true, true);
    let (status, answer) = service.cancel(false, None).await;
    assert_eq!(status, 403, "{answer}");
    assert_eq!(answer["code"], "person_required", "{answer}");
    assert_eq!(
        answer["message"],
        "This action needs the person at the desktop app."
    );

    let (status, answer) = service.cancel(true, None).await;
    assert_eq!(status, 404, "{answer}");
    assert_eq!(answer["code"], "not_found", "{answer}");
}

#[tokio::test]
async fn a_shell_less_service_answers_desktop_required() {
    let dir = TempDir::new();
    let service = service(&dir, false, true);
    assert!(service.person_key.is_none());
    let (status, answer) = service.cancel(false, None).await;
    assert_eq!(status, 403, "{answer}");
    assert_eq!(answer["code"], "desktop_required", "{answer}");
    assert_eq!(
        answer["message"],
        "This action is only available in the desktop app."
    );
}

#[tokio::test]
async fn a_body_over_1_kib_is_refused_with_the_fixed_copy() {
    let dir = TempDir::new();
    let service = service(&dir, true, true);
    let body = format!("{{\"pad\":\"{}\"}}", "x".repeat(2 * 1024));
    let (status, answer) = service.cancel(true, Some(body)).await;
    assert_eq!(status, 400, "{answer}");
    assert_eq!(answer["code"], "invalid_input", "{answer}");
    assert_eq!(answer["message"], "This route takes no body.");
}

#[tokio::test]
async fn the_body_limit_is_exactly_1_kib() {
    let dir = TempDir::new();
    let service = service(&dir, true, true);
    let bare = json!({ "pad": "" }).to_string().len();
    let at_limit = json!({ "pad": "x".repeat(1024 - bare) }).to_string();
    assert_eq!(at_limit.len(), 1024);
    let (status, answer) = service.cancel(true, Some(at_limit)).await;
    assert_eq!(status, 404, "1 KiB is read and ignored: {answer}");
    assert_eq!(answer["code"], "not_found", "{answer}");

    let over = json!({ "pad": "x".repeat(1024 - bare + 1) }).to_string();
    assert_eq!(over.len(), 1025);
    let (status, answer) = service.cancel(true, Some(over)).await;
    assert_eq!(status, 400, "{answer}");
    assert_eq!(answer["code"], "invalid_input", "{answer}");
    assert_eq!(answer["message"], "This route takes no body.");
}

#[tokio::test]
async fn with_code_mode_off_the_cancel_route_does_not_exist() {
    let dir = TempDir::new();
    let service = service(&dir, true, false);
    let (status, answer) = service.cancel(true, None).await;
    assert_eq!(status, 404, "{answer}");
    assert_eq!(answer["code"], "not_found", "{answer}");
    // The flag is checked before the body, so an oversized body is still "no such route".
    let (status, answer) = service
        .cancel(
            true,
            Some(json!({ "pad": "x".repeat(2 * 1024) }).to_string()),
        )
        .await;
    assert_eq!(status, 404, "{answer}");
    assert_eq!(answer["code"], "not_found", "{answer}");
}

fn assert_live(record: &Value, when: &str) {
    assert!(
        matches!(record["status"].as_str(), Some("running" | "paused")),
        "{when}: {record}"
    );
}

/// A real Code Mode run that the gate is holding for a person's approval: the keyless cancel is
/// refused and changes nothing, the person's cancel ends the run and denies the approval, and a
/// second cancel answers the same record. The outer MCP call is still waiting on its 40 s slice,
/// so the test reads and cancels the run while the turn is held.
#[tokio::test]
async fn a_person_cancel_ends_a_live_run_and_denies_its_pending_approval() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, options(true, true), "codex").await;
    let (status, body) = service.put_code_mode("codex", r#"{"enabled":true}"#).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, json!({ "enabled": true }));

    // The script's first inner call has an effect, so the approval gate asks the person and the
    // run cannot finish until that approval is answered.
    let script = format!(
        "await tools.orbyn_thread_organize({});\nreturn \"organized\";",
        json!({ "thread_id": thread, "pinned": true })
    );
    let prompt = format!(
        "[mcp-call:orbyn_code_run]{}[/mcp-call]",
        json!({ "code": script })
    );
    service.admit(&thread, &prompt).await;

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

    let (status, live) = service.get_raw(&format!("/api/code-runs/{run_id}")).await;
    assert_eq!(status, 200, "{live}");
    assert_eq!(live["run_id"], run_id, "{live}");
    assert_live(&live, "before the cancel");
    assert_eq!(
        live["pending"],
        json!([{ "tool": "orbyn_thread_organize", "call": 1 }]),
        "{live}"
    );

    // The Person gate runs before the handler: no key means no cancel and nothing changes.
    let cancel = format!("/api/code-runs/{run_id}/cancel");
    let (status, refused) = service.cancel_at(&cancel, false, None).await;
    assert_eq!(status, 403, "{refused}");
    assert_eq!(refused["code"], "person_required", "{refused}");
    assert_eq!(
        refused["message"], "This action needs the person at the desktop app.",
        "{refused}"
    );
    let (status, live) = service.get_raw(&format!("/api/code-runs/{run_id}")).await;
    assert_eq!(status, 200, "{live}");
    assert_live(&live, "after the keyless refusal");
    assert_eq!(
        live["pending"],
        json!([{ "tool": "orbyn_thread_organize", "call": 1 }]),
        "{live}"
    );
    let pending = service.json("/api/approvals?state=pending").await;
    let pending = pending.as_array().expect("the approval queue is a list");
    assert!(
        pending.iter().any(|approval| approval["id"] == approval_id),
        "{pending:?}"
    );

    // With the key the run ends with the fixed cancel error and no call is left running.
    let (status, cancelled) = service.cancel_at(&cancel, true, None).await;
    assert_eq!(status, 200, "{cancelled}");
    assert_eq!(cancelled["run_id"], run_id, "{cancelled}");
    assert_eq!(cancelled["status"], "error", "{cancelled}");
    assert_eq!(cancelled["error"]["kind"], "cancelled", "{cancelled}");
    assert_eq!(
        cancelled["error"]["message"], "The run was cancelled.",
        "{cancelled}"
    );
    assert_eq!(cancelled["pending"], json!([]), "{cancelled}");
    let (status, stored) = service.get_raw(&format!("/api/code-runs/{run_id}")).await;
    assert_eq!(status, 200, "{stored}");
    assert_eq!(stored, cancelled);

    // The pending approval went through the person's denial path and left the queue.
    let approvals = service.json("/api/approvals").await;
    let decided = approvals
        .as_array()
        .and_then(|list| list.iter().find(|approval| approval["id"] == approval_id))
        .expect("the approval is kept after its decision");
    assert_eq!(decided["state"], "denied", "{decided}");
    assert_eq!(decided["decided_by"], "user", "{decided}");
    let pending = service.json("/api/approvals?state=pending").await;
    let pending = pending.as_array().expect("the approval queue is a list");
    assert!(
        !pending.iter().any(|approval| approval["id"] == approval_id),
        "{pending:?}"
    );

    // The thread's run row carries the cancelled kind without opening the record.
    let runs = service
        .json(&format!("/api/threads/{thread}/code-runs"))
        .await;
    let row = runs["runs"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["run_id"] == run_id))
        .expect("the thread lists the cancelled run");
    assert_eq!(row["status"], "error", "{row}");
    assert_eq!(row["error_kind"], "cancelled", "{row}");

    // A second cancel answers the same record.
    let (status, again) = service.cancel_at(&cancel, true, None).await;
    assert_eq!(status, 200, "{again}");
    assert_eq!(again, cancelled);
}

#[tokio::test]
async fn a_cancel_of_a_run_that_already_completed_is_409_and_changes_nothing() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, options(true, true), "codex").await;
    let (status, body) = service.put_code_mode("codex", r#"{"enabled":true}"#).await;
    assert_eq!(status, 200, "{body}");

    let script = "const projects = await tools.orbyn_project_list({});\nreturn projects.length;";
    let prompt = format!(
        "[mcp-call:orbyn_code_run]{}[/mcp-call]",
        json!({ "code": script })
    );
    service.admit(&thread, &prompt).await;
    assert_eq!(service.finished(&thread).await, "completed");
    let runs = service
        .json(&format!("/api/threads/{thread}/code-runs"))
        .await;
    let completed_id = runs["runs"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["status"] == "completed"))
        .and_then(|row| row["run_id"].as_str())
        .expect("the script completed and the thread lists it")
        .to_owned();

    let (status, answer) = service
        .cancel_at(&format!("/api/code-runs/{completed_id}/cancel"), true, None)
        .await;
    assert_eq!(status, 409, "{answer}");
    assert_eq!(answer["code"], "run_finished", "{answer}");
    assert_eq!(answer["message"], "This run already ended.", "{answer}");
    let (status, record) = service
        .get_raw(&format!("/api/code-runs/{completed_id}"))
        .await;
    assert_eq!(status, 200, "{record}");
    assert_eq!(record["status"], "completed", "{record}");
    assert_eq!(record["error"], Value::Null, "{record}");
}
