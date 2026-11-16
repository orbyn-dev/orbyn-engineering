//! A new device signing in with a trusted device's help, and joining this computer's settings
//! to the account's the first time. The stand-in plays the account server; `answer` plays the
//! trusted device. Nothing here touches a network.

use std::time::Duration;

use orbyn_contracts::{AccountApprovalState, AccountConnectState, AccountSignInOutcome};
use serde_json::json;

use super::{dev, set, stored, until};
use crate::CoreError;
use crate::account::ApprovalDigits;
use crate::account::dev::{Answer, PASSWORD};
use crate::account::settings_record::{self as record, ZONE};
use crate::account::{ConfirmFor, FirstSync, Password, SyncState};
use crate::testing::TempDir;

fn pw(text: &str) -> Password {
    Password::new(text.to_owned())
}

fn pref(key: &str, value: serde_json::Value) -> (String, serde_json::Value) {
    let staged = record::stage_preference(key, None, Some(&value));
    let Some(record::Staged::Field { name, value }) = staged.into_iter().next() else {
        panic!("a preference stages one field")
    };
    (name, value)
}

async fn confirmed(
    a: &crate::account::AccountCenter,
    first: FirstSync,
) -> crate::account::Confirmed {
    a.confirm(ConfirmFor::ConnectSettings(first), pw(PASSWORD))
        .await
        .unwrap();
    a.require_confirmed_last(ConfirmFor::ConnectSettings(first))
        .await
        .unwrap()
}

fn code(error: CoreError) -> &'static str {
    error.code()
}

async fn asking(core: &crate::Core, email: &str) {
    let outcome = core.account.sign_in(email, pw(PASSWORD)).await.unwrap();
    assert_eq!(outcome, AccountSignInOutcome::AwaitingApproval);
}

/// The digits as the person reads them off the trusted device, and an entry that can never match them.
fn shown(core: &crate::Core) -> String {
    core.account
        .stand_in()
        .shown_digits()
        .expect("the trusted device showed digits")
}

/// Every digit changed, so the entry cannot equal `right`.
fn wrong_for(right: &str) -> String {
    right
        .bytes()
        .map(|byte| char::from(b'0' + (byte - b'0' + 1) % 10))
        .collect()
}

fn typed(digits: &str) -> ApprovalDigits {
    ApprovalDigits::new(digits.to_owned())
}

#[tokio::test]
async fn a_new_device_waits_without_a_session_and_is_shown_no_digits() {
    let dir = TempDir::new();
    let core = dev(&dir, "waits");
    asking(&core, "late@example.com").await;
    let view = core.account.view("macos").await;
    assert!(view.session.is_none());
    let approval = view.approval.unwrap();
    assert_eq!(approval.state, AccountApprovalState::Waiting);
    assert_eq!(approval.tries_left, None);
    assert!(approval.expires_in_secs > 590 && approval.expires_in_secs <= 600);
    let polled = core.account.poll_approval().await.unwrap().unwrap();
    assert_eq!(polled.state, AccountApprovalState::Waiting);
    assert!(core.account.view("macos").await.session.is_none());
    // Nothing has been allowed, so there are no digits anywhere; typing now is refused.
    assert!(core.account.stand_in().shown_digits().is_none());
    assert_eq!(
        code(
            core.account
                .submit_digits(typed("123456"))
                .await
                .unwrap_err()
        ),
        "request_not_allowed"
    );
}

#[tokio::test]
async fn allowing_asks_for_the_digits_and_never_signs_in_by_itself() {
    let dir = TempDir::new();
    let core = dev(&dir, "allowed");
    asking(&core, "late@example.com").await;
    core.account.stand_in().answer(Answer::Allow);
    for _ in 0..3 {
        let shown = core.account.poll_approval().await.unwrap().unwrap();
        assert_eq!(shown.state, AccountApprovalState::EnterDigits);
        assert_eq!(shown.tries_left, None);
        assert!(core.account.view("macos").await.session.is_none());
    }
    let right = shown(&core);
    let done = core
        .account
        .submit_digits(typed(&right))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(done.state, AccountApprovalState::Approved);
    let view = core.account.view("macos").await;
    assert_eq!(view.session.unwrap().email, "late@example.com");
    assert!(
        view.approval.is_none(),
        "the request is over once the digits are accepted"
    );
    assert_eq!(core.account.poll_approval().await.unwrap(), None);
}

