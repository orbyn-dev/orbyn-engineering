//! The bus at rest: one revision per key, a stale write refused, nothing stored for a run that
//! has ended, a delivery that is only ever added to, and held messages that leave once sent.

use orbyn_contracts::{CrewObject, Fact};
use serde_json::Value;

use super::*;
use crate::testing::TempDir;

fn task_in(store: &Store) -> String {
    let brief = match CrewObject::parse(
        &serde_json::from_str::<Value>(include_str!(
            "../../../../docs/crew/protocol/schemas/examples/brief.valid.basic.json"
        ))
        .unwrap(),
    )
    .unwrap()
    {
        CrewObject::Brief(brief) => brief,
        _ => unreachable!(),
    };
    let key = match CrewObject::parse(
        &serde_json::from_str::<Value>(include_str!(
            "../../../../docs/crew/protocol/schemas/examples/answer-key.valid.basic.json"
        ))
        .unwrap(),
    )
    .unwrap()
    {
        CrewObject::AnswerKey(key) => key,
        _ => unreachable!(),
    };
    store.create_crew_task(&brief, &key, "run-01").unwrap().id
}

/// The example fact, on `rev` after `base_rev`, under its own content address.
fn fact(rev: u64, base_rev: u64, value: &str) -> Fact {
    let CrewObject::Fact(mut fact) = CrewObject::parse(
        &serde_json::from_str::<Value>(include_str!(
            "../../../../docs/crew/protocol/schemas/examples/fact.valid.observation.json"
        ))
        .unwrap(),
    )
    .unwrap() else {
        unreachable!()
    };
    fact.rev = rev;
    fact.base_rev = base_rev;
    fact.value = Value::String(value.into());
    fact.id = CrewObject::Fact(fact.clone()).computed_id().unwrap();
    fact
}

#[test]
fn the_first_write_on_a_revision_stands_and_a_stale_one_is_reported() {
    let dir = TempDir::new();
    let store = Store::open(&dir.path().join("orbyn.sqlite3"), "epoch-a").unwrap();
    let task = task_in(&store);
    let crew = "crew-01";
    assert_eq!(
        store.add_crew_fact(&task, &fact(1, 0, "one")).unwrap(),
        FactWrite::Stored
    );
    assert_eq!(
        store.add_crew_fact(&task, &fact(2, 1, "two")).unwrap(),
        FactWrite::Stored
    );
    // Another writer read revision 1 and writes after revision 2 stood.
    assert_eq!(
        store.add_crew_fact(&task, &fact(2, 1, "late")).unwrap(),
        FactWrite::Stale { current: 2 }
    );
    let standing = store
        .crew_fact(crew, "crew-demo.build-time")
        .unwrap()
        .unwrap();
    assert_eq!(
        (standing.rev, standing.value),
        (2, Value::String("two".into()))
    );
    assert_eq!(
        store.crew_facts(crew).unwrap().len(),
        1,
        "one revision per key"
    );

    // Facts are immutable rows.
    let refused = store
        .writer()
        .execute("UPDATE crew_facts SET author = 'x'", [])
        .unwrap_err();
    assert!(refused.to_string().contains("immutable"), "{refused}");
}

#[test]
fn a_run_that_ended_takes_no_member_object_but_the_core_still_can() {
    let dir = TempDir::new();
    let store = Store::open(&dir.path().join("orbyn.sqlite3"), "epoch-a").unwrap();
    let task = task_in(&store);
    store.move_crew_task(&task, "cancelled", None).unwrap();
    assert!(matches!(
        store.add_crew_fact(&task, &fact(1, 0, "late")),
        Err(CoreError::RunClosed)
    ));
    assert!(matches!(
        store.add_crew_member_object(&task, &CrewObject::Fact(fact(1, 0, "late"))),
        Err(CoreError::RunClosed)
    ));
    // The core's own path is not closed (H7).
    store
        .add_crew_object(&task, &CrewObject::Fact(fact(1, 0, "core")))
        .unwrap();
}
