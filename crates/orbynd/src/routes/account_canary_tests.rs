//! Canaries for the account routes of the service. The secrets a caller of these routes can hold
//! (the service token, the passwords, the codes, the digits, the invite and the confirmation
//! nonce) are made at run time, planted in requests, and must be absent from every response and
//! its headers, every log line, the kept warnings and the telemetry export built from them, the
//! `Debug` text of the request types, and every file of the data folder. The real adapter's own
//! secrets (device keys, session token, vault key, zone keys, the sealed snapshot) are planted in
//! `orbyn-core`'s `account/tests/canaries.rs`, where the adapter can run; the service build has no
//! Engine client, so these routes run on the development stand-in.

use std::sync::{Arc, Mutex};

use orbyn_contracts::{
    AccountApproved, AccountCode, AccountConfirm, AccountConfirmCancel, AccountDigits,
    AccountInvite, AccountSignIn, AccountSignUp,
};
use orbyn_core::account::AccountGate;
use orbyn_core::operations::telemetry_export::{Endpoints, Exporter};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::account_logging_tests::{canary, captured, text};
use super::account_tests::{SECRET, serve_as};

fn found(haystack: &str, needle: &str) -> bool {
    !needle.is_empty() && haystack.contains(needle)
}

/// The labels of the values that are in `haystack`.
fn leaked<'a>(values: &[(&'a str, String)], haystack: &str) -> Vec<&'a str> {
    values
        .iter()
        .filter(|(_, value)| found(haystack, value))
        .map(|(label, _)| *label)
        .collect()
}

/// A collector that keeps the raw bytes of every request it gets.
async fn collector() -> (String, Arc<Mutex<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1/signal", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let kept = seen.clone();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut raw = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let read = stream.read(&mut chunk).await.unwrap_or(0);
                raw.extend_from_slice(&chunk[..read]);
                let text = String::from_utf8_lossy(&raw).into_owned();
                if let Some((head, rest)) = text.split_once("\r\n\r\n") {
                    let length = head
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|n| n.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if rest.len() >= length {
                        break;
                    }
                }
                if read == 0 {
                    break;
                }
            }
            kept.lock().unwrap().extend_from_slice(&raw);
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                .await;
        }
    });
    (url, seen)
}

/// Every file under `dir`, as lossy text, with its path.
fn folder(dir: &std::path::Path) -> Vec<(std::path::PathBuf, String)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        for entry in std::fs::read_dir(&next).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let bytes = std::fs::read(&path).unwrap_or_default();
                out.push((path, String::from_utf8_lossy(&bytes).into_owned()));
            }
        }
    }
    out
}

/// A response as a caller sees it: the status line, every header and the body.
async fn raw(
    port: u16,
    method: &str,
    path: &str,
    body: &Value,
    bearer: &str,
    nonce: Option<&str>,
) -> String {
    let mut request = reqwest::Client::new()
        .request(
            method.parse().unwrap(),
            format!("http://127.0.0.1:{port}{path}"),
        )
        .bearer_auth(bearer)
        .json(body);
    if let Some(nonce) = nonce {
        request = request.header("x-orbyn-confirm", nonce);
    }
    let response = request.send().await.unwrap();
    let mut shown = format!("{}\n", response.status());
    for (name, value) in response.headers() {
        shown += &format!("{name}: {}\n", String::from_utf8_lossy(value.as_bytes()));
    }
    shown + &response.text().await.unwrap()
}

