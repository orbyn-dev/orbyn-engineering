//! Capacity regressions use real merges and the public store writers.
use super::crew_escapes_tests::{PROJECT, escapes, open, seed};
use super::*;
use orbyn_contracts::{EscapeGap, EscapeSeen, EscapeState};
use rusqlite::params;

fn possible(store: &Store, n: u32) -> (String, String) {
    let task = seed(&store.writer(), n, "waiting", None, "alpha", i64::from(n));
    store.set_merge_reverted(&task, "feed01").unwrap();
    let id = store.crew_escape_of_merge(&task).unwrap().unwrap().id;
    (task, id)
}
fn seen(store: &Store, id: &str) -> EscapeSeen {
    let row = store.crew_escape(id).unwrap().unwrap();
    EscapeSeen {
        state: row.state,
        gap: row.gap,
    }
}
fn fill(store: &Store, id: &str, count: u32) {
    store
        .writer()
        .execute(
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i < ?2)
         INSERT INTO crew_escape_events(escape_id,project_id,kind,decided_by,cause,at)
         SELECT ?1, 'p1', 'reclassified', 'person', 'history:' || i, 1 FROM n WHERE ?2 > 0",
            params![id, count],
        )
        .unwrap();
}

#[test]
fn gap_capacity_reserves_the_last_visible_event_for_a_decision() {
    let (_dir, store) = open();
    let (_, id) = possible(&store, 1);
    for n in 0..98 {
        store
            .set_escape_gap(
                PROJECT,
                &id,
                &seen(&store, &id),
                if n % 2 == 0 {
                    EscapeGap::TestGap
                } else {
                    EscapeGap::ToolingGap
                },
                i64::from(n) + 10,
            )
            .unwrap();
    }
    let before = store.crew_escape_events(&id, 101).unwrap();
    assert_eq!(before.len(), 99);
    assert!(
        store
            .set_escape_gap(PROJECT, &id, &seen(&store, &id), EscapeGap::TestGap, 200)
            .is_err()
    );
    assert_eq!(store.crew_escape_events(&id, 101).unwrap(), before);
    store
        .decide_escape(
            PROJECT,
            &id,
            EscapeDecision::Confirm,
            &seen(&store, &id),
            None,
            201,
        )
        .unwrap();
    assert_eq!(store.crew_escape_events(&id, 101).unwrap().len(), 100);
}

#[test]
fn even_an_identical_gap_is_refused_after_dismissal() {
    let (_dir, store) = open();
    let (_, id) = possible(&store, 1);
    store
        .set_escape_gap(PROJECT, &id, &seen(&store, &id), EscapeGap::TestGap, 10)
        .unwrap();
    store
        .decide_escape(
            PROJECT,
            &id,
            EscapeDecision::Dismiss,
            &seen(&store, &id),
            None,
            11,
        )
        .unwrap();
    assert!(matches!(
        store.set_escape_gap(PROJECT, &id, &seen(&store, &id), EscapeGap::TestGap, 12),
        Err(crate::CoreError::Invalid(_))
    ));
}

#[test]
fn pressure_evicts_dismissed_history_without_reoffering_or_remarking_it() {
    let (_dir, store) = open();
    let (old, old_id) = possible(&store, 1);
    store
        .decide_escape(
            PROJECT,
            &old_id,
            EscapeDecision::Dismiss,
            &seen(&store, &old_id),
            None,
            10,
        )
        .unwrap();
    let (_, current) = possible(&store, 2);
    fill(&store, &old_id, 19_997);
    store
        .decide_escape(
            PROJECT,
            &current,
            EscapeDecision::Confirm,
            &seen(&store, &current),
            None,
            20,
        )
        .unwrap();
    assert!(store.crew_escape(&old_id).unwrap().is_none());
    assert!(store.crew_escape_events(&old_id, 100).unwrap().is_empty());
    assert!(store.mark_escape(PROJECT, &old, None, None, 21).is_err());
    assert!(
        !store
            .crew_escape_scan_candidates(0, 100, None)
            .unwrap()
            .iter()
            .any(|(merge, _)| merge.task_id == old)
    );
    store.set_merge_reverted(&old, "feed02").unwrap();
    assert!(store.crew_escape_of_merge(&old).unwrap().is_none());
    assert_eq!(escapes(&store).len(), 1);
}

