//! GitLab issues through the REST API (docs/issues.md, "GitLab"). The token comes from the keychain, goes only
//! in the `PRIVATE-TOKEN` header of a request to the connection's own https host, and is never logged or put in
//! an error. Everything the tracker returns is untrusted text: it is cut to the limits below, never parsed for
//! instructions, and an error is always the service's own sentence, never the tracker's reply.

use super::frame::cut;
use super::host::validate_host;
use super::query::{AssigneeFilter, IssueQuery, MAX_LIMIT, StateFilter};
use super::{
    IssueSource, MAX_SCOPE_BYTES, MAX_TEMPLATE_BODY_BYTES, MAX_TEMPLATES, NewIssue, ScopeList,
};
use crate::error::{CoreError, Result};
use async_trait::async_trait;
use orbyn_contracts::{
    IssueActor, IssueComment, IssueCounts, IssueDetail, IssueLabel, IssueReaction,
    IssueReactionCount, IssueState, IssueStateCategory, IssueSummary, IssueTemplate, TrackerKind,
};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Method, StatusCode, Url, redirect};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use zeroize::Zeroizing;

const TIMEOUT: Duration = Duration::from_secs(10);
/// The most one answer may hold. A longer one is refused, not cut.
const MAX_RESPONSE: usize = 2 * 1024 * 1024;
const MAX_REDIRECTS: usize = 3;
const MAX_PAGE: u32 = 1000;
const MAX_TITLE: usize = 256;
const MAX_LABEL: usize = 200;
const MAX_LOGIN: usize = 100;
const MAX_URL: usize = 2048;
const MAX_LABELS: usize = 100;
const MAX_ASSIGNEES: usize = 50;
/// The issue body kept (the same as a body the person may write, docs/issues.md). Comments keep the draft's 16 KiB.
const MAX_BODY_BYTES: usize = 64 * 1024;
const MAX_COMMENTS: usize = 50;
const MAX_NOTE_PAGES: u32 = 5;
const MAX_SEGMENTS: usize = 20;
/// One issue's reactions are read back at most this deep: a removal needs the reader's own award id.
const MAX_AWARD_PAGES: u32 = 5;
const MAX_AWARDS_PER_PAGE: usize = 100;

pub(crate) struct GitLab {
    /// `https://host[/prefix]`, validated, no trailing slash.
    host: String,
    token: Zeroizing<String>,
    client: Client,
    timeout: Duration,
}

impl std::fmt::Debug for GitLab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitLab")
            .field("host", &self.host)
            .finish_non_exhaustive()
    }
}

/// The header GitLab reads a personal or project token from. Names are case-insensitive on the wire.
fn token_header() -> HeaderName {
    HeaderName::from_static("private-token")
}

fn unavailable() -> CoreError {
    CoreError::TrackerUnavailable("GitLab did not answer.".into())
}

fn unreadable() -> CoreError {
    CoreError::TrackerUnavailable("GitLab answered something unreadable.".into())
}