#[tokio::test]
async fn a_refusal_closes_the_request_and_signs_nobody_in() {
    let dir = TempDir::new();
    let core = dev(&dir, "closed");
    asking(&core, "late@example.com").await;
    core.account.stand_in().answer(Answer::Deny);
    let shown = core.account.poll_approval().await.unwrap().unwrap();
    assert_eq!(shown.state, AccountApprovalState::Denied);
    // It stays closed on every later look, and the screen can say so until the person leaves it.
    assert_eq!(
        core.account.poll_approval().await.unwrap().unwrap().state,
        AccountApprovalState::Denied
    );
    let view = core.account.view("macos").await;
    assert_eq!(view.approval.unwrap().state, AccountApprovalState::Denied);
    assert!(view.session.is_none());
    assert_eq!(
        core.account.status().state,
        SyncState::SignedOut { reason: None }
    );
    // Leaving clears it; asking again makes a new request.
    core.account.cancel_second_factor().await.unwrap();
    assert!(core.account.view("macos").await.approval.is_none());
}

#[tokio::test]
async fn wrong_digits_keep_the_request_open_and_count_down_the_tries() {
    let dir = TempDir::new();
    let core = dev(&dir, "wrong");
    asking(&core, "late@example.com").await;
    core.account.stand_in().answer(Answer::Allow);
    core.account.poll_approval().await.unwrap();
    let right = shown(&core);
    let wrong = wrong_for(&right);
    let first = core
        .account
        .submit_digits(typed(&wrong))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.state, AccountApprovalState::EnterDigits);
    assert_eq!(first.tries_left, Some(2));
    // The view the page reads says the same, and nobody is signed in.
    let view = core.account.view("macos").await;
    assert_eq!(view.approval.unwrap().tries_left, Some(2));
    assert!(view.session.is_none());
    let second = core
        .account
        .submit_digits(typed(&wrong))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(second.tries_left, Some(1));
    // A wrong entry before the right one does not stop the right one.
    let done = core
        .account
        .submit_digits(typed(&right))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(done.state, AccountApprovalState::Approved);
    assert!(core.account.view("macos").await.session.is_some());
}

#[tokio::test]
async fn the_last_wrong_try_closes_the_request_and_the_right_digits_no_longer_work() {
    let dir = TempDir::new();
    let core = dev(&dir, "locked");
    asking(&core, "late@example.com").await;
    core.account.stand_in().answer(Answer::Allow);
    core.account.poll_approval().await.unwrap();
    let right = shown(&core);
    let wrong = wrong_for(&right);
    for _ in 0..2 {
        core.account.submit_digits(typed(&wrong)).await.unwrap();
    }
    let closed = core
        .account
        .submit_digits(typed(&wrong))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(closed.state, AccountApprovalState::Locked);
    assert_eq!(closed.tries_left, None);
    assert_eq!(closed.expires_in_secs, 0);
    // The right digits come too late: the request is closed and nobody is signed in.
    let after = core
        .account
        .submit_digits(typed(&right))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.state, AccountApprovalState::Locked);
    assert!(core.account.view("macos").await.session.is_none());
    assert_eq!(
        core.account.poll_approval().await.unwrap().unwrap().state,
        AccountApprovalState::Locked
    );
    assert_eq!(
        core.account.status().state,
        SyncState::SignedOut { reason: None }
    );
    assert!(
        core.account.stand_in().shown_digits().is_none(),
        "a locked request shows no digits"
    );
}

#[tokio::test]
async fn the_app_bounds_the_tries_itself_when_the_server_does_not_count() {
    // The stand-in plays a server that never locks: its digits are reissued with fresh tries.
    let dir = TempDir::new();
    let core = dev(&dir, "bounded");
    asking(&core, "late@example.com").await;
    core.account.stand_in().answer(Answer::Allow);
    core.account.poll_approval().await.unwrap();
    let wrong = wrong_for(&shown(&core));
    for _ in 0..crate::account::digits::TRIES {
        core.account.stand_in().refresh_digit_tries();
        core.account.submit_digits(typed(&wrong)).await.unwrap();
    }
    assert_eq!(
        core.account.view("macos").await.approval.unwrap().state,
        AccountApprovalState::Locked
    );
}

