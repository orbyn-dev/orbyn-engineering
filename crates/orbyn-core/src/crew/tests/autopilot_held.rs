//! A person's one-time answer to a held command, through a real `Core`, the real gate and the account's "Confirm it's you"
//! (the development stand-in for the Engine). The lead says no, so a command is denied and held; the person then approves it
//! once, denies it or dismisses it. Values that stand for a secret are built at run time and named by label.

use orbyn_contracts::{ActionKind, AutopilotHeldState as State, StopReason};

use super::autopilot_support::{Scripted, approvers, ask, events, kinds, request, running, scope};
use super::harness::{KEY_MARKER, World, world};
use crate::account::{ConfirmFor, Password};
use crate::crew::{APPROVAL_LIFETIME_SECS, CONFIRM_PREFIX, HELD_LIFETIME_SECS, Mode, ModeScope};
use crate::error::CoreError;
use crate::governance::approvals::Verdict;
use crate::testing::run_git;
use crate::util::unix_now;

/// The development account's password, assembled so no literal sits in the source.
fn account_password() -> String {
    ["correct", "horse", "battery"].join(" ")
}

async fn signed_in(w: &World) {
    // A private account, not one of the stand-in's seeded ones, whose password is public.
    w.core
        .account
        .start_sign_up(
            "K7QM2XWH9QXA3C2D",
            "crew-test@example.org",
            Password::new(account_password()),
        )
        .await
        .unwrap();
    w.core.account.verify_email_code("ABCDEFGH").await.unwrap();
}

/// What the window does: "Confirm it's you" for this hold, answering with the nonce.
async fn nonce_for(w: &World, hold: &str) -> String {
    w.core
        .account
        .confirm(
            ConfirmFor::ApproveDevice(format!("{CONFIRM_PREFIX}{hold}")),
            Password::new(account_password()),
        )
        .await
        .unwrap()
}

/// A running task whose lead says no to everything, with the person signed in.
async fn held_world() -> (World, String) {
    let (w, id) = running().await;
    let lead = Scripted::no("alpha", "alpha");
    w.core.set_crew_approvers(approvers(&lead, &[]));
    signed_in(&w).await;
    (w, id)
}

async fn hold_of(w: &World, id: &str, command: &str) -> String {
    let verdict = request(w, id, ActionKind::Command, command).await;
    assert!(matches!(verdict, Verdict::Deny(_)), "the lead said no");
    let list = w.core.autopilot_held().unwrap();
    list.items
        .iter()
        .find(|item| item.task_id == id && item.command == command && item.state == State::Held)
        .map(|item| item.id.clone())
        .expect("the denied command is held")
}

fn state_of(w: &World, hold: &str) -> State {
    w.core
        .autopilot_held()
        .unwrap()
        .items
        .iter()
        .find(|item| item.id == hold)
        .unwrap()
        .state
}

#[tokio::test]
async fn a_denied_command_is_held_once_and_a_retry_does_not_add_another() {
    let (w, id) = held_world().await;
    let hold = hold_of(&w, &id, "make deploy").await;
    let again = request(&w, &id, ActionKind::Command, "make deploy").await;
    assert!(matches!(again, Verdict::Deny(_)));
    assert_eq!(w.core.autopilot_held().unwrap().items.len(), 1);
    let held = events(&w, &id, "held");
    assert_eq!(held.len(), 1);
    assert_eq!(held[0].body["hold"], hold);
    assert_eq!(held[0].body["decided_by"], "autopilot");
    assert!(
        held[0].body["bound"]
            .as_str()
            .unwrap()
            .starts_with("sha256:"),
        "the row names the bound hash"
    );
}

#[tokio::test]
async fn approving_once_lets_exactly_one_retry_of_the_same_command_through() {
    let (w, id) = held_world().await;
    let hold = hold_of(&w, &id, "make deploy").await;
    let nonce = nonce_for(&w, &hold).await;

    let approved = w
        .core
        .autopilot_approve_once(&hold, Some(&nonce))
        .await
        .unwrap();
    assert_eq!(approved.state, State::Approved);

    let verdict = request(&w, &id, ActionKind::Command, "make deploy").await;
    assert_eq!(verdict, Verdict::Allow, "the one retry");
    let replay = request(&w, &id, ActionKind::Command, "make deploy").await;
    assert!(matches!(replay, Verdict::Deny(_)), "single use");
    assert_eq!(state_of(&w, &hold), State::Used);

    let rows = kinds(&w, &id);
    for kind in ["held", "approved_once", "hold_used"] {
        assert!(rows.iter().any(|k| k == kind), "{kind}: {rows:?}");
    }
    let approved_row = &events(&w, &id, "approved_once")[0];
    assert_eq!(approved_row.body["decided_by"], "person");
    assert_eq!(approved_row.body["via"], "local_session");
    let allowed = events(&w, &id, "allowed");
    assert_eq!(allowed[0].body["decided_by"], "person");
    assert_eq!(allowed[0].body["hold"], hold);
}

