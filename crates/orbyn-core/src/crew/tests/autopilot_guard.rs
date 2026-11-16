//! The blast-radius guard through a real `Core`, the real gate, scripted approvers and a played desktop shell: a destructive command
//! against a production-linked target needs a second key even in Full, is held for the person under On and Off, and the person's
//! one-time approval covers exactly that command and leaves its exit status in the ledger. Fake engines only; nonces are drawn at
//! run time.

use orbyn_contracts::{
    ActionKind, AutopilotHeldState as State, GuardList, GuardTargetAdd, GuardTargetKind,
};
use serde_json::{Value, json};

use super::autopilot_held::{fresh_nonce, grant_for, held_world, hold_of, state_of};
use super::autopilot_support::{Scripted, approvers, events, kinds, request, running, words};
use super::harness::World;
use crate::agents::driver::ItemRecord;
use crate::crew::{Mode, ModeScope};
use crate::error::CoreError;
use crate::governance::approvals::Verdict;
use crate::host::presence::PresenceFor;
use crate::testing::run_git;
use crate::util::unix_now;

/// A destructive database command against a host that is not local and is on no list.
const DROP: &str = "psql -h db.prod.internal -c 'DROP TABLE users'";

fn mode(w: &World, mode: Mode) {
    w.core
        .set_crew_autopilot_mode(&ModeScope::Default, mode)
        .unwrap();
}

fn add(w: &World, list: GuardList, kind: GuardTargetKind, pattern: &str) {
    w.core
        .store
        .add_crew_guard_target(list, pattern, kind, None)
        .unwrap();
}

fn finished(command: &str, code: Option<i64>) -> ItemRecord {
    ItemRecord {
        id: "item-1".into(),
        kind: "commandExecution".into(),
        raw: json!({ "command": command }),
        exit_code: code,
    }
}

fn guard_of(event: &crate::store::AutopilotEvent) -> Value {
    event.body["guard"].clone()
}

#[tokio::test]
async fn under_on_and_off_a_guarded_command_is_denied_and_held_and_no_key_is_asked() {
    for chosen in [Mode::On, Mode::Off] {
        let (w, id) = running().await;
        let lead = Scripted::yes("alpha", "alpha");
        let second = Scripted::yes("beta", "beta");
        w.core.set_crew_approvers(approvers(&lead, &[&second]));
        mode(&w, chosen);

        let verdict = request(&w, &id, ActionKind::Command, DROP).await;
        assert!(matches!(verdict, Verdict::Deny(_)), "{chosen:?}");
        assert_eq!(
            lead.calls() + second.calls(),
            0,
            "{chosen:?}: no key is asked"
        );

        let held = w.core.autopilot_held().unwrap();
        assert_eq!(held.items.len(), 1, "{chosen:?}");
        assert_eq!(held.items[0].state, State::Held);
        let denied = &events(&w, &id, "denied")[0];
        assert_eq!(denied.body["decided_by"], "guard");
        assert_eq!(denied.body["tier"], "guard");
        let guard = guard_of(denied);
        assert_eq!(guard["category"], "database");
        assert_eq!(guard["target"], "db.prod.internal");
        assert_eq!(guard["list"], "none");
        assert!(guard["matched_by"].as_str().unwrap().contains("DROP"));
        // The held row says the same.
        assert_eq!(guard_of(&events(&w, &id, "held")[0]), guard);
    }
}

#[tokio::test]
async fn in_full_with_no_second_vendor_a_guarded_command_is_denied_and_held() {
    let (w, id) = running().await;
    let lead = Scripted::yes("alpha", "alpha");
    // Same vendor family as the lead: not a second key.
    let same = Scripted::yes("alpha-two", "alpha");
    w.core.set_crew_approvers(approvers(&lead, &[&same]));

    let verdict = request(&w, &id, ActionKind::Command, DROP).await;
    assert!(matches!(verdict, Verdict::Deny(_)));
    assert_eq!(same.calls(), 0);
    let denied = &events(&w, &id, "denied")[0];
    assert_eq!(denied.body["needs_second_engine"], true);
    assert_eq!(denied.body["decided_by"], "two_keys");
    assert_eq!(denied.body["lead"]["approve"], true);
    assert_eq!(guard_of(denied)["category"], "database");
    assert_eq!(w.core.autopilot_held().unwrap().items.len(), 1);
}

