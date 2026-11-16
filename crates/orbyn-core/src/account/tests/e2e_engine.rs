//! The cross-repository end-to-end test (docs/account.md, "Testing"): two data folders, which
//! are two devices of one throwaway account, against the Engine's own local compose stack.
//!
//! Sign up on A, join B through the approval, change a setting on A and see it on B, then change
//! it on both while neither syncs, let both reconnect and check the merge and the conflict A is
//! told about. Both devices run the real device client over HTTP with the keys in memory, never in
//! the keychain, under the `LocalOnly` gate.
//!
//! The Engine is whatever answers `ORBYN_E2E_ENGINE_URL` (default `http://127.0.0.1:8080`, what
//! the Engine repository's `scripts/local.sh up` starts). When nothing answers, the test prints
//! why and passes; with `ORBYN_E2E_REQUIRE_ENGINE=1` that is a failure instead, so a broken job
//! cannot hide behind a skip. The invite and the emailed code come from the same script
//! (`ORBYN_E2E_ENGINE_REPO` names the Engine checkout; otherwise a sibling `orbyn-engine` of this
//! checkout or of one of its parents). Passwords, invites and codes are never printed.

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use orbyn_contracts::{AccountApprovalState, AccountSignInOutcome};

use super::*;
use crate::account::{AccountBackendKind, ApprovalDigits, ConfirmFor, Password};

const DEFAULT_URL: &str = "http://127.0.0.1:8080";
const KEY: &str = "preview_appearance";
/// How long any one step waits for the stack before the test fails.
const PATIENCE: Duration = Duration::from_secs(60);

fn engine_url() -> String {
    std::env::var("ORBYN_E2E_ENGINE_URL").unwrap_or_else(|_| DEFAULT_URL.to_owned())
}

/// Whether something answers the Engine's health route with a success status, so an unrelated
/// service on the port (a 404 or a refusal) does not make a plain `cargo test` fail.
async fn answers(url: &str) -> bool {
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    else {
        return false;
    };
    client
        .get(format!("{}/health", url.trim_end_matches('/')))
        .send()
        .await
        .is_ok_and(|answer| answer.status().is_success())
}

/// The Engine checkout with `scripts/local.sh`.
fn engine_repo() -> PathBuf {
    if let Ok(path) = std::env::var("ORBYN_E2E_ENGINE_REPO") {
        return PathBuf::from(path);
    }
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.ancestors()
        .map(|dir| dir.join("orbyn-engine"))
        .find(|dir| dir.join("scripts/local.sh").is_file())
        .expect(
            "the Engine checkout: set ORBYN_E2E_ENGINE_REPO to the folder holding scripts/local.sh",
        )
}

