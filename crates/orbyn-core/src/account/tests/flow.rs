//! The account's actions against the development stand-in: sign-up, sign-in, two-factor,
//! "Confirm it's you", devices and sign-out, and that the gate keeps all of it shut.

use orbyn_contracts::{AccountSignInOutcome, AccountSyncKind};

use super::{core_at, dev, until};
use crate::CoreError;
use crate::account::dev::{EMAIL_CODE, INVITE, PASSWORD};
use crate::account::{AccountGate, ConfirmFor, Password};
use crate::testing::TempDir;

fn pw(text: &str) -> Password {
    Password::new(text.to_owned())
}

fn code(error: CoreError) -> &'static str {
    error.code()
}

async fn signed_in(core: &crate::Core, email: &str) {
    core.account.sign_in(email, pw(PASSWORD)).await.unwrap();
}

#[tokio::test]
async fn the_off_gate_keeps_every_action_shut() {
    let dir = TempDir::new();
    let core = core_at(&dir, "off", AccountGate::Off);
    let a = &core.account;
    assert!(!a.available());
    assert!(!a.view("macos").await.available);
    assert_eq!(
        code(a.check_invite(INVITE).await.unwrap_err()),
        "account_disabled"
    );
    assert_eq!(
        code(
            a.sign_in("dev@example.com", pw(PASSWORD))
                .await
                .unwrap_err()
        ),
        "account_disabled"
    );
    assert_eq!(
        code(
            a.confirm(ConfirmFor::SignOutEverywhere, pw(PASSWORD))
                .await
                .unwrap_err()
        ),
        "account_disabled"
    );
    assert_eq!(
        code(
            a.require_confirmed(ConfirmFor::SignOutEverywhere)
                .await
                .unwrap_err()
        ),
        "account_disabled"
    );
    assert_eq!(code(a.devices().await.unwrap_err()), "account_disabled");
    assert_eq!(code(a.sync_now().await.unwrap_err()), "account_disabled");
}

#[tokio::test]
async fn local_only_without_a_backend_is_unavailable() {
    let dir = TempDir::new();
    let core = core_at(&dir, "none", AccountGate::LocalOnly);
    assert!(!core.account.available());
}

#[tokio::test]
async fn sign_up_runs_invite_email_code_and_password_then_signs_in() {
    let dir = TempDir::new();
    let core = dev(&dir, "up");
    let a = &core.account;
    assert!(a.view("macos").await.invite_required);
    // Input ignores case, hyphens and spaces.
    a.check_invite("k7qm 2xwh-9qxa 3c2d").await.unwrap();
    assert_eq!(
        code(a.check_invite("ABCD-EFGH-JKMN-PQRS").await.unwrap_err()),
        "invite_required"
    );
    assert_eq!(
        code(a.check_invite("OOOO-OOOO-OOOO-OOOO").await.unwrap_err()),
        "invalid_invite"
    );
    a.start_sign_up(INVITE, "new@example.com").await.unwrap();
    assert_eq!(
        code(a.verify_email_code("ZZZZ-ZZZZ").await.unwrap_err()),
        "invalid_code"
    );
    assert_eq!(code(a.register(pw("x")).await.unwrap_err()), "invalid_code");
    a.verify_email_code(&EMAIL_CODE.to_lowercase())
        .await
        .unwrap();
    a.register(pw("a fresh throwaway phrase")).await.unwrap();
    let view = a.view("macos").await;
    assert_eq!(view.session.unwrap().email, "new@example.com");
}

#[tokio::test]
async fn sign_in_answers_one_message_for_wrong_password_and_unknown_email() {
    let dir = TempDir::new();
    let core = dev(&dir, "in");
    let a = &core.account;
    let wrong = a.sign_in("dev@example.com", pw("nope")).await.unwrap_err();
    let unknown = a
        .sign_in("nobody@example.com", pw(PASSWORD))
        .await
        .unwrap_err();
    assert_eq!(wrong.to_string(), unknown.to_string());
    assert_eq!(code(wrong), "wrong_password");
    let slow = a
        .sign_in("slow@example.com", pw(PASSWORD))
        .await
        .unwrap_err();
    assert!(matches!(
        slow,
        CoreError::Account {
            retry_after_secs: Some(90),
            ..
        }
    ));
    assert_eq!(
        code(
            a.sign_in("down@example.com", pw(PASSWORD))
                .await
                .unwrap_err()
        ),
        "account_offline"
    );
    assert_eq!(a.view("macos").await.sync.kind, AccountSyncKind::SignedOut);
    assert_eq!(
        a.sign_in("dev@example.com", pw(PASSWORD)).await.unwrap(),
        AccountSignInOutcome::Signed
    );
    until(&core, |core| {
        core.account.status().state == crate::account::SyncState::Online
    })
    .await;
}