fn too_big() -> CoreError {
    CoreError::TrackerUnavailable("GitLab answered more than we read.".into())
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

impl GitLab {
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
        self.send(Method::GET, path, query, None, what).await
    }

    /// One request of the API, read or write. The token, host, redirect, timeout and size rules are the read's; a
    /// JSON body travels in the body, never in the address, and never as an argument.
    async fn send(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
        what: &str,
    ) -> Result<Fetched> {
        let mut token = HeaderValue::from_str(self.token.as_str()).map_err(|_| {
            CoreError::TrackerNotConnected(
                "The saved GitLab token cannot be used. Connect again with a new one.".into(),
            )
        })?;
        token.set_sensitive(true);
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.host))
            .query(query)
            .header(token_header(), token)
            .header(reqwest::header::ACCEPT, "application/json")
            .timeout(self.timeout);
        if let Some(body) = body {
            request = request
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body.to_string());
        }
        let mut response = request.send().await.map_err(|_| unavailable())?;
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

    /// The tracker's totals, before any filter. Best effort: no answer means no totals, never a failed list.
    async fn counts(&self, project: &str) -> Option<IssueCounts> {
        let path = format!("/api/v4/projects/{project}/issues_statistics");
        let got = self
            .fetch(&path, &[("scope", "all".to_owned())], "project")
            .await
            .ok()?;
        let value: Value = serde_json::from_slice(&got.body).ok()?;
        let counts = value.get("statistics")?.get("counts")?;
        let n = |key: &str| {
            counts
                .get(key)?
                .as_u64()
                .map(|n| u32::try_from(n).unwrap_or(u32::MAX))
        };
        Some(IssueCounts {
            open: n("opened")?,
            closed: n("closed")?,
        })
    }

    /// The username the saved token belongs to: GitLab removes an award by its id, so the reader must know which
    /// award is its own.
    async fn signed_in_login(&self) -> Result<String> {
        let got = self.fetch("/api/v4/user", &[], "user").await?;
        let value = parse(&got.body)?;
        value
            .get("username")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .ok_or_else(unreadable)
    }

    /// The reader's own award of one name on a subject, if there is one: its id is what a removal takes.
    async fn own_award(&self, subject: &str, name: &str, login: &str) -> Result<Option<u64>> {
        for page in 1..=MAX_AWARD_PAGES {
            let got = self
                .fetch(
                    subject,
                    &[
                        ("per_page", MAX_AWARDS_PER_PAGE.to_string()),
                        ("page", page.to_string()),
                    ],
                    "issue",
                )
                .await?;
            let rows = parse(&got.body)?;
            let rows = rows.as_array().ok_or_else(unreadable)?;
            for row in rows {
                if row.get("name").and_then(Value::as_str) == Some(name)
                    && row
                        .pointer("/user/username")
                        .and_then(Value::as_str)
                        .is_some_and(|user| user.eq_ignore_ascii_case(login))
                    && let Some(id) = row.get("id").and_then(Value::as_u64)
                {
                    return Ok(Some(id));
                }
            }
            if rows.len() < MAX_AWARDS_PER_PAGE {
                return Ok(None);
            }
            if page == MAX_AWARD_PAGES {
                // The reader's own award may be on a page this reader does not read: a removal must not answer
                // success while it could not see it.
                return Err(CoreError::TrackerUnavailable(
                    "GitLab's award list is longer than Orbyn reads.".into(),
                ));
            }
        }
        Ok(None)
    }

    /// The issue's reactions as GitLab reports them, each under its own award name. `None` when the read fails:
    /// the reaction is already written, so its answer is the issue without counts rather than an error.
    async fn issue_reactions(&self, project: &str, key: &str) -> Option<Vec<IssueReactionCount>> {
        let path = format!("/api/v4/projects/{project}/issues/{key}/award_emoji");
        let got = self
            .fetch(&path, &[("per_page", "100".to_owned())], "issue")
            .await
            .ok()?;
        let rows = parse(&got.body).ok()?;
        // A full first page may have more behind it: the answer is no counts, never a partial list presented as
        // the totals.
        if rows.as_array()?.len() >= MAX_AWARDS_PER_PAGE {
            return None;
        }
        reaction_counts(&rows)
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
            "GitLab did not accept the saved token. Connect again with a new one.".into(),
        ),
        403 => CoreError::TrackerForbidden("GitLab refused access to this project.".into()),
        404 => CoreError::NotFound(what.to_owned()),
        429 => CoreError::TrackerRateLimited(format!(
            "GitLab asked us to slow down. Try again {}.",
            wait_words(retry_after(headers))
        )),
        _ => unavailable(),
    }
}

/// Seconds to wait, from `Retry-After` (seconds) or `RateLimit-Reset` (a Unix time). Anything else is unknown.
fn retry_after(headers: &HeaderMap) -> Option<u64> {
    let number = |name: &str| headers.get(name)?.to_str().ok()?.trim().parse::<u64>().ok();
    number("retry-after").or_else(|| {
        let reset = number("ratelimit-reset")?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
        Some(reset.saturating_sub(now))
    })
}

fn wait_words(seconds: Option<u64>) -> String {
    match seconds.map(|s| s.min(24 * 60 * 60)) {
        None => "in a minute".to_owned(),
        Some(0 | 1) => "in a second".to_owned(),
        Some(s) if s < 120 => format!("in {s} seconds"),
        Some(s) => format!("in {} minutes", s.div_ceil(60)),
    }
}

