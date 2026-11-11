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

pub(super) const SECRET: &str = "correct horse battery";

pub(super) struct Served {
    pub(super) port: u16,
    routes: BTreeSet<(String, String)>,
    _dir: std::path::PathBuf,
}

pub(super) async fn serve(name: &str, gate: AccountGate, remote: bool) -> Served {
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
    pub(super) async fn call(&self, method: &str, path: &str, body: Value) -> (u16, Value) {
        self.call_with(method, path, body, None).await
    }

    /// As [`Self::call`], sending the nonce a confirmation answered with, as the window that
    /// typed the password does.
    pub(super) async fn call_with(
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

    pub(super) async fn sign_in(&self) {
        let (status, _) = self
            .call(
                "POST",
                "/api/account/sign-in",
                json!({ "email": "dev@example.com", "password": SECRET }),
            )
            .await;
        assert_eq!(status, 200);
    }

    /// The nonce for one guarded route's confirmation.
    pub(super) async fn nonce_for_route(
        &self,
        action: &str,
        device: Option<&str>,
        policy: Option<&str>,
    ) -> String {
        let (status, body) = self
            .call(
                "POST",
                "/api/account/confirm",
                json!({ "password": SECRET, "action": action, "device_id": device, "policy": policy }),
            )
            .await;
        assert_eq!(status, 200);
        body["nonce"].as_str().unwrap().to_owned()
    }

    pub(super) async fn nonce_for_policy(&self, policy: &str) -> String {
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
    pub(super) async fn nonce_for(&self, action: &str, device: Option<&str>) -> String {
        let (status, body) = self.confirm(action, device).await;
        assert_eq!(status, 200);
        body["nonce"].as_str().unwrap().to_owned()
    }

    pub(super) async fn confirm(&self, action: &str, device: Option<&str>) -> (u16, Value) {
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

/// Values that are not the nonce, made from it and never written out as literals: one character
/// flipped, shorter, longer and empty.
pub(super) fn not_the_nonce(nonce: &str) -> Vec<String> {
    let mut flipped = nonce.to_owned();
    let first = flipped.remove(0);
    flipped.insert(0, if first == 'a' { 'b' } else { 'a' });
    vec![
        flipped,
        nonce[..nonce.len() - 1].to_owned(),
        format!("{nonce}{}", &nonce[..1]),
        nonce[..0].to_owned(),
    ]
}

/// What a guarded route's confirmation is for: the action, the device or request id, and the choice.
fn confirmation_for(path: &str) -> (&'static str, Option<&'static str>, Option<&'static str>) {
    match path {
        p if p.starts_with("/api/account/devices/") => ("remove_device", Some("dev-mini"), None),
        "/api/account/sign-out-everywhere" => ("sign_out_everywhere", None, None),
        p if p.contains("/approvals/") => ("approve_device", Some("dev-mini"), None),
        "/api/account/connect/use-device" => ("connect_settings", None, Some("use_device")),
        "/api/account/connect/use-account" => ("connect_settings", None, Some("use_account")),
        other => panic!("a guarded route with no confirmation listed in this test: {other}"),
    }
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
        // The one Crew route that shares the word: it keeps the Autopilot default and has nothing to do with the account.
        .filter(|(_, path)| path != "/api/crew/autopilot/confirm")
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
        // Confirmed with the password for exactly this route, but calling it without the nonce the
        // confirmation answered with (as a caller holding only the service token would): still refused,
        // so the nonce check cannot be removed from the route layer alone. The confirmation is then
        // dropped, so it cannot leak into the next route.
        let (action, device, policy) = confirmation_for(&concrete(path));
        let nonce = served.nonce_for_route(action, device, policy).await;
        let (status, body) = served.call(method, &concrete(path), any_body()).await;
        assert_eq!(
            (status, body["code"].as_str()),
            (403, Some("reauth_required")),
            "account route {index} accepted a confirmation without its nonce"
        );
        for wrong in not_the_nonce(&nonce) {
            let (status, body) = served
                .call_with(method, &concrete(path), any_body(), Some(&wrong))
                .await;
            assert_eq!(
                (status, body["code"].as_str()),
                (403, Some("reauth_required")),
                "account route {index} accepted a wrong nonce"
            );
        }
        served
            .call(
                "POST",
                "/api/account/confirm/cancel",
                json!({ "nonce": nonce }),
            )
            .await;
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

/// Autopilot's always-denied list (N1) refuses by this module's list, so a route the list does not
/// know would be one an agent could call. Failures name a route by its place in the sorted list.
#[tokio::test]
async fn every_account_route_is_a_read_or_on_the_list_autopilot_refuses() {
    use orbyn_core::account::changes::{
        READS as CORE_READS, STATE_CHANGING, changes_account_state,
    };
    let served = serve("autopilot", AccountGate::Off, false).await;
    let routes = served.account_routes();
    let listed = |list: &[(&str, &str)], method: &str, path: &str| {
        list.iter().any(|(m, p)| *m == method && *p == path)
    };
    for (index, (method, path)) in routes.iter().enumerate() {
        let change = listed(STATE_CHANGING, method, path);
        let read = listed(CORE_READS, method, path);
        assert!(
            change != read,
            "account route {index} must be in exactly one of the core's lists, STATE_CHANGING or READS"
        );
        assert_eq!(
            changes_account_state(&concrete(path)),
            change,
            "account route {index} is not refused the way its list says"
        );
        assert_eq!(
            read,
            method == "GET",
            "account route {index}: only a GET is a read"
        );
    }
    for (method, path) in STATE_CHANGING.iter().chain(CORE_READS) {
        assert!(
            routes.contains(&((*method).to_owned(), (*path).to_owned())),
            "a core list names a route that does not exist"
        );
    }
    // The same reads as this service's own list.
    let ours: Vec<String> = READS.iter().map(|(route, _)| (*route).to_owned()).collect();
    let theirs: Vec<String> = CORE_READS.iter().map(|(m, p)| format!("{m} {p}")).collect();
    assert_eq!(ours, theirs);
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
    for wrong in not_the_nonce(&nonce) {
        refused(
            served
                .call_with("DELETE", &delete("dev-mini"), json!({}), Some(&wrong))
                .await,
        );
    }
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
