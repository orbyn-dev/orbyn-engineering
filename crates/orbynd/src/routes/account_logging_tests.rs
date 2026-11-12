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

#[derive(Clone, Default)]
struct Lines(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Lines {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
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
