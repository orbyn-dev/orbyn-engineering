//! Deterministic lease histories: fixed timestamps make reacquisition and stale writers reproducible.

use orbyn_contracts::{
    CrewObject, CrewPathPattern, CrewTimestamp, CrewUtcSeconds, Lease, LeaseMode, LeaseState,
};

use super::Store;
use crate::testing::TempDir;

fn setup() -> (TempDir, Store, String, Lease) {
    let dir = TempDir::new();
    let store = Store::open(&dir.path().join("orbyn.sqlite3"), "lease-tests").unwrap();
    let parse = |text: &str| CrewObject::parse(&serde_json::from_str(text).unwrap()).unwrap();
    let CrewObject::Brief(brief): CrewObject = parse(include_str!(
        "../../../../docs/crew/protocol/schemas/examples/brief.valid.basic.json"
    )) else {
        unreachable!()
    };
    let CrewObject::AnswerKey(key): CrewObject = parse(include_str!(
        "../../../../docs/crew/protocol/schemas/examples/answer-key.valid.basic.json"
    )) else {
        unreachable!()
    };
    let CrewObject::Lease(lease): CrewObject = parse(include_str!(
        "../../../../docs/crew/protocol/schemas/examples/lease.valid.granted.json"
    )) else {
        unreachable!()
    };
    let task = store.create_crew_task(&brief, &key, "run-01").unwrap();
    (dir, store, task.id, lease)
}

fn transition(was: &Lease, state: LeaseState, stamp: &str) -> Lease {
    let mut lease = was.clone();
    lease.state = state;
    lease.created_at = CrewTimestamp::new(stamp).unwrap();
    lease.id = CrewObject::Lease(lease.clone()).computed_id().unwrap();
    lease
}

#[test]
fn same_second_reacquisition_is_current_and_retry_is_idempotent() {
    let (_dir, store, task, grant) = setup();
    let released = transition(&grant, LeaseState::Released, grant.created_at.as_str());
    store.add_crew_lease_grant(&task, &grant).unwrap();
    store
        .end_crew_lease_snapshot(&task, &grant, &released)
        .unwrap();
    store.add_crew_lease_grant(&task, &grant).unwrap();
    let rows = store.crew_leases(&task).unwrap();
    let current = rows.last().unwrap();
    assert_eq!(
        current.state,
        LeaseState::Granted,
        "a successful reacquisition must be held"
    );
    assert_ne!(
        current.id, grant.id,
        "a new acquisition is not the old content object"
    );
    let count = rows.len();
    store.add_crew_lease_grant(&task, &grant).unwrap();
    assert_eq!(
        store.crew_leases(&task).unwrap().len(),
        count,
        "retrying a current grant changes nothing"
    );
}

#[test]
fn a_stale_expiry_snapshot_cannot_end_a_renewed_grant() {
    let (_dir, store, task, grant) = setup();
    store.add_crew_lease_grant(&task, &grant).unwrap();
    let expired = transition(&grant, LeaseState::Expired, "2026-11-12T10:00:01Z");
    store
        .end_crew_lease_snapshot(&task, &grant, &expired)
        .unwrap();
    let mut renewed = transition(&grant, LeaseState::Granted, "2026-11-12T10:00:01Z");
    renewed.expires_at = CrewUtcSeconds::new("2026-11-12T11:00:00Z").unwrap();
    renewed.id = CrewObject::Lease(renewed.clone()).computed_id().unwrap();
    store.add_crew_lease_grant(&task, &renewed).unwrap();
    // An earlier authorization read resumes after the renewal. Its snapshot is the original grant.
    let stale = transition(&grant, LeaseState::Expired, "2026-11-12T10:00:00Z");
    let current = store
        .end_crew_lease_snapshot(&task, &grant, &stale)
        .unwrap()
        .unwrap();
    assert_eq!(
        current.state,
        LeaseState::Granted,
        "return the current grant to the stale reader"
    );
    assert_eq!(
        store.crew_leases(&task).unwrap().last().unwrap().id,
        current.id
    );
    assert!(
        !store
            .crew_leases(&task)
            .unwrap()
            .iter()
            .any(|lease| lease.id == stale.id),
        "no terminal object is appended from a stale snapshot"
    );
}

