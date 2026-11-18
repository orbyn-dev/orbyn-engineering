//! The created check at rest (migration 52): the migration follows 51 and leaves older tables alone, each person's act with its audit
//! row first, an idempotent repeat, the refused transitions, a stale view, isolation between projects, the core's own steps, and the
//! table checks. Nothing here starts a worker or runs git: the store never does.

use orbyn_contracts::{
    EscapeActor, EscapeCheckProof, EscapeCheckSeen, EscapeCheckState, EscapeEventKind, EscapeGap,
    EscapeState,
};
use rusqlite::Connection;

use super::crew_escapes_tests::{PROJECT, escapes, open, seed};
use super::*;
use crate::testing::TempDir;

const AT: i64 = 7_000;
const FIX: &str = "f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1";
const TASK: &str = "sha256:00000000000000000000000000000000000000000000000000000000000000aa";

fn seen(escape: EscapeState, check: Option<EscapeCheckState>) -> EscapeCheckSeen {
    EscapeCheckSeen {
        state: escape,
        gap: None,
        check,
    }
}

fn proposal() -> CheckProposal<'static> {
    CheckProposal {
        path: "tests/escape_check.txt",
        asserts: "The parser rejects an empty input.",
        run: "git grep --no-index -q x -- tests/escape_check.txt",
        fix_commit: FIX,
    }
}

/// A store with one reverted merge whose escape a person confirmed.
fn confirmed() -> (TempDir, Store, String, String) {
    let (dir, store) = open();
    let task = seed(&store.writer(), 1, "waiting", None, "alpha", 100);
    store.set_merge_reverted(&task, "feed01").unwrap();
    let id = escapes(&store)[0].id.clone();
    store
        .decide_escape(
            PROJECT,
            &id,
            EscapeDecision::Confirm,
            &orbyn_contracts::EscapeSeen {
                state: EscapeState::Possible,
                gap: None,
            },
            None,
            AT - 1,
        )
        .unwrap();
    (dir, store, task, id)
}

fn proposed(store: &Store, id: &str) {
    store
        .propose_escape_check(
            PROJECT,
            id,
            &seen(EscapeState::Confirmed, None),
            &proposal(),
            AT,
        )
        .unwrap();
}

fn writing(store: &Store, id: &str) {
    proposed(store, id);
    store
        .accept_escape_check(
            PROJECT,
            id,
            &seen(EscapeState::Confirmed, Some(EscapeCheckState::Proposed)),
            &proposal(),
            TASK,
            AT + 1,
        )
        .unwrap();
}