#[tokio::test]
async fn digits_that_are_not_six_digits_are_refused_and_cost_no_try() {
    let dir = TempDir::new();
    let core = dev(&dir, "malformed");
    asking(&core, "late@example.com").await;
    core.account.stand_in().answer(Answer::Allow);
    core.account.poll_approval().await.unwrap();
    for bad in ["", "12345", "1234567", "12345a", "abcdef"] {
        assert_eq!(
            code(core.account.submit_digits(typed(bad)).await.unwrap_err()),
            "invalid_digits"
        );
    }
    assert_eq!(core.account.stand_in().digit_tries_left(), Some(3));
    // Pasted with a space, as the trusted device shows them.
    let right = shown(&core);
    let pasted = format!("{} {}", &right[..3], &right[3..]);
    let done = core
        .account
        .submit_digits(typed(&pasted))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(done.state, AccountApprovalState::Approved);
}

#[tokio::test]
async fn nothing_prints_the_typed_digits() {
    let dir = TempDir::new();
    let core = dev(&dir, "private");
    asking(&core, "late@example.com").await;
    core.account.stand_in().answer(Answer::Allow);
    core.account.poll_approval().await.unwrap();
    let right = shown(&core);
    let wrong = wrong_for(&right);
    assert!(!format!("{:?}", typed(&wrong)).contains(&wrong));
    let view = core
        .account
        .submit_digits(typed(&wrong))
        .await
        .unwrap()
        .unwrap();
    let text = format!("{view:?} {:?}", core.account.view("macos").await);
    assert!(!text.contains(&wrong));
    assert!(!text.contains(&right));
    let json = serde_json::to_string(&view).unwrap();
    assert!(!json.contains(&wrong));
    assert!(!json.contains(&right));
}

#[tokio::test]
async fn ten_minutes_without_an_answer_end_the_request_on_this_computers_clock() {
    let dir = TempDir::new();
    let core = dev(&dir, "expires");
    core.account.set_approval_life(Duration::from_millis(20));
    asking(&core, "late@example.com").await;
    tokio::time::sleep(Duration::from_millis(40)).await;
    let shown = core.account.poll_approval().await.unwrap().unwrap();
    assert_eq!(shown.state, AccountApprovalState::TimedOut);
    assert_eq!(shown.expires_in_secs, 0);
    assert_eq!(
        core.account.stand_in().cancelled(),
        vec!["req-mine".to_owned()]
    );
    // A late allowance changes nothing: this computer already gave the request up.
    core.account.stand_in().answer(Answer::Allow);
    assert_eq!(
        core.account.poll_approval().await.unwrap().unwrap().state,
        AccountApprovalState::TimedOut
    );
    assert!(core.account.view("macos").await.session.is_none());
}

#[tokio::test]
async fn digits_typed_after_the_ten_minutes_do_not_sign_in() {
    let dir = TempDir::new();
    let core = dev(&dir, "late-digits");
    core.account.set_approval_life(Duration::from_millis(30));
    asking(&core, "late@example.com").await;
    core.account.stand_in().answer(Answer::Allow);
    core.account.poll_approval().await.unwrap();
    let right = shown(&core);
    tokio::time::sleep(Duration::from_millis(60)).await;
    let late = core
        .account
        .submit_digits(typed(&right))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(late.state, AccountApprovalState::TimedOut);
    assert!(core.account.view("macos").await.session.is_none());
}

#[tokio::test]
async fn cancelling_gives_the_request_up() {
    let dir = TempDir::new();
    let core = dev(&dir, "cancel");
    asking(&core, "late@example.com").await;
    core.account.cancel_second_factor().await.unwrap();
    assert_eq!(
        core.account.stand_in().cancelled(),
        vec!["req-mine".to_owned()]
    );
    assert!(core.account.view("macos").await.approval.is_none());
    assert_eq!(core.account.poll_approval().await.unwrap(), None);
}