/// Runs `scripts/local.sh` and returns what it printed. The output holds invites and codes, so a
/// failure reports the exit status only.
fn local_sh(args: &[&str]) -> String {
    let output = Command::new("bash")
        .arg("scripts/local.sh")
        .args(args)
        .current_dir(engine_repo())
        .output()
        .expect("scripts/local.sh could not start");
    assert!(
        output.status.success(),
        "scripts/local.sh {} failed with {}",
        args.first().copied().unwrap_or_default(),
        output.status
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The first `XXXX-XXXX-XXXX-XXXX` in `text`.
fn invite_in(text: &str) -> Option<String> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .find(|word| {
            let groups: Vec<_> = word.split('-').collect();
            groups.len() == 4
                && groups.iter().all(|g| {
                    g.len() == 4
                        && g.chars()
                            .all(|c| c.is_ascii_digit() || c.is_ascii_uppercase())
                })
        })
        .map(str::to_owned)
}

/// The newest `code XXXX-XXXX` the worker logged for `email`.
fn email_code_in(log: &str, email: &str) -> Option<String> {
    let email = email.to_ascii_lowercase();
    log.lines()
        .filter(|line| line.to_ascii_lowercase().contains(&email))
        .filter_map(|line| {
            let after = &line[line.find("code ")? + "code ".len()..];
            let word = after.get(..9)?;
            let (left, right) = word.split_once('-')?;
            let fine = |part: &str| {
                part.len() == 4
                    && part
                        .chars()
                        .all(|c| c.is_ascii_digit() || c.is_ascii_uppercase())
            };
            (fine(left) && fine(right)).then(|| word.to_owned())
        })
        .next_back()
}

/// A random lowercase hex word, for throwaway names and passwords.
fn hex() -> String {
    crate::util::random_bytes::<6>()
        .expect("randomness")
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn device(dir: &TempDir, name: &str, url: &str) -> Core {
    let mut config = CoreConfig::new(dir.path().join(name), "http://127.0.0.1:1".into());
    config.engines.lookup_latest = false;
    config.account_gate = AccountGate::LocalOnly;
    config.account_backend = AccountBackendKind::Engine {
        address: url.to_owned(),
    };
    Core::open(config).unwrap()
}

/// Waits until `ready` holds, waking both workers as it goes: the Engine pushes no notice to a
/// device, so a device learns of the other's change when its worker next runs.
async fn settle(what: &str, cores: &[&Core], ready: impl Fn() -> bool) {
    let deadline = Instant::now() + PATIENCE;
    loop {
        if ready() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{what}: not there after {PATIENCE:?}; status {:?}",
            cores
                .iter()
                .map(|core| core.account.status().state)
                .collect::<Vec<_>>()
        );
        for core in cores {
            core.account.sync_now().await.unwrap();
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn two_devices_of_one_account_follow_each_other_and_merge_after_being_apart() {
    let url = engine_url();
    if !answers(&url).await {
        let reason = format!(
            "nothing answers {url}/health; start the Engine's local stack (scripts/local.sh up) or set ORBYN_E2E_ENGINE_URL"
        );
        assert!(
            std::env::var("ORBYN_E2E_REQUIRE_ENGINE").as_deref() != Ok("1"),
            "ORBYN_E2E_REQUIRE_ENGINE=1 and {reason}"
        );
        eprintln!("e2e_engine skipped: {reason}");
        return;
    }

    // One throwaway account: a made-up address and a password nobody sees.
    let email = format!("e2e-{}@example.com", hex());
    let secret = format!("{}-{}-{}", hex(), hex(), hex());
    let pw = || Password::new(secret.clone());

    let dir = TempDir::new();
    let (a, b) = (device(&dir, "a", &url), device(&dir, "b", &url));
    assert!(
        a.account.available() && b.account.available(),
        "the Engine address must be a loopback IP literal"
    );

    // Sign up on A with a fresh invite, then the code the worker logged for the address.
    let invite = invite_in(&local_sh(&[
        "invite", "create", "--count", "1", "--label", "e2e",
    ]))
    .expect("the invite script printed no invite");
    a.account.check_invite(&invite).await.unwrap();
    a.account
        .start_sign_up(&invite, &email, pw())
        .await
        .unwrap();
    let deadline = Instant::now() + PATIENCE;
    let code = loop {
        if let Some(code) = email_code_in(&local_sh(&["codes", "10m"]), &email) {
            break code;
        }
        assert!(Instant::now() < deadline, "the worker logged no email code");
        tokio::time::sleep(Duration::from_millis(500)).await;
    };
    a.account.verify_email_code(&code).await.unwrap();
    assert_eq!(a.account.devices().await.unwrap().len(), 1);

    // B signs in as the same person: a new device, so a trusted one has to let it in.
    assert_eq!(
        b.account.sign_in(&email, pw()).await.unwrap(),
        AccountSignInOutcome::AwaitingApproval
    );
    let deadline = Instant::now() + PATIENCE;
    let request = loop {
        if let Some(request) = a.account.approval_requests().await.unwrap().pop() {
            break request;
        }
        assert!(Instant::now() < deadline, "A never saw B's request");
        tokio::time::sleep(Duration::from_millis(250)).await;
    };
    let action = ConfirmFor::ApproveDevice(request.id.clone());
    let nonce = a.account.confirm(action.clone(), pw()).await.unwrap();
    let confirmed = a
        .account
        .require_confirmed(action, Some(&nonce))
        .await
        .unwrap();
    // A waits for B to join the request while B keeps asking, as its screen does.
    let (approved, shown) = tokio::join!(a.account.approve(confirmed, &request.id), async {
        let deadline = Instant::now() + PATIENCE;
        loop {
            let view = b.account.poll_approval().await.unwrap().unwrap();
            if view.state == AccountApprovalState::EnterDigits {
                return view;
            }
            assert!(Instant::now() < deadline, "B was never allowed");
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    });
    assert_eq!(shown.state, AccountApprovalState::EnterDigits);
    let digits = ApprovalDigits::new(approved.unwrap().digits);
    let signed = b.account.submit_digits(digits).await.unwrap().unwrap();
    assert_eq!(signed.state, AccountApprovalState::Approved);
    assert_eq!(a.account.devices().await.unwrap().len(), 2);

    // A change on A reaches B.
    let online =
        |core: &Core| core.account.status().state == SyncState::Online && waiting(core) == 0;
    settle("both online", &[&a, &b], || online(&a) && online(&b)).await;
    set(&a, KEY, json!("dark"));
    settle("A's change on B", &[&a, &b], || {
        stored(&b, KEY) == json!("dark") && online(&b)
    })
    .await;
    assert_eq!(stored(&a, KEY), json!("dark"));

    // Both change it while neither syncs. B's edit is the newer one.
    a.account.pause_sync().await.unwrap();
    b.account.pause_sync().await.unwrap();
    set(&a, KEY, json!("light"));
    tokio::time::sleep(Duration::from_millis(20)).await;
    set(&b, KEY, json!("system"));
    assert_eq!((waiting(&a), waiting(&b)), (1, 1));
    // Back online: B's edit reaches the account first; A then meets it with its own still queued.
    b.account.resume_sync().await.unwrap();
    settle("B's edit uploaded", &[&b], || online(&b)).await;
    a.account.resume_sync().await.unwrap();
    settle("the merge on A", &[&a, &b], || {
        stored(&a, KEY) == json!("system") && online(&a) && online(&b)
    })
    .await;
    assert_eq!(stored(&b, KEY), json!("system"));

    // The newer edit won everywhere, and A was told which of its edits lost.
    let lost = a.account.conflicts().unwrap();
    assert_eq!(lost.len(), 1, "{lost:?}");
    assert_eq!(
        (lost[0].kept.clone(), lost[0].lost.clone()),
        (json!("system"), json!("light"))
    );
    assert!(b.account.conflicts().unwrap().is_empty());
    a.account.clear_conflicts().unwrap();

    // Each device forgets its keys and queue; the account itself stays in the stack's own
    // database, which `scripts/local.sh reset` drops.
    a.account.sign_out(true).await.unwrap();
    b.account.sign_out(true).await.unwrap();
}