/// The approval is bound to the command, its arguments and the thread. Each case has its own task: three denials in a row stop one.
#[tokio::test]
async fn an_approval_covers_only_the_exact_command_arguments_and_thread() {
    for case in ["command", "spacing", "arguments", "paths", "thread"] {
        let (w, id) = held_world().await;
        let hold = hold_of(&w, &id, "make deploy").await;
        let nonce = nonce_for(&w, &hold).await;
        w.core
            .autopilot_approve_once(&hold, Some(&nonce))
            .await
            .unwrap();
        let mut other = ask(ActionKind::Command, "make deploy", &[]);
        let mut from = scope(&w, &id);
        match case {
            "command" => other.target = "make deploy now".into(),
            "spacing" => other.target = "make  deploy".into(),
            "arguments" => other.detail = serde_json::json!({ "args": ["--prod"] }),
            "paths" => other.paths = vec!["src/a.rs".into()],
            _ => from.thread_id = "another-thread".into(),
        }
        let verdict = w.core.gate.request(&from, other).await;
        assert!(matches!(verdict, Verdict::Deny(_)), "{case}");
        assert_eq!(state_of(&w, &hold), State::Approved, "{case}: still unused");
        let exact = request(&w, &id, ActionKind::Command, "make deploy").await;
        assert_eq!(
            exact,
            Verdict::Allow,
            "{case}: the exact request still goes"
        );
    }
}

#[tokio::test]
async fn an_approval_needs_this_holds_own_unused_confirmation() {
    let (w, id) = held_world().await;
    let hold = hold_of(&w, &id, "make deploy").await;
    let other = hold_of(&w, &id, "make ship").await;

    // No nonce at all: a caller with only the service token.
    let none = w.core.autopilot_approve_once(&hold, None).await;
    assert!(matches!(none, Err(CoreError::ReauthRequired)));
    let wrong = w.core.autopilot_approve_once(&hold, Some("0000")).await;
    assert!(matches!(wrong, Err(CoreError::ReauthRequired)));

    // A confirmation made for another hold does not cover this one.
    let for_other = nonce_for(&w, &other).await;
    let swapped = w.core.autopilot_approve_once(&hold, Some(&for_other)).await;
    assert!(matches!(swapped, Err(CoreError::ReauthRequired)));

    // A confirmation for something else in the account does not either.
    let device = w
        .core
        .account
        .confirm(
            ConfirmFor::RemoveDevice(hold.clone()),
            Password::new(account_password()),
        )
        .await
        .unwrap();
    let wrong_action = w.core.autopilot_approve_once(&hold, Some(&device)).await;
    assert!(matches!(wrong_action, Err(CoreError::ReauthRequired)));
    assert_eq!(state_of(&w, &hold), State::Held, "nothing was approved");

    // Used up by its first use: a replay with the same nonce fails.
    let nonce = nonce_for(&w, &hold).await;
    w.core
        .autopilot_approve_once(&hold, Some(&nonce))
        .await
        .unwrap();
    let replay = w.core.autopilot_approve_once(&hold, Some(&nonce)).await;
    assert!(matches!(replay, Err(CoreError::ReauthRequired)));
    // And a fresh confirmation cannot approve what is already decided.
    let fresh = nonce_for(&w, &hold).await;
    let twice = w.core.autopilot_approve_once(&hold, Some(&fresh)).await;
    assert!(matches!(twice, Err(CoreError::InvalidTransition { .. })));

    // Signed out, nothing can be approved.
    let third = nonce_for(&w, &other).await;
    w.core.account.forget_confirmation();
    let gone = w.core.autopilot_approve_once(&other, Some(&third)).await;
    assert!(matches!(gone, Err(CoreError::ReauthRequired)));
}

#[tokio::test]
async fn nothing_can_be_approved_without_an_account() {
    let (w, id) = running().await;
    let lead = Scripted::no("alpha", "alpha");
    w.core.set_crew_approvers(approvers(&lead, &[]));
    let hold = hold_of(&w, &id, "make deploy").await;
    let refused = w.core.autopilot_approve_once(&hold, Some("nonce")).await;
    assert!(matches!(refused, Err(CoreError::ReauthRequired)));
}