/// A project path as a person or a remote writes it: two to twenty plain segments, so it names one project
/// and nothing a request could be steered with.
fn path_ok(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').collect();
    path.len() <= MAX_SCOPE_BYTES
        && (2..=MAX_SEGMENTS).contains(&segments.len())
        && segments.iter().all(|s| {
            !s.is_empty()
                && !s.starts_with(['-', '.'])
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
        })
}

/// The scope as the single path segment GitLab wants: the `/`s inside it travel as `%2F`.
fn project_segment(scope: &str) -> Result<String> {
    if path_ok(scope) {
        Ok(scope.replace('/', "%2F"))
    } else {
        Err(CoreError::Invalid(
            "A GitLab scope is a project path such as group/project.".into(),
        ))
    }
}

fn key_ok(key: &str) -> Result<()> {
    if key.is_empty() || key.len() > 9 || !key.chars().all(|c| c.is_ascii_digit()) {
        return Err(CoreError::Invalid("A GitLab issue key is a number.".into()));
    }
    Ok(())
}

/// The path of a GitLab project from a git remote, when the remote is on the connection's host (docs/issues.md,
/// "Scope"). `host` is the stored connection host. The user name, port and a `.git` ending are dropped, and a
/// subgroup path keeps all its segments. A remote of any other host, or a path that names no project, is `None`.
pub(crate) fn gitlab_path_of_remote(url: &str, host: &str) -> Option<String> {
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
    path_ok(path).then(|| path.to_owned())
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
    // The order is part of the filter: a cursor issued for one order is not accepted for another.
    let sort = format!("{:?}", query.sort);
    let parts: [&str; 9] = [
        "gitlab-cursor",
        scope,
        state,
        &labels,
        &assignee,
        &query.text,
        &sort,
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
    let Some(cursor) = query.cursors.get(&format!("gitlab {scope}")) else {
        return Ok(1);
    };
    let refused = || {
        CoreError::Invalid("That page cursor was not issued for this project and filter.".into())
    };
    let (page, tag) = cursor.split_once('.').ok_or_else(refused)?;
    let page: u32 = page.parse().map_err(|_| refused())?;
    if !(2..=MAX_PAGE).contains(&page) || tag != cursor_tag(scope, query, page) {
        return Err(refused());
    }
    Ok(page)
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
        .get("username")
        .and_then(Value::as_str)
        .filter(|l| !l.is_empty())
        .map(|login| IssueActor {
            login: clip(login, MAX_LOGIN),
        })
}

fn summary(host: &str, scope: &str, value: &Value) -> Option<IssueSummary> {
    let iid = value.get("iid").and_then(Value::as_u64)?;
    let closed = text(value, "state").eq_ignore_ascii_case("closed");
    let own_url = format!("{host}/{scope}/-/issues/{iid}");
    let web_url = text(value, "web_url");
    // An address the tracker gives is used only when it stays on the connection's host.
    let url = if web_url.len() <= MAX_URL && web_url.starts_with(&format!("{host}/")) {
        web_url
    } else {
        own_url
    };
    Some(IssueSummary {
        tracker: TrackerKind::Gitlab,
        scope: scope.to_owned(),
        key: iid.to_string(),
        display_key: format!("#{iid}"),
        title: clip(&text(value, "title"), MAX_TITLE),
        url,
        author: value.get("author").and_then(actor),
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
                    .filter_map(Value::as_str)
                    .take(MAX_LABELS)
                    .map(|name| IssueLabel {
                        name: clip(name, MAX_LABEL),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        comment_count: value
            .get("user_notes_count")
            .and_then(Value::as_u64)
            .map_or(0, |n| u32::try_from(n).unwrap_or(u32::MAX)),
    })
}

/// The people's comments from these notes, oldest first. GitLab's system notes are events, not comments.
fn user_comments(notes: &[Value], at_most: usize) -> Vec<IssueComment> {
    notes
        .iter()
        .filter(|n| n.get("system").and_then(Value::as_bool) != Some(true))
        .take(at_most)
        .map(|n| IssueComment {
            id: n
                .get("id")
                .and_then(Value::as_u64)
                .map(|id| id.to_string())
                .unwrap_or_default(),
            author: n.get("author").and_then(actor),
            body: cut(&text(n, "body"), super::frame::MAX_BODY_BYTES).to_owned(),
            created_at: text(n, "created_at"),
        })
        .collect()
}

fn parse(body: &[u8]) -> Result<Value> {
    serde_json::from_slice(body).map_err(|_| unreadable())
}

/// GitLab's own name for one reaction, sent as `name=` on an award and returned as the reaction's word.
fn award_name(reaction: IssueReaction) -> &'static str {
    match reaction {
        IssueReaction::ThumbsUp => "thumbsup",
        IssueReaction::ThumbsDown => "thumbsdown",
        IssueReaction::Laugh => "laughing",
        IssueReaction::Hooray => "tada",
        IssueReaction::Confused => "confused",
        IssueReaction::Heart => "heart",
        IssueReaction::Rocket => "rocket",
        IssueReaction::Eyes => "eyes",
    }
}

/// The awards in one answer, grouped by GitLab's own award name as counts. An award is listed only while somebody
/// holds it, so the counts are the person's own.
fn reaction_counts(rows: &Value) -> Option<Vec<IssueReactionCount>> {
    let rows = rows.as_array()?;
    let mut counts: Vec<IssueReactionCount> = Vec::new();
    for row in rows {
        let Some(name) = row
            .get("name")
            .and_then(Value::as_str)
            .filter(|n| !n.is_empty())
        else {
            continue;
        };
        if let Some(count) = counts.iter_mut().find(|count| count.content == name) {
            count.count = count.count.saturating_add(1);
        } else {
            counts.push(IssueReactionCount {
                content: name.to_owned(),
                count: 1,
            });
        }
    }
    Some(counts)
}

/// A GitLab note id: a 64-bit numeric id, up to 19 digits (note ids are global and already ten digits today),
/// because an award is removed by the note's own award id path.
fn note_id_ok(comment: &str) -> Result<()> {
    if comment.is_empty()
        || comment.len() > 19
        || !comment.chars().all(|c| c.is_ascii_digit())
        || comment.parse::<u64>().is_err()
    {
        return Err(CoreError::Invalid(
            "That comment id is not a GitLab note.".into(),
        ));
    }
    Ok(())
}

/// The assignee ids a create sends: a candidate id is the numeric user id on GitLab, never a login typed by hand.
fn assignee_ids(assignees: &[String]) -> Result<Vec<u64>> {
    assignees
        .iter()
        .map(|id| {
            id.parse::<u64>().map_err(|_| {
                CoreError::Invalid("A GitLab assignee is a numeric candidate id.".into())
            })
        })
        .collect()
}

/// A GitLab template key as one path segment: the tracker's own keys are plain, and anything else is skipped
/// rather than steered into the path.
fn template_segment(key: &str) -> Option<String> {
    (!key.is_empty()
        && key.len() <= 100
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)))
    .then(|| key.to_owned())
}