#[tokio::test]
async fn a_trusted_device_lists_requests_and_letting_one_in_needs_a_confirmation_for_that_request()
{
    let dir = TempDir::new();
    let core = dev(&dir, "trusted");
    let a = &core.account;
    assert_eq!(
        code(a.approval_requests().await.unwrap_err()),
        "reauth_required"
    );
    a.sign_in("dev@example.com", pw(PASSWORD)).await.unwrap();
    let list = a.approval_requests().await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "req-ipad");
    assert!(!list[0].allowed);
    assert_eq!(list[0].near.as_deref(), Some("Near Melbourne"));
    // Signing in is not a confirmation; neither is one for another action or request.
    let ask = || ConfirmFor::ApproveDevice("req-ipad".into());
    assert_eq!(
        code(a.require_confirmed_last(ask()).await.unwrap_err()),
        "reauth_required"
    );
    a.confirm(ConfirmFor::ApproveDevice("req-other".into()), pw(PASSWORD))
        .await
        .unwrap();
    assert_eq!(
        code(a.require_confirmed_last(ask()).await.unwrap_err()),
        "reauth_required"
    );
    a.confirm(ask(), pw(PASSWORD)).await.unwrap();
    let token = a.require_confirmed_last(ask()).await.unwrap();
    // A token for one request cannot let in another.
    assert_eq!(
        code(a.approve(token, "req-other").await.unwrap_err()),
        "reauth_required"
    );
    a.confirm(ask(), pw(PASSWORD)).await.unwrap();
    let token = a.require_confirmed_last(ask()).await.unwrap();
    let approved = a.approve(token, "req-ipad").await.unwrap();
    // Allowing answers with the six digits to show, in two groups; it stays listed as allowed.
    let shown = a.stand_in().digits_shown_for("req-ipad").unwrap();
    assert_eq!(approved.digits.replace(' ', ""), shown);
    assert_eq!(approved.digits.len(), 7);
    let list = a.approval_requests().await.unwrap();
    assert_eq!(list.len(), 1);
    assert!(list[0].allowed);
    assert_eq!(a.stand_in().decided(), vec![("req-ipad".to_owned(), true)]);
    // It is allowed once.
    a.confirm(ask(), pw(PASSWORD)).await.unwrap();
    let again = a.require_confirmed_last(ask()).await.unwrap();
    assert_eq!(
        code(a.approve(again, "req-ipad").await.unwrap_err()),
        "request_gone"
    );
    // Used up on first use.
    assert_eq!(
        code(a.require_confirmed_last(ask()).await.unwrap_err()),
        "reauth_required"
    );
}

#[tokio::test]
async fn refusing_needs_no_confirmation_and_cancel_closes_an_allowed_request() {
    let dir = TempDir::new();
    let core = dev(&dir, "refuse");
    let a = &core.account;
    assert_eq!(
        code(a.deny("req-ipad").await.unwrap_err()),
        "reauth_required"
    );
    a.sign_in("dev@example.com", pw(PASSWORD)).await.unwrap();
    a.deny("req-ipad").await.unwrap();
    assert_eq!(a.stand_in().decided(), vec![("req-ipad".to_owned(), false)]);
    assert!(a.approval_requests().await.unwrap().is_empty());
    // After Allow, Cancel on the digits dialog closes the request and its digits go with it.
    let ask = || ConfirmFor::ApproveDevice("req-ipad".into());
    let dir = TempDir::new();
    let core = dev(&dir, "cancel-allowed");
    let a = &core.account;
    a.sign_in("dev@example.com", pw(PASSWORD)).await.unwrap();
    a.confirm(ask(), pw(PASSWORD)).await.unwrap();
    let token = a.require_confirmed_last(ask()).await.unwrap();
    a.approve(token, "req-ipad").await.unwrap();
    a.deny("req-ipad").await.unwrap();
    assert!(a.approval_requests().await.unwrap().is_empty());
    assert!(a.stand_in().digits_shown_for("req-ipad").is_none());
    assert_eq!(
        a.stand_in().decided(),
        vec![
            ("req-ipad".to_owned(), true),
            ("req-ipad".to_owned(), false)
        ]
    );
}

#[tokio::test]
async fn signing_out_clears_a_confirmation_for_letting_a_device_in() {
    let dir = TempDir::new();
    let core = dev(&dir, "clears");
    let a = &core.account;
    a.sign_in("dev@example.com", pw(PASSWORD)).await.unwrap();
    a.confirm(ConfirmFor::ApproveDevice("req-ipad".into()), pw(PASSWORD))
        .await
        .unwrap();
    a.sign_out(false).await.unwrap();
    assert!(!a.confirmed_for_last(&ConfirmFor::ApproveDevice("req-ipad".into())));
}

// Joining this computer's settings to the account's

async fn with_settings(dir: &TempDir, name: &str) -> crate::Core {
    let core = dev(dir, name);
    set(&core, "preview_appearance", json!("dark"));
    core
}