#[tokio::test]
async fn a_hold_and_an_approval_expire() {
    let (w, id) = held_world().await;
    let hold = hold_of(&w, &id, "make deploy").await;
    let late = unix_now() + HELD_LIFETIME_SECS + 1;
    assert_eq!(
        w.core
            .autopilot_held_at(late)
            .unwrap()
            .items
            .iter()
            .find(|i| i.id == hold)
            .unwrap()
            .state,
        State::Expired
    );
    let nonce = nonce_for(&w, &hold).await;
    let gone = w
        .core
        .autopilot_approve_once_at(&hold, Some(&nonce), late)
        .await;
    assert!(matches!(gone, Err(CoreError::Gone(_))));
    assert_eq!(events(&w, &id, "hold_ended").len(), 1);

    // An approval whose window has passed lets nothing through: it was made, by the clock, long ago.
    let hold = hold_of(&w, &id, "make ship").await;
    let nonce = nonce_for(&w, &hold).await;
    let past = unix_now() - APPROVAL_LIFETIME_SECS - 5;
    w.core
        .autopilot_approve_once_at(&hold, Some(&nonce), past)
        .await
        .unwrap();
    let verdict = request(&w, &id, ActionKind::Command, "make ship").await;
    assert!(matches!(verdict, Verdict::Deny(_)), "the approval ran out");
}

#[tokio::test]
async fn a_held_command_of_an_ended_task_is_gone() {
    let (w, id) = held_world().await;
    let hold = hold_of(&w, &id, "make deploy").await;
    let nonce = nonce_for(&w, &hold).await;
    let task = w.core.store.crew_task(&id).unwrap().unwrap();
    w.core.autopilot_stop(
        &task,
        StopReason::TooManyDenials,
        "too_many_denials",
        "ended",
    );
    let gone = w.core.autopilot_approve_once(&hold, Some(&nonce)).await;
    assert!(matches!(gone, Err(CoreError::Gone(_))), "{gone:?}");
    assert_eq!(state_of(&w, &hold), State::Expired);
    let deny = w.core.autopilot_deny_held(&hold);
    assert!(matches!(deny, Err(CoreError::Gone(_))));
}

