//! The Cursor account usage reader against a local fake server (no real network, no real Cursor).

use super::*;
use crate::issues::fake_http::{Reply, Seen, Server};
use crate::store::{AccountRead, Store, parse_billed_events};
use crate::testing::TempDir;
use serde_json::{Value, json};

const TOKEN: &str = "cursor-SECRET-token-123";
/// 2026-10-11T04:00:00Z in milliseconds.
const SINCE: u64 = 1_791_691_200_000;
const END: u64 = SINCE + 86_400_000;
const ANSWER_SECRET: &str = "raw-answer-secret-detail";

fn token() -> AccessToken {
    AccessToken::from(TOKEN)
}

async fn read_usage(server: &Server) -> CursorAccountUsage {
    read_cursor_account_usage(&token(), SINCE, END, Some(&server.base)).await
}

fn event(index: u64) -> Value {
    json!({
        "timestamp": (SINCE + index).to_string(),
        "model": "claude-4.5",
        "conversationId": format!("conversation-{index}"),
        "tokenUsage": {
            "inputTokens": index + 1,
            "outputTokens": 2,
            "cacheReadTokens": 3,
            "cacheWriteTokens": 4,
            "totalCents": 5,
        },
    })
}

fn page_reply(events: &[Value], total: u64) -> Reply {
    Reply::json(&json!({
        "usageEventsDisplay": events,
        "totalUsageEventsCount": total,
    }))
}

fn page_body(request: &Seen) -> Value {
    request
        .json()
        .unwrap_or_else(|| panic!("a page request body, got {:?}", request.body))
}

fn membership_reply(request: &Seen) -> Option<Reply> {
    if request.path().ends_with("/GetTeams") {
        Some(Reply::json(&json!({ "teams": [{ "id": 7 }] })))
    } else if request.path().ends_with("/GetMe") {
        Some(Reply::json(&json!({ "userId": 42 })))
    } else {
        None
    }
}

#[tokio::test]
async fn a_personal_and_team_read_reconciles_the_duplicated_page_boundary() {
    let first: Vec<Value> = (0..1000).map(event).collect();
    let second = vec![event(999), event(1000)];
    let team = vec![event(2000)];
    let server = Server::start(move |request| {
        if let Some(reply) = membership_reply(request) {
            return reply;
        }
        let body = page_body(request);
        let page = body.get("page").and_then(Value::as_u64).unwrap_or_default();
        let team_id = body.get("teamId").and_then(Value::as_u64);
        match (team_id, page) {
            (None, 1) => page_reply(&first, 1001),
            (None, 2) => page_reply(&second, 1001),
            (Some(7), 1) => page_reply(&team, 1),
            _ => Reply::raw(500, Vec::new()),
        }
    })
    .await;

    let usage = read_usage(&server).await;
    assert_eq!(usage.error, None);
    assert_eq!(usage.records.len(), 1002);
    let duplicated = usage
        .records
        .iter()
        .filter(|record| {
            record.get("conversationId").and_then(Value::as_str) == Some("conversation-999")
        })
        .count();
    assert_eq!(duplicated, 1, "the boundary copy is removed once");
    assert_eq!(
        usage
            .records
            .last()
            .and_then(|record| record.get("conversationId"))
            .and_then(Value::as_str),
        Some("conversation-2000"),
        "the team scope follows the personal one"
    );
    assert!(!format!("{usage:?}").contains(TOKEN));

    let seen = server.seen();
    assert_eq!(seen.len(), 5, "two membership reads and three pages");
    let expected_authorization = format!("Bearer {TOKEN}");
    for request in &seen {
        assert_eq!(
            request.header("authorization"),
            Some(expected_authorization.as_str())
        );
        assert_eq!(request.header("connect-protocol-version"), Some("1"));
    }
    let since = SINCE.to_string();
    let end = END.to_string();
    let mut personal_pages = 0;
    let mut team_pages = 0;
    for request in &seen {
        if !request.path().ends_with("/GetFilteredUsageEvents") {
            assert_eq!(request.json(), Some(json!({})), "a membership body");
            continue;
        }
        let body = page_body(request);
        assert_eq!(body.get("pageSize").and_then(Value::as_u64), Some(1000));
        assert_eq!(
            body.get("startDate").and_then(Value::as_str),
            Some(since.as_str())
        );
        assert_eq!(
            body.get("endDate").and_then(Value::as_str),
            Some(end.as_str())
        );
        if body.get("teamId").is_some() {
            team_pages += 1;
            assert_eq!(body.get("teamId").and_then(Value::as_u64), Some(7));
            assert_eq!(body.get("userId").and_then(Value::as_u64), Some(42));
        } else {
            personal_pages += 1;
            assert!(
                body.get("userId").is_none(),
                "the personal scope has no user filter"
            );
        }
    }
    assert_eq!(personal_pages, 2);
    assert_eq!(team_pages, 1);
}