#[tokio::test]
async fn a_computer_with_settings_asks_once_before_anything_is_sent() {
    let dir = TempDir::new();
    let core = with_settings(&dir, "asks").await;
    let a = &core.account;
    a.sign_in("dev@example.com", pw(PASSWORD)).await.unwrap();
    let view = a.view("macos").await;
    assert_eq!(view.connect, AccountConnectState::Needed);
    assert!(view.session.is_some());
    assert_eq!(a.status().state, SyncState::NotConnected);
    assert_eq!(
        a.stand_in().zones().pulls(),
        0,
        "nothing reaches the account before the choice"
    );
    assert!(a.stand_in().zones().zone(ZONE).is_none());
    // Not now leaves the computer local-only, and it can still connect later.
    a.connect_later().await.unwrap();
    assert_eq!(a.view("macos").await.connect, AccountConnectState::Declined);
    assert_eq!(a.stand_in().zones().pulls(), 0);
    a.connect(FirstSync::Combine, None).await.unwrap();
    assert_eq!(
        a.view("macos").await.connect,
        AccountConnectState::Connected
    );
    until(&core, |c| {
        matches!(c.account.status().state, SyncState::Online)
    })
    .await;
    assert!(a.stand_in().zones().zone(ZONE).is_some());
    let summary = a.view("macos").await.connect_summary.unwrap();
    assert!(summary.synced >= 1);
    a.connect_summary_seen().await.unwrap();
    assert!(a.view("macos").await.connect_summary.is_none());
}

#[tokio::test]
async fn a_computer_with_no_settings_and_a_new_account_have_nothing_to_ask() {
    let dir = TempDir::new();
    let core = dev(&dir, "nothing");
    core.account
        .sign_in("dev@example.com", pw(PASSWORD))
        .await
        .unwrap();
    assert_eq!(
        core.account.view("macos").await.connect,
        AccountConnectState::Connected
    );
    assert_ne!(core.account.status().state, SyncState::NotConnected);
}

#[tokio::test]
async fn a_computer_that_joined_before_does_not_ask_again() {
    let dir = TempDir::new();
    let core = with_settings(&dir, "again").await;
    let a = &core.account;
    a.sign_in("dev@example.com", pw(PASSWORD)).await.unwrap();
    a.connect(FirstSync::Combine, None).await.unwrap();
    until(&core, |c| {
        matches!(c.account.status().state, SyncState::Online)
    })
    .await;
    a.sign_out(false).await.unwrap();
    a.sign_in("dev@example.com", pw(PASSWORD)).await.unwrap();
    assert_eq!(
        a.view("macos").await.connect,
        AccountConnectState::Connected
    );
}

#[tokio::test]
async fn combining_keeps_the_accounts_value_where_it_has_one() {
    let dir = TempDir::new();
    let core = with_settings(&dir, "combine").await;
    let a = &core.account;
    a.stand_in().zones().peer_write(
        ZONE,
        "other",
        crate::util::unix_now_ms() - 1000,
        vec![pref("preview_appearance", json!("light"))],
    );
    a.sign_in("dev@example.com", pw(PASSWORD)).await.unwrap();
    a.connect(FirstSync::Combine, None).await.unwrap();
    until(&core, |c| {
        matches!(c.account.status().state, SyncState::Online)
    })
    .await;
    assert_eq!(stored(&core, "preview_appearance"), json!("light"));
}

#[tokio::test]
async fn using_this_computers_settings_wins_everywhere() {
    let dir = TempDir::new();
    let core = with_settings(&dir, "device").await;
    let a = &core.account;
    a.stand_in().zones().peer_write(
        ZONE,
        "other",
        crate::util::unix_now_ms() - 1000,
        vec![pref("preview_appearance", json!("light"))],
    );
    a.sign_in("dev@example.com", pw(PASSWORD)).await.unwrap();
    a.connect(
        FirstSync::UseDevice,
        Some(confirmed(a, FirstSync::UseDevice).await),
    )
    .await
    .unwrap();
    until(&core, |c| {
        matches!(c.account.status().state, SyncState::Online)
    })
    .await;
    assert_eq!(stored(&core, "preview_appearance"), json!("dark"));
    let zone = a.stand_in().zones().zone(ZONE).unwrap();
    assert_eq!(
        zone.record[&pref("preview_appearance", json!("dark")).0].value,
        json!("dark")
    );
}

