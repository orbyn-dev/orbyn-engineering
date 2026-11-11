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
        self.call_with(method, path, body, None).await
    }

    /// As [`Self::call`], sending the nonce a confirmation answered with, as the window that
    /// typed the password does.
    async fn call_with(
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

    async fn nonce_for_policy(&self, policy: &str) -> String {
        let (status, body) = self
            .call(
                "POST",
                "/api/account/confirm",
                json!({ "password": SECRET, "action": "connect_settings", "policy": policy }),
            )
            .await;
        assert_eq!(status, 200);
        body["nonce"].as_str().unwrap().to_owned()
    }

    /// The password typed again for one action: the status and the nonce it answers with.
    async fn nonce_for(&self, action: &str, device: Option<&str>) -> String {
        let (status, body) = self.confirm(action, device).await;
        assert_eq!(status, 200);
        body["nonce"].as_str().unwrap().to_owned()
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
        "nonce": "0",
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
        guarded, 5,
        "removing a device, signing out everywhere, letting a device in and the two choices that overwrite"
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
    let refused = |(status, body): (u16, Value)| {
        assert_eq!(
            (status, body["code"].as_str()),
            (403, Some("reauth_required"))
        );
    };
    // Signing in did not confirm anything.
    refused(served.call("DELETE", &delete("dev-mini"), json!({})).await);
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
    assert_eq!(served.confirm("remove_device", None).await.0, 400);
    let nonce = served.nonce_for("remove_device", Some("dev-mini")).await;
    // Not for another device, not for the other action, and not without the nonce it answered with.
    refused(
        served
            .call_with("DELETE", &delete("dev-phone"), json!({}), Some(&nonce))
            .await,
    );
    refused(
        served
            .call_with(
                "POST",
                "/api/account/sign-out-everywhere",
                json!({}),
                Some(&nonce),
            )
            .await,
    );
    refused(served.call("DELETE", &delete("dev-mini"), json!({})).await);
    refused(
        served
            .call_with(
                "DELETE",
                &delete("dev-mini"),
                json!({}),
                Some("0".repeat(32).as_str()),
            )
            .await,
    );
    // The device it was for goes, and the confirmation goes with it.
    assert_eq!(
        served
            .call_with("DELETE", &delete("dev-mini"), json!({}), Some(&nonce))
            .await
            .0,
        204
    );
    refused(
        served
            .call_with("DELETE", &delete("dev-mini"), json!({}), Some(&nonce))
            .await,
    );
    // Signing out everywhere has its own, and ends the session and any other confirmation.
    let stale = served.nonce_for("remove_device", Some("dev-phone")).await;
    let nonce = served.nonce_for("sign_out_everywhere", None).await;
    assert_ne!(stale, nonce);
    assert_eq!(
        served
            .call_with(
                "POST",
                "/api/account/sign-out-everywhere",
                json!({}),
                Some(&nonce)
            )
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
    assert_eq!(
        served
            .call_with("DELETE", &delete("dev-phone"), json!({}), Some(&stale))
            .await
            .0,
        403,
        "a confirmation does not outlive the session it was for"
    );
}

#[tokio::test]
async fn a_caller_holding_only_the_service_token_cannot_use_a_confirmation_it_did_not_obtain() {
    let served = serve("engine", AccountGate::LocalOnly, false).await;
    served.sign_in().await;
    // The window types the password and keeps the nonce; an engine with the same token never sees it.
    let nonce = served.nonce_for("sign_out_everywhere", None).await;
    let path = "/api/account/sign-out-everywhere";
    for guess in [
        None,
        Some(""),
        Some("00000000000000000000000000000000"),
        Some("not a nonce"),
    ] {
        let (status, body) = served.call_with("POST", path, json!({}), guess).await;
        assert_eq!(
            (status, body["code"].as_str()),
            (403, Some("reauth_required")),
            "{guess:?}"
        );
    }
    // The failed guesses did not use the confirmation up.
    let (_, view) = served.call("GET", "/api/account", json!({})).await;
    assert!(!view["session"].is_null());
    assert_eq!(
        served
            .call_with("POST", path, json!({}), Some(&nonce))
            .await
            .0,
        204
    );
}

#[tokio::test]
async fn cancelling_drops_the_confirmation_and_only_its_own_nonce_can() {
    let served = serve("cancel", AccountGate::LocalOnly, false).await;
    served.sign_in().await;
    let path = "/api/account/sign-out-everywhere";
    let nonce = served.nonce_for("sign_out_everywhere", None).await;
    // A wrong nonce cancels nothing.
    let cancel = |nonce: String| {
        served.call(
            "POST",
            "/api/account/confirm/cancel",
            json!({ "nonce": nonce }),
        )
    };
    assert_eq!(cancel("0".repeat(32)).await.0, 204);
    let again = served.nonce_for("sign_out_everywhere", None).await;
    assert_eq!(cancel(again.clone()).await.0, 204);
    // The abandoned prompt's confirmation is gone, and so is the one it replaced.
    assert_eq!(
        served
            .call_with("POST", path, json!({}), Some(&again))
            .await
            .0,
        403
    );
    assert_eq!(
        served
            .call_with("POST", path, json!({}), Some(&nonce))
            .await
            .0,
        403
    );
    // The wrong cancel left a live one alone.
    let live = served.nonce_for("sign_out_everywhere", None).await;
    assert_eq!(cancel("1".repeat(32)).await.0, 204);
    assert_eq!(
        served
            .call_with("POST", path, json!({}), Some(&live))
            .await
            .0,
        204
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
    let refused = |(status, body): (u16, Value)| {
        assert_eq!(
            (status, body["code"].as_str()),
            (403, Some("reauth_required"))
        );
    };
    // Signing in did not confirm anything.
    refused(served.call("POST", approve, json!({})).await);
    // A confirmation for another action or another request does not count, with its own nonce.
    let other_action = served.nonce_for("sign_out_everywhere", None).await;
    refused(
        served
            .call_with("POST", approve, json!({}), Some(&other_action))
            .await,
    );
    let other_request = served.nonce_for("approve_device", Some("req-other")).await;
    refused(
        served
            .call_with("POST", approve, json!({}), Some(&other_request))
            .await,
    );
    assert_eq!(served.confirm("approve_device", None).await.0, 400);
    let nonce = served.nonce_for("approve_device", Some("req-ipad")).await;
    refused(served.call("POST", approve, json!({})).await);
    assert_eq!(
        served
            .call_with("POST", approve, json!({}), Some(&nonce))
            .await
            .0,
        204
    );
    let (_, list) = served
        .call("GET", "/api/account/approvals", json!({}))
        .await;
    assert_eq!(list.as_array().map(Vec::len), Some(0));
    // Used up: the same confirmation does not let a second request in.
    refused(
        served
            .call_with("POST", approve, json!({}), Some(&nonce))
            .await,
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
        .call("POST", "/api/account/sign-in/approval", json!({}))
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
            .call("POST", "/api/account/connect", json!({}))
            .await
            .0,
        204,
        "connecting when already connected changes nothing"
    );
}

#[tokio::test]
async fn the_choices_that_overwrite_need_the_password_for_that_choice() {
    let served = serve("overwrite", AccountGate::LocalOnly, false).await;
    served.sign_in().await;
    for path in [
        "/api/account/connect/use-device",
        "/api/account/connect/use-account",
    ] {
        let (status, body) = served.call("POST", path, json!({})).await;
        assert_eq!(
            (status, body["code"].as_str()),
            (403, Some("reauth_required")),
            "{path}"
        );
    }
    // Combine needs no password, so a confirmation for it, or for nothing, is refused.
    for body in [
        json!({ "password": SECRET, "action": "connect_settings", "policy": "combine" }),
        json!({ "password": SECRET, "action": "connect_settings" }),
    ] {
        assert_eq!(
            served.call("POST", "/api/account/confirm", body).await.0,
            400
        );
    }
    let nonce = served.nonce_for_policy("use_account").await;
    // Not for the other choice, and not without the nonce.
    assert_eq!(
        served
            .call_with(
                "POST",
                "/api/account/connect/use-device",
                json!({}),
                Some(&nonce)
            )
            .await
            .0,
        403
    );
    assert_eq!(
        served
            .call("POST", "/api/account/connect/use-account", json!({}))
            .await
            .0,
        403
    );
    // For its own, and used up by it.
    assert_eq!(
        served
            .call_with(
                "POST",
                "/api/account/connect/use-account",
                json!({}),
                Some(&nonce)
            )
            .await
            .0,
        204
    );
    assert_eq!(
        served
            .call_with(
                "POST",
                "/api/account/connect/use-account",
                json!({}),
                Some(&nonce)
            )
            .await
            .0,
        403
    );
}