#[test]
fn an_evicted_possible_merge_requires_an_explicit_person_mark_to_return() {
    let (_dir, store) = open();
    let (old, old_id) = possible(&store, 1);
    let (_, current) = possible(&store, 2);
    fill(&store, &old_id, 19_998);
    store
        .decide_escape(
            PROJECT,
            &current,
            EscapeDecision::Confirm,
            &seen(&store, &current),
            None,
            20,
        )
        .unwrap();
    assert!(store.crew_escape(&old_id).unwrap().is_none());
    let added = store
        .record_possible_escapes(
            PROJECT,
            &[PossibleEscape {
                project_id: PROJECT.into(),
                merge_task_id: old.clone(),
                source: orbyn_contracts::EscapeSource::Revert,
                cause: "again".into(),
                reason: "repeated".into(),
            }],
            21,
        )
        .unwrap();
    assert_eq!((added.added, added.already_held), (0, 1));
    store.set_merge_reverted(&old, "feed02").unwrap();
    assert!(store.crew_escape_of_merge(&old).unwrap().is_none());
    assert_eq!(
        store
            .mark_escape(PROJECT, &old, None, None, 22)
            .unwrap()
            .state,
        EscapeState::Confirmed
    );
}

#[test]
fn a_failed_retention_audit_rolls_back_the_whole_person_decision() {
    let (_dir, store) = open();
    let (_, old) = possible(&store, 1);
    let (_, current) = possible(&store, 2);
    fill(&store, &old, 19_998);
    store.writer().execute_batch("CREATE TRIGGER refuse_retention BEFORE INSERT ON audit_events WHEN NEW.action='crew.escape.retain' BEGIN SELECT RAISE(ABORT,'retention audit unavailable'); END;").unwrap();
    let error = store
        .decide_escape(
            PROJECT,
            &current,
            EscapeDecision::Confirm,
            &seen(&store, &current),
            None,
            20,
        )
        .unwrap_err();
    assert!(
        error.to_string().contains("retention audit unavailable"),
        "{error}"
    );
    assert!(store.crew_escape(&old).unwrap().is_some());
    assert_eq!(
        store.crew_escape(&current).unwrap().unwrap().state,
        EscapeState::Possible
    );
    assert_eq!(store.crew_escape_room(PROJECT).unwrap(), (2, 20_000));
}

#[test]
fn current_target_and_unclassified_orphan_history_cannot_be_cleared() {
    let (_dir, store) = open();
    let (_, current) = possible(&store, 1);
    fill(&store, "orphan", 19_999);
    let error = store
        .decide_escape(
            PROJECT,
            &current,
            EscapeDecision::Confirm,
            &seen(&store, &current),
            None,
            20,
        )
        .unwrap_err();
    assert!(error.to_string().contains("protected history"), "{error}");
    assert_eq!(store.crew_escape_room(PROJECT).unwrap(), (1, 20_000));
    assert_eq!(
        store.crew_escape(&current).unwrap().unwrap().state,
        EscapeState::Possible
    );
}

