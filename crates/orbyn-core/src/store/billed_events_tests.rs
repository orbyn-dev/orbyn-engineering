//! The billed-event store: one report read twice, a window that holds two turns of one
//! conversation, an amount that settles, a record that cannot be read as a billed event, and a
//! store with no rows reading as empty.

use serde_json::{Value, json};

use super::*;
use crate::testing::TempDir;

fn open() -> (TempDir, Store) {
    let dir = TempDir::new();
    let store = Store::open(&dir.path().join("orbyn.db"), "epoch-a").unwrap();
    (dir, store)
}

/// 2026-10-11T04:00:00Z in milliseconds.
const STAMP: u64 = 1_791_691_200_000;

/// One Cursor event. Cursor sends no event id, so this is everything an event is.
fn event(stamp: u64, model: &str, input: u64, output: u64, cents: i64) -> Value {
    json!({
        "timestamp": stamp.to_string(),
        "model": model,
        "conversationId": "conversation-a",
        "tokenUsage": {
            "inputTokens": input,
            "outputTokens": output,
            "cacheReadTokens": 0,
            "cacheWriteTokens": 0,
            "totalCents": cents,
        },
    })
}

fn report(events: Vec<Value>) -> Value {
    json!({ "usageEventsDisplay": events, "totalUsageEventsCount": events.len() })
}

fn parse(events: Vec<Value>) -> Vec<BilledEvent> {
    parse_billed_events("cursor", &report(events))
}

#[test]
fn a_later_window_keeps_the_first_turn_and_adds_the_second() {
    let (_dir, store) = open();
    let first = parse(vec![event(STAMP, "claude-4.5", 100, 20, 12)]);
    assert_eq!(store.record_billed_events(&first).unwrap().new, 1);
    let stored_first = first[0].event_id.clone();

    // The next read of the same window holds both turns of the conversation.
    let second = parse(vec![
        event(STAMP, "claude-4.5", 100, 20, 12),
        event(STAMP, "claude-4.5", 100, 20, 12),
    ]);
    assert_eq!(
        second[0].event_id, stored_first,
        "the first turn keeps its row"
    );
    assert_ne!(
        second[1].event_id, stored_first,
        "the second is its own row"
    );
    let recorded = store.record_billed_events(&second).unwrap();
    assert_eq!(recorded.new, 1, "only the turn that is new is new");

    let totals = store.billed_totals("cursor", None).unwrap();
    assert_eq!(totals.len(), 1);
    assert_eq!(totals[0].input, 200, "both turns are counted");
    assert_eq!(totals[0].output, 40);
    assert_eq!(store.billed_event_count("cursor").unwrap(), 2);
}

#[test]
fn a_settled_amount_updates_the_row_instead_of_adding_one() {
    let (_dir, store) = open();
    let unsettled = parse(vec![event(STAMP, "claude-4.5", 100, 20, 12)]);
    store.record_billed_events(&unsettled).unwrap();
    assert_eq!(
        store.billed_totals("cursor", None).unwrap()[0].reported_cents,
        Some(12)
    );

    // The same event, re-reported once the account settled its amount.
    let settled = parse(vec![event(STAMP, "claude-4.5", 100, 20, 15)]);
    assert_eq!(settled[0].event_id, unsettled[0].event_id, "same event");
    let recorded = store.record_billed_events(&settled).unwrap();
    assert_eq!(recorded.new, 0);
    assert_eq!(recorded.settled, 1);

    let totals = store.billed_totals("cursor", None).unwrap();
    assert_eq!(totals.len(), 1, "one row, not two");
    assert_eq!(totals[0].input, 100, "the event is not counted twice");
    assert_eq!(
        totals[0].reported_cents,
        Some(15),
        "the settled amount stands"
    );
    assert_eq!(store.billed_event_count("cursor").unwrap(), 1);
}

#[test]
fn two_identical_events_in_one_window_are_two_rows() {
    let (_dir, store) = open();
    let events = parse(vec![
        event(STAMP, "claude-4.5", 100, 20, 12),
        event(STAMP, "claude-4.5", 100, 20, 12),
    ]);
    assert_eq!(events.len(), 2);
    assert_ne!(events[0].event_id, events[1].event_id);
    assert_eq!(store.record_billed_events(&events).unwrap().new, 2);
    assert_eq!(store.billed_totals("cursor", None).unwrap()[0].input, 200);
}

#[test]
fn reading_the_same_window_again_writes_nothing_new() {
    let (_dir, store) = open();
    let window = vec![
        event(STAMP, "claude-4.5", 100, 20, 12),
        event(STAMP, "claude-4.5", 7, 3, 1),
    ];
    assert_eq!(
        store
            .record_billed_events(&parse(window.clone()))
            .unwrap()
            .new,
        2
    );
    let again = store.record_billed_events(&parse(window)).unwrap();
    assert_eq!(again.new, 0);
    assert_eq!(again.settled, 0);
    assert_eq!(store.billed_event_count("cursor").unwrap(), 2);
}

#[test]
fn a_window_replaces_only_the_days_it_names() {
    let (_dir, store) = open();
    let day_one = STAMP;
    let day_two = STAMP + 86_400_000;
    store
        .record_billed_events(&parse(vec![
            event(day_one, "claude-4.5", 10, 1, 1),
            event(day_two, "claude-4.5", 20, 2, 2),
        ]))
        .unwrap();
    // A window of the first day only leaves the second day alone.
    store
        .record_billed_events(&parse(vec![event(day_one, "claude-4.5", 30, 3, 3)]))
        .unwrap();
    let totals = store.billed_totals("cursor", None).unwrap();
    assert_eq!(totals.len(), 2);
    assert_eq!(totals[0].day, "2026-10-11");
    assert_eq!(totals[0].input, 30);
    assert_eq!(totals[1].day, "2026-10-12");
    assert_eq!(totals[1].input, 20);
}