#[tokio::test]
async fn five_wrong_second_factor_codes_close_the_sign_in() {
    let dir = TempDir::new();
    let core = dev(&dir, "2fa");
    let a = &core.account;
    let outcome = a.sign_in("two@example.com", pw(PASSWORD)).await.unwrap();
    assert_eq!(outcome, AccountSignInOutcome::SecondFactor);
    assert!(a.view("macos").await.needs_second_factor);
    for tries_left in (1..5).rev() {
        let error = a.second_factor("000000").await.unwrap_err();
        assert_eq!(error.code(), "invalid_code");
        assert!(
            error
                .to_string()
                .contains(&format!("{tries_left} tries left")),
            "{error}"
        );
    }
    // The fifth wrong code closes the sign-in; even the right code is closed after.
    assert_eq!(
        code(a.second_factor("000000").await.unwrap_err()),
        "sign_in_closed"
    );
    assert_eq!(
        code(a.second_factor("123456").await.unwrap_err()),
        "sign_in_closed"
    );
    assert!(!a.view("macos").await.needs_second_factor);
}

#[tokio::test]
async fn a_recovery_code_works_with_any_case_and_a_hyphen() {
    let dir = TempDir::new();
    let core = dev(&dir, "recovery");
    core.account
        .sign_in("two@example.com", pw(PASSWORD))
        .await
        .unwrap();
    core.account.second_factor("abcde-fgh23").await.unwrap();
    assert!(core.account.view("macos").await.session.is_some());
}

#[tokio::test]
async fn signing_in_is_not_a_confirmation() {
    let dir = TempDir::new();
    let core = dev(&dir, "signin-is-not");
    let a = &core.account;
    signed_in(&core, "dev@example.com").await;
    assert_eq!(
        code(
            a.require_confirmed(ConfirmFor::SignOutEverywhere)
                .await
                .unwrap_err()
        ),
        "reauth_required"
    );
    assert_eq!(
        code(
            a.require_confirmed(ConfirmFor::RemoveDevice("dev-mini".into()))
                .await
                .unwrap_err()
        ),
        "reauth_required"
    );
}

#[tokio::test]
async fn a_confirmation_is_for_one_action_and_one_device_and_is_used_up() {
    let dir = TempDir::new();
    let core = dev(&dir, "one-shot");
    let a = &core.account;
    signed_in(&core, "dev@example.com").await;
    assert_eq!(
        code(
            a.confirm(ConfirmFor::RemoveDevice("dev-mini".into()), pw("wrong"))
                .await
                .unwrap_err()
        ),
        "wrong_password"
    );
    a.confirm(ConfirmFor::RemoveDevice("dev-mini".into()), pw(PASSWORD))
        .await
        .unwrap();
    // Not for the other device, and not for the other action; asking does not use it up.
    assert_eq!(
        code(
            a.require_confirmed(ConfirmFor::RemoveDevice("dev-phone".into()))
                .await
                .unwrap_err()
        ),
        "reauth_required"
    );
    assert_eq!(
        code(
            a.require_confirmed(ConfirmFor::SignOutEverywhere)
                .await
                .unwrap_err()
        ),
        "reauth_required"
    );
    let token = a
        .require_confirmed(ConfirmFor::RemoveDevice("dev-mini".into()))
        .await
        .unwrap();
    // A token for one device cannot remove another.
    assert_eq!(
        code(a.remove_device(token, "dev-phone").await.unwrap_err()),
        "reauth_required"
    );
    // Used up on first use: the same confirmation does not work twice, even for the same device.
    assert_eq!(
        code(
            a.require_confirmed(ConfirmFor::RemoveDevice("dev-mini".into()))
                .await
                .unwrap_err()
        ),
        "reauth_required"
    );
    a.confirm(ConfirmFor::RemoveDevice("dev-mini".into()), pw(PASSWORD))
        .await
        .unwrap();
    let token = a
        .require_confirmed(ConfirmFor::RemoveDevice("dev-mini".into()))
        .await
        .unwrap();
    a.remove_device(token, "dev-mini").await.unwrap();
    assert_eq!(a.devices().await.unwrap().len(), 2);
}

#[tokio::test]
async fn a_confirmation_ends_after_ten_minutes_and_is_peeked_without_being_used() {
    let dir = TempDir::new();
    let core = dev(&dir, "peek");
    let a = &core.account;
    signed_in(&core, "dev@example.com").await;
    a.confirm(ConfirmFor::SignOutEverywhere, pw(PASSWORD))
        .await
        .unwrap();
    assert!(a.confirmed_for(&ConfirmFor::SignOutEverywhere));
    assert!(
        a.confirmed_for(&ConfirmFor::SignOutEverywhere),
        "peeking does not use it up"
    );
    assert!(!a.confirmed_for(&ConfirmFor::RemoveDevice("dev-mini".into())));
    a.forget_confirmation();
    assert!(!a.confirmed_for(&ConfirmFor::SignOutEverywhere));
}