#[tokio::test]
async fn a_total_that_changes_between_pages_is_an_error() {
    let first: Vec<Value> = (0..1000).map(event).collect();
    let second = vec![event(1000)];
    let server = Server::start(move |request| {
        if let Some(reply) = membership_reply(request) {
            return reply;
        }
        let body = page_body(request);
        match body.get("page").and_then(Value::as_u64) {
            Some(1) => page_reply(&first, 1001),
            Some(2) => page_reply(&second, 1002),
            _ => Reply::raw(500, Vec::new()),
        }
    })
    .await;

    let usage = read_usage(&server).await;
    assert_eq!(usage.error.as_deref(), Some(GENERIC));
    assert!(usage.records.is_empty());
    assert!(!format!("{usage:?}").contains(TOKEN));
}

#[tokio::test]
async fn an_error_keyed_answer_is_an_error_with_no_detail() {
    let server = Server::start(|request| {
        if let Some(reply) = membership_reply(request) {
            return reply;
        }
        Reply::json(&json!({ "error": ANSWER_SECRET, "code": 13 }))
    })
    .await;

    let usage = read_usage(&server).await;
    assert_eq!(usage.error.as_deref(), Some(GENERIC));
    assert!(usage.records.is_empty());
    let error = usage.error.clone().unwrap_or_default();
    assert!(!error.contains(ANSWER_SECRET));
    assert!(!format!("{usage:?}").contains(ANSWER_SECRET));
}