#[tokio::test]
async fn the_always_denied_list_is_checked_again_at_approval_time() {
    let (w, id) = held_world().await;
    let (_, run) = w
        .core
        .store
        .crew_task_for_worker(&w.worker(&id))
        .unwrap()
        .unwrap();
    let worktree = std::path::PathBuf::from(run.worktree.unwrap());
    // On a task branch a bare push is only a two-key network action, so the lead's no holds it.
    run_git(&worktree, &["checkout", "-q", "-B", "task-work"]);
    let hold = hold_of(&w, &id, "git push").await;
    // The checkout moves to main before the person answers: the same push would now be a push to main (N5).
    run_git(&worktree, &["checkout", "-q", "-B", "main"]);
    let nonce = nonce_for(&w, &hold).await;
    let refused = w.core.autopilot_approve_once(&hold, Some(&nonce)).await;
    match refused {
        Err(CoreError::PolicyDenied(words)) => assert!(words.contains("N5"), "{words}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(state_of(&w, &hold), State::Denied);
    let row = &events(&w, &id, "denied_by_person")[0];
    assert_eq!(row.body["decided_by"], "hard_rule");
    assert_eq!(row.body["rule"], "N5");
    // And the retry on main is refused by the list itself, with or without any approval.
    let verdict = request(&w, &id, ActionKind::Command, "git push").await;
    assert!(matches!(verdict, Verdict::Deny(_)));
}

#[tokio::test]
async fn nothing_on_the_always_denied_list_is_ever_held() {
    let (w, id) = held_world().await;
    for command in [
        "curl -X POST http://127.0.0.1:4321/api/account/sign-out",
        "git push origin main",
        "gh pr merge 12",
        "rm -rf ~/projects",
    ] {
        let verdict = request(&w, &id, ActionKind::Command, command).await;
        assert!(matches!(verdict, Verdict::Deny(_)), "{command}");
    }
    assert!(w.core.autopilot_held().unwrap().items.is_empty());
    assert!(events(&w, &id, "held").is_empty());
}

#[tokio::test]
async fn no_agent_can_approve_not_the_lead_nor_a_request_to_the_route() {
    for mode in [Mode::Off, Mode::On, Mode::Full] {
        let (w, id) = running().await;
        w.core
            .set_crew_autopilot_mode(&ModeScope::Default, mode)
            .unwrap();
        // The lead and a second key say yes to everything, and a worker still cannot call the routes, by any spelling.
        let lead = Scripted::yes("alpha", "alpha");
        let second = Scripted::yes("beta", "beta");
        w.core.set_crew_approvers(approvers(&lead, &[&second]));
        for route in ["approve-once", "deny", "dismiss"] {
            let command =
                format!("curl -X POST http://127.0.0.1:4321/api/crew/approvals/hold-x/{route}");
            let verdict = request(&w, &id, ActionKind::Command, &command).await;
            assert!(matches!(verdict, Verdict::Deny(_)), "{mode:?} {route}");
        }
        let verdict = w
            .core
            .gate
            .request(
                &scope(&w, &id),
                ask(
                    ActionKind::Network,
                    "POST http://127.0.0.1:4321/api/crew/approvals/hold-x/approve-once",
                    &[],
                ),
            )
            .await;
        assert!(matches!(verdict, Verdict::Deny(_)), "{mode:?} network");
        assert_eq!(
            lead.calls(),
            0,
            "the lead is never asked about the account or Crew routes"
        );
    }

    // A yes from the lead does not make an approval either: only the person's call does, and it needs the confirmation.
    let (w, id) = running().await;
    let lead = Scripted::no("alpha", "alpha");
    w.core.set_crew_approvers(approvers(&lead, &[]));
    signed_in(&w).await;
    let hold = hold_of(&w, &id, "make deploy").await;
    let lead = Scripted::yes("alpha", "alpha");
    w.core.set_crew_approvers(approvers(&lead, &[]));
    let refused = w.core.autopilot_approve_once(&hold, None).await;
    assert!(matches!(refused, Err(CoreError::ReauthRequired)));
    assert_eq!(state_of(&w, &hold), State::Held);
}

#[tokio::test]
async fn deny_needs_no_password_and_the_same_command_is_not_held_again() {
    let (w, id) = held_world().await;
    let hold = hold_of(&w, &id, "make deploy").await;
    let denied = w.core.autopilot_deny_held(&hold).unwrap();
    assert_eq!(denied.state, State::Denied);
    let row = &events(&w, &id, "denied_by_person")[0];
    assert_eq!(
        row.body["decided_by"], "local_caller",
        "no proof of who called"
    );
    assert_eq!(row.body["hold"], hold);
    // One decision each: a second answer is refused.
    assert!(matches!(
        w.core.autopilot_deny_held(&hold),
        Err(CoreError::InvalidTransition { .. })
    ));
    assert!(matches!(
        w.core.autopilot_dismiss_held(&hold),
        Err(CoreError::InvalidTransition { .. })
    ));
    // A retry stays denied and is not held again for this window.
    let verdict = request(&w, &id, ActionKind::Command, "make deploy").await;
    assert!(matches!(verdict, Verdict::Deny(_)));
    assert_eq!(w.core.autopilot_held().unwrap().items.len(), 1);
    let nonce = nonce_for(&w, &hold).await;
    assert!(matches!(
        w.core.autopilot_approve_once(&hold, Some(&nonce)).await,
        Err(CoreError::InvalidTransition { .. })
    ));
}

#[tokio::test]
async fn dismissing_is_logged_and_a_retry_may_be_held_again() {
    let (w, id) = held_world().await;
    let hold = hold_of(&w, &id, "make deploy").await;
    let dismissed = w.core.autopilot_dismiss_held(&hold).unwrap();
    assert_eq!(dismissed.state, State::Dismissed);
    assert_eq!(events(&w, &id, "dismissed_by_person").len(), 1);
    let again = hold_of(&w, &id, "make deploy").await;
    assert_ne!(again, hold);
    assert!(matches!(
        w.core.autopilot_deny_held("hold-missing"),
        Err(CoreError::NotFound(_))
    ));
    assert!(matches!(
        w.core.autopilot_deny_held("../../x"),
        Err(CoreError::Invalid(_))
    ));
}

#[tokio::test]
async fn a_racing_approve_and_deny_have_exactly_one_winner() {
    for round in 0..8 {
        let (w, id) = held_world().await;
        let hold = hold_of(&w, &id, &format!("make step-{round}")).await;
        let nonce = nonce_for(&w, &hold).await;
        let (approve, deny) =
            tokio::join!(w.core.autopilot_approve_once(&hold, Some(&nonce)), async {
                w.core.autopilot_deny_held(&hold)
            },);
        assert_eq!(
            usize::from(approve.is_ok()) + usize::from(deny.is_ok()),
            1,
            "round {round}"
        );
        let state = state_of(&w, &hold);
        assert_eq!(
            state,
            if approve.is_ok() {
                State::Approved
            } else {
                State::Denied
            }
        );
        let decided =
            events(&w, &id, "approved_once").len() + events(&w, &id, "denied_by_person").len();
        assert_eq!(decided, 1, "one ledger row for one decision");
    }
}

#[tokio::test]
async fn a_command_that_holds_a_secret_is_never_held_and_nothing_secret_is_stored() {
    let (w, id) = held_world().await;
    let token = ["abcdefgh", "12345678"].concat();
    let command = format!("make deploy token = {token}");
    let verdict = request(&w, &id, ActionKind::Command, &command).await;
    assert!(matches!(verdict, Verdict::Deny(_)));
    assert!(
        w.core.autopilot_held().unwrap().items.is_empty(),
        "a command with a secret in it is on the always-denied list, so it is not held"
    );
    for event in w.core.store.crew_autopilot_events(&id).unwrap() {
        assert!(
            !event.body.to_string().contains(&token),
            "a ledger row held the secret-shaped value ({})",
            event.kind
        );
    }
}

#[tokio::test]
async fn the_answer_keys_text_is_withheld_from_the_command_the_person_reads() {
    let (w, id) = held_world().await;
    let command = format!("make deploy -- echo {KEY_MARKER}");
    let verdict = request(&w, &id, ActionKind::Command, &command).await;
    assert!(matches!(verdict, Verdict::Deny(_)));
    let item = w.core.autopilot_held().unwrap().items.remove(0);
    assert!(
        !item.command.contains(KEY_MARKER),
        "the shown command held the key's text"
    );
    let nonce = nonce_for(&w, &item.id).await;
    w.core
        .autopilot_approve_once(&item.id, Some(&nonce))
        .await
        .unwrap();
    for event in w.core.store.crew_autopilot_events(&id).unwrap() {
        assert!(
            !event.body.to_string().contains(KEY_MARKER),
            "a ledger row held the key's text ({})",
            event.kind
        );
    }
    // The exact command, key text included, is what the retry must repeat.
    assert_eq!(
        request(&w, &id, ActionKind::Command, &command).await,
        Verdict::Allow
    );
}

#[tokio::test]
async fn a_command_too_long_to_read_is_held_but_cannot_be_approved() {
    let (w, id) = held_world().await;
    let long = format!("make {}", "a".repeat(crate::crew::SHOWN_MAX + 10));
    let verdict = request(&w, &id, ActionKind::Command, &long).await;
    assert!(matches!(verdict, Verdict::Deny(_)));
    let item = w.core.autopilot_held().unwrap().items.remove(0);
    assert!(!item.readable);
    let nonce = nonce_for(&w, &item.id).await;
    let refused = w.core.autopilot_approve_once(&item.id, Some(&nonce)).await;
    assert!(matches!(refused, Err(CoreError::Invalid(_))));
    assert_eq!(state_of(&w, &item.id), State::Held);
}

#[tokio::test]
async fn with_the_flag_off_none_of_it_exists() {
    let w = world(
        false,
        std::sync::Arc::new(|_, _| super::harness::Turn::Hang),
    )
    .await;
    assert!(matches!(
        w.core.autopilot_held(),
        Err(CoreError::NotFound(_))
    ));
    assert!(matches!(
        w.core.autopilot_approve_once("hold-x", Some("n")).await,
        Err(CoreError::NotFound(_))
    ));
    assert!(matches!(
        w.core.autopilot_deny_held("hold-x"),
        Err(CoreError::NotFound(_))
    ));
    assert!(matches!(
        w.core.autopilot_dismiss_held("hold-x"),
        Err(CoreError::NotFound(_))
    ));
}

#[tokio::test]
async fn approve_once_is_refused_while_a_seeded_stand_in_account_is_signed_in() {
    let (w, id) = running().await;
    let lead = Scripted::no("alpha", "alpha");
    w.core.set_crew_approvers(approvers(&lead, &[]));
    // The seeded account's password is public, so anything holding the token could confirm as it.
    let outcome = w
        .core
        .account
        .sign_in("dev@example.com", Password::new(account_password()))
        .await
        .unwrap();
    assert_eq!(outcome, orbyn_contracts::AccountSignInOutcome::Signed);
    let hold = hold_of(&w, &id, "make deploy").await;
    let nonce = nonce_for(&w, &hold).await;
    let refused = w.core.autopilot_approve_once(&hold, Some(&nonce)).await;
    match refused {
        Err(CoreError::PolicyDenied(words)) => assert!(words.contains("stand-in"), "{words}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(state_of(&w, &hold), State::Held);
}
