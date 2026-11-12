//! A person's one-time answer to a held command, through a real `Core`, the real gate and a played desktop shell (the
//! presence messages of `autopilot::presence`). The lead says no, so a command is denied and held; the person then approves it
//! once, denies it or dismisses it. Values that stand for a secret are built at run time and named by label.

use orbyn_contracts::{ActionKind, AutopilotHeldState as State, StopReason};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::autopilot_support::{Scripted, approvers, ask, events, kinds, request, running, scope};
use super::harness::{KEY_MARKER, World, world};
use crate::crew::{APPROVAL_LIFETIME_SECS, HELD_LIFETIME_SECS, Mode, ModeScope};
use crate::error::CoreError;
use crate::governance::approvals::Verdict;
use crate::host::presence::PRESENCE_LIFETIME_SECS;
use crate::testing::run_git;
use crate::util::unix_now;

/// Plays the desktop shell: sends one line the way the shell does and returns the service's answer.
fn shell_says(w: &World, line: &Value, now: i64) -> Value {
    let (lines, mut answers) = mpsc::unbounded_channel();
    w.core.shell.attach(lines);
    assert!(w.core.presence_line_at(&line.to_string(), now));
    answers.try_recv().expect("the service answered")
}

fn describe(w: &World, hold: &str, now: i64) -> Value {
    shell_says(
        w,
        &json!({ "type": "presence_describe", "request_id": "r-1", "hold_id": hold }),
        now,
    )
}

const NONCE: &str = "0123456789abcdef0123456789abcdef";

/// The whole of what the shell does after a click on Approve: describe, then grant what was described. Returns the nonce.
fn grant_at(w: &World, hold: &str, nonce: &str, now: i64) -> String {
    let shown = describe(w, hold, now);
    assert_eq!(shown["type"], "presence_hold", "{shown}");
    let ready = shell_says(
        w,
        &json!({
            "type": "presence_grant", "request_id": "r-2", "nonce": nonce, "hold_id": hold,
            "thread_id": shown["thread_id"], "task_id": shown["task_id"],
            "command_sha256": shown["command_sha256"],
        }),
        now,
    );
    assert_eq!(
        ready,
        json!({ "type": "presence_ready", "request_id": "r-2" })
    );
    nonce.to_owned()
}

fn grant_for(w: &World, hold: &str) -> String {
    grant_at(w, hold, NONCE, unix_now())
}