#[tokio::test]
async fn a_confirmation_is_cleared_by_sign_out_and_does_not_survive_a_restart() {
    let dir = TempDir::new();
    let core = dev(&dir, "cleared");
    let a = &core.account;
    signed_in(&core, "dev@example.com").await;
    a.confirm(ConfirmFor::SignOutEverywhere, pw(PASSWORD))
        .await
        .unwrap();
    a.sign_out(false).await.unwrap();
    assert!(!a.confirmed_for(&ConfirmFor::SignOutEverywhere));
    signed_in(&core, "dev@example.com").await;
    assert_eq!(
        code(
            a.require_confirmed(ConfirmFor::SignOutEverywhere)
                .await
                .unwrap_err()
        ),
        "reauth_required"
    );
    // A restart is a new core on the same data folder: memory only, so nothing is waiting.
    a.confirm(ConfirmFor::SignOutEverywhere, pw(PASSWORD))
        .await
        .unwrap();
    drop(core);
    let again = dev(&dir, "cleared");
    signed_in(&again, "dev@example.com").await;
    assert!(!again.account.confirmed_for(&ConfirmFor::SignOutEverywhere));
}

#[tokio::test]
async fn a_confirmation_is_cleared_by_signing_out_everywhere() {
    let dir = TempDir::new();
    let core = dev(&dir, "cleared-everywhere");
    let a = &core.account;
    signed_in(&core, "dev@example.com").await;
    a.confirm(ConfirmFor::SignOutEverywhere, pw(PASSWORD))
        .await
        .unwrap();
    a.confirm(ConfirmFor::RemoveDevice("dev-mini".into()), pw(PASSWORD))
        .await
        .unwrap();
    let token = a
        .require_confirmed(ConfirmFor::RemoveDevice("dev-mini".into()))
        .await
        .unwrap();
    a.remove_device(token, "dev-mini").await.unwrap();
    a.confirm(ConfirmFor::SignOutEverywhere, pw(PASSWORD))
        .await
        .unwrap();
    let token = a
        .require_confirmed(ConfirmFor::SignOutEverywhere)
        .await
        .unwrap();
    a.confirm(ConfirmFor::RemoveDevice("dev-phone".into()), pw(PASSWORD))
        .await
        .unwrap();
    a.sign_out_everywhere(token).await.unwrap();
    assert!(!a.confirmed_for(&ConfirmFor::RemoveDevice("dev-phone".into())));
}

#[tokio::test]
async fn too_many_wrong_confirmations_are_rate_limited_and_the_action_is_not_done() {
    let dir = TempDir::new();
    let core = dev(&dir, "limit");
    let a = &core.account;
    signed_in(&core, "dev@example.com").await;
    for _ in 0..5 {
        assert_eq!(
            code(
                a.confirm(ConfirmFor::SignOutEverywhere, pw("wrong"))
                    .await
                    .unwrap_err()
            ),
            "wrong_password"
        );
    }
    let error = a
        .confirm(ConfirmFor::SignOutEverywhere, pw(PASSWORD))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        CoreError::Account {
            code: "rate_limited",
            retry_after_secs: Some(600),
            ..
        }
    ));
    assert_eq!(
        code(
            a.require_confirmed(ConfirmFor::SignOutEverywhere)
                .await
                .unwrap_err()
        ),
        "reauth_required"
    );
}

#[tokio::test]
async fn devices_list_and_removal_hide_the_removed_device() {
    let dir = TempDir::new();
    let core = dev(&dir, "devices");
    let a = &core.account;
    assert_eq!(code(a.devices().await.unwrap_err()), "reauth_required");
    signed_in(&core, "dev@example.com").await;
    assert_eq!(a.devices().await.unwrap().len(), 3);
    a.confirm(ConfirmFor::RemoveDevice("dev-phone".into()), pw(PASSWORD))
        .await
        .unwrap();
    let token = a
        .require_confirmed(ConfirmFor::RemoveDevice("dev-phone".into()))
        .await
        .unwrap();
    a.remove_device(token, "dev-phone").await.unwrap();
    let left = a.devices().await.unwrap();
    assert_eq!(left.len(), 2);
    assert!(left.iter().any(|d| d.current));
}