#[test]
fn repeated_same_second_cycles_keep_canonical_unique_transitions() {
    let (_dir, store, task, draft) = setup();
    let mut current = store.add_crew_lease_grant(&task, &draft).unwrap();
    for _ in 0..4 {
        let released = transition(&current, LeaseState::Released, draft.created_at.as_str());
        let ended = store
            .end_crew_lease_snapshot(&task, &current, &released)
            .unwrap()
            .unwrap();
        assert_eq!(ended.state, LeaseState::Released);
        assert_ne!(ended.id, current.id);
        current = store.add_crew_lease_grant(&task, &draft).unwrap();
        assert_eq!(store.crew_leases(&task).unwrap().last(), Some(&current));
        assert_eq!(current.state, LeaseState::Granted);
        // Retrying both the original draft and the returned grant changes nothing.
        assert_eq!(store.add_crew_lease_grant(&task, &draft).unwrap(), current);
        assert_eq!(
            store.add_crew_lease_grant(&task, &current).unwrap(),
            current
        );
    }
    let rows = store.crew_leases(&task).unwrap();
    assert_eq!(rows.len(), 9);
    let ids: std::collections::HashSet<_> = rows.iter().map(|lease| lease.id.as_str()).collect();
    assert_eq!(ids.len(), rows.len());
    for lease in &rows {
        let object = CrewObject::Lease(lease.clone());
        object.validate().unwrap();
        assert_eq!(&object.computed_id().unwrap(), object.id());
        assert_eq!(lease.expires_at, draft.expires_at);
    }
}

#[test]
fn a_stale_release_does_not_obscure_the_new_acquisition() {
    let (_dir, store, task, draft) = setup();
    let original = store.add_crew_lease_grant(&task, &draft).unwrap();
    let released = transition(&original, LeaseState::Released, draft.created_at.as_str());
    store
        .end_crew_lease_snapshot(&task, &original, &released)
        .unwrap();
    let renewed = store.add_crew_lease_grant(&task, &draft).unwrap();
    let count = store.crew_leases(&task).unwrap().len();
    let stale = transition(&original, LeaseState::Released, "2026-11-12T10:00:01Z");
    assert_eq!(
        store
            .end_crew_lease_snapshot(&task, &original, &stale)
            .unwrap(),
        Some(renewed)
    );
    assert_eq!(store.crew_leases(&task).unwrap().len(), count);
}

#[test]
fn clock_rollback_cannot_reuse_a_historical_content_id() {
    let (_dir, store, task, draft) = setup();
    let future = transition(&draft, LeaseState::Granted, "2026-11-12T09:30:00Z");
    // An existing append-only history may predate monotonic transition stamps.
    store
        .add_crew_object(&task, &CrewObject::Lease(future.clone()))
        .unwrap();
    let released = transition(&future, LeaseState::Released, draft.created_at.as_str());
    store
        .add_crew_object(&task, &CrewObject::Lease(released))
        .unwrap();
    let renewed = store.add_crew_lease_grant(&task, &future).unwrap();
    assert_ne!(renewed.id, future.id);
    let parse = |lease: &Lease| {
        time::OffsetDateTime::parse(
            lease.created_at.as_str(),
            &time::format_description::well_known::Rfc3339,
        )
        .unwrap()
    };
    assert!(parse(&renewed) > parse(&future));
    assert_eq!(store.crew_leases(&task).unwrap().last(), Some(&renewed));
    assert_eq!(store.crew_leases(&task).unwrap().len(), 3);
}

#[test]
fn ending_a_scoped_lineage_preserves_the_other_paths_and_mode() {
    let (_dir, store, task, broad) = setup();
    let broad = store.add_crew_lease_grant(&task, &broad).unwrap();
    let mut scoped = broad.clone();
    scoped.paths = vec![CrewPathPattern::new("docs/**").unwrap()];
    scoped.id = CrewObject::Lease(scoped.clone()).computed_id().unwrap();
    let scoped = store.add_crew_lease_grant(&task, &scoped).unwrap();
    let mut other_mode = scoped.clone();
    other_mode.mode = if scoped.mode == LeaseMode::Read {
        LeaseMode::Write
    } else {
        LeaseMode::Read
    };
    other_mode.id = CrewObject::Lease(other_mode.clone()).computed_id().unwrap();
    let other_mode = store.add_crew_lease_grant(&task, &other_mode).unwrap();
    let released = transition(&scoped, LeaseState::Released, scoped.created_at.as_str());
    store
        .end_crew_lease_snapshot(&task, &scoped, &released)
        .unwrap();
    let broad_retry = store.add_crew_lease_grant(&task, &broad).unwrap();
    assert_eq!(broad_retry, broad);
    assert_eq!(
        store.add_crew_lease_grant(&task, &other_mode).unwrap(),
        other_mode
    );
    assert_eq!(store.crew_leases(&task).unwrap().len(), 4);
}
