//! Forgejo issues through REST API v1 (docs/issues.md, "Forgejo as built"). The token comes from the keychain,
//! goes only in the `Authorization: token ...` header of a request to the connection's own https host, and is
//! never logged or put in an error. Everything the tracker returns is untrusted text: it is cut to the limits
//! below, never parsed for instructions, and an error is always the service's own sentence, never the tracker's
//! reply.
//!
//! UNVERIFIED against a live Forgejo. The field names, `X-Total-Count`, the `Link` header, `sort=recentupdate`,
//! the comments endpoint (Forgejo 17's API reference lists no page or limit for it, so the answer holds every
//! comment) and the `assignee` filter name (Forgejo 17's API reference calls the list filter `assigned_by`) come
//! from the API as understood; a version or an instance behind a proxy may differ.

use super::frame::cut;
use super::host::validate_host;
use super::query::{AssigneeFilter, IssueQuery, StateFilter};
use super::{IssueSource, MAX_SCOPE_BYTES, ScopeList};
use crate::error::{CoreError, Result};
use async_trait::async_trait;
use orbyn_contracts::{
    IssueActor, IssueComment, IssueCounts, IssueDetail, IssueLabel, IssueState, IssueStateCategory,
    IssueSummary, TrackerKind,
};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, StatusCode, Url, redirect};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
use std::time::Duration;
use zeroize::Zeroizing;

const TIMEOUT: Duration = Duration::from_secs(10);
/// The most one answer may hold. A longer one is refused, not cut.
const MAX_RESPONSE: usize = 2 * 1024 * 1024;
const MAX_REDIRECTS: usize = 3;
const MAX_PAGE: u32 = 1000;
/// Forgejo's own default and maximum page size.
const PAGE_LIMIT: usize = 50;
const MAX_TITLE: usize = 256;
const MAX_LABEL: usize = 200;
const MAX_LOGIN: usize = 100;
const MAX_URL: usize = 2048;
const MAX_LABELS: usize = 100;
const MAX_ASSIGNEES: usize = 50;
/// The issue body kept (the same as a body the person may write, docs/issues.md). Comments keep the draft's 16 KiB.
const MAX_BODY_BYTES: usize = 64 * 1024;
const MAX_COMMENTS: usize = 50;

pub(crate) struct Forgejo {
    /// `https://host[/prefix]`, validated, no trailing slash.
    host: String,
    token: Zeroizing<String>,
    client: Client,
    timeout: Duration,
}

impl std::fmt::Debug for Forgejo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Forgejo")
            .field("host", &self.host)
            .finish_non_exhaustive()
    }
}

/// The header Forgejo reads a personal or OAuth2 token from; the value carries the literal `token ` prefix.
fn token_header() -> HeaderName {
    HeaderName::from_static("authorization")
}

fn unavailable() -> CoreError {
    CoreError::TrackerUnavailable("Forgejo did not answer.".into())
}

fn unreadable() -> CoreError {
    CoreError::TrackerUnavailable("Forgejo answered something unreadable.".into())
}