#[tokio::test]
async fn in_full_two_keys_that_agree_allow_it_with_both_reasons_in_the_ledger() {
    let (w, id) = running().await;
    let lead = Scripted::yes("alpha", "alpha");
    let second = Scripted::yes("beta", "beta");
    w.core.set_crew_approvers(approvers(&lead, &[&second]));

    let verdict = request(&w, &id, ActionKind::Command, DROP).await;
    assert_eq!(verdict, Verdict::Allow);
    assert_eq!((lead.calls(), second.calls()), (1, 1));
    // The lead was told why the command is guarded.
    let seen = lead.seen.lock().unwrap();
    assert_eq!(seen[0].tier, "guard");
    assert!(seen[0].reasons[0].contains("production-linked"));
    drop(seen);

    let allowed = &events(&w, &id, "allowed")[0];
    assert_eq!(allowed.body["decided_by"], "two_keys");
    assert_eq!(allowed.body["lead"]["reason"], "Fits the brief.");
    assert_eq!(allowed.body["second"]["engine"], "beta");
    assert_eq!(
        allowed.body["second"]["answer"]["reason"],
        "Fits the brief."
    );
    assert_eq!(guard_of(allowed)["category"], "database");

    // Its exit status lands in the ledger when the engine reports it, once.
    assert_eq!(w.core.crew_autopilot.exits.len(), 1);
    w.core
        .guard_exit_seen(&w.worker(&id), &finished(DROP, Some(0)));
    let exit = &events(&w, &id, "command_exit")[0];
    assert_eq!(exit.body["exit_code"], 0);
    assert_eq!(exit.body["hold"], Value::Null);
    assert_eq!(exit.body["row"], allowed.id);
    assert_eq!(guard_of(exit), guard_of(allowed));
    w.core
        .guard_exit_seen(&w.worker(&id), &finished(DROP, Some(1)));
    assert_eq!(
        events(&w, &id, "command_exit").len(),
        1,
        "one status per command"
    );
    assert_eq!(w.core.crew_autopilot.exits.len(), 0);
}

#[tokio::test]
async fn in_full_two_keys_that_disagree_deny_it_and_keep_both_reasons() {
    let (w, id) = running().await;
    let lead = Scripted::yes("alpha", "alpha");
    let second = Scripted::no("beta", "beta");
    w.core.set_crew_approvers(approvers(&lead, &[&second]));

    let verdict = request(&w, &id, ActionKind::Command, DROP).await;
    assert!(matches!(verdict, Verdict::Deny(_)));
    let denied = &events(&w, &id, "denied")[0];
    assert_eq!(denied.body["lead"]["approve"], true);
    assert_eq!(denied.body["second"]["answer"]["approve"], false);
    let disagreement = &events(&w, &id, "disagreement")[0];
    assert_eq!(disagreement.body["second"]["engine"], "beta");
    assert_eq!(guard_of(disagreement)["category"], "database");
    assert_eq!(
        w.core.autopilot_held().unwrap().items.len(),
        1,
        "held for the person"
    );
}

#[tokio::test]
async fn a_lead_that_says_no_denies_it_and_the_second_key_is_not_asked() {
    let (w, id) = running().await;
    let lead = Scripted::no("alpha", "alpha");
    let second = Scripted::yes("beta", "beta");
    w.core.set_crew_approvers(approvers(&lead, &[&second]));
    let verdict = request(&w, &id, ActionKind::Command, DROP).await;
    assert!(matches!(verdict, Verdict::Deny(_)));
    assert_eq!(second.calls(), 0);
    assert_eq!(events(&w, &id, "denied")[0].body["decided_by"], "lead");
}

#[tokio::test]
async fn n8_commands_stay_always_denied_and_are_never_held() {
    // Each case has its own task: three denials in a row stop one.
    for chosen in [Mode::Off, Mode::On, Mode::Full] {
        for command in [
            "kubectl delete pod web-1",
            "terraform destroy -auto-approve",
            "helm uninstall web",
            "helm delete web",
            "helm del web",
        ] {
            let (w, id) = running().await;
            let lead = Scripted::yes("alpha", "alpha");
            let second = Scripted::yes("beta", "beta");
            w.core.set_crew_approvers(approvers(&lead, &[&second]));
            mode(&w, chosen);
            let verdict = request(&w, &id, ActionKind::Command, command).await;
            assert!(words(&verdict).contains("N8"), "{chosen:?}: {command}");
            assert!(
                w.core.autopilot_held().unwrap().items.is_empty(),
                "{chosen:?}"
            );
            assert_eq!(lead.calls(), 0);
        }
    }
}

