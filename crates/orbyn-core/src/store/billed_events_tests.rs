//! The billed-event store: migration 74 on an old database, one inference reported twice, a
//! record that cannot be read as a billed event, counters kept apart as the account reports them,
//! and a store with no rows reading as empty.

use serde_json::{Value, json};

use super::*;
use crate::testing::TempDir;

fn open() -> (TempDir, Store) {
    let dir = TempDir::new();
    let store = Store::open(&dir.path().join("orbyn.db"), "epoch-a").unwrap();
    (dir, store)
}

/// One Cursor event: 2026-10-11T04:00:00Z in milliseconds.
fn event(stamp: u64, model: &str, input: u64, output: u64) -> Value {
    json!({
        "timestamp": stamp.to_string(),
        "model": model,
        "conversationId": "conversation-a",
        "tokenUsage": {
            "inputTokens": input,
            "outputTokens": output,
            "cacheReadTokens": 0,
            "cacheWriteTokens": 0,
            "totalCents": 12,
        },
    })
}

fn report(events: Vec<Value>) -> Value {
    json!({ "usageEventsDisplay": events, "totalUsageEventsCount": events.len() })
}

const STAMP: u64 = 1_791_691_200_000;

#[test]
fn one_inference_reported_twice_is_stored_and_counted_once() {
    let (_dir, store) = open();
    let once = parse_billed_events("cursor", &report(vec![event(STAMP, "claude-4.5", 100, 20)]));
    assert_eq!(once.len(), 1);
    assert_eq!(store.record_billed_events(&once).unwrap(), 1);
    // A later read reports the same event again: nothing new is written.
    let again = parse_billed_events("cursor", &report(vec![event(STAMP, "claude-4.5", 100, 20)]));
    assert_eq!(store.record_billed_events(&again).unwrap(), 0);
    let totals = store.billed_totals("cursor", None).unwrap();
    assert_eq!(totals.len(), 1);
    assert_eq!(totals[0].input, 100);
    assert_eq!(totals[0].output, 20);
    assert_eq!(store.billed_event_count("cursor").unwrap(), 1);
}

#[test]
fn identical_events_in_one_page_all_count() {
    let (_dir, store) = open();
    // Two separate inferences that happen to bill the same counters are two events, not one.
    let events = parse_billed_events(
        "cursor",
        &report(vec![
            event(STAMP, "claude-4.5", 100, 20),
            event(STAMP, "claude-4.5", 100, 20),
        ]),
    );
    assert_eq!(events.len(), 2);
    assert_ne!(events[0].event_id, events[1].event_id);
    assert_eq!(store.record_billed_events(&events).unwrap(), 2);
    assert_eq!(store.billed_totals("cursor", None).unwrap()[0].input, 200);
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
fn counters_are_kept_apart_and_cache_and_reasoning_accounting_is_kept() {
    let (_dir, store) = open();
    let parsed = parse_billed_events(
        "cursor",
        &json!({
            "usageEventsDisplay": [{
                "timestamp": STAMP.to_string(),
                "model": "claude-4.5",
                "tokenUsage": {
                    // Cursor reports uncached input and cached input separately, and it bills a
                    // rate per counter: nothing here is folded into another counter.
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
    assert_eq!(store.record_billed_events(&parsed).unwrap(), 1);
    let totals = store.billed_totals("cursor", None).unwrap();
    assert_eq!(totals.len(), 1);
    let row = &totals[0];
    assert_eq!(row.day, "2026-10-11");
    assert_eq!(row.model, "claude-4.5");
    assert_eq!(row.input, 100);
    assert_eq!(row.cache_read, 900);
    assert_eq!(row.cache_write, 50);
    assert_eq!(row.output, 40);
    // The account's own billed amount is kept whole.
    assert_eq!(row.reported_cents, Some(125));
}

#[test]
fn a_missing_store_reads_as_empty() {
    let (_dir, store) = open();
    assert!(store.billed_totals("cursor", None).unwrap().is_empty());
    assert_eq!(store.billed_event_count("cursor").unwrap(), 0);
    // Another engine's history is not this engine's.
    let parsed = parse_billed_events("cursor", &report(vec![event(STAMP, "claude-4.5", 10, 1)]));
    store.record_billed_events(&parsed).unwrap();
    assert!(store.billed_totals("muse", None).unwrap().is_empty());
}

#[test]
fn a_period_keeps_only_its_days_and_its_engine() {
    let (_dir, store) = open();
    let early = 1_791_691_200_000; // 2026-10-11
    let later = 1_791_777_600_000; // 2026-10-12
    store
        .record_billed_events(
            &parse_billed_events(
                "cursor",
                &report(vec![
                    event(early, "claude-4.5", 10, 1),
                    event(later, "claude-4.5", 20, 2),
                ]),
            )
            .into_iter()
            .chain(parse_billed_events(
                "muse",
                &report(vec![event(early, "muse-1", 7, 3)]),
            ))
            .collect::<Vec<_>>(),
        )
        .unwrap();
    assert_eq!(store.billed_event_count("cursor").unwrap(), 2);
    let period = store.billed_totals("cursor", Some("2026-10-12")).unwrap();
    assert_eq!(period.len(), 1);
    assert_eq!(period[0].day, "2026-10-12");
    assert_eq!(period[0].input, 20);
    let all = store.billed_totals("cursor", None).unwrap();
    assert_eq!(all.len(), 2);
}