#[test]
fn migration_preserves_history_and_fences_an_already_open_old_writer() {
    let dir = crate::testing::TempDir::new();
    let path = dir.path().join("old.db");
    let old = rusqlite::Connection::open(&path).unwrap();
    old.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL);").unwrap();
    for (version, sql) in migrations::MIGRATIONS.iter().filter(|(v, _)| *v <= 56) {
        old.execute_batch(sql).unwrap();
        old.execute(
            "INSERT INTO schema_migrations VALUES(?1,'fixture')",
            [version],
        )
        .unwrap();
    }
    let task = seed(&old, 1, "waiting", None, "alpha", 1);
    old.execute(
        "UPDATE crew_merges SET review_state='reverted' WHERE task_id=?1",
        [&task],
    )
    .unwrap();
    let id: String = old
        .query_row(
            "SELECT id FROM crew_escapes WHERE merge_task_id=?1",
            [&task],
            |r| r.get(0),
        )
        .unwrap();
    let store = Store::open(&path, "new-writer").unwrap();
    assert_eq!(store.crew_escape_events(&id, 100).unwrap().len(), 1);
    assert!(
        old.execute(
            "UPDATE crew_escapes SET gap='test_gap',gap_by='person' WHERE id=?1",
            [&id]
        )
        .is_err(),
        "an already-open pre-migration writer must be fenced"
    );
    assert!(old.execute("INSERT INTO crew_escape_events(escape_id,project_id,kind,decided_by,cause,at) VALUES(?1,'p1','reclassified','person','old',1)",[&id]).is_err());
    assert!(store.crew_escape(&id).unwrap().unwrap().gap.is_none());
    store
        .set_escape_gap(PROJECT, &id, &seen(&store, &id), EscapeGap::TestGap, 10)
        .unwrap();
    assert_eq!(store.crew_escape_events(&id, 100).unwrap().len(), 2);
    assert!(
        store
            .writer()
            .execute("DELETE FROM crew_escape_events WHERE escape_id=?1", [&id])
            .is_err(),
        "current writers still need an audited retention grant"
    );
    drop(store);
    let reopened = Store::open(&path, "reopen").unwrap();
    assert_eq!(reopened.crew_escape_events(&id, 100).unwrap().len(), 2);
}

#[test]
fn full_scanner_preflight_is_read_only_and_allows_eligible_retention() {
    let (_dir, store) = open();
    let (_, old) = possible(&store, 1);
    let next = seed(&store.writer(), 2, "waiting", None, "alpha", 2);
    fill(&store, &old, 19_999);
    assert!(store.crew_escape_can_record(PROJECT).unwrap());
    assert_eq!(store.crew_escape_room(PROJECT).unwrap(), (1, 20_000));
    let added = store
        .record_possible_escapes(
            PROJECT,
            &[PossibleEscape {
                project_id: PROJECT.into(),
                merge_task_id: next.clone(),
                source: orbyn_contracts::EscapeSource::PostMergeCheck,
                cause: "check:next".into(),
                reason: "Failed after merge".into(),
            }],
            20,
        )
        .unwrap();
    assert_eq!(added.added, 1);
    assert!(store.crew_escape(&old).unwrap().is_none());
    assert!(store.crew_escape_of_merge(&next).unwrap().is_some());
    assert_eq!(store.crew_escape_room(PROJECT).unwrap(), (1, 1));
}

#[test]
fn recorded_check_tasks_are_permanent_and_prevent_retention() {
    let (_dir, store) = open();
    let (_, old) = possible(&store, 1);
    let (_, current) = possible(&store, 2);
    store.writer().execute("INSERT INTO crew_escape_check_tasks(task_id,escape_id,created_unix) VALUES('old-check',?1,1)",[&old]).unwrap();
    fill(&store, &old, 19_998);
    assert!(
        store
            .decide_escape(
                PROJECT,
                &current,
                EscapeDecision::Confirm,
                &seen(&store, &current),
                None,
                20
            )
            .is_err()
    );
    assert!(store.crew_escape(&old).unwrap().is_some());
    assert!(
        store
            .writer()
            .execute(
                "DELETE FROM crew_escape_check_tasks WHERE task_id='old-check'",
                []
            )
            .is_err()
    );
}

