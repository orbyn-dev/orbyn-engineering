//! Linear issues through its GraphQL API (docs/issues.md, "Linear as built"). One endpoint,
//! `https://api.linear.app/graphql`, with fixed query strings and GraphQL variables; nothing the person types is ever
//! spliced into the query. Queries only, never a mutation. The token comes from the keychain, goes only in the
//! `Authorization` header of a request to that one host, and is never logged or put in an error. Everything the
//! tracker returns is untrusted text: it is cut to the limits below and is never parsed for instructions.
//!
//! UNVERIFIED against a live Linear. The query and field names, the filter comparators, the error codes in
//! `extensions.code`, the rate-limit headers and `teams(first: 20)` come from Linear's public GraphQL schema and
//! developer documentation as understood; a workspace, version or proxy may differ.

use super::frame::cut;
use super::host::validate_host;
use super::query::{AssigneeFilter, IssueQuery, StateFilter};
use super::{IssueSource, ScopeList};
use crate::error::{CoreError, Result};
use async_trait::async_trait;
use orbyn_contracts::{
    IssueActor, IssueComment, IssueDetail, IssueState, IssueStateCategory, IssueSummary,
    TrackerKind,
};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Client, StatusCode, Url, redirect};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
use std::time::Duration;
use zeroize::Zeroizing;

/// The one Linear origin. The API endpoint is `{host}/graphql`.
pub(super) const LINEAR_HOST: &str = "https://api.linear.app";
const TIMEOUT: Duration = Duration::from_secs(10);
/// The most one answer may hold. A longer one is refused, not cut.
const MAX_RESPONSE: usize = 2 * 1024 * 1024;
const MAX_REDIRECTS: usize = 3;
/// The most issues one page asks for; the service caps a listing at 100.
const MAX_PAGE: usize = 100;
const MAX_TITLE: usize = 256;
const MAX_LABEL: usize = 200;
const MAX_LOGIN: usize = 100;
const MAX_ID: usize = 100;
const MAX_URL: usize = 2048;
const MAX_LABELS: usize = 100;
/// The most label names a filter may carry (docs/issues.md, "Filters").
const MAX_LABEL_FILTERS: usize = 20;
/// The issue body kept (the same as a body the person may write, docs/issues.md). Comments keep the draft's 16 KiB.
const MAX_BODY_BYTES: usize = 64 * 1024;
const MAX_COMMENTS: usize = 50;
/// The most teams a connection with no stored scope reads.
const MAX_TEAMS: usize = 20;
/// One cursor as `query.rs` accepts it back.
const MAX_CURSOR_BYTES: usize = 200;

/// The fields a listing asks for. No description and no comments: up to 100 bodies would blow the 2 MiB cap.
const LIST_FIELDS: &str = "identifier title url createdAt updatedAt completedAt canceledAt \
     state { name type } creator { name displayName } assignee { name displayName } \
     labels(first: 100) { nodes { name } }";
/// The fields a detail asks for, including the body. Comments are added by the detail query.
const DETAIL_FIELDS: &str = "identifier title description url createdAt updatedAt completedAt canceledAt \
     state { name type } creator { name displayName } assignee { name displayName } \
     labels(first: 100) { nodes { name } }";
/// The one read a connection with no stored scope makes (docs/issues.md, "Linear as built").
const TEAMS_QUERY: &str = "query OrbynTeams { teams(first: 20) { nodes { key } } }";

pub(crate) struct Linear {
    /// `https://api.linear.app`, validated, no trailing slash.
    host: String,
    token: Zeroizing<String>,
    client: Client,
    timeout: Duration,
}

impl std::fmt::Debug for Linear {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Linear")
            .field("host", &self.host)
            .finish_non_exhaustive()
    }
}

fn unavailable() -> CoreError {
    CoreError::TrackerUnavailable("Linear did not answer.".into())
}

fn unreadable() -> CoreError {
    CoreError::TrackerUnavailable("Linear answered something unreadable.".into())
}

fn too_big() -> CoreError {
    CoreError::TrackerUnavailable("Linear answered more than we read.".into())
}

fn same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}