/// A running task whose lead says no to everything, with a desktop shell attached.
async fn held_world() -> (World, String) {
    let (w, id) = running().await;
    let lead = Scripted::no("alpha", "alpha");
    w.core.set_crew_approvers(approvers(&lead, &[]));
    // The channel stays open for the whole test, so the service has a shell to answer to.
    let (lines, answers) = mpsc::unbounded_channel();
    w.core.shell.attach(lines);
    std::mem::forget(answers);
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
    let nonce = grant_for(&w, &hold);

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
    assert_eq!(approved_row.body["via"], "desktop_presence");
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
        let nonce = grant_for(&w, &hold);
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
async fn an_approval_needs_this_holds_own_unused_grant() {
    let (w, id) = held_world().await;
    let hold = hold_of(&w, &id, "make deploy").await;
    let other = hold_of(&w, &id, "make ship").await;

    // No nonce at all: a caller with only the service token. A wrong one, and one that nothing granted.
    let none = w.core.autopilot_approve_once(&hold, None).await;
    assert!(matches!(none, Err(CoreError::PresenceRequired)));
    let wrong = w.core.autopilot_approve_once(&hold, Some("0000")).await;
    assert!(matches!(wrong, Err(CoreError::PresenceRequired)));
    let unknown = w
        .core
        .autopilot_approve_once("hold-nope", Some(NONCE))
        .await;
    assert!(matches!(unknown, Err(CoreError::PresenceRequired)));

    // A grant made for another hold does not cover this one.
    let for_other = grant_for(&w, &other);
    let swapped = w.core.autopilot_approve_once(&hold, Some(&for_other)).await;
    assert!(matches!(swapped, Err(CoreError::PresenceRequired)));
    assert_eq!(state_of(&w, &hold), State::Held, "nothing was approved");

    // A wrong nonce does not burn the grant: the right one still works afterwards.
    let nonce = grant_for(&w, &hold);
    let wrong = w
        .core
        .autopilot_approve_once(&hold, Some("fedcba9876543210fedcba9876543210"))
        .await;
    assert!(matches!(wrong, Err(CoreError::PresenceRequired)));
    w.core
        .autopilot_approve_once(&hold, Some(&nonce))
        .await
        .unwrap();

    // Used up by its first use: a replay with the same nonce fails, and so does a fresh grant on a decided hold.
    let replay = w.core.autopilot_approve_once(&hold, Some(&nonce)).await;
    assert!(matches!(replay, Err(CoreError::PresenceRequired)));
    let refused = describe(&w, &hold, unix_now());
    assert_eq!(refused["type"], "presence_refused");
    assert_eq!(refused["code"], "gone");
    let twice = w.core.autopilot_approve_once(&hold, Some(NONCE)).await;
    assert!(matches!(twice, Err(CoreError::PresenceRequired)));
}

#[tokio::test]
async fn the_shell_is_told_what_the_dialog_shows_and_nothing_from_the_window() {
    let (w, id) = held_world().await;
    let command = format!("make deploy -- echo {KEY_MARKER}");
    request(&w, &id, ActionKind::Command, &command).await;
    let hold = w.core.autopilot_held().unwrap().items.remove(0);
    let shown = describe(&w, &hold.id, unix_now());
    assert_eq!(shown["type"], "presence_hold");
    assert_eq!(shown["request_id"], "r-1");
    assert_eq!(shown["hold_id"], hold.id);
    assert_eq!(shown["task_id"], id);
    assert_eq!(shown["thread_id"], w.worker(&id));
    assert_eq!(shown["command_shown"], hold.command);
    assert!(
        !shown["command_shown"]
            .as_str()
            .unwrap()
            .contains(KEY_MARKER)
    );
    assert_eq!(shown["command_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(shown["task_title"], "Add a parse_duration function.");
    let run = w.core.store.crew_run(&id).unwrap().unwrap();
    assert_eq!(shown["worker_name"], run.member);
    assert!(!shown.to_string().contains(KEY_MARKER));
    // A line that is not for this side is left for the next reader, and a request id the shell cannot have made is dropped.
    assert!(
        !w.core
            .presence_line_at(r#"{"type":"open_link"}"#, unix_now())
    );
    assert!(!w.core.presence_line_at("not json", unix_now()));
    assert!(
        w.core.presence_line_at(
            &json!({ "type": "presence_describe", "request_id": "bad id!", "hold_id": hold.id })
                .to_string(),
            unix_now()
        )
    );
}

#[tokio::test]
async fn a_grant_is_recorded_only_for_the_hold_as_it_stands_now() {
    let (w, id) = held_world().await;
    let hold = hold_of(&w, &id, "make deploy").await;
    let shown = describe(&w, &hold, unix_now());
    let grant = |what: Value, now: i64| {
        let mut line = json!({
            "type": "presence_grant", "request_id": "r-2", "nonce": NONCE, "hold_id": hold,
            "thread_id": shown["thread_id"], "task_id": shown["task_id"],
            "command_sha256": shown["command_sha256"],
        });
        line.as_object_mut()
            .unwrap()
            .extend(what.as_object().unwrap().clone());
        shell_says(&w, &line, now)
    };
    let code = |answer: Value| {
        assert_eq!(answer["type"], "presence_refused", "{answer}");
        answer["code"].as_str().unwrap().to_owned()
    };
    // Another command's hash, another thread, another task, another hold: nothing is recorded.
    assert_eq!(
        code(grant(
            json!({ "command_sha256": "0".repeat(64) }),
            unix_now()
        )),
        "mismatch"
    );
    assert_eq!(
        code(grant(json!({ "thread_id": "another-thread" }), unix_now())),
        "mismatch"
    );
    assert_eq!(
        code(grant(json!({ "task_id": "another-task" }), unix_now())),
        "mismatch"
    );
    assert_eq!(
        code(grant(json!({ "hold_id": "hold-nope" }), unix_now())),
        "not_found"
    );
    assert_eq!(
        code(grant(json!({ "hold_id": "../x" }), unix_now())),
        "invalid"
    );
    assert_eq!(
        code(grant(json!({ "nonce": "short" }), unix_now())),
        "invalid"
    );
    assert_eq!(
        code(grant(json!({ "nonce": NONCE.to_uppercase() }), unix_now())),
        "invalid"
    );
    assert!(matches!(
        w.core.autopilot_approve_once(&hold, Some(NONCE)).await,
        Err(CoreError::PresenceRequired)
    ));
    assert_eq!(w.core.presence.len(), 0);

    // The hold ends before the person clicks: the grant is refused, and the end of the task is the same.
    w.core.autopilot_dismiss_held(&hold).unwrap();
    assert_eq!(code(grant(json!({}), unix_now())), "gone");
    assert_eq!(w.core.presence.len(), 0);
    let hold2 = hold_of(&w, &id, "make ship").await;
    let task = w.core.store.crew_task(&id).unwrap().unwrap();
    w.core.autopilot_stop(
        &task,
        StopReason::TooManyDenials,
        "too_many_denials",
        "ended",
    );
    assert_eq!(describe(&w, &hold2, unix_now())["code"], "gone");
}

#[tokio::test]
async fn a_second_grant_replaces_the_first_and_a_grant_runs_out() {
    let (w, id) = held_world().await;
    let hold = hold_of(&w, &id, "make deploy").await;
    let first = grant_for(&w, &hold);
    let second = grant_at(&w, &hold, "fedcba9876543210fedcba9876543210", unix_now());
    assert_eq!(w.core.presence.len(), 1);
    let old = w.core.autopilot_approve_once(&hold, Some(&first)).await;
    assert!(
        matches!(old, Err(CoreError::PresenceRequired)),
        "the older grant is gone"
    );

    // Sixty seconds on, the grant has run out; the hold itself is still open.
    let late = unix_now() + PRESENCE_LIFETIME_SECS;
    let expired = w
        .core
        .autopilot_approve_once_at(&hold, Some(&second), late)
        .await;
    assert!(matches!(expired, Err(CoreError::PresenceRequired)));
    assert_eq!(state_of(&w, &hold), State::Held);
    let live = w
        .core
        .autopilot_approve_once_at(
            &hold,
            Some(&second),
            unix_now() + PRESENCE_LIFETIME_SECS - 2,
        )
        .await
        .unwrap();
    assert_eq!(live.state, State::Approved);
}

#[tokio::test]
async fn without_a_desktop_shell_nothing_can_be_approved() {
    let (w, id) = running().await;
    let lead = Scripted::no("alpha", "alpha");
    w.core.set_crew_approvers(approvers(&lead, &[]));
    let hold = hold_of(&w, &id, "make deploy").await;
    for nonce in [None, Some(NONCE)] {
        let refused = w.core.autopilot_approve_once(&hold, nonce).await;
        assert!(
            matches!(refused, Err(CoreError::DesktopRequired)),
            "{nonce:?}"
        );
    }
    assert_eq!(state_of(&w, &hold), State::Held);
    assert_eq!(CoreError::DesktopRequired.code(), "desktop_required");
    assert_eq!(CoreError::PresenceRequired.code(), "presence_required");
    assert_eq!(CoreError::DesktopRequired.status(), 403);
    assert_eq!(CoreError::PresenceRequired.status(), 403);
}

#[tokio::test]
async fn a_hold_and_an_approval_expire() {
    let (w, id) = held_world().await;
    let hold = hold_of(&w, &id, "make deploy").await;
    let late = unix_now() + HELD_LIFETIME_SECS + 1;
    // The grant was made just before the hold ran out, so it is still live when the hold is over: the answer is `gone`.
    let nonce = grant_at(&w, &hold, NONCE, late - 2);
    let gone = w
        .core
        .autopilot_approve_once_at(&hold, Some(&nonce), late)
        .await;
    assert!(matches!(gone, Err(CoreError::Gone(_))));
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
    assert_eq!(
        w.core.presence.len(),
        1,
        "a refusal does not use the grant up"
    );
    assert_eq!(events(&w, &id, "hold_ended").len(), 1);

    // An approval whose window has passed lets nothing through: it was made, by the clock, long ago.
    let hold = hold_of(&w, &id, "make ship").await;
    let past = unix_now() - APPROVAL_LIFETIME_SECS - 5;
    let nonce = grant_at(&w, &hold, NONCE, past);
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
    let nonce = grant_for(&w, &hold);
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
    let nonce = grant_for(&w, &hold);
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

    // A yes from the lead does not make an approval either: only the person's call does, and it needs the desktop app's grant.
    let (w, id) = running().await;
    let lead = Scripted::no("alpha", "alpha");
    w.core.set_crew_approvers(approvers(&lead, &[]));
    let (lines, answers) = mpsc::unbounded_channel();
    w.core.shell.attach(lines);
    std::mem::forget(answers);
    let hold = hold_of(&w, &id, "make deploy").await;
    let lead = Scripted::yes("alpha", "alpha");
    w.core.set_crew_approvers(approvers(&lead, &[]));
    let refused = w.core.autopilot_approve_once(&hold, None).await;
    assert!(matches!(refused, Err(CoreError::PresenceRequired)));
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
    // A person who clicked Approve before the deny lost the race: the hold is decided and the grant is not used.
    let (w, id) = held_world().await;
    let hold = hold_of(&w, &id, "make ship").await;
    let nonce = grant_for(&w, &hold);
    w.core.autopilot_deny_held(&hold).unwrap();
    assert!(matches!(
        w.core.autopilot_approve_once(&hold, Some(&nonce)).await,
        Err(CoreError::InvalidTransition { .. })
    ));
    assert_eq!(w.core.presence.len(), 1);
    assert_eq!(state_of(&w, &hold), State::Denied);
}

#[tokio::test]
async fn a_deny_after_an_approval_loses_and_is_one_row() {
    let (w, id) = held_world().await;
    let hold = hold_of(&w, &id, "make deploy").await;
    let nonce = grant_for(&w, &hold);
    w.core
        .autopilot_approve_once(&hold, Some(&nonce))
        .await
        .unwrap();
    assert!(matches!(
        w.core.autopilot_deny_held(&hold),
        Err(CoreError::InvalidTransition { .. })
    ));
    assert_eq!(state_of(&w, &hold), State::Approved);
    assert_eq!(events(&w, &id, "denied_by_person").len(), 0);
    assert_eq!(events(&w, &id, "approved_once").len(), 1);
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
        let nonce = grant_for(&w, &hold);
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
    let nonce = grant_for(&w, &item.id);
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
    // The dialog is never offered it, and no grant can be made for it.
    let refused = describe(&w, &item.id, unix_now());
    assert_eq!(refused["code"], "unreadable");
    let refused = w.core.autopilot_approve_once(&item.id, Some(NONCE)).await;
    assert!(matches!(refused, Err(CoreError::PresenceRequired)));
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
async fn the_dialog_text_has_no_newline_control_or_bidi_character() {
    let (w, id) = held_world().await;
    let command = "make deploy\u{202E}\n\u{200B}check\u{2028}\u{3164}x";
    // The always-denied list may well read such a command itself; the hold is made directly, as the chain's denial would.
    let task = w.core.store.crew_task(&id).unwrap().unwrap();
    let hold = w
        .core
        .held_make(
            &task,
            &w.worker(&id),
            &ask(ActionKind::Command, command, &[]),
            "The lead said no.",
            &[],
            unix_now(),
        )
        .expect("held");
    let shown = describe(&w, &hold, unix_now());
    assert_eq!(shown["type"], "presence_hold", "{shown}");
    let text = shown["command_shown"].as_str().unwrap();
    assert!(
        text.contains('\u{23CE}') && text.contains('\u{FFFD}'),
        "{text:?}"
    );
    for field in ["command_shown", "worker_name", "task_title"] {
        let value = shown[field].as_str().unwrap();
        assert!(
            !value
                .chars()
                .any(|c| c.is_control() || ('\u{200B}'..='\u{202E}').contains(&c)),
            "{field}: {value:?}"
        );
    }
    assert_eq!(shown["truncated"], false);
    // The hash is of the real command, not of what is shown.
    assert_eq!(shown["command_sha256"].as_str().unwrap().len(), 64);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_approvals_with_the_same_nonce_racing_have_exactly_one_success() {
    for round in 0..10 {
        let (w, id) = held_world().await;
        let hold = hold_of(&w, &id, &format!("make step-{round}")).await;
        let nonce = grant_for(&w, &hold);
        let mut tasks = Vec::new();
        for _ in 0..4 {
            let (core, hold, nonce) = (w.core.clone(), hold.clone(), nonce.clone());
            tasks.push(tokio::spawn(async move {
                core.autopilot_approve_once(&hold, Some(&nonce)).await
            }));
        }
        let mut wins = 0;
        for task in tasks {
            match task.await.unwrap() {
                Ok(_) => wins += 1,
                Err(CoreError::PresenceRequired | CoreError::InvalidTransition { .. }) => {}
                Err(other) => panic!("{other:?}"),
            }
        }
        assert_eq!(wins, 1, "round {round}");
        assert_eq!(events(&w, &id, "approved_once").len(), 1);
        assert_eq!(w.core.presence.len(), 0);
    }
}