#[async_trait]
impl IssueSource for GitLab {
    fn can_write(&self) -> bool {
        true
    }

    async fn create(&self, scope: &str, new: &NewIssue<'_>) -> Result<IssueDetail> {
        let project = project_segment(scope)?;
        // GitLab takes labels as one comma-separated value, so a name with a comma in it would silently become two.
        if new.labels.iter().any(|label| label.contains(',')) {
            return Err(CoreError::Invalid(
                "A GitLab label cannot contain a comma.".into(),
            ));
        }
        let payload = json!({
            "title": new.title,
            "description": new.body,
            "labels": new.labels.join(","),
            "assignee_ids": assignee_ids(new.assignees)?,
        });
        let path = format!("/api/v4/projects/{project}/issues");
        let got = self
            .send(Method::POST, &path, &[], Some(&payload), "project")
            .await?;
        let value = parse(&got.body)?;
        let summary = summary(&self.host, scope, &value).ok_or_else(unreadable)?;
        // A new issue has no comments: its own reply is the whole answer, so a successful create is never reported
        // as a failure by a second read.
        Ok(IssueDetail {
            summary,
            body: cut(&text(&value, "description"), MAX_BODY_BYTES).to_owned(),
            comments: Vec::new(),
            linked_pull_requests: Vec::new(),
            linked_threads: Vec::new(),
            reactions: None,
            states: None,
        })
    }

