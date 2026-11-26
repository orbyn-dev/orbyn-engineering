//! Cursor's billed events, read from the account's dashboard API. Ported from T3's
//! `packages/provider-cursor/src/server/accountUsage.ts` (MIT, commit 9168ca73d) without its
//! credential exchange, account cache, six-ahead page prefetch or dedupe keys: the access token is
//! an argument, pages and scopes are read sequentially, and the raw `usageEventsDisplay` objects
//! come back for [`crate::store::parse_billed_events`].
//!
//! Nothing here reads a credential from the keychain, a file, an environment variable or a
//! setting, exchanges an API key, or is called by anything yet. The token is wrapped in
//! [`AccessToken`], whose `Debug` prints `[redacted]` and which has no `Display`. A failure from
//! Cursor's answer is replaced by one plain sentence, so no detail of the answer reaches a caller.

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderValue};
use reqwest::{Client, StatusCode, redirect};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::time::Duration;
use zeroize::Zeroizing;

use crate::store::AccountRead;

/// Cursor's dashboard endpoint unless a caller passes another one.
pub(crate) const DEFAULT_CURSOR_API_ENDPOINT: &str = "https://api2.cursor.sh";

const DASHBOARD: &str = "aiserver.v1.DashboardService";
const GET_TEAMS: &str = "GetTeams";
const GET_ME: &str = "GetMe";
const GET_USAGE: &str = "GetFilteredUsageEvents";
const PAGE_SIZE: usize = 1000;
/// The most pages a read makes before the first answer names a count.
const MAX_PAGES_WITHOUT_COUNT: u64 = 1000;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(60);
/// JavaScript's `Number.MAX_SAFE_INTEGER`, which T3 accepts for ids and counts.
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
const SIGN_IN: &str = "Sign in to Cursor again to read account usage.";
const GENERIC: &str = "Cursor account usage could not be read.";

/// A Cursor access token, and nothing that prints it. There is deliberately no `Display`.
#[derive(Clone)]
pub(crate) struct AccessToken(Zeroizing<String>);

impl From<String> for AccessToken {
    fn from(token: String) -> Self {
        Self(Zeroizing::new(token))
    }
}

impl From<&str> for AccessToken {
    fn from(token: &str) -> Self {
        Self::from(token.to_owned())
    }
}

impl std::fmt::Debug for AccessToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[redacted]")
    }
}

impl AccessToken {
    fn expose(&self) -> &str {
        self.0.as_str()
    }
}

/// A window of the account's billed history, both bounds inclusive milliseconds of its own clock.
#[allow(dead_code)] // no caller until the owner approves the credential read
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Window {
    pub(crate) since_ms: u64,
    pub(crate) until_ms: u64,
}

/// How long a window the store holds answers a read (T3's `CURSOR_ACCOUNT_TTL_MS`).
#[allow(dead_code)] // no caller until the owner approves the credential read
pub(crate) const FRESH_FOR_MS: u64 = 60_000;
/// Dashboard events finalize after they were first read, so a re-read covers this much again
/// (T3's `REFETCH_OVERLAP_MS`).
#[allow(dead_code)] // no caller until the owner approves the credential read
pub(crate) const REFETCH_OVERLAP_MS: u64 = 3_600_000;

/// The window to read next, or nothing when the store already answers the period asked for.
///
/// A window read less than a minute ago answers the period when it reaches back to the period's
/// own start. Otherwise the read starts at the newest end, because that is where the spend a
/// person opens the screen to see is; then it covers the last covered hour again, because Cursor
/// finalizes an amount after it first reported it; then it walks older history backwards, one span
/// at a time, and never past the period's start. A span bounds one read, not the history: a caller
/// with no limit passes `u64::MAX`.
///
/// The order, the one-hour overlap and the one-minute freshness are T3's `cursorFetchRange` and
/// `CURSOR_ACCOUNT_TTL_MS` (`accountCache.ts`, MIT, commit 9168ca73d).
#[allow(dead_code)] // no caller until the owner approves the credential read
pub(crate) fn next_window(
    read: Option<AccountRead>,
    retention_start_ms: u64,
    now_ms: u64,
    max_span_ms: u64,
) -> Option<Window> {
    if let Some(read) = read
        && read.since_ms <= retention_start_ms
        && now_ms.saturating_sub(read.fetched_at_ms) < FRESH_FOR_MS
    {
        return None;
    }
    // The newest window a span allows, from as far back as the period reaches.
    let newest = Window {
        since_ms: retention_start_ms.max(now_ms.saturating_sub(max_span_ms)),
        until_ms: now_ms,
    };
    match read {
        // Nothing read, or nothing reaching back as far as the period begins.
        None => Some(newest),
        Some(read) if read.until_ms.saturating_sub(REFETCH_OVERLAP_MS) < retention_start_ms => {
            Some(newest)
        }
        // The window ends behind the clock, or begins before the period's start: top it up.
        Some(read) if read.until_ms < now_ms || read.since_ms <= retention_start_ms => {
            // Overlap at most half the span, so a narrow read still moves forward.
            let since_ms = read
                .until_ms
                .saturating_sub(REFETCH_OVERLAP_MS.min(max_span_ms / 2));
            Some(Window {
                since_ms,
                until_ms: now_ms.min(since_ms.saturating_add(max_span_ms)),
            })
        }
        // Older history, at the end the store does not hold yet.
        Some(read) => Some(Window {
            since_ms: retention_start_ms.max(read.since_ms.saturating_sub(max_span_ms)),
            until_ms: read.since_ms,
        }),
    }
}