#[tokio::test]
async fn using_the_accounts_settings_uploads_nothing_of_this_computers() {
    let dir = TempDir::new();
    let core = with_settings(&dir, "account").await;
    let a = &core.account;
    a.stand_in().zones().peer_write(
        ZONE,
        "other",
        crate::util::unix_now_ms() - 1000,
        vec![pref("preview_appearance", json!("light"))],
    );
    a.sign_in("dev@example.com", pw(PASSWORD)).await.unwrap();
    a.connect(
        FirstSync::UseAccount,
        Some(confirmed(a, FirstSync::UseAccount).await),
    )
    .await
    .unwrap();
    until(&core, |c| {
        matches!(c.account.status().state, SyncState::Online)
    })
    .await;
    assert_eq!(stored(&core, "preview_appearance"), json!("light"));
    let zone = a.stand_in().zones().zone(ZONE).unwrap();
    assert_eq!(zone.record.len(), 1, "only what the account already held");
}

#[tokio::test]
async fn the_choices_that_overwrite_need_a_confirmation_for_that_choice() {
    let dir = TempDir::new();
    let core = with_settings(&dir, "guarded").await;
    let a = &core.account;
    a.sign_in("dev@example.com", pw(PASSWORD)).await.unwrap();
    for first in [FirstSync::UseDevice, FirstSync::UseAccount] {
        assert_eq!(
            code(a.connect(first, None).await.unwrap_err()),
            "reauth_required"
        );
    }
    // A confirmation for the other choice, or for another action, is no good.
    let token = confirmed(a, FirstSync::UseAccount).await;
    assert_eq!(
        code(
            a.connect(FirstSync::UseDevice, Some(token))
                .await
                .unwrap_err()
        ),
        "reauth_required"
    );
    assert_eq!(a.view("macos").await.connect, AccountConnectState::Needed);
    assert_eq!(a.stand_in().zones().pulls(), 0);
    // Used up by the choice it was for.
    let token = confirmed(a, FirstSync::UseDevice).await;
    a.connect(FirstSync::UseDevice, Some(token)).await.unwrap();
    assert_eq!(
        a.view("macos").await.connect,
        AccountConnectState::Connected
    );
}

#[tokio::test]
async fn a_second_account_on_the_same_data_folder_starts_from_nothing() {
    let dir = TempDir::new();
    let core = with_settings(&dir, "two-accounts").await;
    let a = &core.account;
    a.sign_in("dev@example.com", pw(PASSWORD)).await.unwrap();
    a.connect(FirstSync::Combine, None).await.unwrap();
    until(&core, |c| {
        matches!(c.account.status().state, SyncState::Online)
    })
    .await;
    assert_eq!(
        core.store.account_owner(ZONE).unwrap().as_deref(),
        Some("dev@example.com")
    );
    // A change the first account never received, kept when signing out.
    a.stand_in().zones().set_online(false);
    set(&core, "preview_appearance", json!("light"));
    a.sign_out(false).await.unwrap();
    assert!(core.store.account_outbox_len(ZONE).unwrap() > 0);
    let pushes = a.stand_in().zones().pushes().len();
    a.stand_in().zones().set_online(true);
    a.sign_in("delete@example.com", pw(PASSWORD)).await.unwrap();
    assert_eq!(
        a.view("macos").await.connect,
        AccountConnectState::Needed,
        "a new account asks"
    );
    assert_eq!(
        core.store.account_outbox_len(ZONE).unwrap(),
        0,
        "the first account\'s changes are gone"
    );
    assert!(core.store.account_record(ZONE).unwrap().is_empty());
    assert_eq!(
        a.stand_in().zones().pushes().len(),
        pushes,
        "nothing was sent to the second account"
    );
    a.connect(FirstSync::Combine, None).await.unwrap();
    assert_eq!(
        core.store.account_owner(ZONE).unwrap().as_deref(),
        Some("delete@example.com")
    );
    // The person's own settings are untouched.
    assert_eq!(stored(&core, "preview_appearance"), json!("light"));
}

#[tokio::test]
async fn an_allowed_request_that_was_given_up_signs_nobody_in() {
    let dir = TempDir::new();
    let core = dev(&dir, "race");
    asking(&core, "late@example.com").await;
    let session = crate::account::SessionInfo {
        email: "late@example.com".into(),
        device_id: "dev-this".into(),
        device_name: "This computer".into(),
    };
    // The person cancelled just as the answer arrived.
    core.account.cancel_second_factor().await.unwrap();
    assert_eq!(
        core.account
            .finish_approval("req-mine", session)
            .await
            .unwrap(),
        None
    );
    let view = core.account.view("macos").await;
    assert!(view.session.is_none());
    assert_eq!(
        core.account.status().state,
        SyncState::SignedOut { reason: None }
    );
}

