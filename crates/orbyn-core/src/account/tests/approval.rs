//! A new device signing in with a trusted device's help, and joining this computer's settings
//! to the account's the first time. The stand-in plays the account server; `answer` plays the
//! trusted device. Nothing here touches a network.

use std::time::Duration;

use orbyn_contracts::{AccountApprovalState, AccountConnectState, AccountSignInOutcome};
use serde_json::json;

use super::{dev, set, stored, until};
use crate::CoreError;
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

#[tokio::test]
async fn a_new_device_shows_the_digits_and_waits_without_a_session() {
    let dir = TempDir::new();
    let core = dev(&dir, "waits");
    asking(&core, "late@example.com").await;
    let view = core.account.view("macos").await;
    assert!(view.session.is_none());
    let approval = view.approval.unwrap();
    assert_eq!(approval.state, AccountApprovalState::Waiting);
    assert_eq!(approval.digits, "418 207");
    assert!(approval.expires_in_secs > 590 && approval.expires_in_secs <= 600);
    let polled = core.account.poll_approval().await.unwrap().unwrap();
    assert_eq!(polled.state, AccountApprovalState::Waiting);
    assert!(core.account.view("macos").await.session.is_none());
}

#[tokio::test]
async fn when_a_trusted_device_allows_it_the_new_device_signs_in() {
    let dir = TempDir::new();
    let core = dev(&dir, "allowed");
    asking(&core, "late@example.com").await;
    core.account.stand_in().answer(Answer::Allow);
    let shown = core.account.poll_approval().await.unwrap().unwrap();
    assert_eq!(shown.state, AccountApprovalState::Approved);
    let view = core.account.view("macos").await;
    assert_eq!(view.session.unwrap().email, "late@example.com");
    assert!(
        view.approval.is_none(),
        "the request is over once it is allowed"
    );
    assert_eq!(core.account.poll_approval().await.unwrap(), None);
}

#[tokio::test]
async fn a_refusal_and_a_mismatch_close_the_request_and_sign_nobody_in() {
    for (answer, state) in [
        (Answer::Deny, AccountApprovalState::Denied),
        (Answer::CodesDiffer, AccountApprovalState::CodesDiffer),
    ] {
        let dir = TempDir::new();
        let core = dev(&dir, "closed");
        asking(&core, "late@example.com").await;
        core.account.stand_in().answer(answer);
        let shown = core.account.poll_approval().await.unwrap().unwrap();
        assert_eq!(shown.state, state);
        // It stays closed on every later look, and the screen can say why until the person leaves it.
        assert_eq!(
            core.account.poll_approval().await.unwrap().unwrap().state,
            state
        );
        let view = core.account.view("macos").await;
        assert_eq!(view.approval.unwrap().state, state);
        assert!(view.session.is_none());
        assert_eq!(
            core.account.status().state,
            SyncState::SignedOut { reason: None }
        );
        // Leaving clears it; asking again makes a new request.
        core.account.cancel_second_factor().await.unwrap();
        assert!(core.account.view("macos").await.approval.is_none());
    }
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
    assert_eq!(list[0].digits, "418 207");
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
    a.approve(token, "req-ipad").await.unwrap();
    assert!(a.approval_requests().await.unwrap().is_empty());
    assert_eq!(
        a.stand_in().decided(),
        vec![("req-ipad".to_owned(), Some(true))]
    );
    // Used up on first use.
    assert_eq!(
        code(a.require_confirmed_last(ask()).await.unwrap_err()),
        "reauth_required"
    );
}

#[tokio::test]
async fn refusing_needs_no_confirmation_and_says_whether_the_codes_differed() {
    let dir = TempDir::new();
    let core = dev(&dir, "refuse");
    let a = &core.account;
    assert_eq!(
        code(a.deny("req-ipad", false).await.unwrap_err()),
        "reauth_required"
    );
    a.sign_in("dev@example.com", pw(PASSWORD)).await.unwrap();
    a.deny("req-ipad", true).await.unwrap();
    assert_eq!(
        a.stand_in().decided(),
        vec![("req-ipad".to_owned(), Some(false))]
    );
    assert!(a.approval_requests().await.unwrap().is_empty());
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
    a.deny("req-ipad", false).await.unwrap();
    assert_eq!(
        code(a.approve(token().await, "req-ipad").await.unwrap_err()),
        "request_gone"
    );
    assert_eq!(
        code(a.deny("req-ipad", false).await.unwrap_err()),
        "request_gone"
    );
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
    let first = core.account.poll_approval().await.unwrap().unwrap();
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