/// What one read found: the raw events the store takes, or one plain sentence.
#[allow(dead_code)] // no caller until the owner approves the credential read
#[derive(Clone, Debug)]
pub(crate) struct CursorAccountUsage {
    pub(crate) records: Vec<Value>,
    pub(crate) error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Failure {
    /// A page answered 401 or 403.
    SignIn,
    /// Everything else, with no detail kept from the answer.
    Generic,
}

type Read<T> = Result<T, Failure>;

enum Answer {
    Body(Value),
    SignIn,
    Failed,
}

/// One page's place in the reconciled read.
struct Page {
    start: usize,
    end: usize,
    overlap: usize,
}

/// Reads the account's billed events for one window. `base` replaces the Cursor endpoint for a
/// test; `None` uses [`DEFAULT_CURSOR_API_ENDPOINT`].
///
/// The token is an argument and is never stored. `since_ms` and `end_date` are the same
/// millisecond bounds every page asks for.
#[allow(dead_code)] // no caller until the owner approves the credential read
pub(crate) async fn read_cursor_account_usage(
    token: &AccessToken,
    since_ms: u64,
    end_date: u64,
    base: Option<&str>,
) -> CursorAccountUsage {
    let base = base.unwrap_or(DEFAULT_CURSOR_API_ENDPOINT);
    let base = base.trim_end_matches('/');
    match tokio::time::timeout(READ_TIMEOUT, read_account(token, since_ms, end_date, base)).await {
        Ok(Ok(records)) => CursorAccountUsage {
            records,
            error: None,
        },
        Ok(Err(Failure::SignIn)) => CursorAccountUsage {
            records: Vec::new(),
            error: Some(SIGN_IN.to_owned()),
        },
        Ok(Err(Failure::Generic)) | Err(_) => CursorAccountUsage {
            records: Vec::new(),
            error: Some(GENERIC.to_owned()),
        },
    }
}

async fn read_account(
    token: &AccessToken,
    since_ms: u64,
    end_date: u64,
    base: &str,
) -> Read<Vec<Value>> {
    if since_ms > end_date {
        return Err(Failure::Generic);
    }
    // Plain http is for a loopback fake server only: the bearer token never crosses a network in clear.
    let plain = base.starts_with("http://");
    if plain && !is_loopback(base) {
        return Err(Failure::Generic);
    }
    let client = build_client(!plain)?;
    let (team_ids, user_id) = membership(&client, base, token).await?;
    let mut scopes = vec![None];
    scopes.extend(team_ids.into_iter().map(Some));
    let mut records = Vec::new();
    for team_id in scopes {
        records
            .extend(read_scope(&client, base, token, since_ms, end_date, team_id, user_id).await?);
    }
    Ok(records)
}

/// The host of a plain-http endpoint is exactly `localhost`, `127.0.0.1` or `[::1]`: a prefix match would let
/// `http://localhost.example.test` or `http://127.0.0.1.example.test` take the bearer token in clear.
fn is_loopback(base: &str) -> bool {
    reqwest::Url::parse(base).is_ok_and(|url| {
        url.scheme() == "http"
            && url.username().is_empty()
            && url.password().is_none()
            && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
    })
}

/// No proxy, no referer and no redirect at all: the bearer token would follow one.
fn build_client(https_only: bool) -> Read<Client> {
    Client::builder()
        .https_only(https_only)
        .no_proxy()
        .referer(false)
        .redirect(redirect::Policy::none())
        .build()
        .map_err(|_| Failure::Generic)
}

fn endpoint(base: &str, method: &str) -> String {
    format!("{base}/{DASHBOARD}/{method}")
}

async fn membership(client: &Client, base: &str, token: &AccessToken) -> Read<(Vec<u64>, u64)> {
    let teams = membership_answer(client, base, token, GET_TEAMS).await?;
    let me = membership_answer(client, base, token, GET_ME).await?;

    let mut team_ids = Vec::new();
    match teams.get("teams") {
        None => {}
        Some(Value::Array(teams)) => {
            for team in teams {
                let Some(id) = team.get("id").and_then(positive_integer) else {
                    return Err(Failure::Generic);
                };
                if !team_ids.contains(&id) {
                    team_ids.push(id);
                }
            }
        }
        Some(_) => return Err(Failure::Generic),
    }
    let Some(user_id) = me.get("userId").and_then(positive_integer) else {
        return Err(Failure::Generic);
    };
    Ok((team_ids, user_id))
}

async fn membership_answer(
    client: &Client,
    base: &str,
    token: &AccessToken,
    method: &str,
) -> Read<Map<String, Value>> {
    let url = endpoint(base, method);
    let value = match post(client, &url, token, &json!({})).await {
        Answer::Body(value) => value,
        Answer::SignIn | Answer::Failed => return Err(Failure::Generic),
    };
    let Value::Object(body) = value else {
        return Err(Failure::Generic);
    };
    if has_error(&body) {
        return Err(Failure::Generic);
    }
    Ok(body)
}

async fn read_scope(
    client: &Client,
    base: &str,
    token: &AccessToken,
    since_ms: u64,
    end_date: u64,
    team_id: Option<u64>,
    user_id: u64,
) -> Read<Vec<Value>> {
    let url = endpoint(base, GET_USAGE);
    let mut total: Option<u64> = None;
    let mut pages = Vec::new();
    let mut normalized: Vec<Option<Value>> = Vec::new();
    let mut previous: Vec<String> = Vec::new();
    let mut page_number = 1_u64;
    loop {
        let page_limit = match total {
            Some(total) => total
                .div_ceil(PAGE_SIZE as u64)
                .saturating_mul(2)
                .saturating_add(1),
            None => MAX_PAGES_WITHOUT_COUNT,
        };
        if page_number > page_limit {
            return Err(Failure::Generic);
        }
        let body = page_body(page_number, since_ms, end_date, team_id, user_id);
        let value = match post(client, &url, token, &body).await {
            Answer::Body(value) => value,
            Answer::SignIn => return Err(Failure::SignIn),
            Answer::Failed => return Err(Failure::Generic),
        };
        let (events, count) = checked_page(value, total)?;
        if let Some(count) = count {
            total = Some(count);
        }
        let start = normalized.len();
        let current: Vec<String> = events.iter().map(canonical).collect();
        let overlap = boundary_overlap(&previous, &current);
        previous = current;
        for raw in &events {
            normalized.push(normalized_event(raw, since_ms, end_date)?);
        }
        pages.push(Page {
            start,
            end: normalized.len(),
            overlap,
        });
        if events.len() < PAGE_SIZE {
            break;
        }
        page_number += 1;
    }

    let raw_count = u64::try_from(normalized.len()).map_err(|_| Failure::Generic)?;
    if let Some(total) = total
        && raw_count < total
    {
        return Err(Failure::Generic);
    }
    let mut removals = total.map_or(0, |total| raw_count - total);
    let mut records = Vec::new();
    for page in &pages {
        let overlap = u64::try_from(page.overlap).map_err(|_| Failure::Generic)?;
        let removal_count = removals.min(overlap);
        removals -= removal_count;
        let removal_count = usize::try_from(removal_count).map_err(|_| Failure::Generic)?;
        for (offset, slot) in normalized[page.start..page.end].iter_mut().enumerate() {
            let record = slot.take();
            if offset < removal_count {
                continue;
            }
            if let Some(record) = record {
                records.push(record);
            }
        }
    }
    if removals != 0 {
        return Err(Failure::Generic);
    }
    Ok(records)
}

fn page_body(page: u64, since_ms: u64, end_date: u64, team_id: Option<u64>, user_id: u64) -> Value {
    let mut body = json!({
        "page": page,
        "pageSize": PAGE_SIZE,
        "startDate": since_ms.to_string(),
        "endDate": end_date.to_string(),
    });
    if let Some(team_id) = team_id {
        body["teamId"] = json!(team_id);
        body["userId"] = json!(user_id);
    }
    body
}

/// One page's events and count, checked exactly as T3 checks them. An empty object is one empty
/// page; an object that names only a count holds no events.
fn checked_page(value: Value, total: Option<u64>) -> Read<(Vec<Value>, Option<u64>)> {
    let Value::Object(body) = value else {
        return Err(Failure::Generic);
    };
    if has_error(&body) {
        return Err(Failure::Generic);
    }
    let count = if body.is_empty() {
        Some(0)
    } else {
        body.get("totalUsageEventsCount")
            .map(|value| safe_integer(value).ok_or(Failure::Generic))
            .transpose()?
    };
    if let (Some(previous), Some(count)) = (total, count)
        && previous != count
    {
        return Err(Failure::Generic);
    }
    let events =
        if body.is_empty() || (body.len() == 1 && body.contains_key("totalUsageEventsCount")) {
            Vec::new()
        } else {
            let Some(Value::Array(events)) = body.get("usageEventsDisplay") else {
                return Err(Failure::Generic);
            };
            events.clone()
        };
    if events.len() > PAGE_SIZE {
        return Err(Failure::Generic);
    }
    Ok((events, count))
}

/// One raw event when it is a token event inside the window, or `None` when it is not a billed
/// event at all. A malformed token total or event shape fails the whole read, as in T3.
fn normalized_event(raw: &Value, since_ms: u64, end_date: u64) -> Read<Option<Value>> {
    let Some(event) = raw.as_object() else {
        return Ok(None);
    };
    let Some(usage) = event.get("tokenUsage").filter(|usage| !usage.is_null()) else {
        return Ok(None);
    };
    for key in [
        "inputTokens",
        "outputTokens",
        "cacheReadTokens",
        "cacheWriteTokens",
        "totalCents",
    ] {
        if let Some(value) = usage.get(key) {
            let Some(number) = value.as_f64().filter(|number| number.is_finite()) else {
                return Err(Failure::Generic);
            };
            if number < 0.0 {
                return Err(Failure::Generic);
            }
        }
    }
    let Some(timestamp) = event.get("timestamp").and_then(timestamp_number) else {
        return Err(Failure::Generic);
    };
    let Some(model) = event.get("model").and_then(Value::as_str) else {
        return Err(Failure::Generic);
    };
    if model.is_empty() {
        return Err(Failure::Generic);
    }
    if timestamp < since_ms as f64 || timestamp > end_date as f64 {
        return Ok(None);
    }
    Ok(Some(raw.clone()))
}

fn has_error(body: &Map<String, Value>) -> bool {
    ["error", "message", "code"]
        .iter()
        .any(|key| body.contains_key(*key))
}

fn safe_integer(value: &Value) -> Option<u64> {
    let number = value.as_f64()?;
    (number.is_finite() && number.fract() == 0.0 && (0.0..=MAX_SAFE_INTEGER).contains(&number))
        .then_some(number as u64)
}

fn positive_integer(value: &Value) -> Option<u64> {
    safe_integer(value).filter(|number| *number > 0)
}

/// Cursor sends milliseconds as a number or as a string of them.
fn timestamp_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64().filter(|number| number.is_finite()),
        Value::String(text) if !text.trim().is_empty() => text
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|number| number.is_finite()),
        _ => None,
    }
}