#[tokio::test]
async fn a_page_401_or_403_answers_the_sign_in_sentence() {
    for status in [401_u16, 403] {
        let server = Server::start(move |request| {
            if let Some(reply) = membership_reply(request) {
                return reply;
            }
            Reply::raw(
                status,
                format!(r#"{{"message":"{ANSWER_SECRET}"}}"#).into_bytes(),
            )
        })
        .await;

        let usage = read_usage(&server).await;
        assert_eq!(usage.error.as_deref(), Some(SIGN_IN), "{status}");
        assert!(usage.records.is_empty(), "{status}");
        assert!(!format!("{usage:?}").contains(TOKEN), "{status}");
    }
}

#[tokio::test]
async fn a_redirect_is_not_followed_and_the_other_host_sees_nothing() {
    let elsewhere = Server::start(|_| Reply::json(&json!({ "teams": [] }))).await;
    let target = format!("{}/aiserver.v1.DashboardService/GetTeams", elsewhere.base);
    let server =
        Server::start(move |_| Reply::raw(302, Vec::new()).header("location", &target)).await;

    let usage = read_usage(&server).await;
    assert_eq!(usage.error.as_deref(), Some(GENERIC));
    assert!(usage.records.is_empty());
    assert!(elsewhere.seen().is_empty(), "the other host saw nothing");
    assert_eq!(server.seen().len(), 1, "the redirect is not retried");
}

#[tokio::test]
async fn a_negative_token_usage_is_an_error() {
    let mut bad = event(0);
    bad["tokenUsage"]["totalCents"] = json!(-1);
    let server = Server::start(move |request| {
        if let Some(reply) = membership_reply(request) {
            return reply;
        }
        page_reply(std::slice::from_ref(&bad), 1)
    })
    .await;

    let usage = read_usage(&server).await;
    assert_eq!(usage.error.as_deref(), Some(GENERIC));
    assert!(usage.records.is_empty());
}

#[test]
fn the_token_debug_is_redacted() {
    let token = token();
    let debug = format!("{token:?}");
    assert_eq!(debug, "[redacted]");
    assert!(!debug.contains(TOKEN));
}

#[tokio::test]
async fn the_records_feed_the_store_unchanged() {
    let personal = vec![event(0)];
    let team = vec![event(1)];
    let server = Server::start(move |request| {
        if let Some(reply) = membership_reply(request) {
            return reply;
        }
        let body = page_body(request);
        if body.get("teamId").is_some() {
            page_reply(&team, 1)
        } else {
            page_reply(&personal, 1)
        }
    })
    .await;

    let usage = read_usage(&server).await;
    assert_eq!(usage.error, None);
    assert_eq!(usage.records.len(), 2);
    let report = json!({
        "usageEventsDisplay": usage.records,
        "totalUsageEventsCount": 2,
    });
    let events = parse_billed_events("cursor", &report);
    assert_eq!(events.len(), 2);

    let dir = TempDir::new();
    let store = Store::open(&dir.path().join("orbyn.db"), "epoch-a").unwrap();
    assert_eq!(store.record_billed_events(&events).unwrap().new, 2);
    let totals = store.billed_totals("cursor", None).unwrap();
    assert_eq!(totals.len(), 1);
    assert_eq!(totals[0].model, "claude-4.5");
    assert_eq!(totals[0].input, 3);
    assert_eq!(totals[0].output, 4);
    assert_eq!(totals[0].cache_read, 6);
    assert_eq!(totals[0].cache_write, 8);
    assert_eq!(totals[0].reported_cents, Some(10));
}

#[tokio::test]
async fn plain_http_to_another_host_is_refused_before_any_request() {
    let usage =
        read_cursor_account_usage(&token(), SINCE, END, Some("http://cursor.example.invalid"))
            .await;
    assert_eq!(usage.error.as_deref(), Some(GENERIC));
    assert!(usage.records.is_empty());
}

#[test]
fn only_the_exact_loopback_hosts_may_take_the_token_over_plain_http() {
    for base in [
        "http://localhost",
        "http://localhost:8080",
        "http://127.0.0.1:3000",
        "http://[::1]:3000",
    ] {
        assert!(is_loopback(base), "{base}");
    }
    for base in [
        "http://localhost.evil.test",
        "http://127.0.0.1.evil.test",
        "http://localhost@evil.test",
        "http://127.0.0.1@evil.test",
        "http://localhost:80@evil.test",
        "http://cursor.example.invalid",
        "https://localhost",
        "localhost",
    ] {
        assert!(!is_loopback(base), "{base}");
    }
}

const MINUTE: u64 = 60_000;
const HOUR: u64 = 3_600_000;
const DAY: u64 = 86_400_000;
/// A wall clock the policy is asked about, so no test waits or reads the machine's own clock.
/// 2026-10-11T04:00:00Z, far enough from the epoch for a month of history behind it.
const NOW: u64 = 1_791_691_200_000;

fn read(since_ms: u64, until_ms: u64, fetched_at_ms: u64) -> Option<AccountRead> {
    Some(AccountRead {
        since_ms,
        until_ms,
        fetched_at_ms,
    })
}

/// The first read of an account starts at the newest end, because that is where the spend a
/// person opens the screen to see is, and it reaches back only as far as one span allows.
#[test]
fn the_first_read_starts_at_the_newest_end_of_the_period() {
    assert_eq!(
        next_window(None, 0, NOW, DAY),
        Some(Window {
            since_ms: NOW - DAY,
            until_ms: NOW,
        })
    );
    assert_eq!(
        next_window(None, NOW - 30 * DAY, NOW, u64::MAX),
        Some(Window {
            since_ms: NOW - 30 * DAY,
            until_ms: NOW,
        }),
        "with no span limit the read reaches back to the period's own start"
    );
}

/// A read the store still holds is not repeated, and one that no longer covers the period asked
/// for is not used as an answer.
#[test]
fn a_read_is_answered_from_the_store_only_while_it_is_fresh_and_covers_the_period() {
    let fresh = read(0, NOW - 30_000, NOW - 30_000);
    assert_eq!(next_window(fresh, 0, NOW, DAY), None);

    // A minute after it was read, the same window is read again.
    let stale = read(0, NOW - HOUR, NOW - FRESH_FOR_MS);
    let window = next_window(stale, 0, NOW, DAY).expect("a stale read is repeated");
    assert_eq!(window.until_ms, NOW, "the read brings the newest events");
    assert_eq!(
        window.since_ms,
        NOW - HOUR - REFETCH_OVERLAP_MS,
        "and overlaps the last covered hour, because an amount settles after it is first read"
    );

    // A period that starts before the store's window cannot be answered from it.
    let behind = read(NOW - 2 * DAY, NOW, NOW);
    assert!(next_window(behind, NOW - 5 * DAY, NOW, DAY).is_some());
}

/// A fresh window that ends before the period starts holds nothing the period can be answered
/// with, so the newest window is read instead of the store answering with nothing.
#[test]
fn a_fresh_read_that_ends_before_the_period_does_not_answer_it() {
    let period_start = NOW - HOUR;
    let covered = read(NOW - 10 * DAY, NOW - 6 * HOUR, NOW);
    assert_eq!(
        next_window(covered, period_start, NOW, DAY),
        Some(Window {
            since_ms: period_start,
            until_ms: NOW,
        }),
        "the store holds nothing in the period, so the newest window is read"
    );
}

/// History older than the store's window is read backwards, one span at a time, and never past
/// the period's start.
#[test]
fn older_history_is_read_backwards_one_span_at_a_time() {
    let covered = read(NOW - 2 * DAY, NOW, NOW);
    assert_eq!(
        next_window(covered, 0, NOW, DAY),
        Some(Window {
            since_ms: NOW - 3 * DAY,
            until_ms: NOW - 2 * DAY,
        })
    );
    assert_eq!(
        next_window(read(NOW - 2 * DAY, NOW, NOW), NOW - 5 * DAY, NOW, u64::MAX),
        Some(Window {
            since_ms: NOW - 5 * DAY,
            until_ms: NOW - 2 * DAY,
        }),
        "the period's start is the oldest end, not before it"
    );
}

/// A window whose end is behind the clock is topped up forwards, at most one span at a time.
#[test]
fn a_window_behind_the_clock_is_topped_up_forwards() {
    let behind = read(NOW - 10 * DAY, NOW - 30 * HOUR, NOW - 30 * HOUR);
    let window = next_window(behind, NOW - 10 * DAY, NOW, DAY).expect("the newest events are read");
    assert_eq!(
        window.since_ms,
        NOW - 30 * HOUR - REFETCH_OVERLAP_MS.min(DAY / 2),
        "a narrower read overlaps less so that it still moves forward"
    );
    assert_eq!(
        window.until_ms,
        window.since_ms + DAY,
        "one span of the read, not the whole gap to the clock"
    );
}

/// The last hour already read is asked for again even when the window reaches the clock, because
/// that is where Cursor finalizes the amounts it first reported unsettled.
#[test]
fn a_settled_amount_is_seen_by_asking_for_the_last_covered_hour_again() {
    let window = next_window(
        read(NOW - 10 * DAY, NOW, NOW - FRESH_FOR_MS),
        NOW - 5 * DAY,
        NOW,
        DAY,
    )
    .expect("the settled amounts are read");
    assert_eq!(
        window,
        Window {
            since_ms: NOW - REFETCH_OVERLAP_MS,
            until_ms: NOW,
        }
    );
}

/// What the store would hold after reading `window`: two windows that meet or overlap are one
/// window, the rule `store` keeps its billing window by.
fn stored(
    previous: Option<AccountRead>,
    window: Window,
    fetched_at_ms: u64,
) -> Option<AccountRead> {
    let (since_ms, until_ms) = match previous {
        Some(previous)
            if window.since_ms <= previous.until_ms.saturating_add(1)
                && previous.since_ms <= window.until_ms.saturating_add(1) =>
        {
            (
                previous.since_ms.min(window.since_ms),
                previous.until_ms.max(window.until_ms),
            )
        }
        _ => (window.since_ms, window.until_ms),
    };
    Some(AccountRead {
        since_ms,
        until_ms,
        fetched_at_ms,
    })
}

/// A stored window whose end is after the clock was written before the clock moved back, or is stale: a future
/// fetch time also reads as fresh, because the subtraction saturates. Its bounds cannot name a window this read may
/// ask for, so the newest window a span allows is read instead.
#[test]
fn a_stored_window_that_ends_after_the_clock_reads_the_newest_window() {
    let period_start = NOW - 10 * DAY;
    // The record the finding names: a minute-span period, fetched after the clock moved back.
    let ahead = read(NOW + 2 * MINUTE, NOW + 3 * MINUTE, NOW + 3 * MINUTE);
    assert_eq!(
        next_window(ahead, period_start, NOW, MINUTE),
        Some(Window {
            since_ms: NOW - MINUTE,
            until_ms: NOW,
        }),
        "the newest window, not the future one the read's own bounds name"
    );
    assert_eq!(
        next_window(ahead, period_start, NOW, DAY),
        Some(Window {
            since_ms: NOW - DAY,
            until_ms: NOW,
        }),
        "one span still bounds the read that resets the walk"
    );
    assert_eq!(
        next_window(
            read(NOW + HOUR, NOW + 2 * HOUR, NOW),
            period_start,
            NOW,
            DAY
        ),
        Some(Window {
            since_ms: NOW - DAY,
            until_ms: NOW,
        }),
        "a read whose whole window is ahead of the clock is reset the same way"
    );
    // Coverage that ends at or before the clock keeps today's path.
    assert_eq!(
        next_window(read(NOW - 2 * DAY, NOW, NOW), 0, NOW, DAY),
        Some(Window {
            since_ms: NOW - 3 * DAY,
            until_ms: NOW - 2 * DAY,
        }),
        "a window that ends at the clock is still walked back one span"
    );
}

/// A period shorter than the one-hour overlap is walked back one span at a time, rather than the
/// newest window being selected again on every call.
#[test]
fn a_period_shorter_than_the_overlap_is_walked_back_to_its_start() {
    let period_start = NOW - 30 * MINUTE;
    let mut covered = None;
    let mut steps = 0;
    while let Some(window) = next_window(covered, period_start, NOW, MINUTE) {
        covered = stored(covered, window, NOW);
        steps += 1;
        assert!(
            steps <= 40,
            "each read walks one minute further back: {covered:?}"
        );
    }
    assert_eq!(
        covered.expect("the period is read").since_ms,
        period_start,
        "the stored start reaches the period's start, not only the newest minute"
    );
}

/// A clock that moves between two reads does not hold the walk back at the newest window: it still
/// reaches the period's start.
#[test]
fn a_clock_that_moves_between_reads_still_walks_back_to_the_period_start() {
    let period_start = NOW - 10 * DAY;
    let mut covered = None;
    let mut steps = 0;
    for call in 0..20 {
        // Each call is a later instant, and the last one is far enough behind to be re-read.
        let now = NOW + call as u64 * 30_000;
        let Some(window) = next_window(covered, period_start, now, DAY) else {
            break;
        };
        covered = stored(covered, window, now);
        steps += 1;
    }
    let covered = covered.expect("the period is read");
    assert_eq!(
        covered.since_ms, period_start,
        "the walk reaches the period's start in {steps} reads, not only the newest window"
    );
}