#[tokio::test]
async fn a_local_target_is_not_guarded_and_goes_to_the_lead_alone() {
    let (w, id) = running().await;
    let lead = Scripted::yes("alpha", "alpha");
    let second = Scripted::yes("beta", "beta");
    w.core.set_crew_approvers(approvers(&lead, &[&second]));
    let verdict = request(
        &w,
        &id,
        ActionKind::Command,
        "psql -h localhost -c 'DROP TABLE users'",
    )
    .await;
    assert_eq!(verdict, Verdict::Allow);
    assert_eq!((lead.calls(), second.calls()), (1, 0));
    assert!(events(&w, &id, "allowed")[0].body.get("guard").is_none());
    assert_eq!(
        w.core.crew_autopilot.exits.len(),
        0,
        "only guarded commands are watched"
    );
}

#[tokio::test]
async fn the_devices_lists_exempt_force_and_prod_wins() {
    let (w, id) = running().await;
    let lead = Scripted::yes("alpha", "alpha");
    let second = Scripted::yes("beta", "beta");
    w.core.set_crew_approvers(approvers(&lead, &[&second]));

    add(
        &w,
        GuardList::Safe,
        GuardTargetKind::Host,
        "db.prod.internal",
    );
    assert_eq!(
        request(&w, &id, ActionKind::Command, DROP).await,
        Verdict::Allow
    );
    assert_eq!(second.calls(), 0, "safe: the lead alone");

    // A local name forced by the prod list.
    add(&w, GuardList::Prod, GuardTargetKind::Host, "localhost");
    let local = "psql -h localhost -c 'DROP TABLE users'";
    assert_eq!(
        request(&w, &id, ActionKind::Command, local).await,
        Verdict::Allow
    );
    assert_eq!(second.calls(), 1, "prod: a second key was asked");

    // In both lists: prod wins.
    add(
        &w,
        GuardList::Prod,
        GuardTargetKind::Host,
        "db.prod.internal",
    );
    assert_eq!(
        request(&w, &id, ActionKind::Command, DROP).await,
        Verdict::Allow
    );
    assert_eq!(second.calls(), 2);
    let rows = events(&w, &id, "allowed");
    assert_eq!(guard_of(&rows[2])["list"], "prod");
    assert!(rows[0].body.get("guard").is_none());
}

#[tokio::test]
async fn a_repository_file_cannot_loosen_the_guard() {
    let (w, id) = running().await;
    let lead = Scripted::no("alpha", "alpha");
    w.core.set_crew_approvers(approvers(&lead, &[]));
    mode(&w, Mode::On);
    let (_, run) = w
        .core
        .store
        .crew_task_for_worker(&w.worker(&id))
        .unwrap()
        .unwrap();
    let tree = std::path::PathBuf::from(run.worktree.unwrap());
    std::fs::create_dir_all(tree.join(".orbyn")).unwrap();
    std::fs::write(
        tree.join(".orbyn/crew.json"),
        json!({ "safe_targets": ["db.prod.internal"], "allowed_hosts": ["db.prod.internal"] })
            .to_string(),
    )
    .unwrap();
    let verdict = request(&w, &id, ActionKind::Command, DROP).await;
    assert!(matches!(verdict, Verdict::Deny(_)));
    assert_eq!(events(&w, &id, "denied")[0].body["decided_by"], "guard");
}