fn audits(store: &Store) -> Vec<String> {
    store
        .reader()
        .prepare(
            "SELECT action FROM audit_events WHERE action LIKE 'crew.escape.check.%' ORDER BY id",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

fn kinds(store: &Store, id: &str) -> Vec<EscapeEventKind> {
    store
        .crew_escape_events(id, 50)
        .unwrap()
        .iter()
        .map(|e| e.kind)
        .collect()
}

fn proof(fails: bool, passes: bool) -> EscapeCheckProof {
    EscapeCheckProof {
        escaped_commit: "e".repeat(40),
        fix_commit: FIX.into(),
        fails_on_escape: fails,
        passes_on_fix: passes,
    }
}

#[test]
fn migration_52_follows_51_and_leaves_older_tables_alone() {
    let versions: Vec<i64> = migrations::MIGRATIONS.iter().map(|(v, _)| *v).collect();
    assert!(versions.windows(2).all(|w| w[1] == w[0] + 1));
    assert!(versions.contains(&52));

    let dir = TempDir::new();
    let path = dir.path().join("old.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(
        "PRAGMA foreign_keys=ON; CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL)",
    )
    .unwrap();
    for (version, sql) in migrations::MIGRATIONS.iter().filter(|(v, _)| *v <= 51) {
        conn.execute_batch(sql).unwrap();
        conn.execute(
            "INSERT INTO schema_migrations VALUES (?1,'fixture')",
            [version],
        )
        .unwrap();
    }
    let task = seed(&conn, 1, "reverted", Some("feed01"), "alpha", 100);
    let before: (i64, i64) = conn
        .query_row(
            "SELECT (SELECT COUNT(*) FROM crew_escapes), (SELECT COUNT(*) FROM crew_escape_events)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    drop(conn);

    let store = Store::open(&path, "checks-upgrade").unwrap();
    let upgrade = store.upgrade().expect("an upgrade");
    assert_eq!(
        (upgrade.from, upgrade.to),
        (51, migrations::MIGRATIONS.last().unwrap().0)
    );
    let reader = store.reader();
    let after: (i64, i64, i64) = reader
        .query_row(
            "SELECT (SELECT COUNT(*) FROM crew_escapes), (SELECT COUNT(*) FROM crew_escape_events), (SELECT COUNT(*) FROM crew_escape_checks)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((after.0, after.1), before, "the escape log is untouched");
    assert_eq!(after.2, 0);
    drop(reader);
    assert_eq!(
        store.crew_escape_of_merge(&task).unwrap(),
        None,
        "the old release logged no escape, and the upgrade adds none"
    );
}

#[test]
fn a_proposal_is_one_row_one_audit_row_and_one_event_and_starts_nothing() {
    let (_dir, store, _task, id) = confirmed();
    let (escape, check) = store
        .propose_escape_check(
            PROJECT,
            &id,
            &seen(EscapeState::Confirmed, None),
            &proposal(),
            AT,
        )
        .unwrap();
    assert_eq!(escape.state, EscapeState::Confirmed);
    assert_eq!(
        (
            check.state,
            check.task_id.clone(),
            check.proof.clone(),
            check.attempts
        ),
        (EscapeCheckState::Proposed, None, None, 0)
    );
    assert_eq!(check.fix_commit, FIX);
    assert_eq!(audits(&store), ["crew.escape.check.propose"]);
    let events = store.crew_escape_events(&id, 10).unwrap();
    let last = events.last().unwrap();
    assert_eq!(last.kind, EscapeEventKind::CheckProposed);
    assert_eq!(last.decided_by, EscapeActor::Person);
    assert!(last.cause.starts_with("person:"), "{}", last.cause);
    assert_eq!(
        store
            .reader()
            .query_row(
                "SELECT COUNT(*) FROM crew_runs WHERE task_id = ?1",
                [TASK],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0,
        "a proposal writes no task"
    );

    // The same proposal again is the same row and writes nothing more.
    let again = store
        .propose_escape_check(
            PROJECT,
            &id,
            &seen(EscapeState::Confirmed, Some(EscapeCheckState::Proposed)),
            &proposal(),
            AT + 5,
        )
        .unwrap();
    assert_eq!(again.1, check);
    assert_eq!(audits(&store).len(), 1);
    assert_eq!(kinds(&store, &id).len(), events.len());
}

#[test]
fn only_a_confirmed_escape_with_a_test_gap_can_have_a_proposal() {
    let (_dir, store) = open();
    let task = seed(&store.writer(), 1, "waiting", None, "alpha", 100);
    store.set_merge_reverted(&task, "feed01").unwrap();
    let id = escapes(&store)[0].id.clone();
    let before = audits(&store).len();
    let possible = store.propose_escape_check(
        PROJECT,
        &id,
        &seen(EscapeState::Possible, None),
        &proposal(),
        AT,
    );
    assert!(
        matches!(possible, Err(CoreError::Invalid(_))),
        "{possible:?}"
    );

    store
        .decide_escape(
            PROJECT,
            &id,
            EscapeDecision::Confirm,
            &orbyn_contracts::EscapeSeen {
                state: EscapeState::Possible,
                gap: None,
            },
            None,
            AT,
        )
        .unwrap();
    store
        .set_escape_gap(
            PROJECT,
            &id,
            &orbyn_contracts::EscapeSeen {
                state: EscapeState::Confirmed,
                gap: None,
            },
            EscapeGap::ToolingGap,
            AT,
        )
        .unwrap();
    let tooling = store.propose_escape_check(
        PROJECT,
        &id,
        &EscapeCheckSeen {
            state: EscapeState::Confirmed,
            gap: Some(EscapeGap::ToolingGap),
            check: None,
        },
        &proposal(),
        AT,
    );
    let Err(CoreError::Invalid(why)) = tooling else {
        panic!("a tooling gap has no proposal");
    };
    assert!(why.contains("tooling gap"), "{why}");
    assert!(store.crew_escape_check(&id).unwrap().is_none());
    assert_eq!(
        audits(&store).len(),
        before,
        "a refused proposal writes no check audit row"
    );
}

#[test]
fn a_stale_view_and_another_projects_escape_are_refused_and_write_nothing() {
    let (_dir, store, _task, id) = confirmed();
    let stale = store.propose_escape_check(
        PROJECT,
        &id,
        &seen(EscapeState::Possible, None),
        &proposal(),
        AT,
    );
    assert!(matches!(stale, Err(CoreError::Stale(_))), "{stale:?}");
    let other = store.propose_escape_check(
        "another-project",
        &id,
        &seen(EscapeState::Confirmed, None),
        &proposal(),
        AT,
    );
    assert!(matches!(other, Err(CoreError::NotFound(_))), "{other:?}");
    assert!(store.crew_escape_check(&id).unwrap().is_none());
    assert!(audits(&store).is_empty());

    proposed(&store, &id);
    // The check changed since the person looked: accepting a check seen as absent is stale.
    let accepted = store.accept_escape_check(
        PROJECT,
        &id,
        &seen(EscapeState::Confirmed, None),
        &proposal(),
        TASK,
        AT,
    );
    assert!(matches!(accepted, Err(CoreError::Stale(_))), "{accepted:?}");
    assert_eq!(audits(&store), ["crew.escape.check.propose"]);
    let elsewhere = store.accept_escape_check(
        "another-project",
        &id,
        &seen(EscapeState::Confirmed, Some(EscapeCheckState::Proposed)),
        &proposal(),
        TASK,
        AT,
    );
    assert!(
        matches!(elsewhere, Err(CoreError::NotFound(_))),
        "{elsewhere:?}"
    );
}

#[test]
fn accept_writes_the_audit_row_and_writing_with_the_task_and_a_repeat_changes_nothing() {
    let (_dir, store, _task, id) = confirmed();
    proposed(&store, &id);
    let (_, check, started) = store
        .accept_escape_check(
            PROJECT,
            &id,
            &seen(EscapeState::Confirmed, Some(EscapeCheckState::Proposed)),
            &proposal(),
            TASK,
            AT + 1,
        )
        .unwrap();
    assert!(started);
    assert_eq!(
        (check.state, check.task_id.as_deref(), check.attempts),
        (EscapeCheckState::Writing, Some(TASK), 1)
    );
    assert_eq!(
        audits(&store),
        ["crew.escape.check.propose", "crew.escape.check.accept"]
    );
    let detail: String = store
        .reader()
        .query_row(
            "SELECT detail FROM audit_events WHERE action = 'crew.escape.check.accept'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(detail.contains(TASK), "{detail}");
    assert_eq!(
        store.crew_escape_check_for_task(TASK).unwrap().unwrap(),
        check
    );

    let (_, again, started) = store
        .accept_escape_check(
            PROJECT,
            &id,
            &seen(EscapeState::Confirmed, None),
            &proposal(),
            "sha256:other",
            AT + 9,
        )
        .unwrap();
    assert!(!started, "the same act twice starts nothing");
    assert_eq!(again, check);
    assert_eq!(audits(&store).len(), 2);
}

#[test]
fn a_start_that_failed_before_any_task_returns_the_check_to_a_proposal() {
    let (_dir, store, _task, id) = confirmed();
    writing(&store, &id);
    let back = store
        .unwind_escape_check_accept(PROJECT, &id, TASK, "No engine could take the task.", AT + 2)
        .unwrap();
    assert_eq!(
        (back.state, back.task_id, back.note.as_deref()),
        (
            EscapeCheckState::Proposed,
            None,
            Some("No engine could take the task.")
        )
    );
    assert_eq!(
        audits(&store).last().unwrap(),
        "crew.escape.check.start_failed"
    );
    // A task that is not the check's changes nothing.
    writing(&store, &id);
    let same = store
        .unwind_escape_check_accept(PROJECT, &id, "sha256:other", "x", AT + 3)
        .unwrap();
    assert_eq!(same.state, EscapeCheckState::Writing);
}

#[test]
fn the_core_records_a_proven_or_failed_end_once_and_only_for_its_own_task() {
    let (_dir, store, _task, id) = confirmed();
    writing(&store, &id);
    let wrong = store
        .finish_escape_check(
            &id,
            "sha256:other",
            &CheckFinish::Proven(proof(true, true)),
            AT + 2,
        )
        .unwrap();
    assert!(!wrong);
    assert_eq!(
        store.crew_escape_check(&id).unwrap().unwrap().state,
        EscapeCheckState::Writing
    );

    let wrote = store
        .finish_escape_check(&id, TASK, &CheckFinish::Proven(proof(true, true)), AT + 3)
        .unwrap();
    assert!(wrote);
    let check = store.crew_escape_check(&id).unwrap().unwrap();
    assert_eq!(check.state, EscapeCheckState::Proven);
    assert_eq!(check.proof, Some(proof(true, true)));
    let events = store.crew_escape_events(&id, 50).unwrap();
    let last = events.last().unwrap();
    assert_eq!(
        (last.kind, last.decided_by, last.cause.as_str()),
        (
            EscapeEventKind::CheckProven,
            EscapeActor::Core,
            format!("core:{TASK}:1").as_str()
        )
    );
    // Recorded once: a second end writes nothing.
    assert!(
        !store
            .finish_escape_check(
                &id,
                TASK,
                &CheckFinish::Failed {
                    proof: None,
                    note: "late".into()
                },
                AT + 4
            )
            .unwrap()
    );
    assert_eq!(
        kinds(&store, &id)
            .iter()
            .filter(|k| **k == EscapeEventKind::CheckFailed)
            .count(),
        0
    );
    // The merge of a proven check is recorded once.
    assert!(store.merge_escape_check(&id, TASK, AT + 5).unwrap());
    assert!(!store.merge_escape_check(&id, TASK, AT + 6).unwrap());
    assert_eq!(
        store.crew_escape_check(&id).unwrap().unwrap().state,
        EscapeCheckState::Merged
    );
    assert_eq!(
        kinds(&store, &id).last(),
        Some(&EscapeEventKind::CheckMerged)
    );
}

#[test]
fn a_failed_check_is_retried_to_a_proposal_that_only_accept_starts() {
    let (_dir, store, _task, id) = confirmed();
    writing(&store, &id);
    store
        .finish_escape_check(
            &id,
            TASK,
            &CheckFinish::Failed {
                proof: Some(proof(false, true)),
                note: "It passes on the escaped merge too, so it proves nothing.".into(),
            },
            AT + 2,
        )
        .unwrap();
    let failed = store.crew_escape_check(&id).unwrap().unwrap();
    assert_eq!(failed.state, EscapeCheckState::Failed);
    assert_eq!(failed.proof, Some(proof(false, true)));
    assert_eq!(
        kinds(&store, &id).last(),
        Some(&EscapeEventKind::CheckFailed)
    );

    let seen_failed = seen(EscapeState::Confirmed, Some(EscapeCheckState::Failed));
    // Accept is for a proposal only.
    let early = store.accept_escape_check(PROJECT, &id, &seen_failed, &proposal(), TASK, AT + 3);
    assert!(matches!(early, Err(CoreError::Invalid(_))), "{early:?}");

    let (_, retried) = store
        .retry_escape_check(PROJECT, &id, &seen_failed, AT + 4)
        .unwrap();
    assert_eq!(
        (retried.state, retried.task_id, retried.proof, retried.note),
        (EscapeCheckState::Proposed, None, None, None)
    );
    assert_eq!(audits(&store).last().unwrap(), "crew.escape.check.retry");
    let last = store.crew_escape_events(&id, 50).unwrap().pop().unwrap();
    assert_eq!(last.kind, EscapeEventKind::CheckProposed);
    assert!(last.cause.starts_with("retry:"), "{}", last.cause);
    // Retrying again changes nothing; retrying a check that has not failed is refused.
    let (_, same) = store
        .retry_escape_check(PROJECT, &id, &seen_failed, AT + 5)
        .unwrap();
    assert_eq!(same.state, EscapeCheckState::Proposed);
    assert_eq!(
        audits(&store)
            .iter()
            .filter(|a| a.ends_with("retry"))
            .count(),
        1
    );
    writing(&store, &id);
    let busy = store.retry_escape_check(
        PROJECT,
        &id,
        &seen(EscapeState::Confirmed, Some(EscapeCheckState::Writing)),
        AT + 6,
    );
    assert!(matches!(busy, Err(CoreError::Invalid(_))), "{busy:?}");
}

#[test]
fn withdraw_works_from_each_open_state_once_and_never_after_the_merge() {
    for state in ["proposed", "writing", "failed", "proven"] {
        let (_dir, store, _task, id) = confirmed();
        match state {
            "proposed" => proposed(&store, &id),
            "writing" => writing(&store, &id),
            "failed" => {
                writing(&store, &id);
                store
                    .finish_escape_check(
                        &id,
                        TASK,
                        &CheckFinish::Failed {
                            proof: None,
                            note: "no".into(),
                        },
                        AT + 2,
                    )
                    .unwrap();
            }
            _ => {
                writing(&store, &id);
                store
                    .finish_escape_check(&id, TASK, &CheckFinish::Proven(proof(true, true)), AT + 2)
                    .unwrap();
            }
        }
        let now = store.crew_escape_check(&id).unwrap().unwrap().state;
        let (_, check, was) = store
            .withdraw_escape_check(
                PROJECT,
                &id,
                &seen(EscapeState::Confirmed, Some(now)),
                AT + 9,
            )
            .unwrap();
        assert_eq!(check.state, EscapeCheckState::Withdrawn, "{state}");
        assert_eq!(was, Some(now), "{state}");
        assert_eq!(
            kinds(&store, &id).last(),
            Some(&EscapeEventKind::CheckWithdrawn)
        );
        let (_, again, was) = store
            .withdraw_escape_check(
                PROJECT,
                &id,
                &seen(EscapeState::Confirmed, Some(EscapeCheckState::Withdrawn)),
                AT + 10,
            )
            .unwrap();
        assert_eq!((again.state, was), (EscapeCheckState::Withdrawn, None));
        // A late end of the worker cannot revive a withdrawn check.
        assert!(
            !store
                .finish_escape_check(&id, TASK, &CheckFinish::Proven(proof(true, true)), AT + 11)
                .unwrap()
        );
        assert_eq!(
            store.crew_escape_check(&id).unwrap().unwrap().state,
            EscapeCheckState::Withdrawn
        );
    }

    let (_dir, store, _task, id) = confirmed();
    writing(&store, &id);
    store
        .finish_escape_check(&id, TASK, &CheckFinish::Proven(proof(true, true)), AT + 2)
        .unwrap();
    store.merge_escape_check(&id, TASK, AT + 3).unwrap();
    let merged = store.withdraw_escape_check(
        PROJECT,
        &id,
        &seen(EscapeState::Confirmed, Some(EscapeCheckState::Merged)),
        AT + 4,
    );
    assert!(matches!(merged, Err(CoreError::Invalid(_))), "{merged:?}");
}

#[test]
fn a_withdrawn_check_may_be_proposed_again_and_a_live_one_may_not() {
    let (_dir, store, _task, id) = confirmed();
    proposed(&store, &id);
    let other = CheckProposal {
        path: "tests/other.txt",
        ..proposal()
    };
    let busy = store.propose_escape_check(
        PROJECT,
        &id,
        &seen(EscapeState::Confirmed, Some(EscapeCheckState::Proposed)),
        &other,
        AT + 1,
    );
    assert!(matches!(busy, Err(CoreError::Invalid(_))), "{busy:?}");
    store
        .withdraw_escape_check(
            PROJECT,
            &id,
            &seen(EscapeState::Confirmed, Some(EscapeCheckState::Proposed)),
            AT + 2,
        )
        .unwrap();
    let (_, fresh) = store
        .propose_escape_check(
            PROJECT,
            &id,
            &seen(EscapeState::Confirmed, Some(EscapeCheckState::Withdrawn)),
            &other,
            AT + 3,
        )
        .unwrap();
    assert_eq!(
        (fresh.state, fresh.path.as_str(), fresh.attempts),
        (EscapeCheckState::Proposed, "tests/other.txt", 0)
    );
}

#[test]
fn the_table_refuses_a_writing_check_with_no_task_and_an_unknown_state_and_follows_its_escape() {
    let (_dir, store, _task, id) = confirmed();
    proposed(&store, &id);
    let writer = store.writer();
    let no_task = writer.execute(
        "UPDATE crew_escape_checks SET state = 'writing' WHERE escape_id = ?1",
        [&id],
    );
    assert!(no_task.is_err(), "writing needs a task");
    let unknown = writer.execute(
        "UPDATE crew_escape_checks SET state = 'running' WHERE escape_id = ?1",
        [&id],
    );
    assert!(unknown.is_err());
    let short = writer.execute(
        "UPDATE crew_escape_checks SET fix_commit = 'abc' WHERE escape_id = ?1",
        [&id],
    );
    assert!(short.is_err(), "a fix commit is a full id");
    let bad_proof = writer.execute(
        "UPDATE crew_escape_checks SET proof = 'not json' WHERE escape_id = ?1",
        [&id],
    );
    assert!(bad_proof.is_err());
    drop(writer);

    // Damaged rows fail the read, never a guess.
    store
        .writer()
        .execute_batch("UPDATE crew_escape_checks SET proof = '{\"x\":1}'")
        .unwrap();
    assert!(matches!(
        store.crew_escape_check(&id),
        Err(CoreError::Unreadable(_))
    ));
}

const COMMIT: &str = "c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0";
const HASH: &str = "0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a";

fn writing_seen() -> EscapeCheckSeen {
    seen(EscapeState::Confirmed, Some(EscapeCheckState::Writing))
}

fn record_diff(store: &Store, id: &str, commit: &str, hash: &str) -> bool {
    store
        .record_escape_check_diff(
            id,
            TASK,
            commit,
            hash,
            &["tests/escape_check.txt".to_owned()],
            AT + 5,
        )
        .unwrap()
}

#[test]
fn a_diff_is_recorded_for_the_writing_check_of_that_task_only_and_a_new_one_clears_the_approval() {
    let (_dir, store, _task, id) = confirmed();
    proposed(&store, &id);
    // A proposal has no diff.
    assert!(!record_diff(&store, &id, COMMIT, HASH));
    writing(&store, &id);
    assert!(
        !store
            .record_escape_check_diff(&id, "sha256:other", COMMIT, HASH, &[], AT + 5)
            .unwrap(),
        "another task's diff is not recorded"
    );
    assert!(record_diff(&store, &id, COMMIT, HASH));
    assert!(
        !record_diff(&store, &id, COMMIT, HASH),
        "the same change twice changes nothing"
    );
    let check = store.crew_escape_check(&id).unwrap().unwrap();
    assert_eq!(
        (
            check.diff_commit.as_deref(),
            check.diff_hash.as_deref(),
            check.diff_paths.as_slice()
        ),
        (
            Some(COMMIT),
            Some(HASH),
            &["tests/escape_check.txt".to_owned()][..]
        )
    );
    store
        .approve_escape_check(PROJECT, &id, &writing_seen(), COMMIT, HASH, AT + 6)
        .unwrap();
    assert!(
        store
            .crew_escape_check(&id)
            .unwrap()
            .unwrap()
            .approved_commit
            .is_some()
    );
    // A different change drops the approval of the earlier one.
    assert!(record_diff(&store, &id, &"d".repeat(40), HASH));
    let check = store.crew_escape_check(&id).unwrap().unwrap();
    assert_eq!((check.approved_commit, check.approved_hash), (None, None));
}

#[test]
fn an_approval_binds_the_exact_commit_and_hash_with_the_audit_row_first_and_a_repeat_changes_nothing()
 {
    let (_dir, store, _task, id) = confirmed();
    writing(&store, &id);
    // Nothing waits yet.
    let none = store.approve_escape_check(PROJECT, &id, &writing_seen(), COMMIT, HASH, AT + 6);
    assert!(matches!(none, Err(CoreError::Invalid(_))), "{none:?}");
    record_diff(&store, &id, COMMIT, HASH);
    let audits_before = audits(&store).len();
    for (commit, hash) in [(&"a".repeat(40)[..], HASH), (COMMIT, &"b".repeat(64)[..])] {
        let wrong = store.approve_escape_check(PROJECT, &id, &writing_seen(), commit, hash, AT + 6);
        assert!(matches!(wrong, Err(CoreError::Stale(_))), "{wrong:?}");
    }
    let old_view = store.approve_escape_check(
        PROJECT,
        &id,
        &seen(EscapeState::Confirmed, Some(EscapeCheckState::Proposed)),
        COMMIT,
        HASH,
        AT + 6,
    );
    assert!(matches!(old_view, Err(CoreError::Stale(_))), "{old_view:?}");
    let elsewhere = store.approve_escape_check(
        "another-project",
        &id,
        &writing_seen(),
        COMMIT,
        HASH,
        AT + 6,
    );
    assert!(
        matches!(elsewhere, Err(CoreError::NotFound(_))),
        "{elsewhere:?}"
    );
    assert_eq!(
        audits(&store).len(),
        audits_before,
        "a refused approval writes nothing"
    );

    let (_, check, written) = store
        .approve_escape_check(PROJECT, &id, &writing_seen(), COMMIT, HASH, AT + 6)
        .unwrap();
    assert!(written);
    assert_eq!(
        (
            check.approved_commit.as_deref(),
            check.approved_hash.as_deref()
        ),
        (Some(COMMIT), Some(HASH))
    );
    assert_eq!(audits(&store).last().unwrap(), "crew.escape.check.approve");
    let detail: String = store
        .reader()
        .query_row(
            "SELECT detail FROM audit_events WHERE action = 'crew.escape.check.approve'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(detail.contains(COMMIT) && detail.contains(HASH), "{detail}");
    let (_, again, written) = store
        .approve_escape_check(PROJECT, &id, &writing_seen(), COMMIT, HASH, AT + 9)
        .unwrap();
    assert!(!written);
    assert_eq!(again, check);
    assert_eq!(audits(&store).len(), audits_before + 1);
    // A withdrawn check cannot be approved.
    store
        .withdraw_escape_check(PROJECT, &id, &writing_seen(), AT + 10)
        .unwrap();
    let late = store.approve_escape_check(
        PROJECT,
        &id,
        &seen(EscapeState::Confirmed, Some(EscapeCheckState::Withdrawn)),
        "e".repeat(40).as_str(),
        HASH,
        AT + 11,
    );
    assert!(matches!(late, Err(CoreError::Invalid(_))), "{late:?}");
}

#[test]
fn accept_is_refused_when_the_proposal_changed_or_the_escape_became_a_tooling_gap_and_clears_any_diff()
 {
    let (_dir, store, _task, id) = confirmed();
    proposed(&store, &id);
    let proposed_seen = seen(EscapeState::Confirmed, Some(EscapeCheckState::Proposed));
    for changed in [
        CheckProposal {
            path: "tests/other.txt",
            ..proposal()
        },
        CheckProposal {
            asserts: "Something else.",
            ..proposal()
        },
        CheckProposal {
            run: "git grep -q y",
            ..proposal()
        },
        CheckProposal {
            fix_commit: &"9".repeat(40),
            ..proposal()
        },
    ] {
        let refused =
            store.accept_escape_check(PROJECT, &id, &proposed_seen, &changed, TASK, AT + 1);
        assert!(matches!(refused, Err(CoreError::Stale(_))), "{refused:?}");
    }
    assert_eq!(
        store.crew_escape_check(&id).unwrap().unwrap().state,
        EscapeCheckState::Proposed
    );
    store
        .set_escape_gap(
            PROJECT,
            &id,
            &orbyn_contracts::EscapeSeen {
                state: EscapeState::Confirmed,
                gap: None,
            },
            EscapeGap::ToolingGap,
            AT + 2,
        )
        .unwrap();
    let tooling = store.accept_escape_check(
        PROJECT,
        &id,
        &EscapeCheckSeen {
            state: EscapeState::Confirmed,
            gap: Some(EscapeGap::ToolingGap),
            check: Some(EscapeCheckState::Proposed),
        },
        &proposal(),
        TASK,
        AT + 3,
    );
    assert!(matches!(tooling, Err(CoreError::Invalid(_))), "{tooling:?}");
    assert_eq!(
        store.crew_escape_check(&id).unwrap().unwrap().state,
        EscapeCheckState::Proposed
    );

    // A retry after a failure starts clean: no diff and no approval carry over.
    let (_dir, store, _task, id) = confirmed();
    writing(&store, &id);
    record_diff(&store, &id, COMMIT, HASH);
    store
        .approve_escape_check(PROJECT, &id, &writing_seen(), COMMIT, HASH, AT + 6)
        .unwrap();
    store
        .finish_escape_check(
            &id,
            TASK,
            &CheckFinish::Failed {
                note: "no".into(),
                proof: None,
            },
            AT + 7,
        )
        .unwrap();
    store
        .retry_escape_check(
            PROJECT,
            &id,
            &seen(EscapeState::Confirmed, Some(EscapeCheckState::Failed)),
            AT + 8,
        )
        .unwrap();
    let check = store.crew_escape_check(&id).unwrap().unwrap();
    assert_eq!(
        (
            check.diff_commit,
            check.diff_hash,
            check.approved_commit,
            check.approved_hash,
            check.diff_paths.len()
        ),
        (None, None, None, None, 0)
    );
}

#[test]
fn the_tick_finds_writing_checks_with_no_task_and_withdrawn_checks_still_queued() {
    let (_dir, store, task, id) = confirmed();
    // `TASK` is no task of this store's: a writing check on it is one a crash left behind.
    writing(&store, &id);
    let stuck = store.crew_escape_checks_without_task().unwrap();
    assert_eq!(
        stuck
            .iter()
            .map(|c| c.escape_id.as_str())
            .collect::<Vec<_>>(),
        [id.as_str()]
    );
    store
        .withdraw_escape_check(PROJECT, &id, &writing_seen(), AT + 2)
        .unwrap();
    assert!(store.crew_escape_checks_without_task().unwrap().is_empty());
    assert!(
        store
            .crew_escape_checks_unproven_queued()
            .unwrap()
            .is_empty()
    );
    // A check on a real task, withdrawn while that task waits in the queue.
    let (_dir, store, _task, id2) = confirmed();
    let task2 = format!("sha256:{:064x}", 0xbb);
    store
        .reader()
        .execute(
            "INSERT INTO crew_tasks (id, crew_id, run_id, created_at, updated_at) VALUES (?1, 'crew-1', 'run-bb', 't', 't')",
            [&task2],
        )
        .unwrap();
    proposed(&store, &id2);
    store
        .accept_escape_check(
            PROJECT,
            &id2,
            &seen(EscapeState::Confirmed, Some(EscapeCheckState::Proposed)),
            &proposal(),
            &task2,
            AT + 1,
        )
        .unwrap();
    assert!(store.crew_escape_checks_without_task().unwrap().is_empty());
    store
        .withdraw_escape_check(PROJECT, &id2, &writing_seen(), AT + 2)
        .unwrap();
    store
        .add_merge_entry(&crate::crew::fixtures::merge_entry(&task2, "/p", "x"))
        .unwrap();
    assert_eq!(
        store.crew_escape_checks_unproven_queued().unwrap(),
        [(id2.clone(), task2.clone())]
    );
    let _ = task;
}

#[test]
fn every_task_an_accept_created_is_recorded_for_good_and_found_through_its_escape() {
    let (_dir, store, _task, id) = confirmed();
    assert_eq!(
        store.crew_escape_check_task_status(TASK).unwrap(),
        (false, None)
    );
    writing(&store, &id);
    let (recorded, current) = store.crew_escape_check_task_status(TASK).unwrap();
    assert!(recorded);
    assert_eq!(current.unwrap().task_id.as_deref(), Some(TASK));
    // Fail, retry: the check forgets its task, the record does not.
    store
        .finish_escape_check(
            &id,
            TASK,
            &CheckFinish::Failed {
                note: "no".into(),
                proof: None,
            },
            AT + 3,
        )
        .unwrap();
    store
        .retry_escape_check(
            PROJECT,
            &id,
            &seen(EscapeState::Confirmed, Some(EscapeCheckState::Failed)),
            AT + 4,
        )
        .unwrap();
    let (recorded, current) = store.crew_escape_check_task_status(TASK).unwrap();
    assert!(recorded);
    assert_eq!(current.unwrap().task_id, None);
    // Withdraw and propose again: the check row is replaced, the record stays.
    store
        .withdraw_escape_check(
            PROJECT,
            &id,
            &seen(EscapeState::Confirmed, Some(EscapeCheckState::Proposed)),
            AT + 5,
        )
        .unwrap();
    store
        .propose_escape_check(
            PROJECT,
            &id,
            &seen(EscapeState::Confirmed, Some(EscapeCheckState::Withdrawn)),
            &proposal(),
            AT + 6,
        )
        .unwrap();
    let (recorded, current) = store.crew_escape_check_task_status(TASK).unwrap();
    assert!(recorded);
    assert_eq!(current.unwrap().state, EscapeCheckState::Proposed);
    let count: i64 = store
        .reader()
        .query_row(
            "SELECT COUNT(*) FROM crew_escape_check_tasks WHERE task_id = ?1",
            [TASK],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn a_withdraw_is_refused_while_the_merge_lands_and_nothing_changes() {
    let (_dir, store, _task, id) = confirmed();
    let task2 = format!("sha256:{:064x}", 0xcc);
    store
        .reader()
        .execute(
            "INSERT INTO crew_tasks (id, crew_id, run_id, created_at, updated_at) VALUES (?1, 'crew-1', 'run-cc', 't', 't')",
            [&task2],
        )
        .unwrap();
    proposed(&store, &id);
    store
        .accept_escape_check(
            PROJECT,
            &id,
            &seen(EscapeState::Confirmed, Some(EscapeCheckState::Proposed)),
            &proposal(),
            &task2,
            AT + 1,
        )
        .unwrap();
    store
        .add_merge_entry(&crate::crew::fixtures::merge_entry(&task2, "/p", "x"))
        .unwrap();
    store
        .set_merge_entry_state(&task2, "merging", None)
        .unwrap();
    let audits_before = audits(&store).len();
    let refused = store.withdraw_escape_check(PROJECT, &id, &writing_seen(), AT + 2);
    assert!(
        matches!(refused, Err(CoreError::Invalid(ref why)) if why.contains("landing")),
        "{refused:?}"
    );
    assert_eq!(audits(&store).len(), audits_before);
    assert_eq!(
        store.crew_escape_check(&id).unwrap().unwrap().state,
        EscapeCheckState::Writing
    );
    store.set_merge_entry_state(&task2, "queued", None).unwrap();
    store
        .withdraw_escape_check(PROJECT, &id, &writing_seen(), AT + 3)
        .unwrap();
}

/// Every step of a check's life inside ONE clock second, even with the same task id each attempt (the worst a same-tick accept can
/// give): the unique `(escape, kind, cause)` of the trail never refuses a person's act, and the trail stays complete and in order.
#[test]
fn a_check_event_never_collides_when_the_whole_life_runs_inside_one_tick() {
    let (_dir, store, _task, id) = confirmed();
    let open = Some(EscapeCheckState::Proposed);
    let failed = || CheckFinish::Failed {
        proof: None,
        note: "no".into(),
    };
    let propose = || {
        store.propose_escape_check(
            PROJECT,
            &id,
            &seen(EscapeState::Confirmed, None),
            &proposal(),
            AT,
        )
    };
    // Propose, withdraw, propose again, accept, fail, retry, accept again, fail again, retry again: all at the same second.
    propose().unwrap();
    store
        .withdraw_escape_check(PROJECT, &id, &seen(EscapeState::Confirmed, open), AT)
        .unwrap();
    store
        .propose_escape_check(
            PROJECT,
            &id,
            &seen(EscapeState::Confirmed, Some(EscapeCheckState::Withdrawn)),
            &proposal(),
            AT,
        )
        .unwrap();
    for attempt in 1..=2 {
        let (_, _, started) = store
            .accept_escape_check(
                PROJECT,
                &id,
                &seen(EscapeState::Confirmed, open),
                &proposal(),
                TASK,
                AT,
            )
            .unwrap();
        assert!(started, "attempt {attempt} starts one worker");
        assert!(store.finish_escape_check(&id, TASK, &failed(), AT).unwrap());
        store
            .retry_escape_check(
                PROJECT,
                &id,
                &seen(EscapeState::Confirmed, Some(EscapeCheckState::Failed)),
                AT,
            )
            .unwrap();
    }
    assert_eq!(store.crew_escape_check(&id).unwrap().unwrap().attempts, 2);
    use EscapeEventKind::*;
    assert_eq!(
        kinds(&store, &id),
        [
            Detected,
            Confirmed,
            CheckProposed,
            CheckWithdrawn,
            CheckProposed,
            CheckFailed,
            CheckProposed,
            CheckFailed,
            CheckProposed
        ]
    );
    let causes: std::collections::HashSet<String> = store
        .crew_escape_events(&id, 50)
        .unwrap()
        .into_iter()
        .map(|e| e.cause)
        .collect();
    assert_eq!(causes.len(), 9, "every event has its own cause: {causes:?}");
}

/// A double click on each route of a person's act is one change: the second answers with the check as it is, writes no audit row, no
/// event and no second attempt.
#[test]
fn a_double_click_on_each_act_changes_nothing_the_second_time() {
    let (_dir, store, _task, id) = confirmed();
    let proposed_seen = seen(EscapeState::Confirmed, Some(EscapeCheckState::Proposed));
    for _ in 0..2 {
        store
            .propose_escape_check(
                PROJECT,
                &id,
                &seen(EscapeState::Confirmed, None),
                &proposal(),
                AT,
            )
            .unwrap();
    }
    for _ in 0..2 {
        store
            .accept_escape_check(PROJECT, &id, &proposed_seen, &proposal(), TASK, AT)
            .unwrap();
    }
    assert_eq!(store.crew_escape_check(&id).unwrap().unwrap().attempts, 1);
    store
        .finish_escape_check(
            &id,
            TASK,
            &CheckFinish::Failed {
                proof: None,
                note: "no".into(),
            },
            AT,
        )
        .unwrap();
    for _ in 0..2 {
        store
            .retry_escape_check(
                PROJECT,
                &id,
                &seen(EscapeState::Confirmed, Some(EscapeCheckState::Failed)),
                AT,
            )
            .unwrap();
    }
    for _ in 0..2 {
        store
            .withdraw_escape_check(PROJECT, &id, &proposed_seen, AT)
            .unwrap();
    }
    use EscapeEventKind::*;
    assert_eq!(
        kinds(&store, &id),
        [
            Detected,
            Confirmed,
            CheckProposed,
            CheckFailed,
            CheckProposed,
            CheckWithdrawn
        ]
    );
    assert_eq!(
        audits(&store),
        [
            "crew.escape.check.propose",
            "crew.escape.check.accept",
            "crew.escape.check.retry",
            "crew.escape.check.withdraw"
        ]
    );
}

/// An event already on the trail for the same escape, kind and cause is "already recorded": the act still lands and nothing errors.
#[test]
fn an_event_that_is_already_recorded_is_not_an_error_to_the_person() {
    let (_dir, store, _task, id) = confirmed();
    writing(&store, &id);
    store
        .writer()
        .execute(
            "INSERT INTO crew_escape_events (escape_id, project_id, kind, decided_by, cause, at)
             VALUES (?1, ?2, 'check_failed', 'core', ?3, 1)",
            rusqlite::params![id, PROJECT, format!("core:{TASK}:1")],
        )
        .unwrap();
    let wrote = store
        .finish_escape_check(
            &id,
            TASK,
            &CheckFinish::Failed {
                proof: None,
                note: "no".into(),
            },
            AT + 2,
        )
        .unwrap();
    assert!(wrote);
    assert_eq!(
        store.crew_escape_check(&id).unwrap().unwrap().state,
        EscapeCheckState::Failed
    );
    assert_eq!(
        kinds(&store, &id)
            .iter()
            .filter(|k| **k == EscapeEventKind::CheckFailed)
            .count(),
        1
    );
}