fn too_big() -> CoreError {
    CoreError::TrackerUnavailable("Forgejo answered more than we read.".into())
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

impl Forgejo {
    /// A reader of one connection. A host that is not `https://` is refused here, before any request.
    pub(crate) fn new(host: &str, token: Zeroizing<String>) -> Result<Self> {
        Ok(Self {
            host: validate_host(host)?,
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

    /// One read of the API. `what` names the thing a 404 means is missing.
    async fn fetch(&self, path: &str, query: &[(&str, String)], what: &str) -> Result<Fetched> {
        let mut token =
            HeaderValue::from_str(&format!("token {}", self.token.as_str())).map_err(|_| {
                CoreError::TrackerNotConnected(
                    "The saved Forgejo token cannot be used. Connect again with a new one.".into(),
                )
            })?;
        token.set_sensitive(true);
        let mut response = self
            .client
            .get(format!("{}{path}", self.host))
            .query(query)
            .header(token_header(), token)
            .header(reqwest::header::ACCEPT, "application/json")
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|_| unavailable())?;
        let status = response.status();
        if !status.is_success() {
            return Err(failure(status, response.headers(), what));
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

    /// The tracker's totals, before any filter, from `X-Total-Count` on one open and one closed read.
    /// Best effort: no answer means no totals, never a failed list.
    async fn counts(&self, path: &str) -> Option<IssueCounts> {
        let (open, closed) = tokio::join!(self.count(path, "open"), self.count(path, "closed"));
        Some(IssueCounts {
            open: open.ok()?,
            closed: closed.ok()?,
        })
    }

    async fn count(&self, path: &str, state: &str) -> Result<u32> {
        let got = self
            .fetch(
                path,
                &[
                    ("type", "issues".to_owned()),
                    ("state", state.to_owned()),
                    ("limit", "1".to_owned()),
                ],
                "repository",
            )
            .await?;
        let total = got
            .headers
            .get("x-total-count")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .ok_or_else(unreadable)?;
        Ok(u32::try_from(total).unwrap_or(u32::MAX))
    }

    /// The login the saved token belongs to, for the `me` assignee filter.
    async fn signed_in_login(&self) -> Result<String> {
        let got = self.fetch("/api/v1/user", &[], "user").await?;
        let value = parse(&got.body)?;
        value
            .get("login")
            .and_then(Value::as_str)
            .filter(|login| !login.is_empty())
            .map(str::to_owned)
            .ok_or_else(unreadable)
    }

    /// The `assignee` parameter for a filter. Forgejo's issue list has no way to ask for no assignee.
    async fn assignee_param(&self, filter: &AssigneeFilter) -> Result<Option<String>> {
        match filter {
            AssigneeFilter::Any => Ok(None),
            AssigneeFilter::Login(login) => Ok(Some(login.clone())),
            AssigneeFilter::Me => Ok(Some(self.signed_in_login().await?)),
            AssigneeFilter::NoOne => Err(CoreError::Invalid(
                "Forgejo cannot filter for unassigned issues.".into(),
            )),
        }
    }
}

struct Fetched {
    headers: HeaderMap,
    body: Vec<u8>,
}

/// A failed status in the service's own words. The tracker's body is never passed on.
fn failure(status: StatusCode, headers: &HeaderMap, what: &str) -> CoreError {
    match status.as_u16() {
        401 => CoreError::TrackerNotConnected(
            "Forgejo did not accept the saved token. Connect again with a new one.".into(),
        ),
        403 => CoreError::TrackerForbidden("Forgejo refused access to this repository.".into()),
        404 => CoreError::NotFound(what.to_owned()),
        429 => CoreError::TrackerRateLimited(format!(
            "Forgejo asked us to slow down. Try again {}.",
            wait_words(retry_after(headers))
        )),
        _ => unavailable(),
    }
}

/// Seconds to wait, from `Retry-After`. Anything else is unknown.
fn retry_after(headers: &HeaderMap) -> Option<u64> {
    headers
        .get("retry-after")?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
}

fn wait_words(seconds: Option<u64>) -> String {
    match seconds.map(|s| s.min(24 * 60 * 60)) {
        None => "in a minute".to_owned(),
        Some(0 | 1) => "in a second".to_owned(),
        Some(s) if s < 120 => format!("in {s} seconds"),
        Some(s) => format!("in {} minutes", s.div_ceil(60)),
    }
}

/// One owner or repository name: plain enough that a request cannot be steered with it.
fn segment_ok(segment: &str) -> bool {
    !segment.is_empty()
        && !segment.starts_with(['-', '.'])
        && segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
}

/// Percent-encode one path segment, so an `owner` or `repo` can never add a path separator or a query.
fn encode_segment(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The `owner` and `repo` path segments of a scope. Forgejo has no subgroups, so it is exactly two segments.
fn repository(scope: &str) -> Result<(String, String)> {
    let segments: Vec<&str> = scope.split('/').collect();
    if scope.len() <= MAX_SCOPE_BYTES
        && segments.len() == 2
        && segments.iter().all(|s| segment_ok(s))
    {
        Ok((encode_segment(segments[0]), encode_segment(segments[1])))
    } else {
        Err(CoreError::Invalid("A Forgejo scope is owner/repo.".into()))
    }
}

fn key_ok(key: &str) -> Result<()> {
    if key.is_empty() || key.len() > 9 || !key.chars().all(|c| c.is_ascii_digit()) {
        return Err(CoreError::Invalid(
            "A Forgejo issue key is a number.".into(),
        ));
    }
    Ok(())
}

/// The `owner/repo` of a Forgejo project from a git remote, when the remote is on the connection's host
/// (docs/issues.md, "Scope"). `host` is the stored connection host. The user name, port and a `.git` ending are
/// dropped, and Forgejo has no subgroups: only exactly two path segments name a repository. A remote of any
/// other host, or a path that names no repository, is `None`.
pub(crate) fn forgejo_path_of_remote(url: &str, host: &str) -> Option<String> {
    let wanted = host.trim().strip_prefix("https://")?;
    let (wanted_authority, wanted_prefix) = wanted.split_once('/').unwrap_or((wanted, ""));
    let wanted_name = wanted_authority
        .rsplit_once(':')
        .map_or(wanted_authority, |(name, _)| name);
    if wanted_name.is_empty() {
        return None;
    }
    let url = url.trim();
    let (authority, path, prefixed) = if let Some(rest) = url
        .strip_prefix("https://")
        .map(|r| (r, true))
        .or_else(|| url.strip_prefix("ssh://").map(|r| (r, false)))
    {
        let (authority, path) = rest.0.split_once('/')?;
        (authority, path, rest.1)
    } else if url.contains("://") {
        return None;
    } else {
        let (authority, path) = url.split_once(':')?;
        (authority, path, false)
    };
    let authority = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let name = authority.rsplit_once(':').map_or(authority, |(h, _)| h);
    if !name.eq_ignore_ascii_case(wanted_name) {
        return None;
    }
    let mut path = path.trim_matches('/');
    if prefixed && !wanted_prefix.is_empty() {
        path = path
            .strip_prefix(wanted_prefix.trim_matches('/'))?
            .strip_prefix('/')?;
    }
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, repo) = path.split_once('/')?;
    if repo.contains('/') || !segment_ok(owner) || !segment_ok(repo) {
        return None;
    }
    Some(format!("{owner}/{repo}"))
}

/// What binds a cursor to its scope and filters: a cursor made for other ones does not verify.
fn cursor_tag(scope: &str, query: &IssueQuery, page: u32) -> String {
    let state = match query.state {
        StateFilter::Open => "open",
        StateFilter::Closed => "closed",
        StateFilter::All => "all",
    };
    let labels = query.labels.join("\u{1f}");
    let assignee = format!("{:?}", query.assignee);
    let limit = query.limit.to_string();
    let page = page.to_string();
    let parts: [&str; 8] = [
        "forgejo-cursor",
        scope,
        state,
        &labels,
        &assignee,
        &query.text,
        &limit,
        &page,
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

fn cursor_for(scope: &str, query: &IssueQuery, page: u32) -> String {
    format!("{page}.{}", cursor_tag(scope, query, page))
}

/// The page this listing asks for: the first, or the one its cursor names. A cursor that was not issued for
/// this scope and these filters is refused.
fn page_of(scope: &str, query: &IssueQuery) -> Result<u32> {
    let Some(cursor) = query.cursors.get(&format!("forgejo {scope}")) else {
        return Ok(1);
    };
    let refused = || {
        CoreError::Invalid("That page cursor was not issued for this repository and filter.".into())
    };
    let (page, tag) = cursor.split_once('.').ok_or_else(refused)?;
    let page: u32 = page.parse().map_err(|_| refused())?;
    if !(2..=MAX_PAGE).contains(&page) || tag != cursor_tag(scope, query, page) {
        return Err(refused());
    }
    Ok(page)
}

/// The page `Link` names as next, when Forgejo sent one.
fn next_page_from_link(headers: &HeaderMap) -> Option<u32> {
    for header in headers.get_all("link").iter() {
        let text = header.to_str().ok()?;
        for part in text.split(',') {
            let Some((target, params)) = part.trim().split_once(';') else {
                continue;
            };
            let is_next = params.split(';').any(|param| {
                let (name, value) = param.trim().split_once('=').unwrap_or((param.trim(), ""));
                name.trim().eq_ignore_ascii_case("rel")
                    && value.trim().trim_matches('"').eq_ignore_ascii_case("next")
            });
            if !is_next {
                continue;
            }
            let target = target.trim().trim_start_matches('<').trim_end_matches('>');
            let url = Url::parse(target).ok()?;
            return url
                .query_pairs()
                .find(|(key, _)| key == "page")
                .and_then(|(_, value)| value.parse::<u32>().ok());
        }
    }
    None
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
    value
        .get("login")
        .and_then(Value::as_str)
        .filter(|login| !login.is_empty())
        .map(|login| IssueActor {
            login: clip(login, MAX_LOGIN),
        })
}

fn summary(host: &str, scope: &str, value: &Value) -> Option<IssueSummary> {
    // `type=issues` asks Forgejo to leave pull requests out, and a row that carries one anyway is not an issue.
    if value
        .get("pull_request")
        .is_some_and(|pull| !pull.is_null())
    {
        return None;
    }
    let number = value.get("number").and_then(Value::as_u64)?;
    let closed = text(value, "state").eq_ignore_ascii_case("closed");
    let own_url = format!("{host}/{scope}/issues/{number}");
    let web_url = text(value, "html_url");
    // An address the tracker gives is used only when it stays on the connection's host.
    let url = if web_url.len() <= MAX_URL && web_url.starts_with(&format!("{host}/")) {
        web_url
    } else {
        own_url
    };
    Some(IssueSummary {
        tracker: TrackerKind::Forgejo,
        scope: scope.to_owned(),
        key: number.to_string(),
        display_key: format!("#{number}"),
        title: clip(&text(value, "title"), MAX_TITLE),
        url,
        author: value.get("user").and_then(actor),
        state: IssueState {
            category: if closed {
                IssueStateCategory::Closed
            } else {
                IssueStateCategory::Open
            },
            name: if closed { "Closed" } else { "Open" }.to_owned(),
            close_reason: None,
        },
        created_at: text(value, "created_at"),
        updated_at: text(value, "updated_at"),
        closed_at: Some(text(value, "closed_at")).filter(|t| !t.is_empty()),
        assignees: value
            .get("assignees")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(actor).take(MAX_ASSIGNEES).collect())
            .unwrap_or_default(),
        labels: value
            .get("labels")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|label| label.get("name").and_then(Value::as_str))
                    .take(MAX_LABELS)
                    .map(|name| IssueLabel {
                        name: clip(name, MAX_LABEL),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        comment_count: value
            .get("comments")
            .and_then(Value::as_u64)
            .map_or(0, |n| u32::try_from(n).unwrap_or(u32::MAX)),
    })
}

/// The people's comments from this answer, oldest first as Forgejo returns them.
fn comments_from(rows: &[Value]) -> Vec<IssueComment> {
    rows.iter()
        .take(MAX_COMMENTS)
        .map(|comment| IssueComment {
            id: comment
                .get("id")
                .and_then(Value::as_u64)
                .map(|id| id.to_string())
                .unwrap_or_default(),
            author: comment.get("user").and_then(actor),
            body: cut(&text(comment, "body"), super::frame::MAX_BODY_BYTES).to_owned(),
            created_at: text(comment, "created_at"),
        })
        .collect()
}

fn parse(body: &[u8]) -> Result<Value> {
    serde_json::from_slice(body).map_err(|_| unreadable())
}

#[async_trait]
impl IssueSource for Forgejo {
    async fn list(&self, scope: &str, query: &IssueQuery) -> Result<ScopeList> {
        let (owner, repo) = repository(scope)?;
        let page = page_of(scope, query)?;
        if query.labels.iter().any(|label| label.contains(',')) {
            return Err(CoreError::Invalid(
                "A Forgejo label filter cannot contain a comma.".into(),
            ));
        }
        let per_page = query.limit.clamp(1, PAGE_LIMIT);
        let assignee = self.assignee_param(&query.assignee).await?;
        let mut params: Vec<(&str, String)> = vec![
            ("type", "issues".to_owned()),
            (
                "state",
                match query.state {
                    StateFilter::Open => "open",
                    StateFilter::Closed => "closed",
                    StateFilter::All => "all",
                }
                .to_owned(),
            ),
            ("sort", "recentupdate".to_owned()),
            ("limit", per_page.to_string()),
            ("page", page.to_string()),
        ];
        if !query.labels.is_empty() {
            params.push(("labels", query.labels.join(",")));
        }
        if let Some(login) = assignee {
            params.push(("assignee", login));
        }
        if !query.text.is_empty() {
            params.push(("q", query.text.clone()));
        }
        let path = format!("/api/v1/repos/{owner}/{repo}/issues");
        let (listed, counts) =
            tokio::join!(self.fetch(&path, &params, "repository"), self.counts(&path));
        let listed = listed?;
        let value = parse(&listed.body)?;
        let rows = value.as_array().ok_or_else(unreadable)?;
        let issues: Vec<IssueSummary> = rows
            .iter()
            .filter_map(|v| summary(&self.host, scope, v))
            .collect();
        let next_page = next_page_from_link(&listed.headers)
            .or_else(|| (rows.len() == per_page).then_some(page + 1));
        Ok(ScopeList {
            issues,
            counts,
            next: next_page
                .filter(|n| (2..=MAX_PAGE).contains(n))
                .map(|n| cursor_for(scope, query, n)),
        })
    }

    async fn get(&self, scope: &str, key: &str) -> Result<IssueDetail> {
        let (owner, repo) = repository(scope)?;
        key_ok(key)?;
        let issue_path = format!("/api/v1/repos/{owner}/{repo}/issues/{key}");
        let issue = self.fetch(&issue_path, &[], "issue").await?;
        let value = parse(&issue.body)?;
        let summary = summary(&self.host, scope, &value)
            .ok_or_else(|| CoreError::NotFound("issue".into()))?;
        let comments = self
            .fetch(&format!("{issue_path}/comments"), &[], "issue")
            .await?;
        let parsed = parse(&comments.body)?;
        let rows = parsed.as_array().ok_or_else(unreadable)?;
        Ok(IssueDetail {
            summary,
            body: cut(&text(&value, "body"), MAX_BODY_BYTES).to_owned(),
            comments: comments_from(rows),
            linked_pull_requests: Vec::new(),
            linked_threads: Vec::new(),
        })
    }
}

#[cfg(test)]
#[path = "forgejo_tests.rs"]
mod tests;