#[tokio::test]
async fn approving_a_guard_hold_once_works_through_presence_and_leaves_the_exit_status() {
    let (w, id) = held_world().await;
    mode(&w, Mode::On);
    let hold = hold_of(&w, &id, DROP).await;
    // A hold made by the guard is not approvable without the person's grant.
    assert!(matches!(
        w.core.autopilot_approve_once(&hold, None).await,
        Err(CoreError::PresenceRequired)
    ));
    let nonce = grant_for(&w, &hold);
    w.core
        .autopilot_approve_once(&hold, Some(&nonce))
        .await
        .unwrap();

    let approved = &events(&w, &id, "approved_once")[0];
    assert_eq!(approved.body["decided_by"], "person");
    assert_eq!(approved.body["via"], "desktop_presence");
    assert_eq!(guard_of(approved)["target"], "db.prod.internal");
    assert_eq!(
        approved.body["action"]["target"], DROP,
        "the command is in the row"
    );

    // The one retry goes through; it is not asked of any key.
    assert_eq!(
        request(&w, &id, ActionKind::Command, DROP).await,
        Verdict::Allow
    );
    assert!(matches!(
        request(&w, &id, ActionKind::Command, DROP).await,
        Verdict::Deny(_)
    ));
    assert_eq!(state_of(&w, &hold), State::Used);
    let used = &events(&w, &id, "hold_used")[0];
    assert_eq!(guard_of(used)["category"], "database");
    let allowed = &events(&w, &id, "allowed")[0];
    assert_eq!(allowed.body["decided_by"], "person");
    assert_eq!(guard_of(allowed)["category"], "database");

    // Another command's finish is not this one's; this one's finish is.
    w.core
        .guard_exit_seen(&w.worker(&id), &finished("ls src", Some(0)));
    assert!(events(&w, &id, "command_exit").is_empty());
    w.core.guard_exit_seen(
        &w.worker(&id),
        &finished(&format!("zsh -lc \"{DROP}\""), Some(3)),
    );
    let exit = &events(&w, &id, "command_exit")[0];
    assert_eq!(exit.body["exit_code"], 3);
    assert_eq!(exit.body["hold"], hold);
    assert_eq!(exit.body["row"], allowed.id);
    assert_eq!(exit.body["action"]["target"], DROP);
    assert!(kinds(&w, &id).iter().any(|k| k == "approved_once"));
}

#[tokio::test]
async fn an_exit_status_that_never_arrives_is_written_as_unknown() {
    let (w, id) = held_world().await;
    mode(&w, Mode::On);
    let hold = hold_of(&w, &id, DROP).await;
    let nonce = grant_for(&w, &hold);
    w.core
        .autopilot_approve_once(&hold, Some(&nonce))
        .await
        .unwrap();
    assert_eq!(
        request(&w, &id, ActionKind::Command, DROP).await,
        Verdict::Allow
    );
    assert_eq!(w.core.crew_autopilot.exits.len(), 1);

    // An item with no exit code does not end the watch.
    w.core
        .guard_exit_seen(&w.worker(&id), &finished(DROP, None));
    assert!(events(&w, &id, "command_exit").is_empty());
    // Before the ten minutes are up nothing is written; after, the status is exactly `unknown`.
    w.core.exit_sweep(unix_now() + 60);
    assert!(events(&w, &id, "command_exit").is_empty());
    w.core.exit_sweep(unix_now() + 601);
    let exit = &events(&w, &id, "command_exit")[0];
    assert_eq!(exit.body["exit_code"], "unknown");
    assert_eq!(exit.body["hold"], hold);
    assert_eq!(w.core.crew_autopilot.exits.len(), 0);
    w.core.exit_sweep(unix_now() + 100_000);
    assert_eq!(events(&w, &id, "command_exit").len(), 1, "written once");
}

fn body(list: GuardList, pattern: &str) -> GuardTargetAdd {
    GuardTargetAdd {
        list,
        pattern: pattern.to_owned(),
        kind: GuardTargetKind::Host,
        note: None,
    }
}