#[tokio::test]
async fn a_request_that_expired_was_answered_or_was_given_up_cannot_be_answered() {
    let dir = TempDir::new();
    let core = dev(&dir, "gone");
    let a = &core.account;
    a.sign_in("dev@example.com", pw(PASSWORD)).await.unwrap();
    let ask = || ConfirmFor::ApproveDevice("req-ipad".into());
    let token = || async {
        a.confirm(ask(), pw(PASSWORD)).await.unwrap();
        a.require_confirmed_last(ask()).await.unwrap()
    };
    // Answered once, it is gone.
    a.deny("req-ipad").await.unwrap();
    assert_eq!(
        code(a.approve(token().await, "req-ipad").await.unwrap_err()),
        "request_gone"
    );
    assert_eq!(code(a.deny("req-ipad").await.unwrap_err()), "request_gone");
    // Given up by the other device.
    let (_, stand_in) = (0, a.stand_in());
    stand_in.cancel_incoming("req-ipad");
    assert_eq!(
        code(a.approve(token().await, "req-ipad").await.unwrap_err()),
        "request_gone"
    );
    // Left for too long.
    let dir = TempDir::new();
    let core = dev(&dir, "gone-late");
    let a = &core.account;
    a.sign_in("dev@example.com", pw(PASSWORD)).await.unwrap();
    a.stand_in().set_incoming_life(Duration::from_millis(20));
    tokio::time::sleep(Duration::from_millis(40)).await;
    assert!(a.approval_requests().await.unwrap().is_empty());
    a.confirm(ask(), pw(PASSWORD)).await.unwrap();
    let late = a.require_confirmed_last(ask()).await.unwrap();
    assert_eq!(
        code(a.approve(late, "req-ipad").await.unwrap_err()),
        "request_gone"
    );
    assert!(a.stand_in().decided().is_empty(), "nothing was approved");
}

#[tokio::test]
async fn a_second_look_at_an_allowed_request_keeps_the_session_the_first_made() {
    let dir = TempDir::new();
    let core = dev(&dir, "twice");
    asking(&core, "late@example.com").await;
    core.account.stand_in().answer(Answer::Allow);
    let session = crate::account::SessionInfo {
        email: "late@example.com".into(),
        device_id: "dev-this".into(),
        device_name: "This computer".into(),
    };
    // Two looks both heard "allowed" from the Engine; the second arrives after the first signed in.
    core.account.poll_approval().await.unwrap();
    let first = core
        .account
        .submit_digits(typed(&shown(&core)))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.state, AccountApprovalState::Approved);
    assert_eq!(
        core.account
            .finish_approval("req-mine", session)
            .await
            .unwrap(),
        None
    );
    assert!(core.account.view("macos").await.session.is_some());
    assert!(
        core.account.stand_in().signed_in(),
        "the Engine session was not ended"
    );
    // And a second poll through the route does the same.
    assert_eq!(core.account.poll_approval().await.unwrap(), None);
    assert!(core.account.stand_in().signed_in());
}

#[tokio::test]
async fn the_same_account_in_another_case_keeps_its_record_and_unsent_changes() {
    let dir = TempDir::new();
    let core = with_settings(&dir, "case").await;
    let a = &core.account;
    a.sign_in("dev@example.com", pw(PASSWORD)).await.unwrap();
    a.connect(FirstSync::Combine, None).await.unwrap();
    until(&core, |c| {
        matches!(c.account.status().state, SyncState::Online)
    })
    .await;
    a.stand_in().zones().set_online(false);
    set(&core, "preview_appearance", json!("light"));
    a.sign_out(false).await.unwrap();
    let waiting = core.store.account_outbox_len(ZONE).unwrap();
    assert!(waiting > 0);
    // Signing in again with the address in another case is the same account; offline, so the
    // worker cannot upload and empty the outbox before it is looked at.
    a.sign_in(" Dev@Example.com ", pw(PASSWORD)).await.unwrap();
    assert_eq!(core.store.account_outbox_len(ZONE).unwrap(), waiting);
    assert!(!core.store.account_record(ZONE).unwrap().is_empty());
    let owner = core.store.account_owner(ZONE).unwrap().unwrap();
    assert!(owner.eq_ignore_ascii_case("dev@example.com"));
}

