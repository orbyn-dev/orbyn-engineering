//! A person's one-time answer to a held command over HTTP: the confirmation, the single use, the remote refusal, and the
//! refusals that make an agent's call useless. The table test in `crew_tests.rs` already walks every route with the flag off
//! (404 for every method) and checks the marker and the guards; this file drives the three answers themselves. The account is
//! the development stand-in for the Engine, and the password is assembled at run time.

use std::path::PathBuf;
use std::sync::Arc;

use orbyn_contracts as wire;
use orbyn_core::account::{AccountBackendKind, AccountGate, Password};
use orbyn_core::crew::Stamper;
use orbyn_core::crew::fixtures::hold_command;
use orbyn_core::{Core, CoreConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::router;
use crate::http::AppState;
use crate::http::access::Access;

struct Served {
    port: u16,
    core: Arc<Core>,
    dir: PathBuf,
}

async fn serve(name: &str, remote: bool) -> Served {
    let dir = std::env::temp_dir().join(format!("orbynd-held-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut config = CoreConfig::new(dir.clone(), "http://127.0.0.1:1".into());
    config.crew_preview = true;
    config.account_gate = AccountGate::LocalOnly;
    config.account_backend = AccountBackendKind::Development;
    let core = Arc::new(Core::open(config).unwrap());
    let app = router(AppState {
        core: core.clone(),
        access: Arc::new(Access::new("token", vec![], None)),
        remote,
        ui_dir: None,
        images: Arc::new(orbyn_core::browser::remote_images::ImageProxy::default()),
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await });
    Served { port, core, dir }
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn password() -> String {
    ["correct", "horse", "battery"].join(" ")
}

impl Served {
    async fn call(
        &self,
        method: &str,
        path: &str,
        body: Value,
        nonce: Option<&str>,
    ) -> (u16, Value) {
        let url = format!("http://127.0.0.1:{}{path}", self.port);
        let mut request = reqwest::Client::new()
            .request(method.parse().unwrap(), url)
            .bearer_auth("token")
            .json(&body);
        if let Some(nonce) = nonce {
            request = request.header("x-orbyn-confirm", nonce);
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    /// A private account, not one of the stand-in's seeded ones, whose password is public.
    async fn sign_in(&self) {
        self.core
            .account
            .start_sign_up(
                "K7QM2XWH9QXA3C2D",
                "crew-test@example.org",
                Password::new(password()),
            )
            .await
            .unwrap();
        self.core
            .account
            .verify_email_code("ABCDEFGH")
            .await
            .unwrap();
    }

    /// What the window does before approving: "Confirm it's you" for this hold.
    async fn nonce_for(&self, hold: &str) -> String {
        let (status, body) = self
            .call(
                "POST",
                "/api/account/confirm",
                json!({ "password": password(), "action": "approve_device",
                        "device_id": format!("{}{hold}", orbyn_core::crew::CONFIRM_PREFIX) }),
                None,
            )
            .await;
        assert_eq!(status, 200);
        body["nonce"].as_str().unwrap().to_owned()
    }

    /// A stored task with a worker, holding `command`: the hold's id.
    fn held(&self, label: &str, command: &str) -> (String, String) {
        let stamper = Stamper {
            crew_id: wire::CrewId::new("crew-1").unwrap(),
            engine: wire::CrewToken::new("claude").unwrap(),
            member: wire::CrewMember::new("lead-01").unwrap(),
            now: wire::CrewTimestamp::new("2026-11-12T10:00:00Z").unwrap(),
        };
        let brief = stamper
            .brief(&json!({
                "kind": "implement",
                "outcome": format!("Task {label}."),
                "inputs": [],
                "files": [{ "path": "src/**", "mode": "write" }],
                "allowed_effects": ["read_repo", "write_leased", "run_checks"],
                "stop_when": ["Done."],
                "budget": { "turns": 20, "minutes": 30 }
            }))
            .unwrap();
        let key = stamper
            .answer_key(
                &json!({
                    "checks": [{ "id": "chk-run", "kind": "command",
                        "params": { "run": format!("echo key-check-{label}-text") }, "proves": "It runs." }],
                    "protected_paths": ["tests/**"]
                }),
                &brief.id,
            )
            .unwrap();
        let task = self
            .core
            .store
            .create_crew_task(&brief, &key, &format!("run-{label}"))
            .unwrap();
        let checkout = self.dir.join(format!("checkout-{label}"));
        std::fs::create_dir_all(&checkout).unwrap();
        let hold = hold_command(&self.core, &task.id, &checkout, command);
        (task.id, hold)
    }

    fn state(&self, hold: &str) -> String {
        let list = self.core.autopilot_held().unwrap();
        let item = list.items.iter().find(|i| i.id == hold).unwrap();
        serde_json::to_value(item.state)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned()
    }
}

#[tokio::test]
async fn a_person_approves_once_with_the_confirmation_and_it_is_used_up() {
    let app = serve("approve", false).await;
    let (_, hold) = app.held("a", "make deploy");
    app.sign_in().await;
    let path = format!("/api/crew/approvals/{hold}/approve-once");

    let (status, body) = app
        .call("GET", "/api/crew/approvals/held", json!({}), None)
        .await;
    assert_eq!(status, 200);
    assert_eq!(body["items"][0]["state"], "held");
    assert_eq!(body["items"][0]["command"], "make deploy");

    // The service token alone is not enough: no confirmation, a wrong one, and one for another hold.
    let (status, body) = app.call("POST", &path, json!({}), None).await;
    assert_eq!((status, &body["code"]), (403, &json!("reauth_required")));
    let (status, _) = app
        .call("POST", &path, json!({}), Some("not-a-nonce"))
        .await;
    assert_eq!(status, 403);
    let (_, other) = app.held("b", "make ship");
    let for_other = app.nonce_for(&other).await;
    let (status, _) = app.call("POST", &path, json!({}), Some(&for_other)).await;
    assert_eq!(status, 403);
    assert_eq!(app.state(&hold), "held", "nothing was approved");

    let nonce = app.nonce_for(&hold).await;
    let (status, body) = app.call("POST", &path, json!({}), Some(&nonce)).await;
    assert_eq!((status, &body["state"]), (200, &json!("approved")));
    assert_eq!(body["command"], "make deploy");

    // Used up: the same nonce again, and a fresh one on a decided hold.
    let (status, _) = app.call("POST", &path, json!({}), Some(&nonce)).await;
    assert_eq!(status, 403);
    let fresh = app.nonce_for(&hold).await;
    let (status, body) = app.call("POST", &path, json!({}), Some(&fresh)).await;
    assert_eq!((status, &body["code"]), (409, &json!("invalid_transition")));

    let (_, log) = app
        .call(
            "GET",
            "/api/crew/approvals?kind=approved_once",
            json!({}),
            None,
        )
        .await;
    assert_eq!(log["total"], 1);
    assert_eq!(log["rows"][0]["decided_by"], "person");
}

#[tokio::test]
async fn deny_and_dismiss_need_no_password_and_are_decided_once() {
    let app = serve("deny", false).await;
    let (_, one) = app.held("a", "make deploy");
    let (_, two) = app.held("b", "make ship");
    let (status, body) = app
        .call(
            "POST",
            &format!("/api/crew/approvals/{one}/deny"),
            json!({}),
            None,
        )
        .await;
    assert_eq!((status, &body["state"]), (200, &json!("denied")));
    let (status, body) = app
        .call(
            "POST",
            &format!("/api/crew/approvals/{two}/dismiss"),
            json!({}),
            None,
        )
        .await;
    assert_eq!((status, &body["state"]), (200, &json!("dismissed")));
    for (id, verb) in [(&one, "dismiss"), (&two, "deny")] {
        let (status, _) = app
            .call(
                "POST",
                &format!("/api/crew/approvals/{id}/{verb}"),
                json!({}),
                None,
            )
            .await;
        assert_eq!(status, 409);
    }
    let (status, _) = app
        .call(
            "POST",
            "/api/crew/approvals/hold-nothing/deny",
            json!({}),
            None,
        )
        .await;
    assert_eq!(status, 404);
    let (_, log) = app
        .call("GET", "/api/crew/approvals", json!({}), None)
        .await;
    let kinds: Vec<&str> = log["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["kind"].as_str().unwrap())
        .collect();
    assert!(kinds.contains(&"denied_by_person") && kinds.contains(&"dismissed_by_person"));
}

#[tokio::test]
async fn a_hold_whose_task_ended_answers_gone() {
    let app = serve("gone", false).await;
    let (task, hold) = app.held("a", "make deploy");
    app.sign_in().await;
    app.core
        .store
        .move_crew_task(&task, "cancelled", None)
        .unwrap();
    let nonce = app.nonce_for(&hold).await;
    let (status, body) = app
        .call(
            "POST",
            &format!("/api/crew/approvals/{hold}/approve-once"),
            json!({}),
            Some(&nonce),
        )
        .await;
    assert_eq!((status, &body["code"]), (410, &json!("gone")));
    let (status, _) = app
        .call(
            "POST",
            &format!("/api/crew/approvals/{hold}/deny"),
            json!({}),
            None,
        )
        .await;
    assert_eq!(status, 410);
}

#[tokio::test]
async fn nothing_can_be_approved_without_a_signed_in_account() {
    let app = serve("noaccount", false).await;
    let (_, hold) = app.held("a", "make deploy");
    let (status, _) = app
        .call(
            "POST",
            &format!("/api/crew/approvals/{hold}/approve-once"),
            json!({}),
            Some("0".repeat(32).as_str()),
        )
        .await;
    assert_eq!(status, 403);
    assert_eq!(app.state(&hold), "held");
}

#[tokio::test]
async fn a_remote_viewer_can_read_the_holds_and_answer_none() {
    let app = serve("remote", true).await;
    let (_, hold) = app.held("a", "make deploy");
    let (status, _) = app
        .call("GET", "/api/crew/approvals/held", json!({}), None)
        .await;
    assert_eq!(status, 200);
    for verb in ["approve-once", "deny", "dismiss"] {
        let (status, body) = app
            .call(
                "POST",
                &format!("/api/crew/approvals/{hold}/{verb}"),
                json!({}),
                Some("n"),
            )
            .await;
        assert_eq!(
            (status, &body["code"]),
            (403, &json!("local_only")),
            "{verb}"
        );
    }
    assert_eq!(app.state(&hold), "held");
}