#[tokio::test]
async fn tightening_needs_no_proof_and_loosening_needs_the_persons_grant() {
    let (w, _) = running().await;
    // Tighten: a prod entry, then remove a safe one.
    let prod = w
        .core
        .guard_target_add(&body(GuardList::Prod, "db.prod.internal"), None)
        .unwrap();
    add(&w, GuardList::Safe, GuardTargetKind::Host, "stage.internal");
    let safe = w
        .core
        .guard_targets()
        .unwrap()
        .items
        .into_iter()
        .find(|t| t.list == GuardList::Safe)
        .unwrap();
    w.core.guard_target_remove(&safe.id, None).unwrap();

    // No desktop shell: loosening answers desktop_required.
    assert!(matches!(
        w.core.guard_target_add(
            &body(GuardList::Safe, "stage.internal"),
            Some(&fresh_nonce())
        ),
        Err(CoreError::DesktopRequired)
    ));
    assert!(matches!(
        w.core.guard_target_remove(&prod.id, None),
        Err(CoreError::DesktopRequired)
    ));

    // A shell is there, but nothing can mint the grant: presence_required, and the lists are unchanged.
    let (lines, answers) = tokio::sync::mpsc::unbounded_channel();
    w.core.shell.attach(lines);
    std::mem::forget(answers);
    for nonce in [None, Some(fresh_nonce())] {
        assert!(matches!(
            w.core
                .guard_target_add(&body(GuardList::Safe, "stage.internal"), nonce.as_deref()),
            Err(CoreError::PresenceRequired)
        ));
        assert!(matches!(
            w.core.guard_target_remove(&prod.id, nonce.as_deref()),
            Err(CoreError::PresenceRequired)
        ));
    }
    assert_eq!(w.core.guard_targets().unwrap().items.len(), 1);

    // A grant for this very change loosens it, once; one for another pattern or action does not.
    let nonce = fresh_nonce();
    let now = unix_now();
    let wrong = |action: &str, pattern: &str| PresenceFor::GuardLoosen {
        action: action.into(),
        kind: "host".into(),
        pattern: pattern.into(),
    };
    w.core
        .presence
        .put(wrong("add_safe", "other.internal"), nonce.clone(), now);
    assert!(matches!(
        w.core
            .guard_target_add(&body(GuardList::Safe, "stage.internal"), Some(&nonce)),
        Err(CoreError::PresenceRequired)
    ));
    w.core
        .presence
        .put(wrong("remove_prod", "stage.internal"), nonce.clone(), now);
    assert!(matches!(
        w.core
            .guard_target_add(&body(GuardList::Safe, "stage.internal"), Some(&nonce)),
        Err(CoreError::PresenceRequired)
    ));
    w.core
        .presence
        .put(wrong("add_safe", "stage.internal"), nonce.clone(), now);
    w.core
        .guard_target_add(&body(GuardList::Safe, "stage.internal"), Some(&nonce))
        .unwrap();
    assert!(matches!(
        w.core
            .guard_target_add(&body(GuardList::Safe, "again.internal"), Some(&nonce)),
        Err(CoreError::PresenceRequired)
    ));
    let nonce = fresh_nonce();
    w.core.presence.put(
        wrong("remove_prod", "db.prod.internal"),
        nonce.clone(),
        unix_now(),
    );
    w.core.guard_target_remove(&prod.id, Some(&nonce)).unwrap();
    assert_eq!(w.core.guard_targets().unwrap().items.len(), 1);
}

