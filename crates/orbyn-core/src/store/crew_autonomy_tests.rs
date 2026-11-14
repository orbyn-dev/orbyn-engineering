//! Autonomy by area at rest: the audit trail is never changed or removed, a write that the table refuses leaves the mode as it was,
//! and the migration follows the one before it.

use serde_json::json;

use super::crew_schema::CREW_AUTONOMY;
use super::migrations::MIGRATIONS;
use super::*;
use crate::testing::TempDir;

fn demote(store: &Store, cause: &str, at: i64) -> bool {
    store
        .record_area_change(&AreaWrite {
            project_id: "p1",
            area: "src/a",
            kind: "demoted",
            from_mode: Some("full"),
            to_mode: Some("on"),
            decided_by: "breaker",
            cause,
            record: &json!({ "merges": 5, "reverts": 2, "window_days": 14, "escapes": null }),
            at,
            change: AreaChange::Demote {
                trip_reason: json!({ "reverts": 2 }),
                recovery_since: at,
            },
        })
        .unwrap()
}

#[test]
fn the_migration_follows_the_one_before_it() {
    for (i, (version, _)) in MIGRATIONS.iter().enumerate() {
        assert_eq!(
            *version,
            i64::try_from(i).unwrap() + 1,
            "numbering has no gap"
        );
    }
    let (version, sql) = MIGRATIONS[43];
    assert_eq!(version, 44);
    assert_eq!(sql, CREW_AUTONOMY);
    assert!(
        MIGRATIONS[42].1.contains("crew_guard_targets"),
        "43 is the guard's lists"
    );
}

#[test]
fn the_trail_is_never_changed_or_removed() {
    let dir = TempDir::new();
    let store = Store::open(&dir.path().join("orbyn.db"), "a").unwrap();
    assert!(demote(&store, "cause-1", 100));
    let conn = store.writer();
    for sql in [
        "UPDATE crew_area_events SET cause = 'other'",
        "UPDATE crew_area_events SET at = 1",
        "DELETE FROM crew_area_events",
    ] {
        let error = conn.execute(sql, []).unwrap_err().to_string();
        assert!(error.contains("immutable"), "{sql}: {error}");
    }
    drop(conn);
    assert_eq!(store.area_events("p1", "src/a", 10).unwrap().len(), 1);
}

#[test]
fn the_same_cause_is_recorded_once() {
    let dir = TempDir::new();
    let store = Store::open(&dir.path().join("orbyn.db"), "a").unwrap();
    assert!(demote(&store, "cause-1", 100));
    assert!(!demote(&store, "cause-1", 200), "a retry records nothing");
    let mode = store.area_mode("p1", "src/a").unwrap().unwrap();
    assert_eq!((mode.updated_at, mode.tripped_at), (100, Some(100)));
    assert_eq!(store.area_events("p1", "src/a", 10).unwrap().len(), 1);
}

#[test]
fn an_event_the_table_refuses_leaves_the_mode_as_it_was() {
    let dir = TempDir::new();
    let store = Store::open(&dir.path().join("orbyn.db"), "a").unwrap();
    demote(&store, "cause-1", 100);
    let before = store.area_mode("p1", "src/a").unwrap().unwrap();
    for (kind, by, change) in [
        (
            "recovery_reset",
            "nobody",
            AreaChange::ResetRecovery { since: 500 },
        ),
        ("made_up", "breaker", AreaChange::Restore),
    ] {
        let write = AreaWrite {
            project_id: "p1",
            area: "src/a",
            kind,
            from_mode: None,
            to_mode: None,
            decided_by: by,
            cause: "cause-2",
            record: &json!({}),
            at: 500,
            change,
        };
        assert!(store.record_area_change(&write).is_err(), "{kind} by {by}");
    }
    assert_eq!(store.area_mode("p1", "src/a").unwrap().unwrap(), before);
    assert_eq!(
        store.area_events("p1", "src/a", 10).unwrap().len(),
        1,
        "no event without its change"
    );
}

#[test]
fn the_tables_refuse_what_the_design_forbids() {
    let dir = TempDir::new();
    let store = Store::open(&dir.path().join("orbyn.db"), "a").unwrap();
    let conn = store.writer();
    for sql in [
        "INSERT INTO crew_area_modes (project_id, area, state, updated_at) VALUES ('p', 'a', 'half', 1)",
        "INSERT INTO crew_area_modes (project_id, area, state, trip_reason, updated_at) VALUES ('p', 'a', 'follows', 'not json', 1)",
        "INSERT INTO crew_area_modes (project_id, area, state, override, updated_at) VALUES ('p', 'a', 'follows', 'full', 1)",
    ] {
        assert!(conn.execute(sql, []).is_err(), "{sql}");
    }
    assert_eq!(
        conn.execute("INSERT INTO crew_area_modes (project_id, area, state, override, updated_at) VALUES ('p', 'a', 'follows', 'off', 1)", [])
            .unwrap(),
        1
    );
}