#[tokio::test]
async fn a_row_from_before_the_owner_was_recorded_is_claimed_not_dropped() {
    let dir = TempDir::new();
    let core = with_settings(&dir, "null-owner").await;
    let a = &core.account;
    a.sign_in("dev@example.com", pw(PASSWORD)).await.unwrap();
    a.connect(FirstSync::Combine, None).await.unwrap();
    until(&core, |c| {
        matches!(c.account.status().state, SyncState::Online)
    })
    .await;
    a.stand_in().zones().set_online(false);
    set(&core, "preview_appearance", json!("light"));
    a.sign_out(false).await.unwrap();
    core.store.account_forget_owner(ZONE).unwrap();
    let waiting = core.store.account_outbox_len(ZONE).unwrap();
    assert!(waiting > 0);
    a.stand_in().zones().set_online(true);
    a.sign_in("dev@example.com", pw(PASSWORD)).await.unwrap();
    assert_eq!(
        core.store.account_owner(ZONE).unwrap().as_deref(),
        Some("dev@example.com")
    );
    assert!(!core.store.account_record(ZONE).unwrap().is_empty());
}

#[tokio::test]
async fn an_ended_session_is_told_once_and_not_after_the_persons_own_sign_out_everywhere() {
    let dir = TempDir::new();
    let core = dev(&dir, "ended");
    let a = &core.account;
    assert!(!a.view("macos").await.ended_notice);
    a.sign_in("dev@example.com", pw(PASSWORD)).await.unwrap();
    // The service ends the session: the next authenticated call finds out.
    a.stand_in().sign_out_elsewhere();
    assert_eq!(code(a.devices().await.unwrap_err()), "session_ended");
    assert!(a.view("macos").await.ended_notice);
    // A new window, a remount or a reload sees the same, until it is dismissed once.
    assert!(a.view("macos").await.ended_notice);
    a.ended_seen().await.unwrap();
    assert!(!a.view("macos").await.ended_notice);
    // A new sign-in and a new ending are told again.
    a.sign_in("dev@example.com", pw(PASSWORD)).await.unwrap();
    a.stand_in().sign_out_elsewhere();
    assert_eq!(code(a.devices().await.unwrap_err()), "session_ended");
    assert!(a.view("macos").await.ended_notice);
    // Signing out everywhere is the person\'s own doing.
    a.sign_in("dev@example.com", pw(PASSWORD)).await.unwrap();
    a.confirm(ConfirmFor::SignOutEverywhere, pw(PASSWORD))
        .await
        .unwrap();
    let token = a
        .require_confirmed_last(ConfirmFor::SignOutEverywhere)
        .await
        .unwrap();
    a.sign_out_everywhere(token).await.unwrap();
    assert!(!a.view("macos").await.ended_notice);
}

#[tokio::test]
async fn dismissing_while_signed_in_does_not_hide_the_next_real_notice() {
    let dir = TempDir::new();
    let core = dev(&dir, "early-dismiss");
    let a = &core.account;
    a.sign_in("dev@example.com", pw(PASSWORD)).await.unwrap();
    // A stale tab or an engine calls it early, while nothing has ended.
    a.ended_seen().await.unwrap();
    a.stand_in().sign_out_elsewhere();
    assert_eq!(code(a.devices().await.unwrap_err()), "session_ended");
    assert!(
        a.view("macos").await.ended_notice,
        "the real notice is still told"
    );
    a.ended_seen().await.unwrap();
    assert!(!a.view("macos").await.ended_notice);
}

#[tokio::test]
async fn signing_up_on_a_computer_with_settings_asks_how_to_join_them() {
    let dir = TempDir::new();
    let core = with_settings(&dir, "signup-asks").await;
    let a = &core.account;
    a.start_sign_up(
        crate::account::dev::INVITE,
        "fresh@example.com",
        pw("a fresh throwaway phrase"),
    )
    .await
    .unwrap();
    a.verify_email_code(crate::account::dev::EMAIL_CODE)
        .await
        .unwrap();
    let view = a.view("macos").await;
    assert_eq!(view.session.unwrap().email, "fresh@example.com");
    assert_eq!(view.connect, AccountConnectState::Needed);
    assert_eq!(a.status().state, SyncState::NotConnected);
    assert!(a.stand_in().zones().zone(ZONE).is_none());
}