    async fn comment(&self, scope: &str, key: &str, body: &str) -> Result<IssueComment> {
        let project = project_segment(scope)?;
        key_ok(key)?;
        let path = format!("/api/v4/projects/{project}/issues/{key}/notes");
        let got = self
            .send(
                Method::POST,
                &path,
                &[],
                Some(&json!({ "body": body })),
                "issue",
            )
            .await?;
        let value = parse(&got.body)?;
        Ok(IssueComment {
            id: value
                .get("id")
                .and_then(Value::as_u64)
                .map(|id| id.to_string())
                .ok_or_else(unreadable)?,
            author: value.get("author").and_then(actor),
            body: cut(&text(&value, "body"), MAX_BODY_BYTES).to_owned(),
            created_at: text(&value, "created_at"),
        })
    }

    async fn react(
        &self,
        scope: &str,
        key: &str,
        comment: Option<&str>,
        reaction: IssueReaction,
        on: bool,
    ) -> Result<IssueDetail> {
        let project = project_segment(scope)?;
        key_ok(key)?;
        if let Some(comment) = comment {
            note_id_ok(comment)?;
        }
        let award = award_name(reaction);
        let subject = match comment {
            Some(comment) => {
                format!("/api/v4/projects/{project}/issues/{key}/notes/{comment}/award_emoji")
            }
            None => format!("/api/v4/projects/{project}/issues/{key}/award_emoji"),
        };
        // GitLab deletes an award by its id and takes no name there: the reader's own award is looked up first, so
        // both directions answer the same way when the reaction is already (or no longer) there.
        let login = self.signed_in_login().await?;
        let own = self.own_award(&subject, award, &login).await?;
        match (on, own) {
            (true, None) => {
                self.send(
                    Method::POST,
                    &subject,
                    &[("name", award.to_owned())],
                    None,
                    "issue",
                )
                .await?;
            }
            (false, Some(id)) => {
                self.send(
                    Method::DELETE,
                    &format!("{subject}/{id}"),
                    &[],
                    None,
                    "issue",
                )
                .await?;
            }
            // Already there, or nothing to take back: nothing is written and the answer is the issue.
            (_, _) => {}
        }
        let mut detail = self.get(scope, key).await?;
        detail.reactions = self.issue_reactions(&project, key).await;
        Ok(detail)
    }

