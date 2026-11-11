//! The account routes: the gate, the remote refusal and the re-entry guard. The guard test
//! walks the whole route table, so a new route that changes account state, under any method
//! or prefix, fails here unless it is listed with its reason or refused without the
//! confirmation. Failures name a route by its place in the sorted list, never by its text.

use std::collections::BTreeSet;
use std::sync::Arc;

use orbyn_core::account::{AccountBackendKind, AccountGate};
use orbyn_core::{Core, CoreConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::account::{OPEN, READS};
use super::router;
use super::tests::registered;
use crate::http::AppState;
use crate::http::access::Access;

const SECRET: &str = "correct horse battery";

struct Served {
    port: u16,
    routes: BTreeSet<(String, String)>,
    _dir: std::path::PathBuf,
}

async fn serve(name: &str, gate: AccountGate, remote: bool) -> Served {
    let dir = std::env::temp_dir().join(format!("orbynd-account-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut config = CoreConfig::new(dir.clone(), "http://127.0.0.1:1".into());
    config.account_gate = gate;
    config.account_backend = AccountBackendKind::Development;
    let core = Arc::new(Core::open(config).unwrap());
    let app = router(AppState {
        core,
        access: Arc::new(Access::new("token", vec![], None)),
        remote,
        ui_dir: None,
        images: Arc::new(orbyn_core::browser::remote_images::ImageProxy::default()),
    });
    let routes = registered(&app);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await });
    Served {
        port,
        routes,
        _dir: dir,
    }
}

impl Served {
    async fn call(&self, method: &str, path: &str, body: Value) -> (u16, Value) {
        let url = format!("http://127.0.0.1:{}{path}", self.port);
        let response = reqwest::Client::new()
            .request(method.parse().unwrap(), url)
            .bearer_auth("token")
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    /// Every route under `/api/account`, in sorted order.
    fn account_routes(&self) -> Vec<(String, String)> {
        self.routes
            .iter()
            .filter(|(_, path)| path.starts_with("/api/account"))
            .cloned()
            .collect()
    }

    async fn sign_in(&self) {
        let (status, _) = self
            .call(
                "POST",
                "/api/account/sign-in",
                json!({ "email": "dev@example.com", "password": SECRET }),
            )
            .await;
        assert_eq!(status, 200);
    }

    async fn confirm(&self, action: &str, device: Option<&str>) -> (u16, Value) {
        self.call(
            "POST",
            "/api/account/confirm",
            json!({ "password": SECRET, "action": action, "device_id": device }),
        )
        .await
    }
}

/// A body every account request type accepts, so a route is refused for its gate or its
/// guard and never for a missing field.
fn any_body() -> Value {
    json!({
        "invite": "K7QM-2XWH-9QXA-3C2D",
        "email": "dev@example.com",
        "code": "ABCD-EFGH",
        "password": "x",
        "action": "sign_out_everywhere",
        "policy": "combine",
        "discard_unsynced": false,
    })
}

fn concrete(path: &str) -> String {
    path.replace("{id}", "dev-mini")
}

fn key(method: &str, path: &str) -> String {
    format!("{method} {path}")
}

#[tokio::test]
async fn the_off_gate_answers_account_disabled_on_every_account_route() {
    let served = serve("off", AccountGate::Off, false).await;
    let routes = served.account_routes();
    assert!(routes.len() >= 14);
    for (index, (method, path)) in routes.iter().enumerate() {
        let (status, body) = served.call(method, &concrete(path), any_body()).await;
        if key(method, path) == "GET /api/account" {
            assert_eq!(status, 200);
            assert_eq!(body["available"], false, "nothing of the account shows");
            continue;
        }
        assert_eq!(
            (status, body["code"].as_str()),
            (403, Some("account_disabled")),
            "account route {index}"
        );
    }
}

#[tokio::test]
async fn a_remote_viewer_cannot_reach_any_account_route() {
    let served = serve("remote", AccountGate::LocalOnly, true).await;
    for (index, (method, path)) in served.account_routes().iter().enumerate() {
        let (status, body) = served.call(method, &concrete(path), any_body()).await;
        assert_eq!(
            (status, body["code"].as_str()),
            (403, Some("local_only")),
            "account route {index}"
        );
    }
}

#[tokio::test]
async fn no_route_outside_the_account_prefix_names_an_account_sign_in_sign_out_or_device() {
    let served = serve("prefix", AccountGate::Off, false).await;
    let strays = served
        .routes
        .iter()
        .filter(|(_, path)| !path.starts_with("/api/account"))
        .filter(|(_, path)| {
            [
                "account", "sign-in", "sign-out", "signin", "signout", "device", "password",
                "confirm",
            ]
            .iter()
            .any(|word| path.contains(word))
        })
        .count();
    assert_eq!(
        strays, 0,
        "a route that changes account state must live under /api/account, where the guard test reaches it"
    );
}

#[tokio::test]
async fn every_account_route_is_a_read_open_for_a_reason_or_needs_a_confirmation() {
    let served = serve("reauth", AccountGate::LocalOnly, false).await;
    served.sign_in().await;
    assert!(OPEN.iter().chain(READS).all(|(_, why)| !why.is_empty()));
    let (mut reads, mut open, mut guarded) = (Vec::new(), Vec::new(), 0);
    for (index, (method, path)) in served.account_routes().iter().enumerate() {
        let route = key(method, path);
        if READS.iter().any(|(listed, _)| *listed == route) {
            reads.push(route);
            continue;
        }
        assert_ne!(
            method, "GET",
            "account route {index} is a GET that is not listed in READS: a GET must change nothing, and the list says which are reads"
        );
        if OPEN.iter().any(|(listed, _)| *listed == route) {
            open.push(route);
            continue;
        }
        // Signed in, with no confirmation: anything else must be refused.
        let (status, body) = served.call(method, &concrete(path), any_body()).await;
        assert_eq!(
            (status, body["code"].as_str()),
            (403, Some("reauth_required")),
            "account route {index} changes the account and must need a confirmation, or be listed in OPEN with a reason"
        );
        guarded += 1;
    }
    assert_eq!(
        guarded, 3,
        "removing a device, signing out everywhere and letting a device in"
    );
    // The lists hold no route that no longer exists.
    for (listed, _) in OPEN {
        assert!(
            open.iter().any(|seen| seen == listed),
            "an OPEN entry is not a route"
        );
    }
    for (listed, _) in READS {
        assert!(
            reads.iter().any(|seen| seen == listed),
            "a READS entry is not a route"
        );
    }
}

#[tokio::test]
async fn a_confirmation_is_for_one_action_one_device_and_one_use() {
    let served = serve("one-use", AccountGate::LocalOnly, false).await;
    served.sign_in().await;
    let delete = |id: &'static str| format!("/api/account/devices/{id}");
    // Signing in did not confirm anything.
    let (status, body) = served.call("DELETE", &delete("dev-mini"), json!({})).await;
    assert_eq!(
        (status, body["code"].as_str()),
        (403, Some("reauth_required"))
    );
    // A wrong password confirms nothing and is not echoed.
    let (status, body) = served
        .call(
            "POST",
            "/api/account/confirm",
            json!({ "password": "wrong-one", "action": "remove_device", "device_id": "dev-mini" }),
        )
        .await;
    assert_eq!(
        (status, body["code"].as_str()),
        (403, Some("wrong_password"))
    );
    assert!(!body.to_string().contains("wrong-one"));
    // A device needs its id.
    let (status, _) = served.confirm("remove_device", None).await;
    assert_eq!(status, 400);
    assert_eq!(
        served.confirm("remove_device", Some("dev-mini")).await.0,
        204
    );
    // Not for another device, and not for the other action.
    let (status, body) = served.call("DELETE", &delete("dev-phone"), json!({})).await;
    assert_eq!(
        (status, body["code"].as_str()),
        (403, Some("reauth_required"))
    );
    let (status, _) = served
        .call("POST", "/api/account/sign-out-everywhere", json!({}))
        .await;
    assert_eq!(status, 403);
    // The device it was for goes, and the confirmation goes with it.
    assert_eq!(
        served
            .call("DELETE", &delete("dev-mini"), json!({}))
            .await
            .0,
        204
    );
    let (status, body) = served.call("DELETE", &delete("dev-mini"), json!({})).await;
    assert_eq!(
        (status, body["code"].as_str()),
        (403, Some("reauth_required"))
    );
    // Signing out everywhere has its own, and ends the session and any other confirmation.
    assert_eq!(
        served.confirm("remove_device", Some("dev-phone")).await.0,
        204
    );
    assert_eq!(served.confirm("sign_out_everywhere", None).await.0, 204);
    assert_eq!(
        served
            .call("POST", "/api/account/sign-out-everywhere", json!({}))
            .await
            .0,
        204
    );
    let (_, view) = served.call("GET", "/api/account", json!({})).await;
    assert!(view["session"].is_null());
    assert!(
        view.get("confirmed").is_none(),
        "the view carries no confirmation state"
    );
    served.sign_in().await;
    let (status, _) = served.call("DELETE", &delete("dev-phone"), json!({})).await;
    assert_eq!(
        status, 403,
        "a confirmation does not outlive the session it was for"
    );
}

#[tokio::test]
async fn the_view_names_the_platform_and_never_a_secret() {
    let served = serve("view", AccountGate::LocalOnly, false).await;
    let (_, view) = served.call("GET", "/api/account", json!({})).await;
    assert_eq!(view["available"], true);
    assert_eq!(view["invite_required"], true);
    assert_eq!(view["platform"], std::env::consts::OS);
    let (status, body) = served
        .call(
            "POST",
            "/api/account/sign-in",
            json!({ "email": "dev@example.com", "password": "wrong-one" }),
        )
        .await;
    assert_eq!(
        (status, body["code"].as_str()),
        (403, Some("wrong_password"))
    );
    assert!(!body.to_string().contains("wrong-one"));
}

/// The development switch is compiled only into debug builds: its one use in `main.rs` sits
/// under `cfg(debug_assertions)`, and the release arm of that function does nothing.
#[test]
fn the_development_account_switch_is_debug_only() {
    // A Windows checkout may use CRLF line endings.
    let source = include_str!("../main.rs").replace("\r\n", "\n");
    let uses: Vec<usize> = source
        .match_indices("ORBYN_ACCOUNT_DEV")
        .map(|(at, _)| at)
        .collect();
    assert_eq!(uses.len(), 1, "one place reads the switch");
    let before = &source[..uses[0]];
    let function = before
        .rfind("fn development_account")
        .expect("the function");
    assert!(
        before[..function]
            .trim_end()
            .ends_with("#[cfg(debug_assertions)]"),
        "the function that reads the switch must be debug-only"
    );
    assert!(source.contains(
        "#[cfg(not(debug_assertions))]\nfn development_account(_config: &mut CoreConfig) {}"
    ));
    assert!(
        !source.contains("AccountGate::LocalOnly") || before.contains("#[cfg(debug_assertions)]")
    );
}

#[tokio::test]
async fn letting_a_new_device_in_needs_the_password_for_that_request_and_is_used_up() {
    let served = serve("approve", AccountGate::LocalOnly, false).await;
    served.sign_in().await;
    let (status, list) = served
        .call("GET", "/api/account/approvals", json!({}))
        .await;
    assert_eq!(status, 200);
    assert_eq!(list[0]["digits"], "418 207");
    assert_eq!(list[0]["device_name"], "iPad");
    let approve = "/api/account/approvals/req-ipad/approve";
    // Signing in did not confirm anything.
    let (status, body) = served.call("POST", approve, json!({})).await;
    assert_eq!(
        (status, body["code"].as_str()),
        (403, Some("reauth_required"))
    );
    // A confirmation for another action or another request does not count.
    assert_eq!(served.confirm("sign_out_everywhere", None).await.0, 204);
    assert_eq!(served.call("POST", approve, json!({})).await.0, 403);
    assert_eq!(
        served.confirm("approve_device", Some("req-other")).await.0,
        204
    );
    assert_eq!(served.call("POST", approve, json!({})).await.0, 403);
    let (status, _) = served.confirm("approve_device", None).await;
    assert_eq!(status, 400);
    assert_eq!(
        served.confirm("approve_device", Some("req-ipad")).await.0,
        204
    );
    assert_eq!(served.call("POST", approve, json!({})).await.0, 204);
    let (_, list) = served
        .call("GET", "/api/account/approvals", json!({}))
        .await;
    assert_eq!(list.as_array().map(Vec::len), Some(0));
    // Used up: the same confirmation does not let a second request in.
    let (status, body) = served.call("POST", approve, json!({})).await;
    assert_eq!(
        (status, body["code"].as_str()),
        (403, Some("reauth_required"))
    );
}

#[tokio::test]
async fn refusing_a_new_device_needs_no_password_and_lets_nobody_in() {
    let served = serve("deny", AccountGate::LocalOnly, false).await;
    served.sign_in().await;
    let deny = "/api/account/approvals/req-ipad/deny";
    assert_eq!(
        served
            .call("POST", deny, json!({ "codes_differ": true }))
            .await
            .0,
        204
    );
    let (_, list) = served
        .call("GET", "/api/account/approvals", json!({}))
        .await;
    assert_eq!(list.as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn a_new_device_waits_for_a_trusted_one_and_can_give_up() {
    let served = serve("waiting", AccountGate::LocalOnly, false).await;
    let (status, body) = served
        .call(
            "POST",
            "/api/account/sign-in",
            json!({ "email": "late@example.com", "password": SECRET }),
        )
        .await;
    assert_eq!(
        (status, body["outcome"].as_str()),
        (200, Some("awaiting_approval"))
    );
    let (_, view) = served.call("GET", "/api/account", json!({})).await;
    assert_eq!(view["approval"]["state"], "waiting");
    assert_eq!(view["approval"]["digits"], "418 207");
    assert!(view["session"].is_null());
    let (status, polled) = served
        .call("GET", "/api/account/sign-in/approval", json!({}))
        .await;
    assert_eq!((status, polled["state"].as_str()), (200, Some("waiting")));
    assert_eq!(
        served
            .call("POST", "/api/account/sign-in/cancel", json!({}))
            .await
            .0,
        204
    );
    let (_, view) = served.call("GET", "/api/account", json!({})).await;
    assert!(view.get("approval").is_none());
}

#[tokio::test]
async fn the_first_sync_asks_how_to_join_the_settings_once() {
    let served = serve("connect", AccountGate::LocalOnly, false).await;
    served.sign_in().await;
    let (_, view) = served.call("GET", "/api/account", json!({})).await;
    assert_eq!(
        view["connect"], "connected",
        "a computer with no settings of its own has nothing to join"
    );
    assert_eq!(
        served
            .call(
                "POST",
                "/api/account/connect",
                json!({ "policy": "combine" })
            )
            .await
            .0,
        204,
        "connecting when already connected changes nothing"
    );
}