#[tokio::test]
async fn no_account_route_answers_logs_exports_or_writes_a_service_secret() {
    let _ = captured();
    let service_token = canary("service-token");
    let other_token = canary("other-token");
    let served = serve_as("canaries", AccountGate::LocalOnly, false, &service_token).await;

    let password = canary("password");
    let again = canary("password-again");
    let code = canary("code");
    let digits = canary("digits");
    let invite = canary("invite");
    let nonce = canary("nonce");
    let email = "canary@example.com";
    let values: Vec<(&str, String)> = vec![
        ("the service token", service_token.clone()),
        ("another bearer token", other_token.clone()),
        ("a password", password.clone()),
        ("a second password", again.clone()),
        ("a code", code.clone()),
        ("the digits", digits.clone()),
        ("an invite", invite.clone()),
        ("a nonce", nonce.clone()),
        ("the stand-in's password", SECRET.to_owned()),
    ];

    // A sign-in that works, so the routes answer for a signed-in account too, and its confirmation.
    served.sign_in().await;
    let confirmed = served.nonce_for("sign_out_everywhere", None).await;
    let mut values = values;
    values.push(("a confirmation nonce", confirmed.clone()));

    // Every account route, every method, with the service token, with a wrong one and with none,
    // each carrying the canaries in every field a body can have, and once with the confirmation.
    let body = json!({
        "invite": invite, "email": email, "code": code, "digits": digits, "password": password,
        "action": "sign_out_everywhere", "policy": "combine", "nonce": nonce,
        "discard_unsynced": false, "device_id": "dev-mini",
    });
    let shape = json!({ "password": 4_242_424_242_u64, "extra": again });
    let mut answers = String::new();
    for (method, path) in served.account_routes() {
        let path = path.replace("{id}", "dev-mini");
        for bearer in [&service_token, &other_token] {
            for sent in [&body, &shape] {
                answers += &raw(served.port, &method, &path, sent, bearer, None).await;
                answers.push('\n');
            }
        }
        answers += &raw(
            served.port,
            &method,
            &path,
            &body,
            &service_token,
            Some(&confirmed),
        )
        .await;
        answers.push('\n');
    }
    // The pages that report the service's own state.
    for path in ["/api/diagnostics", "/api/info", "/api/account"] {
        answers += &raw(served.port, "GET", path, &json!({}), &service_token, None).await;
        answers.push('\n');
    }
    assert!(answers.len() > 2_000, "the routes answered almost nothing");

    // The only secrets a caller is told are the ones it asked for: its own nonce comes back from
    // `/confirm`, and no other value does.
    let own_nonce_only: Vec<&str> = leaked(&values, &answers)
        .into_iter()
        .filter(|label| *label != "a confirmation nonce")
        .collect();
    assert_eq!(
        own_nonce_only,
        Vec::<&str>::new(),
        "a response carries a secret"
    );

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Logs, and the warnings the diagnostics page keeps.
    let logs = text();
    assert!(
        !logs.is_empty(),
        "the capture saw nothing: it is not attached"
    );
    assert_eq!(leaked(&values, &logs), Vec::<&str>::new(), "in a log line");
    let events = orbyn_core::operations::diagnostics::recent_events();
    let kept: String = events
        .iter()
        .map(|e| format!("{} {} {} {}\n", e.at, e.level, e.target, e.message))
        .collect();
    assert_eq!(
        leaked(&values, &kept),
        Vec::<&str>::new(),
        "in a kept warning"
    );

    // The telemetry export of those warnings: the same records the service queues, sent to a
    // collector that keeps the bytes.
    let (url, received) = collector().await;
    let exporter = Exporter::new(Endpoints {
        logs: Some(url),
        metrics: None,
    })
    .unwrap();
    for e in &events {
        exporter.offer(&e.at, &e.level, &e.target, &e.message);
    }
    exporter.flush().await;
    let sent = String::from_utf8_lossy(&received.lock().unwrap()).into_owned();
    assert!(
        found(&sent, "resourceLogs"),
        "the export sent no batch: it is not exercised"
    );
    assert_eq!(
        leaked(&values, &sent),
        Vec::<&str>::new(),
        "in the telemetry export"
    );

    // The data folder: nothing the routes were sent is in any file, in the clear.
    let files = folder(served.dir());
    assert!(!files.is_empty(), "the data folder is empty");
    for (path, content) in &files {
        assert_eq!(
            leaked(&values, content),
            Vec::<&str>::new(),
            "{} holds a secret",
            path.display()
        );
    }
}

/// The request types print their secrets as `[redacted]`.
#[test]
fn the_secret_request_types_print_without_their_secrets() {
    let secret = canary("secret");
    let must_not_print: Vec<String> = vec![
        format!(
            "{:?}",
            serde_json::from_value::<AccountSignIn>(
                json!({ "email": "a@example.com", "password": secret })
            )
            .unwrap()
        ),
        format!(
            "{:?}",
            serde_json::from_value::<AccountDigits>(json!({ "digits": secret })).unwrap()
        ),
        format!(
            "{:?}",
            serde_json::from_value::<AccountConfirmCancel>(json!({ "nonce": secret })).unwrap()
        ),
        format!(
            "{:?}",
            serde_json::from_value::<AccountConfirm>(
                json!({ "password": secret, "action": "sign_out_everywhere" })
            )
            .unwrap()
        ),
        format!(
            "{:#?}",
            serde_json::from_value::<AccountSignUp>(
                json!({ "invite": "x", "email": "a@example.com", "password": secret })
            )
            .unwrap()
        ),
        format!(
            "{:?}",
            serde_json::from_value::<AccountApproved>(json!({ "digits": secret })).unwrap()
        ),
    ];
    for printed in must_not_print {
        assert!(!printed.contains(&secret), "{printed}");
    }
}

/// KNOWN ISSUE, found by these canaries and reported, not fixed here (tests only): the invite
/// and the email or second-factor code are plain `String`s in their request types, which derive
/// `Debug`, so formatting one with `{:?}` prints the invite or the code. Nothing formats them
/// today (the logging tests prove no log line carries them), so it is a trap rather than a leak,
/// but the password, the digits and the nonce are `SecretText` and these are not. The test states
/// the property that should hold and is ignored until the types are changed.
#[test]
#[ignore = "known issue: the invite and the codes print in Debug (reported, not fixed in this test-only change)"]
fn the_invite_and_the_codes_print_as_redacted_too() {
    let secret = canary("secret");
    let printed = [
        format!(
            "{:?}",
            serde_json::from_value::<AccountInvite>(json!({ "invite": secret })).unwrap()
        ),
        format!(
            "{:?}",
            serde_json::from_value::<AccountCode>(json!({ "code": secret })).unwrap()
        ),
        format!(
            "{:?}",
            serde_json::from_value::<AccountSignUp>(
                json!({ "invite": secret, "email": "a@example.com", "password": "x" })
            )
            .unwrap()
        ),
    ];
    for shown in printed {
        assert!(!shown.contains(&secret), "{shown}");
    }
}