    async fn templates(&self, scope: &str) -> Result<Vec<IssueTemplate>> {
        let project = project_segment(scope)?;
        // One page of 100 covers the 50 kept: GitLab pages this list at 20 by default, and a project with more
        // than that would silently answer only the first page.
        let listed = self
            .fetch(
                &format!("/api/v4/projects/{project}/templates/issues"),
                &[("per_page", "100".to_owned())],
                "project",
            )
            .await?;
        let value = parse(&listed.body)?;
        let entries = value.as_array().ok_or_else(unreadable)?;
        let mut templates = Vec::new();
        for entry in entries.iter().take(MAX_TEMPLATES) {
            let Some(key) = entry
                .get("key")
                .and_then(Value::as_str)
                .and_then(template_segment)
            else {
                continue;
            };
            let name = clip(
                entry
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|name| !name.is_empty())
                    .unwrap_or(&key),
                MAX_TITLE,
            );
            // GitLab's project templates carry one markdown body and no labels or description of their own.
            let content = self
                .fetch(
                    &format!("/api/v4/projects/{project}/templates/issues/{}", key),
                    &[],
                    "template",
                )
                .await?;
            let value = parse(&content.body)?;
            templates.push(IssueTemplate {
                id: key,
                name,
                description: None,
                body: cut(&text(&value, "content"), MAX_TEMPLATE_BODY_BYTES).to_owned(),
                labels: Vec::new(),
            });
        }
        Ok(templates)
    }

    async fn list(&self, scope: &str, query: &IssueQuery) -> Result<ScopeList> {
        let project = project_segment(scope)?;
        let page = page_of(scope, query)?;
        let per_page = query.limit.clamp(1, MAX_LIMIT);
        let mut params: Vec<(&str, String)> = vec![
            (
                "state",
                match query.state {
                    StateFilter::Open => "opened",
                    StateFilter::Closed => "closed",
                    StateFilter::All => "all",
                }
                .to_owned(),
            ),
            ("order_by", "updated_at".to_owned()),
            ("sort", "desc".to_owned()),
            ("per_page", per_page.to_string()),
            ("page", page.to_string()),
        ];
        if !query.labels.is_empty() {
            if query.labels.iter().any(|l| l.contains(',')) {
                return Err(CoreError::Invalid(
                    "A GitLab label filter cannot contain a comma.".into(),
                ));
            }
            params.push(("labels", query.labels.join(",")));
        }
        match &query.assignee {
            AssigneeFilter::Any => {}
            AssigneeFilter::Me => params.push(("scope", "assigned_to_me".to_owned())),
            AssigneeFilter::NoOne => params.push(("assignee_id", "None".to_owned())),
            AssigneeFilter::Login(login) => params.push(("assignee_username", login.clone())),
        }
        if !query.text.is_empty() {
            params.push(("search", query.text.clone()));
        }
        let path = format!("/api/v4/projects/{project}/issues");
        let (listed, counts) =
            tokio::join!(self.fetch(&path, &params, "project"), self.counts(&project));
        let listed = listed?;
        let value = parse(&listed.body)?;
        let rows = value.as_array().ok_or_else(unreadable)?;
        let issues: Vec<IssueSummary> = rows
            .iter()
            .filter_map(|v| summary(&self.host, scope, v))
            .collect();
        let advertised = listed
            .headers
            .get("x-next-page")
            .and_then(|v| v.to_str().ok())
            .map(str::trim);
        let next_page = match advertised {
            Some(text) => text.parse::<u32>().ok(),
            None => (rows.len() == per_page).then_some(page + 1),
        };
        Ok(ScopeList {
            issues,
            counts,
            next: next_page
                .filter(|n| (2..=MAX_PAGE).contains(n))
                .map(|n| cursor_for(scope, query, n)),
        })
    }

    async fn get(&self, scope: &str, key: &str) -> Result<IssueDetail> {
        let project = project_segment(scope)?;
        key_ok(key)?;
        let issue_path = format!("/api/v4/projects/{project}/issues/{key}");
        let issue = self.fetch(&issue_path, &[], "issue").await?;
        let value = parse(&issue.body)?;
        let summary = summary(&self.host, scope, &value)
            .ok_or_else(|| CoreError::NotFound("issue".into()))?;
        let mut comments = Vec::new();
        let mut page = 1u32;
        while page <= MAX_NOTE_PAGES {
            let notes = self
                .fetch(
                    &format!("{issue_path}/notes"),
                    &[
                        ("per_page", "100".to_owned()),
                        ("order_by", "created_at".to_owned()),
                        ("sort", "asc".to_owned()),
                        ("page", page.to_string()),
                    ],
                    "issue",
                )
                .await?;
            let parsed = parse(&notes.body)?;
            let rows = parsed.as_array().ok_or_else(unreadable)?;
            comments.extend(user_comments(rows, MAX_COMMENTS - comments.len()));
            if comments.len() >= MAX_COMMENTS {
                break;
            }
            let advertised = notes
                .headers
                .get("x-next-page")
                .and_then(|v| v.to_str().ok())
                .map(str::trim);
            let next = match advertised {
                Some(text) => text.parse::<u32>().ok(),
                None => (rows.len() >= 100).then_some(page + 1),
            };
            match next {
                Some(next) if next > page => page = next,
                _ => break,
            }
        }
        Ok(IssueDetail {
            summary,
            body: cut(&text(&value, "description"), MAX_BODY_BYTES).to_owned(),
            comments,
            linked_pull_requests: Vec::new(),
            linked_threads: Vec::new(),
            reactions: None,
            states: None,
        })
    }
}

#[cfg(test)]
#[path = "gitlab_tests.rs"]
mod tests;