#[test]
fn the_identity_holds_no_session_identifier_and_no_amount() {
    let (_dir, store) = open();
    let first = parse(vec![event(STAMP, "claude-4.5", 100, 20, 12)]);
    let id = &first[0].event_id;
    assert!(
        !id.contains("conversation-a"),
        "the session id is not stored as it came: {id}"
    );
    let digest = id.split(':').next().unwrap();
    assert_eq!(digest.len(), 64, "a sha-256 digest in hex");
    assert!(digest.chars().all(|c| c.is_ascii_hexdigit()));

    // An amount that settles does not make a new event.
    let settled = parse(vec![event(STAMP, "claude-4.5", 100, 20, 99)]);
    assert_eq!(settled[0].event_id, first[0].event_id);
    // A different conversation, model or counter does.
    let other = parse_billed_events(
        "cursor",
        &json!({ "usageEventsDisplay": [{
            "timestamp": STAMP.to_string(),
            "model": "claude-4.5",
            "conversationId": "conversation-b",
            "tokenUsage": { "inputTokens": 100, "outputTokens": 20, "totalCents": 12 },
        }]}),
    );
    assert_ne!(other[0].event_id, first[0].event_id);
    store.record_billed_events(&first).unwrap();
    assert_eq!(store.billed_event_count("cursor").unwrap(), 1);
}

#[test]
fn records_that_are_not_a_billed_event_are_dropped_not_fabricated() {
    let parsed = parse_billed_events(
        "cursor",
        &json!({
            "usageEventsDisplay": [
                // No usage block: a non-token event, which proves no spend.
                json!({ "timestamp": STAMP.to_string(), "model": "claude-4.5" }),
                // Usage with no timestamp that names a day.
                json!({ "model": "claude-4.5", "tokenUsage": { "inputTokens": 5, "outputTokens": 5 } }),
                json!({ "timestamp": "not-a-number", "model": "claude-4.5",
                        "tokenUsage": { "inputTokens": 5, "outputTokens": 5 } }),
                // No model.
                json!({ "timestamp": STAMP.to_string(), "tokenUsage": { "inputTokens": 5, "outputTokens": 5 } }),
                json!({ "timestamp": STAMP.to_string(), "model": "  ", "tokenUsage": { "inputTokens": 5, "outputTokens": 5 } }),
                // A negative counter is not a bill that can be counted.
                json!({ "timestamp": STAMP.to_string(), "model": "claude-4.5",
                        "tokenUsage": { "inputTokens": -5, "outputTokens": 5, "totalCents": 3 } }),
                // No tokens and no billed amount.
                json!({ "timestamp": STAMP.to_string(), "model": "claude-4.5",
                        "tokenUsage": { "inputTokens": 0, "outputTokens": 0, "totalCents": 0 } }),
                // Not an object at all.
                "garbage",
            ]
        }),
    );
    assert!(parsed.is_empty(), "{parsed:?}");
}

#[test]
fn counters_are_kept_apart_and_cache_accounting_is_kept() {
    let (_dir, store) = open();
    let parsed = parse_billed_events(
        "cursor",
        &json!({
            "usageEventsDisplay": [{
                "timestamp": STAMP.to_string(),
                "model": "claude-4.5",
                "conversationId": "conversation-a",
                "tokenUsage": {
                    // Cursor reports uncached input and cached input separately, and bills a rate
                    // per counter: nothing is folded into another counter.
                    "inputTokens": 100,
                    "cacheReadTokens": 900,
                    "cacheWriteTokens": 50,
                    // Reasoning is billed inside output for Cursor, so output stays whole.
                    "outputTokens": 40,
                    "totalCents": 125,
                },
            }]
        }),
    );
    assert_eq!(store.record_billed_events(&parsed).unwrap().new, 1);
    let totals = store.billed_totals("cursor", None).unwrap();
    assert_eq!(totals.len(), 1);
    let row = &totals[0];
    assert_eq!(row.day, "2026-10-11");
    assert_eq!(row.model, "claude-4.5");
    assert_eq!(row.input, 100);
    assert_eq!(row.cache_read, 900);
    assert_eq!(row.cache_write, 50);
    assert_eq!(row.output, 40);
    assert_eq!(row.reported_cents, Some(125));
}

#[test]
fn a_missing_store_reads_as_empty() {
    let (_dir, store) = open();
    assert!(store.billed_totals("cursor", None).unwrap().is_empty());
    assert_eq!(store.billed_event_count("cursor").unwrap(), 0);
    // Another engine's history is not this engine's.
    store
        .record_billed_events(&parse(vec![event(STAMP, "claude-4.5", 10, 1, 1)]))
        .unwrap();
    assert!(store.billed_totals("muse", None).unwrap().is_empty());
}

#[test]
fn a_period_keeps_only_its_days_and_its_engine() {
    let (_dir, store) = open();
    let later = STAMP + 86_400_000;
    let mut events = parse(vec![
        event(STAMP, "claude-4.5", 10, 1, 1),
        event(later, "claude-4.5", 20, 2, 2),
    ]);
    events.extend(parse_billed_events(
        "muse",
        &report(vec![event(STAMP, "muse-1", 7, 3, 1)]),
    ));
    store.record_billed_events(&events).unwrap();
    assert_eq!(store.billed_event_count("cursor").unwrap(), 2);
    let period = store.billed_totals("cursor", Some("2026-10-12")).unwrap();
    assert_eq!(period.len(), 1);
    assert_eq!(period[0].day, "2026-10-12");
    assert_eq!(period[0].input, 20);
    assert_eq!(store.billed_totals("cursor", None).unwrap().len(), 2);
}
