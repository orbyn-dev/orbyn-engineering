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
    assert_eq!((upgrade.from, upgrade.to), (51, 52));
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
    let accepted =
        store.accept_escape_check(PROJECT, &id, &seen(EscapeState::Confirmed, None), TASK, AT);
    assert!(matches!(accepted, Err(CoreError::Stale(_))), "{accepted:?}");
    assert_eq!(audits(&store), ["crew.escape.check.propose"]);
    let elsewhere = store.accept_escape_check(
        "another-project",
        &id,
        &seen(EscapeState::Confirmed, Some(EscapeCheckState::Proposed)),
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
            format!("core:{TASK}").as_str()
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
    let early = store.accept_escape_check(PROJECT, &id, &seen_failed, TASK, AT + 3);
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
