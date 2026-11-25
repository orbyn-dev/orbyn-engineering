//! The Cursor account usage reader against a local fake server (no real network, no real Cursor).

use super::*;
use crate::issues::fake_http::{Reply, Seen, Server};
use crate::store::{Store, parse_billed_events};
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