/// No proxy, no referer and no redirect to another origin: the token header would follow it.
fn build_client(https_only: bool) -> Result<Client> {
    Client::builder()
        .https_only(https_only)
        .no_proxy()
        .referer(false)
        .redirect(redirect::Policy::custom(|attempt| {
            let same = attempt
                .previous()
                .first()
                .is_some_and(|first| same_origin(first, attempt.url()));
            if same && attempt.previous().len() <= MAX_REDIRECTS {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .map_err(|_| unavailable())
}

fn shared_client() -> Result<Client> {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    if let Some(client) = CLIENT.get() {
        return Ok(client.clone());
    }
    let client = build_client(true)?;
    Ok(CLIENT.get_or_init(|| client).clone())
}

/// The fixed Linear origin, validated as a person's host and then as the one endpoint. A stored http host, another
/// https host or a host with user info is refused here, before any request.
pub(super) fn validate_linear_host(input: &str) -> Result<String> {
    let host = validate_host(input)?;
    if host != LINEAR_HOST {
        return Err(CoreError::Invalid(
            "Linear has one host: https://api.linear.app.".into(),
        ));
    }
    Ok(host)
}

/// A team key: `^[A-Z][A-Z0-9]{1,9}$`.
fn team_key_ok(scope: &str) -> bool {
    let bytes = scope.as_bytes();
    (2..=10).contains(&bytes.len())
        && bytes[0].is_ascii_uppercase()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

pub(super) fn team_key(scope: &str) -> Result<()> {
    if team_key_ok(scope) {
        Ok(())
    } else {
        Err(CoreError::Invalid(
            "A Linear scope is a team key such as ENG.".into(),
        ))
    }
}

/// An issue identifier: `^[A-Z][A-Z0-9]{1,9}-[0-9]{1,9}$`.
fn identifier_ok(key: &str) -> bool {
    let Some((team, number)) = key.split_once('-') else {
        return false;
    };
    team_key_ok(team)
        && !number.is_empty()
        && number.len() <= 9
        && number.bytes().all(|b| b.is_ascii_digit())
}

fn uuid_ok(key: &str) -> bool {
    let bytes = key.as_bytes();
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(index, byte)| {
            let dash = matches!(index, 8 | 13 | 18 | 23);
            if dash {
                *byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

fn issue_key(key: &str) -> Result<()> {
    if identifier_ok(key) || uuid_ok(key) {
        Ok(())
    } else {
        Err(CoreError::Invalid(
            "A Linear issue is an identifier such as ENG-123 or its id.".into(),
        ))
    }
}

/// The label filters as the tracker accepts them: a name, at most this reader's 200 characters, and at most 20
/// names per filter. A label outside that is refused before a request.
fn check_filters(query: &IssueQuery) -> Result<()> {
    if query.labels.len() > MAX_LABEL_FILTERS {
        return Err(CoreError::Invalid(format!(
            "Filter by at most {MAX_LABEL_FILTERS} labels."
        )));
    }
    if query.labels.iter().any(|label| {
        label.is_empty() || label.chars().count() > MAX_LABEL || label.chars().any(char::is_control)
    }) {
        return Err(CoreError::Invalid(
            "A Linear label filter is a name of at most 200 characters, with no control characters."
                .into(),
        ));
    }
    Ok(())
}

/// What binds a cursor to its team and filters: a cursor made for other ones does not verify.
fn cursor_tag(scope: &str, query: &IssueQuery) -> String {
    let state = match query.state {
        StateFilter::Open => "open",
        StateFilter::Closed => "closed",
        StateFilter::All => "all",
    };
    let labels = query.labels.join("\u{1f}");
    let assignee = format!("{:?}", query.assignee);
    let limit = query.limit.to_string();
    let parts: [&str; 7] = [
        "linear-cursor",
        scope,
        state,
        &labels,
        &assignee,
        &query.text,
        &limit,
    ];
    let mut hash = Sha256::new();
    for part in parts {
        hash.update(part.as_bytes());
        hash.update([0]);
    }
    hash.finalize()
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Linear's `endCursor` with the tag for this team and filter. A cursor too long for `query.rs` is not emitted.
fn cursor_for(scope: &str, query: &IssueQuery, end: &str) -> Option<String> {
    let cursor = format!("{}.{end}", cursor_tag(scope, query));
    (cursor.len() <= MAX_CURSOR_BYTES).then_some(cursor)
}

/// The `after` cursor this listing asks for: none for the first page, or the one its cursor names. A cursor that was
/// not issued for this team and these filters is refused.
fn after_of(scope: &str, query: &IssueQuery) -> Result<Option<String>> {
    let Some(cursor) = query.cursors.get(&format!("linear {scope}")) else {
        return Ok(None);
    };
    let refused =
        || CoreError::Invalid("That page cursor was not issued for this team and filter.".into());
    if cursor.len() > MAX_CURSOR_BYTES || cursor.chars().any(char::is_control) {
        return Err(refused());
    }
    let (tag, end) = cursor.split_once('.').ok_or_else(refused)?;
    if end.is_empty() || end.len() > MAX_CURSOR_BYTES || tag != cursor_tag(scope, query) {
        return Err(refused());
    }
    Ok(Some(end.to_owned()))
}

/// A page of issues. Only the filters that are set go in the filter object, so an empty list is not a filter that
/// matches nothing. Every value is a variable; the query text is assembled from the fixed pieces here.
fn list_body(scope: &str, query: &IssueQuery, after: Option<&str>) -> Value {
    let mut declared = vec![
        "$team: String!".to_owned(),
        "$first: Int!".to_owned(),
        "$after: String".to_owned(),
    ];
    let mut filter = vec!["team: {key: {eq: $team}}".to_owned()];
    let mut variables = json!({
        "team": scope,
        "first": query.limit.clamp(1, MAX_PAGE),
        "after": after,
    });
    match query.state {
        StateFilter::Open => {
            declared.push("$state: [String!]".to_owned());
            filter.push("state: {type: {in: $state}}".to_owned());
            variables["state"] = json!(["triage", "backlog", "unstarted", "started"]);
        }
        StateFilter::Closed => {
            declared.push("$state: [String!]".to_owned());
            filter.push("state: {type: {in: $state}}".to_owned());
            variables["state"] = json!(["completed", "canceled", "duplicate"]);
        }
        StateFilter::All => {}
    }
    match &query.assignee {
        AssigneeFilter::Any => {}
        AssigneeFilter::Me => filter.push("assignee: {isMe: {eq: true}}".to_owned()),
        AssigneeFilter::NoOne => filter.push("assignee: {null: true}".to_owned()),
        // Linear has no login handle: this filter is the person's name as Linear shows it.
        AssigneeFilter::Login(name) => {
            declared.push("$assignee: String!".to_owned());
            filter.push("assignee: {name: {eq: $assignee}}".to_owned());
            variables["assignee"] = json!(name);
        }
    }
    if !query.labels.is_empty() {
        // One condition per label inside `and`, so every named label must be on the issue (docs/issues.md, "Filters").
        let mut conditions = Vec::with_capacity(query.labels.len());
        for (index, label) in query.labels.iter().enumerate() {
            declared.push(format!("$label{index}: String!"));
            conditions.push(format!("{{labels: {{name: {{eq: $label{index}}}}}}}"));
            variables[format!("label{index}")] = json!(label);
        }
        filter.push(format!("and: [{}]", conditions.join(", ")));
    }
    if !query.text.is_empty() {
        declared.push("$text: String!".to_owned());
        filter.push(
            "or: [{title: {containsIgnoreCase: $text}}, {description: {containsIgnoreCase: $text}}]"
                .to_owned(),
        );
        variables["text"] = json!(query.text);
    }
    let query = format!(
        "query OrbynIssues({}) {{ issues(filter: {{{}}}, first: $first, after: $after, orderBy: updatedAt) \
         {{ nodes {{ {LIST_FIELDS} }} pageInfo {{ hasNextPage endCursor }} }} }}",
        declared.join(", "),
        filter.join(", ")
    );
    json!({ "query": query, "variables": variables })
}

/// The one detail read. The issue and its first 50 comments come back together.
fn detail_body(key: &str) -> Value {
    let query = format!(
        "query OrbynIssue($id: String!) {{ issue(id: $id) {{ {DETAIL_FIELDS} \
         comments(first: 50) {{ nodes {{ id body createdAt user {{ name displayName }} }} }} }} }}"
    );
    json!({ "query": query, "variables": { "id": key } })
}

struct Fetched {
    headers: HeaderMap,
    body: Vec<u8>,
}

/// A failed status in the service's own words. The tracker's body is never passed on.
fn rate_limited(headers: &HeaderMap) -> CoreError {
    CoreError::TrackerRateLimited(format!(
        "Linear asked us to slow down. Try again {}.",
        wait_words(wait_seconds(headers))
    ))
}

fn failure(status: StatusCode, headers: &HeaderMap) -> CoreError {
    match status.as_u16() {
        401 => CoreError::TrackerNotConnected(
            "Linear did not accept the saved token. Connect again with a new one.".into(),
        ),
        403 => CoreError::TrackerForbidden("Linear refused access to this team.".into()),
        404 => CoreError::NotFound("issue".into()),
        429 => rate_limited(headers),
        _ => unavailable(),
    }
}

/// Seconds to wait, from an integer `Retry-After`, then from Linear's `X-RateLimit-Requests-Reset` (UTC epoch
/// milliseconds). An HTTP-date `Retry-After` and anything else are unknown.
fn wait_seconds(headers: &HeaderMap) -> Option<u64> {
    if let Some(seconds) = headers
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
    {
        return Some(seconds);
    }
    let reset = headers
        .get("x-ratelimit-requests-reset")?
        .to_str()
        .ok()?
        .trim()
        .parse::<i64>()
        .ok()?;
    let millis = reset.saturating_sub(crate::util::unix_now_ms()).max(0);
    Some(u64::try_from(millis / 1000).unwrap_or(u64::MAX))
}

fn wait_words(seconds: Option<u64>) -> String {
    match seconds.map(|s| s.min(24 * 60 * 60)) {
        None => "in a minute".to_owned(),
        Some(0 | 1) => "in a second".to_owned(),
        Some(s) if s < 120 => format!("in {s} seconds"),
        Some(s) => format!("in {} minutes", s.div_ceil(60)),
    }
}

/// Case, spaces, underscores and hyphens do not change an error kind: `AUTHENTICATION_ERROR`,
/// `authentication error` and `AuthenticationError` are the same.
fn error_kind(text: &str) -> String {
    text.chars()
        .filter(|ch| !matches!(ch, ' ' | '_' | '-'))
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

/// Linear answers HTTP 200 with an `errors` array on failure. `extensions.type` is read before `extensions.code`;
/// the tracker's `message` is never used, and any kind this reader does not know is the fixed generic failure.
fn graphql_failure(headers: &HeaderMap, value: &Value, detail: bool) -> Option<CoreError> {
    let errors = value.get("errors")?;
    let Some(rows) = errors.as_array() else {
        return Some(unreadable());
    };
    if rows.is_empty() {
        return None;
    }
    let kind = rows.iter().find_map(|error| {
        let extensions = error.get("extensions")?;
        ["type", "code"].iter().find_map(|key| {
            extensions
                .get(*key)
                .and_then(Value::as_str)
                .filter(|kind| !kind.is_empty())
                .map(error_kind)
        })
    });
    Some(match kind.as_deref() {
        Some("authenticationerror" | "authentication") => CoreError::TrackerNotConnected(
            "Linear did not accept the saved token. Connect again with a new one.".into(),
        ),
        Some("forbidden") => {
            CoreError::TrackerForbidden("Linear refused access to this team.".into())
        }
        Some("ratelimited" | "ratelimit") => rate_limited(headers),
        Some("invalidinput" | "notfound" | "entitynotfound") if detail => {
            CoreError::NotFound("issue".into())
        }
        _ => unavailable(),
    })
}

fn clip(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn actor(value: &Value) -> Option<IssueActor> {
    let name = ["displayName", "name"].iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
    })?;
    Some(IssueActor {
        login: clip(name, MAX_LOGIN),
    })
}

/// The issue's web address: Linear's own `url` when it is an `https://linear.app` address with no user info or
/// port, otherwise the one built from the identifier.
fn web_url(identifier: &str, value: &Value) -> String {
    let own = format!("https://linear.app/issue/{identifier}");
    let Some(text) = value.get("url").and_then(Value::as_str) else {
        return own;
    };
    if text.len() > MAX_URL {
        return own;
    }
    let Ok(url) = Url::parse(text) else {
        return own;
    };
    if url.scheme() == "https"
        && url.host_str() == Some("linear.app")
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
    {
        text.to_owned()
    } else {
        own
    }
}

fn summary(scope: &str, value: &Value, comment_count: u32) -> Option<IssueSummary> {
    let identifier = value.get("identifier")?.as_str()?;
    if !identifier_ok(identifier) {
        return None;
    }
    let state = value.get("state")?;
    let state_type = state.get("type")?.as_str()?;
    let state_name = state.get("name")?.as_str()?;
    let closed = matches!(state_type, "completed" | "canceled" | "duplicate");
    let closed_at = if closed {
        let key = if state_type == "completed" {
            "completedAt"
        } else {
            "canceledAt"
        };
        value
            .get(key)
            .and_then(Value::as_str)
            .filter(|time| !time.is_empty())
            .map(str::to_owned)
    } else {
        None
    };
    let labels = value
        .get("labels")
        .and_then(|labels| labels.get("nodes"))
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|label| label.get("name").and_then(Value::as_str))
                .take(MAX_LABELS)
                .map(|name| orbyn_contracts::IssueLabel {
                    name: clip(name, MAX_LABEL),
                })
                .collect()
        })
        .unwrap_or_default();
    Some(IssueSummary {
        tracker: TrackerKind::Linear,
        scope: scope.to_owned(),
        key: identifier.to_owned(),
        display_key: identifier.to_owned(),
        title: clip(&text(value, "title"), MAX_TITLE),
        url: web_url(identifier, value),
        author: value.get("creator").and_then(actor),
        state: IssueState {
            category: if closed {
                IssueStateCategory::Closed
            } else {
                IssueStateCategory::Open
            },
            name: clip(state_name, MAX_LABEL),
            close_reason: None,
        },
        created_at: text(value, "createdAt"),
        updated_at: text(value, "updatedAt"),
        closed_at,
        assignees: value.get("assignee").and_then(actor).into_iter().collect(),
        labels,
        comment_count,
    })
}

/// The newest 50 comments the answer carries, shown oldest first. The sort is stable, so comments with the same
/// `createdAt` keep Linear's order.
fn comments_from(rows: &[Value]) -> Vec<IssueComment> {
    let mut comments: Vec<IssueComment> = rows
        .iter()
        .take(MAX_COMMENTS)
        .map(|comment| IssueComment {
            id: clip(&text(comment, "id"), MAX_ID),
            author: comment.get("user").and_then(actor),
            body: cut(&text(comment, "body"), super::frame::MAX_BODY_BYTES).to_owned(),
            created_at: text(comment, "createdAt"),
        })
        .collect();
    comments.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    comments
}

impl Linear {
    /// A reader of the one Linear endpoint. A stored host that is not `https://api.linear.app` is refused here,
    /// before any request.
    pub(crate) fn new(host: &str, token: Zeroizing<String>) -> Result<Self> {
        Ok(Self {
            host: validate_linear_host(host)?,
            token,
            client: shared_client()?,
            timeout: TIMEOUT,
        })
    }

    /// A reader of a local fake server over plain http, which only tests may build.
    #[cfg(test)]
    pub(crate) fn for_test(base: &str, token: &str, timeout: Duration) -> Self {
        Self {
            host: base.trim_end_matches('/').to_owned(),
            token: Zeroizing::new(token.to_owned()),
            client: build_client(false).unwrap(),
            timeout,
        }
    }

    /// One GraphQL read. Only a `query` is ever sent (docs/issues.md, "Linear as built").
    async fn fetch(&self, body: &Value) -> Result<Fetched> {
        let mut token = HeaderValue::from_str(self.token.as_str()).map_err(|_| {
            CoreError::TrackerNotConnected(
                "The saved Linear token cannot be used. Connect again with a new one.".into(),
            )
        })?;
        token.set_sensitive(true);
        let mut response = self
            .client
            .post(format!("{}/graphql", self.host))
            .header(reqwest::header::AUTHORIZATION, token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(reqwest::header::ACCEPT, "application/json")
            .body(body.to_string())
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|_| unavailable())?;
        let status = response.status();
        if !status.is_success() {
            return Err(failure(status, response.headers()));
        }
        if response
            .content_length()
            .is_some_and(|n| n > MAX_RESPONSE as u64)
        {
            return Err(too_big());
        }
        let headers = response.headers().clone();
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
            if body.len() + chunk.len() > MAX_RESPONSE {
                return Err(too_big());
            }
            body.extend_from_slice(&chunk);
        }
        Ok(Fetched { headers, body })
    }

    /// The `data` of a read whose `errors` array is empty. `detail` decides whether a not-found error is the
    /// contract's `not_found` (for a single issue) or the generic failure (for a listing).
    fn answer(got: &Fetched, detail: bool) -> Result<Value> {
        let value: Value = serde_json::from_slice(&got.body).map_err(|_| unreadable())?;
        if let Some(error) = graphql_failure(&got.headers, &value, detail) {
            return Err(error);
        }
        Ok(value)
    }
}

#[async_trait]
impl IssueSource for Linear {
    async fn list(&self, scope: &str, query: &IssueQuery) -> Result<ScopeList> {
        team_key(scope)?;
        check_filters(query)?;
        let after = after_of(scope, query)?;
        let got = self
            .fetch(&list_body(scope, query, after.as_deref()))
            .await?;
        let value = Self::answer(&got, false)?;
        let issues = value.pointer("/data/issues").ok_or_else(unreadable)?;
        let rows = issues
            .get("nodes")
            .and_then(Value::as_array)
            .ok_or_else(unreadable)?;
        let page_info = issues.get("pageInfo").ok_or_else(unreadable)?;
        let has_next = page_info
            .get("hasNextPage")
            .and_then(Value::as_bool)
            .ok_or_else(unreadable)?;
        let next = if has_next {
            let end = page_info
                .get("endCursor")
                .and_then(Value::as_str)
                .filter(|end| !end.is_empty() && !end.chars().any(char::is_control))
                .ok_or_else(unreadable)?;
            cursor_for(scope, query, end)
        } else {
            None
        };
        Ok(ScopeList {
            issues: rows
                .iter()
                .filter_map(|row| summary(scope, row, 0))
                // A row from another team is not part of this connection's scope.
                .filter(|issue| {
                    issue
                        .key
                        .split_once('-')
                        .is_some_and(|(team, _)| team == scope)
                })
                .collect(),
            counts: None,
            next,
        })
    }

    async fn get(&self, scope: &str, key: &str) -> Result<IssueDetail> {
        team_key(scope)?;
        issue_key(key)?;
        if identifier_ok(key) && !key.split_once('-').is_some_and(|(team, _)| team == scope) {
            return Err(CoreError::NotFound("issue".into()));
        }
        let got = self.fetch(&detail_body(key)).await?;
        let value = Self::answer(&got, true)?;
        let node = value.pointer("/data/issue").ok_or_else(unreadable)?;
        if node.is_null() {
            return Err(CoreError::NotFound("issue".into()));
        }
        let rows = node
            .pointer("/comments/nodes")
            .and_then(Value::as_array)
            .ok_or_else(unreadable)?;
        let comments = comments_from(rows);
        let comment_count = u32::try_from(comments.len()).unwrap_or(u32::MAX);
        let summary = summary(scope, node, comment_count).ok_or_else(unreadable)?;
        // A UUID names no team before the read, so its identifier is checked after the read.
        if !summary
            .key
            .split_once('-')
            .is_some_and(|(team, _)| team == scope)
        {
            return Err(CoreError::NotFound("issue".into()));
        }
        Ok(IssueDetail {
            summary,
            body: cut(&text(node, "description"), MAX_BODY_BYTES).to_owned(),
            comments,
            linked_pull_requests: Vec::new(),
            linked_threads: Vec::new(),
        })
    }

    /// The team keys the saved key can see, used by a connection that stores no scope. At most 20 are kept.
    async fn scopes(&self) -> Result<Vec<String>> {
        let got = self.fetch(&json!({ "query": TEAMS_QUERY })).await?;
        let value = Self::answer(&got, false)?;
        let rows = value
            .pointer("/data/teams/nodes")
            .and_then(Value::as_array)
            .ok_or_else(unreadable)?;
        let mut scopes = Vec::new();
        for row in rows {
            if scopes.len() == MAX_TEAMS {
                break;
            }
            let Some(key) = row.get("key").and_then(Value::as_str) else {
                continue;
            };
            if team_key_ok(key) && !scopes.iter().any(|scope| scope == key) {
                scopes.push(key.to_owned());
            }
        }
        Ok(scopes)
    }
}

#[cfg(test)]
#[path = "linear_tests.rs"]
mod tests;