#[test]
fn a_later_decision_audit_failure_restores_evicted_rows_events_and_tombstones() {
    let (_dir, store) = open();
    let (_, old) = possible(&store, 1);
    let (_, current) = possible(&store, 2);
    fill(&store, &old, 19_998);
    store.writer().execute_batch("CREATE TRIGGER refuse_confirm BEFORE INSERT ON audit_events WHEN NEW.action='crew.escape.confirm' BEGIN SELECT RAISE(ABORT,'confirm audit unavailable'); END;").unwrap();
    assert!(
        store
            .decide_escape(
                PROJECT,
                &current,
                EscapeDecision::Confirm,
                &seen(&store, &current),
                None,
                20
            )
            .unwrap_err()
            .to_string()
            .contains("confirm audit unavailable")
    );
    assert!(store.crew_escape(&old).unwrap().is_some());
    assert_eq!(store.crew_escape_room(PROJECT).unwrap(), (2, 20_000));
    for table in ["crew_escape_tombstones", "crew_escape_retention_grants"] {
        let count: i64 = store
            .reader()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}

#[test]
fn unprunable_capacity_never_undoes_an_api_or_sql_revert() {
    let (_dir, store) = open();
    let api = seed(&store.writer(), 1, "waiting", None, "alpha", 1);
    let sql = seed(&store.writer(), 2, "waiting", None, "alpha", 2);
    fill(&store, "orphan", 20_000);
    assert!(!store.crew_escape_can_record(PROJECT).unwrap());
    store.set_merge_reverted(&api, "feed01").unwrap();
    store
        .writer()
        .execute(
            "UPDATE crew_merges SET review_state='reverted' WHERE task_id=?1",
            [&sql],
        )
        .unwrap();
    assert!(store.crew_escape_of_merge(&api).unwrap().is_none());
    assert!(store.crew_escape_of_merge(&sql).unwrap().is_none());
    assert_eq!(store.crew_escape_room(PROJECT).unwrap(), (0, 20_000));
}

#[test]
fn current_protocol_works_with_untrusted_schema_and_parent_cleanup_keeps_proof_history() {
    let (_dir, store) = open();
    let (task, id) = possible(&store, 1);
    store
        .writer()
        .pragma_update(None, "trusted_schema", "OFF")
        .unwrap();
    store
        .set_escape_gap(PROJECT, &id, &seen(&store, &id), EscapeGap::TestGap, 10)
        .unwrap();
    store.writer().execute("INSERT INTO crew_escape_check_tasks(task_id,escape_id,created_unix) VALUES('permanent-proof',?1,1)",[&id]).unwrap();
    store
        .writer()
        .execute("DELETE FROM crew_merges WHERE task_id=?1", [&task])
        .unwrap();
    assert!(store.crew_escape(&id).unwrap().is_none());
    assert_eq!(
        store.crew_escape_events(&id, 100).unwrap().len(),
        2,
        "orphan events retain their immutable history"
    );
    assert!(
        store
            .crew_escape_check_task_status("permanent-proof")
            .unwrap()
            .0
    );
    store.delete_thread("lead-1", "user").unwrap();
}

#[test]
fn a_concurrent_confirmation_is_never_cleared_as_confirmed() {
    let (_dir, store) = open();
    let (_, old) = possible(&store, 1);
    let (_, current) = possible(&store, 2);
    fill(&store, &old, 19_998);
    let old_seen = seen(&store, &old);
    let current_seen = seen(&store, &current);
    let barrier = std::sync::Barrier::new(2);
    let (a, b) = std::thread::scope(|scope| {
        let a = scope.spawn(|| {
            barrier.wait();
            store.decide_escape(PROJECT, &old, EscapeDecision::Confirm, &old_seen, None, 20)
        });
        let b = scope.spawn(|| {
            barrier.wait();
            store.decide_escape(
                PROJECT,
                &current,
                EscapeDecision::Confirm,
                &current_seen,
                None,
                20,
            )
        });
        (a.join().unwrap(), b.join().unwrap())
    });
    assert!(a.is_ok() || b.is_ok());
    for (id, result) in [(&old, a), (&current, b)] {
        if result.is_ok() {
            assert_eq!(
                store.crew_escape(id).unwrap().unwrap().state,
                EscapeState::Confirmed
            );
        }
    }
    let count: i64 = store
        .reader()
        .query_row(
            "SELECT COUNT(*) FROM crew_escape_tombstones WHERE state='confirmed'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn check_rows_are_protected_even_if_a_legacy_escape_is_possible() {
    let (_dir, store) = open();
    let (_, old) = possible(&store, 1);
    let (_, current) = possible(&store, 2);
    store.writer().execute("INSERT INTO crew_escape_checks(escape_id,project_id,path,asserts,run,fix_commit,state,proposed_unix,updated_unix) VALUES(?1,'p1','tests/regression.rs','regression','cargo test',?2,'proposed',1,1)",params![old,"a".repeat(40)]).unwrap();
    fill(&store, &old, 19_998);
    assert!(
        store
            .decide_escape(
                PROJECT,
                &current,
                EscapeDecision::Confirm,
                &seen(&store, &current),
                None,
                20
            )
            .is_err()
    );
    assert!(store.crew_escape(&old).unwrap().is_some());
    assert!(store.crew_escape_check(&old).unwrap().is_some());
}

#[test]
fn project_pressure_cannot_clear_another_projects_eligible_history() {
    let (_dir, store) = open();
    let (_, current) = possible(&store, 1);
    let foreign = seed(&store.writer(), 2, "waiting", None, "alpha", 2);
    store.writer().execute_batch("INSERT INTO projects(id,name,path,trusted,is_git,created_at,managed,scratch) VALUES('p2','Other','/other',0,1,'t',0,0); INSERT INTO threads(id,project_id,title,engine,cwd,created_at,updated_at) VALUES('other-lead','p2','Other','fake','/other','t','t');").unwrap();
    store
        .writer()
        .execute(
            "UPDATE crew_runs SET lead_thread_id='other-lead' WHERE task_id=?1",
            [&foreign],
        )
        .unwrap();
    store.set_merge_reverted(&foreign, "feed01").unwrap();
    let other = store.crew_escape_of_merge(&foreign).unwrap().unwrap();
    fill(&store, "orphan", 19_999);
    assert!(
        store
            .decide_escape(
                PROJECT,
                &current,
                EscapeDecision::Confirm,
                &seen(&store, &current),
                None,
                20
            )
            .is_err()
    );
    assert_eq!(store.crew_escape(&other.id).unwrap().unwrap(), other);
    assert_eq!(store.crew_escape_room("p2").unwrap(), (1, 1));
}

#[test]
fn repair_invalid_scan_items_cannot_clear_history_for_zero_admissions() {
    let (_dir, store) = open();
    let (_, old) = possible(&store, 1);
    let foreign = seed(&store.writer(), 2, "waiting", None, "alpha", 2);
    store.writer().execute_batch("INSERT INTO projects(id,name,path,trusted,is_git,created_at,managed,scratch) VALUES('p2','Other','/other',0,1,'t',0,0); INSERT INTO threads(id,project_id,title,engine,cwd,created_at,updated_at) VALUES('other-lead','p2','Other','fake','/other','t','t');").unwrap();
    store
        .writer()
        .execute(
            "UPDATE crew_runs SET lead_thread_id='other-lead' WHERE task_id=?1",
            [&foreign],
        )
        .unwrap();
    fill(&store, &old, 19_999);
    let batch = vec!["sha256:missing".to_owned(), foreign]
        .into_iter()
        .map(|merge_task_id| PossibleEscape {
            project_id: PROJECT.into(),
            merge_task_id,
            source: orbyn_contracts::EscapeSource::Revert,
            cause: "revert:again".into(),
            reason: "reverted".into(),
        })
        .collect::<Vec<_>>();
    let added = store.record_possible_escapes(PROJECT, &batch, 30).unwrap();
    assert_eq!((added.added, added.already_held), (0, 2));
    assert!(
        store.crew_escape(&old).unwrap().is_some(),
        "zero admissions must not clear eligible history"
    );
    assert_eq!(store.crew_escape_room(PROJECT).unwrap(), (1, 20_000));
}

#[test]
fn repair_reverting_a_nonmerged_task_cannot_clear_history() {
    let (_dir, store) = open();
    let (_, old) = possible(&store, 1);
    let not_merged = seed(&store.writer(), 2, "waiting", None, "alpha", 2);
    store
        .writer()
        .execute("DELETE FROM crew_merges WHERE task_id=?1", [&not_merged])
        .unwrap();
    fill(&store, &old, 19_999);
    store.set_merge_reverted(&not_merged, "feed02").unwrap();
    assert!(
        store.crew_escape(&old).unwrap().is_some(),
        "a no-op revert must not clear history"
    );
    assert_eq!(store.crew_escape_room(PROJECT).unwrap(), (1, 20_000));
}

#[test]
fn repair_duplicate_valid_scan_items_use_one_admission_slot() {
    let (_dir, store) = open();
    let (_, old) = possible(&store, 1);
    let protected = seed(&store.writer(), 2, "waiting", None, "alpha", 2);
    let protected = store
        .mark_escape(PROJECT, &protected, None, None, 10)
        .unwrap();
    fill(&store, &protected.id, 19_997);
    let incoming = seed(&store.writer(), 3, "waiting", None, "alpha", 3);
    let item = PossibleEscape {
        project_id: PROJECT.into(),
        merge_task_id: incoming.clone(),
        source: orbyn_contracts::EscapeSource::Revert,
        cause: "revert:new".into(),
        reason: "reverted".into(),
    };
    let added = store
        .record_possible_escapes(PROJECT, &[item.clone(), item], 30)
        .unwrap();
    assert_eq!((added.added, added.already_held), (1, 1));
    assert!(store.crew_escape(&old).unwrap().is_none());
    assert_eq!(
        store.crew_escape(&protected.id).unwrap().unwrap(),
        protected
    );
    assert!(store.crew_escape_of_merge(&incoming).unwrap().is_some());
    assert_eq!(store.crew_escape_room(PROJECT).unwrap(), (2, 20_000));
}

#[test]
fn repair_legacy_detail_shows_latest_decisions_without_losing_initial_provenance() {
    let dir = crate::testing::TempDir::new();
    let mut config = crate::CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
    config.crew_preview = true;
    config.engines.lookup_latest = false;
    config.engines.known_engine_ids = Some(Vec::new());
    let core = crate::Core::open(config).unwrap();
    for (n, decision, last) in [
        (
            1,
            EscapeDecision::Confirm,
            orbyn_contracts::EscapeEventKind::Confirmed,
        ),
        (
            2,
            EscapeDecision::Dismiss,
            orbyn_contracts::EscapeEventKind::Dismissed,
        ),
    ] {
        let (_, id) = possible(&core.store, n);
        fill(&core.store, &id, 150);
        core.store
            .decide_escape(PROJECT, &id, decision, &seen(&core.store, &id), None, 200)
            .unwrap();
        let stored = core.store.crew_escape_events(&id, 200).unwrap();
        assert_eq!(stored.len(), 152);
        let detail = core.crew_escape_detail(&id).unwrap();
        assert!(detail.truncated);
        assert_eq!(detail.events.len(), 100);
        assert_eq!(
            detail.events.last().unwrap().kind,
            last,
            "the latest decision must remain visible in a legacy trail"
        );
        assert!(detail.events.windows(2).all(|pair| pair[0].id < pair[1].id));
        assert_eq!(detail.events.first().unwrap().id, stored[52].id);
        assert_eq!(core.store.crew_escape_events(&id, 200).unwrap(), stored);
        assert_eq!(
            core.store.crew_escape_events(&id, 1).unwrap()[0].kind,
            orbyn_contracts::EscapeEventKind::Detected
        );
    }
}