#[test]
fn a_mode_update_that_fails_after_the_event_commits_nothing() {
    let dir = TempDir::new();
    let store = Store::open(&dir.path().join("orbyn.db"), "a").unwrap();
    store
        .writer()
        .execute_batch(
            "CREATE TRIGGER test_mode_refused BEFORE INSERT ON crew_area_modes BEGIN SELECT RAISE(ABORT, 'mode refused'); END;",
        )
        .unwrap();
    let write = AreaWrite {
        project_id: "p1",
        area: "src/a",
        kind: "demoted",
        from_mode: Some("full"),
        to_mode: Some("on"),
        decided_by: "breaker",
        cause: "cause-1",
        record: &json!({}),
        at: 100,
        change: AreaChange::Demote {
            trip_reason: json!({}),
            recovery_since: 100,
        },
    };
    let error = store.record_area_change(&write).unwrap_err().to_string();
    assert!(error.contains("mode refused"), "{error}");
    assert!(
        store.area_events("p1", "src/a", 10).unwrap().is_empty(),
        "the event rolled back with the mode"
    );
    assert!(store.area_mode("p1", "src/a").unwrap().is_none());
}

fn person<'a>(
    choice: Option<&'a str>,
    trip: Option<i64>,
    record: &'a serde_json::Value,
) -> PersonWrite<'a> {
    PersonWrite {
        project_id: "p1",
        area: "src/a",
        choice,
        seen: Some(SeenTrip(trip)),
        reason: Some("checked"),
        from_mode: "on",
        to_mode: "full",
        record,
        at: 300,
    }
}

fn audit_rows(store: &Store) -> i64 {
    store
        .reader()
        .query_row(
            "SELECT COUNT(*) FROM audit_events WHERE action LIKE 'crew.area.%'",
            [],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn a_persons_choice_writes_the_audit_row_first_and_a_refused_event_undoes_it() {
    let dir = TempDir::new();
    let store = Store::open(&dir.path().join("orbyn.db"), "a").unwrap();
    assert!(demote(&store, "cause-1", 100));
    let record = json!({ "merges": 5, "reverts": 2, "window_days": 14, "escapes": null });
    store
        .writer()
        .execute_batch(
            "CREATE TRIGGER refuse_set BEFORE INSERT ON crew_area_events WHEN NEW.kind = 'override_set'
             BEGIN SELECT RAISE(ABORT, 'refused'); END;",
        )
        .unwrap();
    let write = person(Some("keep_ceiling"), Some(100), &record);
    assert!(store.record_area_override(&write).is_err());
    assert_eq!(
        audit_rows(&store),
        0,
        "the audit row went with the refused event"
    );
    let mode = store.area_mode("p1", "src/a").unwrap().unwrap();
    assert_eq!(mode.person_override, None, "the area stayed as it was");

    store
        .writer()
        .execute_batch("DROP TRIGGER refuse_set")
        .unwrap();
    assert!(store.record_area_override(&write).unwrap());
    assert!(
        !store.record_area_override(&write).unwrap(),
        "the same choice again"
    );
    assert_eq!(audit_rows(&store), 1);
    let event = &store.area_events("p1", "src/a", 5).unwrap()[0];
    let audit_id: i64 = store
        .reader()
        .query_row(
            "SELECT id FROM audit_events WHERE action = 'crew.area.override'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(event.cause, format!("person:{audit_id}"));
    assert_eq!(event.decided_by, "person");
    let mode = store.area_mode("p1", "src/a").unwrap().unwrap();
    assert_eq!(mode.person_override.as_deref(), Some("keep_ceiling"));
    assert_eq!(
        (mode.state.as_str(), mode.tripped_at),
        ("demoted", Some(100))
    );

    // A person who saw an older trip is stale, and a follow ends the override and nothing else.
    let old = person(None, Some(99), &record);
    assert!(matches!(
        store.record_area_override(&old),
        Err(crate::error::CoreError::Stale(_))
    ));
    assert!(
        store
            .record_area_override(&person(None, Some(100), &record))
            .unwrap()
    );
    let mode = store.area_mode("p1", "src/a").unwrap().unwrap();
    assert_eq!(
        (mode.person_override, mode.state.as_str()),
        (None, "demoted")
    );
    assert_eq!(audit_rows(&store), 2);
}