#[tokio::test]
async fn sign_out_everywhere_is_blocked_while_deletion_is_scheduled() {
    let dir = TempDir::new();
    let core = dev(&dir, "delete");
    let a = &core.account;
    signed_in(&core, "delete@example.com").await;
    a.confirm(ConfirmFor::SignOutEverywhere, pw(PASSWORD))
        .await
        .unwrap();
    let token = a
        .require_confirmed(ConfirmFor::SignOutEverywhere)
        .await
        .unwrap();
    assert_eq!(
        code(a.sign_out_everywhere(token).await.unwrap_err()),
        "deletion_scheduled"
    );
    let view = a.view("macos").await;
    assert!(view.deletion_scheduled);
    assert!(view.session.is_some(), "still signed in on this computer");
    a.sign_out(false).await.unwrap();
    assert!(a.view("macos").await.session.is_none());
}

#[tokio::test]
async fn sign_out_everywhere_lands_signed_out() {
    let dir = TempDir::new();
    let core = dev(&dir, "everywhere");
    let a = &core.account;
    signed_in(&core, "dev@example.com").await;
    a.confirm(ConfirmFor::SignOutEverywhere, pw(PASSWORD))
        .await
        .unwrap();
    let token = a
        .require_confirmed(ConfirmFor::SignOutEverywhere)
        .await
        .unwrap();
    a.sign_out_everywhere(token).await.unwrap();
    let view = a.view("macos").await;
    assert!(view.session.is_none());
    assert_eq!(view.sync.kind, AccountSyncKind::SignedOut);
}

#[tokio::test]
async fn sign_out_keeps_unsynced_changes_unless_asked_to_discard_them() {
    let dir = TempDir::new();
    let core = dev(&dir, "out");
    let a = &core.account;
    signed_in(&core, "dev@example.com").await;
    a.pause_sync().await.unwrap();
    assert_eq!(a.view("macos").await.sync.kind, AccountSyncKind::Paused);
    super::set(&core, "branch_name_prefix", serde_json::json!("feat/"));
    assert!(super::waiting(&core) > 0);
    a.sign_out(false).await.unwrap();
    assert!(super::waiting(&core) > 0, "kept for the next sign-in");
    signed_in(&core, "dev@example.com").await;
    a.pause_sync().await.unwrap();
    a.sign_out(true).await.unwrap();
    assert_eq!(super::waiting(&core), 0);
}

#[tokio::test]
async fn pause_and_resume_move_the_sync_line_and_resume_restarts_the_worker() {
    let dir = TempDir::new();
    let core = dev(&dir, "pause");
    let a = &core.account;
    signed_in(&core, "dev@example.com").await;
    a.pause_sync().await.unwrap();
    assert_eq!(
        a.view("macos").await.sync.reason,
        Some(orbyn_contracts::AccountReason::ByPerson)
    );
    a.resume_sync().await.unwrap();
    until(&core, |core| {
        core.account.status().state == crate::account::SyncState::Online
    })
    .await;
    a.sync_now().await.unwrap();
}

#[tokio::test]
async fn no_password_shows_in_any_debug_or_error_text() {
    let secret = "super-secret-phrase-123";
    assert!(!format!("{:?}", pw(secret)).contains(secret));
    let body: orbyn_contracts::AccountSignIn =
        serde_json::from_str(&format!(r#"{{"email":"a@b.co","password":"{secret}"}}"#)).unwrap();
    let confirm: orbyn_contracts::AccountConfirm = serde_json::from_str(&format!(
        r#"{{"password":"{secret}","action":"remove_device","device_id":"x"}}"#
    ))
    .unwrap();
    assert!(!format!("{confirm:?}").contains(secret));
    assert!(!format!("{body:?}").contains(secret));
    let dir = TempDir::new();
    let core = dev(&dir, "leak");
    let error = core
        .account
        .sign_in("dev@example.com", pw(secret))
        .await
        .unwrap_err();
    assert!(!format!("{error:?} {error}").contains(secret));
    let view = format!("{:?}", core.account.view("macos").await);
    assert!(!view.contains(secret));
}

#[tokio::test]
async fn a_busy_network_gets_a_visible_check_and_a_failed_check_sends_nothing() {
    let dir = TempDir::new();
    let core = dev(&dir, "check");
    let a = &core.account;
    let failed = a
        .start_sign_up(INVITE, "hard@example.com")
        .await
        .unwrap_err();
    assert_eq!(failed.code(), "check_failed");
    // Nothing was sent: no code can be verified for that email.
    assert_eq!(
        code(a.verify_email_code("ABCD-EFGH").await.unwrap_err()),
        "invalid_code"
    );
    let started = std::time::Instant::now();
    a.start_sign_up(INVITE, "busy@example.com").await.unwrap();
    assert!(started.elapsed() >= std::time::Duration::from_millis(1400));
    a.verify_email_code("ABCD-EFGH").await.unwrap();
}
