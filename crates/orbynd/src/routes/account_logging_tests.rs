//! No account request leaves a password, a code, the digits or a confirmation nonce in a log
//! line, at the most verbose level the service can run at. A subscriber at TRACE with no filter
//! catches every event of every thread of this test process, in the layout the service writes
//! (and the service's own warning keeper, `RecentEvents`, is attached too). The values are made
//! at run time and named by their label, so no secret is written in this file.

use std::sync::{Arc, Mutex, OnceLock};

use orbyn_core::account::AccountGate;
use serde_json::{Value, json};
use tracing::Level;
use tracing_subscriber::Layer as _;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;

use super::account_tests::{SECRET, serve};

/// The most the capture keeps. Older lines go first, so a runaway logger cannot fill memory and
/// the lines of the request under test, which are the newest, are the ones that stay.
const CAPTURE_CAP: usize = 8 * 1024 * 1024;

#[derive(Clone, Default)]
struct Lines(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Lines {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let mut kept = self.0.lock().unwrap();
        kept.extend_from_slice(bytes);
        if kept.len() > CAPTURE_CAP {
            let excess = kept.len() - CAPTURE_CAP;
            kept.drain(..excess);
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Lines {
    type Writer = Lines;

    fn make_writer(&'a self) -> Lines {
        self.clone()
    }
}

/// One capture for the whole test process: a global subscriber at TRACE, nothing filtered.
///
/// `set_global_default` panics if any other test in this binary sets a global subscriber, so none
/// may. CI runs these with nextest, one process per test, where each test owns its subscriber;
/// under plain `cargo test` the tests of this file share the one capture, which is harmless
/// because every value is unique to its test.
fn captured() -> &'static Lines {
    static LINES: OnceLock<Lines> = OnceLock::new();
    LINES.get_or_init(|| {
        let lines = Lines::default();
        let subscriber = tracing_subscriber::registry()
            .with(
                tracing_subscriber::fmt::layer()
                    .with_writer(lines.clone())
                    .with_ansi(false)
                    .with_target(true)
                    .with_thread_ids(true)
                    .with_filter(tracing_subscriber::filter::LevelFilter::from_level(
                        Level::TRACE,
                    )),
            )
            .with(
                crate::recent_events::RecentEvents
                    .with_filter(tracing_subscriber::filter::LevelFilter::WARN),
            );
        tracing::subscriber::set_global_default(subscriber)
            .expect("no other test sets a global subscriber");
        lines
    })
}

fn text() -> String {
    String::from_utf8_lossy(&captured().0.lock().unwrap()).into_owned()
}

/// A value nobody could guess and no source file holds: `label` and a random tail.
fn canary(label: &str) -> String {
    let mut bytes = [0u8; 12];
    getrandom::fill(&mut bytes).unwrap();
    let tail: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{label}-{tail}")
}

#[tokio::test]
async fn no_account_request_leaves_a_password_a_code_the_digits_or_a_nonce_in_a_log() {
    // The subscriber must be in place before anything runs.
    let _ = captured();
    let served = serve("logging", AccountGate::LocalOnly, false).await;

    let password = canary("password");
    let again = canary("password-again");
    let sign_up_code = canary("sign-up-code");
    let second_code = canary("second-factor-code");
    let digits = canary("digits");
    let nonce = canary("nonce");
    let invite = canary("invite");
    let wrong_type = 4_242_424_242_u64;

    // Each route is sent with its secret, and once more with the wrong shape, so a failure that
    // echoes its input is caught as well as a success. Answers are not the point; the logs are.
    let calls: Vec<(&str, &str, Value, Option<&str>)> = vec![
        (
            "POST",
            "/api/account/sign-up/invite",
            json!({ "invite": invite }),
            None,
        ),
        (
            "POST",
            "/api/account/sign-up/start",
            json!({ "invite": invite, "email": "new@example.com", "password": password }),
            None,
        ),
        (
            "POST",
            "/api/account/sign-up/code",
            json!({ "code": sign_up_code }),
            None,
        ),
        (
            "POST",
            "/api/account/sign-in",
            json!({ "email": "dev@example.com", "password": password }),
            None,
        ),
        (
            "POST",
            "/api/account/sign-in/second-factor",
            json!({ "code": second_code }),
            None,
        ),
        (
            "POST",
            "/api/account/sign-in/digits",
            json!({ "digits": digits }),
            None,
        ),
        (
            "POST",
            "/api/account/confirm",
            json!({ "password": again, "action": "sign_out_everywhere" }),
            None,
        ),
        (
            "POST",
            "/api/account/confirm/cancel",
            json!({ "nonce": nonce }),
            None,
        ),
        (
            "POST",
            "/api/account/approvals/request-1/approve",
            json!({}),
            Some(nonce.as_str()),
        ),
        // The wrong shape: a number where the text should be, and an unknown field.
        (
            "POST",
            "/api/account/sign-in",
            json!({ "email": "dev@example.com", "password": wrong_type, "extra": password }),
            None,
        ),
    ];
    for (method, path, body, header) in &calls {
        served.call_with(method, path, body.clone(), *header).await;
    }

    // And the real thing: a sign-in and a confirmation that succeed with the stand-in's password.
    served.sign_in().await;
    let confirmed = served.nonce_for("sign_out_everywhere", None).await;

    // Let any task or blocking thread finish its writes.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let logged = text();
    assert!(
        !logged.is_empty(),
        "the capture saw nothing: it is not attached"
    );
    let events: String = orbyn_core::operations::diagnostics::recent_events()
        .iter()
        .map(|e| format!("{} {} {}\n", e.level, e.target, e.message))
        .collect();
    for (label, value) in [
        ("password", password.as_str()),
        ("password again", again.as_str()),
        ("sign-up code", sign_up_code.as_str()),
        ("second-factor code", second_code.as_str()),
        ("digits", digits.as_str()),
        ("nonce", nonce.as_str()),
        ("invite", invite.as_str()),
        ("confirmation nonce", confirmed.as_str()),
        ("stand-in password", SECRET),
        ("wrong-typed value", wrong_type.to_string().as_str()),
    ] {
        assert!(
            !logged.contains(value),
            "the {label} is in a log line: {}",
            logged
                .lines()
                .find(|line| line.contains(value))
                .unwrap_or_default()
        );
        assert!(
            !events.contains(value),
            "the {label} is in the kept warnings"
        );
    }
}

#[test]
fn the_secret_text_types_print_as_redacted() {
    let value = canary("secret-text");
    let body: orbyn_contracts::AccountSignIn =
        serde_json::from_value(json!({ "email": "a@example.com", "password": value })).unwrap();
    for printed in [format!("{body:?}"), format!("{body:#?}")] {
        assert!(!printed.contains(&value), "{printed}");
        assert!(printed.contains("[redacted]"), "{printed}");
    }
}

/// The line of the capture that carries `needle`, for a failure message.
fn line_with(logged: &str, needle: &str) -> String {
    logged
        .lines()
        .find(|line| line.contains(needle))
        .unwrap_or_default()
        .to_owned()
}

#[tokio::test]
async fn a_server_error_is_logged_at_error_level_and_carries_no_request_value() {
    let _ = captured();
    let served = serve("logging-5xx", AccountGate::LocalOnly, false).await;
    let password = canary("password-5xx");
    let email_code = canary("code-5xx");

    // The stand-in's `down@example.com` is an Engine that cannot be reached: a 503, which is a
    // server error, so the service logs it with `tracing::error!`.
    let (status, _) = served
        .call(
            "POST",
            "/api/account/sign-in",
            json!({ "email": "down@example.com", "password": password, "code": email_code }),
        )
        .await;
    assert_eq!(status, 503, "the request must fail as a server error");
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let logged = text();
    assert!(
        logged
            .lines()
            .any(|line| line.contains("ERROR") && line.contains("request failed")),
        "no ERROR line for the failed request was captured"
    );
    for value in [&password, &email_code] {
        assert!(
            !logged.contains(value.as_str()),
            "a request value is in a log line: {}",
            line_with(&logged, value)
        );
    }
}

/// A canary string where the body wants another type, and one in the field the service reads as
/// an enum. `ApiJson` answers with serde's rejection text, which can name the value it refused.
async fn wrong_shape(label: &str) -> (String, String, String, String) {
    let _ = captured();
    let served = serve(label, AccountGate::LocalOnly, false).await;
    let password = canary("password-shape");
    let action = canary("action-shape");
    let (status, body) = served
        .call(
            "POST",
            "/api/account/confirm",
            json!({ "password": password, "action": action }),
        )
        .await;
    assert_eq!(status, 400, "{body}");
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    (password, action, body.to_string(), text())
}

#[tokio::test]
async fn a_request_of_the_wrong_shape_leaves_no_canary_in_a_log() {
    let (password, action, _body, logged) = wrong_shape("logging-shape").await;
    for value in [&password, &action] {
        assert!(
            !logged.contains(value.as_str()),
            "a value of the refused request is in a log line: {}",
            line_with(&logged, value)
        );
    }
}

/// KNOWN ISSUE (Engineering 2 tracks it): `ApiJson` puts serde's rejection text in the 400 body,
/// and for an unknown enum value that text names the value it was sent. It goes back to the
/// caller that sent it, never to a log, so it is not a leak to another party, but a value typed
/// into the wrong field is echoed. Not fixed here (tests only); this test states the property
/// that should hold and is ignored until the issue is closed.
#[tokio::test]
#[ignore = "known issue: the 400 body for a refused value echoes it (Engineering 2 tracks it)"]
async fn the_body_of_a_request_of_the_wrong_shape_carries_no_canary() {
    let (password, action, body, _logged) = wrong_shape("logging-shape-body").await;
    for value in [&password, &action] {
        assert!(
            !body.contains(value.as_str()),
            "the answer echoes a value: {body}"
        );
    }
}
