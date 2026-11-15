//! Registration/upgrade regressions for review controls, through the ordered
//! forge migration 45 and review-control migration 46.

use rusqlite::{Connection, params};

use super::{Store, migrations};
use crate::testing::TempDir;

#[test]
fn review_controls_upgrade_from_44_preserves_existing_data_and_reopens() {
    let dir = TempDir::new();
    let path = dir.path().join("store.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL)").unwrap();
    for (version, sql) in migrations::MIGRATIONS.iter().filter(|(v, _)| *v <= 44) {
        conn.execute_batch(sql).unwrap();
        conn.execute(
            "INSERT INTO schema_migrations VALUES (?1,'fixture')",
            [version],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO settings(key,value) VALUES ('review-migration-control','preserved')",
        [],
    )
    .unwrap();
    drop(conn);
    let store = Store::open(&path, "review-upgrade").unwrap();
    let upgrade = store
        .upgrade()
        .expect("46 must be registered before this gate runs");
    assert_eq!(upgrade.from, 44);
    assert!(upgrade.to >= 46);
    assert_eq!(
        store
            .reader()
            .query_row(
                "SELECT value FROM settings WHERE key='review-migration-control'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "preserved"
    );
    assert_eq!(
        store
            .reader()
            .query_row("SELECT COUNT(*) FROM crew_review_controls", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let initial = store.crew_acp_tiers().unwrap();
    assert_eq!(initial.revision, 0);
    assert!(initial.assignments.is_empty());
    drop(store);
    let reopened = Store::open(&path, "review-reopen").unwrap();
    assert!(reopened.upgrade().is_none());
    assert_eq!(reopened.crew_acp_tiers().unwrap().revision, 0);
}

#[test]
fn saved_tier_pins_and_a_later_empty_override_survive_reopen() {
    let dir = TempDir::new();
    let path = dir.path().join("tier-pins.db");
    let pins = std::collections::BTreeMap::from([(
        "fixture-acp".into(),
        crate::crew::AcpTierAssignment {
            tier: orbyn_contracts::RouteTier::Lead,
            model: "fixture-default".into(),
        },
    )]);
    let store = Store::open(&path, "pin-save").unwrap();
    assert_eq!(store.save_crew_acp_tiers(0, &pins).unwrap().revision, 1);
    drop(store);
    let reopened = Store::open(&path, "pin-reopen").unwrap();
    assert!(reopened.upgrade().is_none());
    let saved = reopened.crew_acp_tiers().unwrap();
    assert_eq!(saved.revision, 1);
    assert_eq!(saved.assignments, pins);
    assert!(
        reopened
            .save_crew_acp_tiers(0, &std::collections::BTreeMap::new())
            .is_err()
    );
    assert_eq!(reopened.crew_acp_tiers().unwrap().assignments, pins);
    assert_eq!(
        reopened
            .save_crew_acp_tiers(1, &std::collections::BTreeMap::new())
            .unwrap()
            .revision,
        2
    );
    drop(reopened);
    let cleared = Store::open(&path, "clear-reopen")
        .unwrap()
        .crew_acp_tiers()
        .unwrap();
    assert_eq!(
        cleared.revision, 2,
        "the empty map remains an explicit override of config seeds"
    );
    assert!(cleared.assignments.is_empty());
}

#[test]
fn a_failed_review_schema_attempt_has_no_partial_tables_or_version_receipt() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_at TEXT NOT NULL)",
    )
    .unwrap();
    for (version, sql) in migrations::MIGRATIONS.iter().filter(|(v, _)| *v <= 44) {
        conn.execute_batch(sql).unwrap();
        conn.execute(
            "INSERT INTO schema_migrations VALUES (?1,'fixture')",
            [version],
        )
        .unwrap();
    }
    {
        let tx = conn.transaction().unwrap();
        tx.execute_batch(include_str!("crew_review_controls.sql"))
            .unwrap();
        assert!(
            tx.execute_batch("INSERT INTO no_such_table VALUES (1)")
                .is_err()
        );
        // Drop the failed attempt, as migrate does before returning an error.
    }
    assert_eq!(conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE name IN ('crew_review_controls','crew_acp_tiers','crew_acp_tier_revision','crew_review_publications')", [], |r|r.get::<_,i64>(0)).unwrap(), 0);
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version=46",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    // A clean retry uses the same SQL and records its version in the transaction.
    let tx = conn.transaction().unwrap();
    tx.execute_batch(include_str!("crew_review_controls.sql"))
        .unwrap();
    tx.execute(
        "INSERT INTO schema_migrations VALUES (?1,'retry')",
        params![46],
    )
    .unwrap();
    tx.commit().unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version=46",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}