/// The longest suffix of `previous` that is also a prefix of `current`: the page boundary's
/// duplicated events, in order.
fn boundary_overlap(previous: &[String], current: &[String]) -> usize {
    let mut sequence: Vec<&str> = current.iter().map(String::as_str).collect();
    sequence.push("");
    sequence.extend(previous.iter().map(String::as_str));
    let mut lengths = vec![0usize; sequence.len()];
    for index in 1..sequence.len() {
        let mut length = lengths[index - 1];
        while length > 0 && sequence[index] != sequence[length] {
            length = lengths[length - 1];
        }
        if sequence[index] == sequence[length] {
            length += 1;
        }
        lengths[index] = length;
    }
    lengths.last().copied().unwrap_or(0)
}

/// JSON with object keys sorted, so equal events in another key order compare equal.
fn canonical(value: &Value) -> String {
    match value {
        Value::Array(items) => {
            let items: Vec<String> = items.iter().map(canonical).collect();
            format!("[{}]", items.join(","))
        }
        Value::Object(map) => {
            let entries: BTreeMap<&String, &Value> = map.iter().collect();
            let entries: Vec<String> = entries
                .into_iter()
                .map(|(key, value)| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(key).unwrap_or_default(),
                        canonical(value)
                    )
                })
                .collect();
            format!("{{{}}}", entries.join(","))
        }
        _ => serde_json::to_string(value).unwrap_or_else(|_| "null".to_owned()),
    }
}

async fn post(client: &Client, url: &str, token: &AccessToken, body: &Value) -> Answer {
    let Ok(mut authorization) = HeaderValue::from_str(&format!("Bearer {}", token.expose())) else {
        return Answer::Failed;
    };
    authorization.set_sensitive(true);
    let response = client
        .post(url)
        .header(AUTHORIZATION, authorization)
        .header("connect-protocol-version", "1")
        .header(CONTENT_TYPE, "application/json")
        .body(body.to_string())
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await;
    let Ok(response) = response else {
        return Answer::Failed;
    };
    match response.status() {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => return Answer::SignIn,
        status if status.is_success() => {}
        _ => return Answer::Failed,
    }
    match response.json::<Value>().await {
        Ok(value) => Answer::Body(value),
        Err(_) => Answer::Failed,
    }
}

#[cfg(test)]
#[path = "cursor_usage_tests.rs"]
mod tests;