#[tokio::test]
async fn the_lists_are_bounded_and_refuse_a_secret_a_url_and_a_duplicate() {
    let (w, _) = running().await;
    let long = "a".repeat(254);
    for bad in [
        "https://user:pw@db.example.com/x",
        "user@db.example.com",
        "db.example.com:5432",
        "a b",
        "",
        "*",
        "a..b",
        ".db",
        "db/x",
        long.as_str(),
    ] {
        assert!(
            matches!(
                w.core.guard_target_add(&body(GuardList::Prod, bad), None),
                Err(CoreError::Invalid(_))
            ),
            "{bad}"
        );
    }
    // A name the secret scan objects to (built at run time).
    let token = format!("ghp_{}", "a1".repeat(19));
    assert!(matches!(
        w.core
            .guard_target_add(&body(GuardList::Prod, &token), None),
        Err(CoreError::Invalid(_))
    ));
    // A note with a secret is refused, not stored redacted; a plain note is kept.
    let mut with_note = body(GuardList::Prod, "db.example.com");
    with_note.note = Some(format!("password = {token}"));
    assert!(matches!(
        w.core.guard_target_add(&with_note, None),
        Err(CoreError::Invalid(_))
    ));
    with_note.note = Some("the main customer database".into());
    let kept = w.core.guard_target_add(&with_note, None).unwrap();
    assert_eq!(kept.note.as_deref(), Some("the main customer database"));
    assert!(
        matches!(
            w.core
                .guard_target_add(&body(GuardList::Prod, "DB.example.com"), None),
            Err(CoreError::Invalid(_))
        ),
        "a duplicate, whatever the case"
    );
    assert_eq!(
        w.core
            .guard_target_add(&body(GuardList::Prod, "*.Example.org"), None)
            .unwrap()
            .pattern,
        "*.example.org"
    );

    // The audit rows are written for each change, and none holds the secret.
    let audits: Vec<String> = {
        let conn = w.core.store.reader();
        let mut q = conn
            .prepare("SELECT action || ' ' || detail FROM audit_events WHERE action LIKE 'crew.guard.%' ORDER BY id")
            .unwrap();
        q.query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    assert_eq!(audits.len(), 2);
    assert!(
        audits
            .iter()
            .all(|a| a.starts_with("crew.guard.add") && !a.contains("ghp_"))
    );

    // 200 entries in all, then no more.
    let held = w.core.guard_targets().unwrap().items.len();
    for n in held..200 {
        add(
            &w,
            GuardList::Prod,
            GuardTargetKind::Host,
            &format!("h{n}.internal"),
        );
    }
    assert!(matches!(
        w.core
            .guard_target_add(&body(GuardList::Prod, "one-more.internal"), None),
        Err(CoreError::Invalid(_))
    ));
    assert_eq!(w.core.guard_targets().unwrap().max_entries, 200);
}

#[tokio::test]
async fn the_test_reads_a_command_and_changes_nothing() {
    let (w, id) = running().await;
    add(
        &w,
        GuardList::Safe,
        GuardTargetKind::Host,
        "*.stage.internal",
    );
    let thread = w.worker(&id);
    let guarded = w.core.guard_test(DROP, Some(&thread)).await.unwrap();
    assert_eq!(guarded.verdict, orbyn_contracts::GuardVerdict::Guarded);
    assert_eq!(
        guarded.category,
        Some(orbyn_contracts::GuardCategory::Database)
    );
    assert_eq!(guarded.target.as_deref(), Some("db.prod.internal"));
    assert!(guarded.production_linked && !guarded.local);

    let exempt = w
        .core
        .guard_test("psql -h db.stage.internal -c 'DROP TABLE users'", None)
        .await
        .unwrap();
    assert_eq!(exempt.verdict, orbyn_contracts::GuardVerdict::Exempt);
    assert_eq!(exempt.matched_entry.unwrap().pattern, "*.stage.internal");

    let local = w
        .core
        .guard_test("psql -h localhost -c 'DROP TABLE users'", None)
        .await
        .unwrap();
    assert_eq!(local.verdict, orbyn_contracts::GuardVerdict::Local);
    assert!(local.local && !local.production_linked);

    let plain = w.core.guard_test("ls -la", None).await.unwrap();
    assert_eq!(plain.verdict, orbyn_contracts::GuardVerdict::NotDestructive);

    // A secret in the command is not echoed; an empty or oversized command is refused.
    let token = format!("ghp_{}", "a1".repeat(19));
    let shown = w
        .core
        .guard_test(&format!("echo {token}"), None)
        .await
        .unwrap();
    assert!(!shown.command.contains("ghp_"));
    assert!(w.core.guard_test("  ", None).await.is_err());
    assert!(w.core.guard_test(&"a".repeat(2001), None).await.is_err());
    assert!(
        w.core.autopilot_held().unwrap().items.is_empty(),
        "nothing was held or run"
    );
}

fn worktree_of(w: &World, id: &str) -> std::path::PathBuf {
    let (_, run) = w
        .core
        .store
        .crew_task_for_worker(&w.worker(id))
        .unwrap()
        .unwrap();
    std::path::PathBuf::from(run.worktree.unwrap())
}

const PUSH: &str = "git push origin :old";

#[tokio::test]
async fn an_approval_is_void_when_the_resolved_target_changed_after_the_hold() {
    // Before approving.
    let (w, id) = held_world().await;
    mode(&w, Mode::On);
    let tree = worktree_of(&w, &id);
    run_git(
        &tree,
        &["remote", "add", "origin", "git@github.com:o/r.git"],
    );
    let hold = hold_of(&w, &id, PUSH).await;
    assert_eq!(events(&w, &id, "held")[0].body["guard"]["category"], "git");
    run_git(
        &tree,
        &["remote", "set-url", "origin", "git@evil.example:o/r.git"],
    );
    let nonce = grant_for(&w, &hold);
    let refused = w.core.autopilot_approve_once(&hold, Some(&nonce)).await;
    match refused {
        Err(CoreError::PolicyDenied(why)) => assert!(why.contains("target changed"), "{why}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(state_of(&w, &hold), State::Held, "nothing was approved");
    // Pointed back, the same grant works.
    run_git(
        &tree,
        &["remote", "set-url", "origin", "git@github.com:o/r.git"],
    );
    w.core
        .autopilot_approve_once(&hold, Some(&nonce))
        .await
        .unwrap();

    // After approving, before the retry.
    let (w, id) = held_world().await;
    mode(&w, Mode::On);
    let tree = worktree_of(&w, &id);
    run_git(
        &tree,
        &["remote", "add", "origin", "git@github.com:o/r.git"],
    );
    let hold = hold_of(&w, &id, PUSH).await;
    let nonce = grant_for(&w, &hold);
    w.core
        .autopilot_approve_once(&hold, Some(&nonce))
        .await
        .unwrap();
    run_git(&tree, &["remote", "set-url", "origin", "/srv/other.git"]);
    let verdict = request(&w, &id, ActionKind::Command, PUSH).await;
    assert!(
        matches!(verdict, Verdict::Deny(_)),
        "the approval no longer applies"
    );
    assert_eq!(state_of(&w, &hold), State::Approved, "and was not used up");
}

#[tokio::test]
async fn a_command_the_parser_cannot_read_is_guarded_through_the_gate() {
    let (w, id) = running().await;
    let lead = Scripted::yes("alpha", "alpha");
    w.core.set_crew_approvers(approvers(&lead, &[]));
    mode(&w, Mode::On);
    let verdict = request(
        &w,
        &id,
        ActionKind::Command,
        "$CMD DROP TABLE x -h db.example.com",
    )
    .await;
    assert!(matches!(verdict, Verdict::Deny(_)));
    let denied = &events(&w, &id, "denied")[0];
    assert_eq!(denied.body["decided_by"], "guard", "{}", denied.body);
    assert_eq!(guard_of(denied)["category"], "unparsed");
}

#[tokio::test]
async fn only_guard_holds_are_watched_and_only_for_exactly_their_command() {
    // A hold the guard did not make: no exit watch.
    let (w, id) = held_world().await;
    let hold = hold_of(&w, &id, "make deploy").await;
    let nonce = grant_for(&w, &hold);
    w.core
        .autopilot_approve_once(&hold, Some(&nonce))
        .await
        .unwrap();
    assert_eq!(
        request(&w, &id, ActionKind::Command, "make deploy").await,
        Verdict::Allow
    );
    assert_eq!(w.core.crew_autopilot.exits.len(), 0);

    // A guard hold: a command that merely contains the watched one is not it; the exact one, wrapped in a shell, is.
    let (w, id) = held_world().await;
    mode(&w, Mode::On);
    let hold = hold_of(&w, &id, DROP).await;
    let nonce = grant_for(&w, &hold);
    w.core
        .autopilot_approve_once(&hold, Some(&nonce))
        .await
        .unwrap();
    assert_eq!(
        request(&w, &id, ActionKind::Command, DROP).await,
        Verdict::Allow
    );
    assert_eq!(w.core.crew_autopilot.exits.len(), 1);
    let thread = w.worker(&id);
    w.core.guard_exit_seen(
        &thread,
        &finished(&format!("echo hi; {DROP}; echo bye"), Some(0)),
    );
    w.core
        .guard_exit_seen("another-thread", &finished(DROP, Some(0)));
    assert!(events(&w, &id, "command_exit").is_empty());
    w.core
        .guard_exit_seen(&thread, &finished(&format!("bash -lc \"{DROP}\""), Some(0)));
    assert_eq!(events(&w, &id, "command_exit")[0].body["exit_code"], 0);
}

#[tokio::test]
async fn a_loosening_grant_is_for_one_kind() {
    let (w, _) = running().await;
    let (lines, answers) = tokio::sync::mpsc::unbounded_channel();
    w.core.shell.attach(lines);
    std::mem::forget(answers);
    let nonce = fresh_nonce();
    w.core.presence.put(
        PresenceFor::GuardLoosen {
            action: "add_safe".into(),
            kind: "bucket".into(),
            pattern: "stage.internal".into(),
        },
        nonce.clone(),
        unix_now(),
    );
    assert!(
        matches!(
            w.core
                .guard_target_add(&body(GuardList::Safe, "stage.internal"), Some(&nonce)),
            Err(CoreError::PresenceRequired)
        ),
        "a grant for a bucket does not add a host"
    );
}
