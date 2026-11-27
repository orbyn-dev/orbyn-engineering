//! Azure DevOps work items through the REST API (docs/issues.md, "Azure DevOps as built"). The PAT comes from
//! the keychain, goes only in the `Authorization: Basic base64(":" + PAT)` header of a request to the
//! connection's own https host, and is never logged or put in an error. Listing needs one WIQL POST, which only
//! reads: its body is fixed text plus validated filter values, with single quotes doubled and every value
//! length-capped, so no value can change the query's structure. Everything the tracker returns is untrusted text:
//! it is cut to the limits below, HTML is reduced to plain text, it is never parsed for instructions, and an
//! error is always the service's own sentence, never the tracker's reply.
//!
//! UNVERIFIED against a live Azure DevOps. The WIQL grammar, `$top`, the batch form of `/workitems?ids=` with its
//! `fields` list, the work item fields (`System.State` values, `System.CreatedDate`, `System.ChangedDate`,
//! `_links.html.href`), the identity fields (`uniqueName`, `displayName`), `System.Tags` as a semicolon-separated
//! string, the `Microsoft.VSTS.Common.ClosedDate` field and the preview comments shape (`text` as plain text,
//! `createdBy`, `createdDate`, `id` or `commentId`, `$top=50` and `order=asc`, and whether that endpoint pages)
//! are from the REST API as understood; a service version, a process template or a proxy may differ.

use super::frame::cut;
use super::host::validate_host;
use super::query::{AssigneeFilter, IssueQuery, StateFilter};
use super::{IssueSource, MAX_SCOPE_BYTES, NewIssue, ScopeList};
use crate::error::{CoreError, Result};
use async_trait::async_trait;
use base64::Engine as _;
use orbyn_contracts::{
    IssueActor, IssueComment, IssueDetail, IssueLabel, IssueReaction, IssueState,
    IssueStateCategory, IssueSummary, TrackerKind,
};
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use reqwest::{Client, Method, StatusCode, Url, redirect};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;
use zeroize::Zeroizing;

const TIMEOUT: Duration = Duration::from_secs(10);
/// The most one answer may hold. A longer one is refused, not cut.
const MAX_RESPONSE: usize = 2 * 1024 * 1024;
const MAX_REDIRECTS: usize = 3;
/// The work items one listing page asks for, and the ids one `/workitems?ids=` read may name.
const PAGE_LIMIT: usize = 50;
/// The deepest offset a cursor may name. WIQL has no offset, so a page is sliced from the first
/// `offset + page` ids; this bounds the offset only, not the number of pages a listing may walk.
const MAX_OFFSET: u32 = 1000;
const MAX_TITLE: usize = 256;
const MAX_LABEL: usize = 200;
const MAX_LOGIN: usize = 100;
const MAX_STATE: usize = 100;
/// The timestamp fields are opaque text; the contract shows them, so they are cut like any other text.
const MAX_DATE: usize = 64;
const MAX_URL: usize = 2048;
const MAX_LABELS: usize = 100;
/// The issue body kept (the same as a body the person may write, docs/issues.md). Comments keep the draft's 16 KiB.
const MAX_BODY_BYTES: usize = 64 * 1024;
const MAX_COMMENTS: usize = 50;
/// The most of a filter value WIQL embeds.
const MAX_TEXT: usize = 200;
const MAX_ASSIGNEE: usize = 320;
/// The closed `System.State` names, and the open ones for the record: an unknown state is open.
const CLOSED_STATES: [&str; 5] = ["Closed", "Done", "Resolved", "Removed", "Completed"];
const HEAD: &str = "SELECT [System.Id] FROM WorkItems WHERE [System.TeamProject] = @project AND [System.WorkItemType] <> 'Test Case'";
const ORDER: &str = " ORDER BY [System.ChangedDate] DESC";
/// The only fields a listing asks for: exactly what `summary` maps, so a large description is never read.
const LIST_FIELDS: &str = "System.Id,System.Title,System.State,System.CreatedBy,System.AssignedTo,System.CreatedDate,System.ChangedDate,System.Tags,Microsoft.VSTS.Common.ClosedDate";

pub(crate) struct Azure {
    /// `https://dev.azure.com` or `https://{org}.visualstudio.com`, validated, no trailing slash and no
    /// `dev.azure.com/{org}` path (that organisation is kept in `host_org`).
    host: String,
    /// The organisation the host itself names: the single path segment of `dev.azure.com/{org}`, or the
    /// subdomain of `{org}.visualstudio.com`.
    host_org: Option<String>,
    /// True for `{org}.visualstudio.com`, whose request path is `{project}` instead of `{org}/{project}`.
    legacy: bool,
    token: Zeroizing<String>,
    client: Client,
    timeout: Duration,
}

impl std::fmt::Debug for Azure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Azure")
            .field("host", &self.host)
            .finish_non_exhaustive()
    }
}

fn unavailable() -> CoreError {
    CoreError::TrackerUnavailable("Azure DevOps did not answer.".into())
}

fn unreadable() -> CoreError {
    CoreError::TrackerUnavailable("Azure DevOps answered something unreadable.".into())
}

fn too_big() -> CoreError {
    CoreError::TrackerUnavailable("Azure DevOps answered more than we read.".into())
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

/// One connection's collection base: the host without a `dev.azure.com/{org}` path, which organisation it names,
/// and whether it is a `{org}.visualstudio.com` host.
fn host_shape(host: &str) -> Result<(String, Option<String>, bool)> {
    let rest = host.strip_prefix("https://").ok_or_else(host_refused)?;
    let (authority, path) = rest.split_once('/').map_or((rest, ""), |(a, p)| (a, p));
    // The cloud-host allowlist names bare hosts (`dev.azure.com`, `{org}.visualstudio.com`); a port would send the
    // PAT to `host:port`, which it does not name. (GitLab and Forgejo accept a port for self-managed hosts by
    // design; Linear's fixed host is the one other reader that refuses one.)
    if authority.contains(':') {
        return Err(host_refused());
    }
    let authority = authority.to_ascii_lowercase();
    let name = authority.as_str();
    let path = path.trim_matches('/');
    if name == "dev.azure.com" {
        if path.is_empty() {
            return Ok((format!("https://{authority}"), None, false));
        }
        if !path.contains('/') && plain_segment(path) {
            return Ok((format!("https://{authority}"), Some(path.to_owned()), false));
        }
        return Err(host_refused());
    }
    let Some(org) = name.strip_suffix(".visualstudio.com") else {
        return Err(host_refused());
    };
    if path.is_empty() && plain_segment(org) {
        return Ok((format!("https://{authority}"), Some(org.to_owned()), true));
    }
    Err(host_refused())
}

fn host_refused() -> CoreError {
    CoreError::Invalid(
        "An Azure DevOps host is https://dev.azure.com, https://dev.azure.com/<organization> or https://<organization>.visualstudio.com."
            .into(),
    )
}

/// One organisation or project name: plain enough that a request cannot be steered with it. Azure DevOps
/// project names may contain a space; each name is percent-encoded before it becomes a path segment.
fn plain_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment.len() <= 128
        && !segment.starts_with(['-', '.', ' '])
        && !segment.ends_with(' ')
        && segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '.' | '_' | '-'))
}

/// The `organization` and `project` of a scope. Azure DevOps work items live per project.
fn scope_parts(scope: &str) -> Result<(String, String)> {
    let refused = || CoreError::Invalid("An Azure DevOps scope is organization/project.".into());
    if scope.len() > MAX_SCOPE_BYTES {
        return Err(refused());
    }
    let (org, project) = scope.split_once('/').ok_or_else(refused)?;
    if project.contains('/') || !plain_segment(org) || !plain_segment(project) {
        return Err(refused());
    }
    Ok((org.to_owned(), project.to_owned()))
}

/// A path segment as RFC 3986 allows it, with every byte outside the unreserved set percent-encoded. Names are
/// validated ASCII, so this only ever adds `%20` today, but it is a complete encoder rather than a substitution.
fn encode_segment(segment: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0xf) as usize] as char);
        }
    }
    out
}

/// One percent-encoded path segment as text. A segment that is not valid UTF-8 after decoding is refused.
fn decode_segment(segment: &str) -> Option<String> {
    let bytes = segment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let hex = bytes.get(at + 1..at + 3)?;
            if !hex.iter().all(u8::is_ascii_hexdigit) {
                return None;
            }
            out.push(u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The authority (user and port dropped, lower case) and the raw path segments of a git remote.
fn remote_parts(url: &str) -> Option<(String, Vec<String>)> {
    let url = url.trim().trim_end_matches('/');
    let (authority, path) = if let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("ssh://"))
    {
        let (authority, path) = rest.split_once('/')?;
        (authority, path.to_owned())
    } else if url.contains("://") {
        return None;
    } else {
        let (authority, path) = url.split_once(':')?;
        (authority, path.to_owned())
    };
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let authority = authority
        .rsplit_once(':')
        .map_or(authority, |(host, _)| host);
    let segments = path
        .trim_matches('/')
        .split('/')
        .map(str::to_owned)
        .collect();
    Some((authority.to_ascii_lowercase(), segments))
}

/// The organisation and project of an Azure DevOps remote, when the remote is on the connection's host
/// (docs/issues.md, "Scope"). `host` is the stored connection host. Accepted forms are
/// `https://dev.azure.com/{org}/{project}/_git/{repo}` (user info dropped),
/// `git@ssh.dev.azure.com:v3/{org}/{project}/{repo}` (and its `ssh://` twin), and
/// `https://{org}.visualstudio.com/{project}/_git/{repo}` only when the connection host is that same
/// `{org}.visualstudio.com` host. The organisation is then the subdomain. A `.git` ending and a trailing slash
/// are dropped, percent-encoded names are decoded, and exactly the documented segments must remain: anything
/// else, including a remote of another host, is `None`.
pub(crate) fn azure_scope_of_remote(url: &str, host: &str) -> Option<String> {
    let host = host.trim();
    if !host.starts_with("https://") {
        return None;
    }
    let rest = host.strip_prefix("https://")?;
    let (host_authority, host_path) = rest.split_once('/').map_or((rest, ""), |(a, p)| (a, p));
    let host_authority = host_authority.to_ascii_lowercase();
    let host_name = host_authority
        .rsplit_once(':')
        .map_or(host_authority.as_str(), |(name, _)| name);
    let host_path = host_path.trim_matches('/');
    enum Kind {
        Cloud { org: Option<String> },
        Visual { org: String, authority: String },
    }
    let kind = if host_name == "dev.azure.com" {
        if host_path.is_empty() {
            Kind::Cloud { org: None }
        } else if !host_path.contains('/') && plain_segment(host_path) {
            Kind::Cloud {
                org: Some(host_path.to_owned()),
            }
        } else {
            return None;
        }
    } else {
        let org = host_name.strip_suffix(".visualstudio.com")?;
        if !host_path.is_empty() || !plain_segment(org) {
            return None;
        }
        Kind::Visual {
            org: org.to_owned(),
            authority: host_name.to_owned(),
        }
    };
    let (authority, raw) = remote_parts(url)?;
    let mut segments: Vec<String> = raw
        .iter()
        .map(|segment| decode_segment(segment))
        .collect::<Option<_>>()?;
    if let Some(last) = segments.last_mut()
        && let Some(stripped) = last.strip_suffix(".git")
    {
        *last = stripped.to_owned();
    }
    if segments.last().is_none_or(String::is_empty) {
        return None;
    }
    match kind {
        Kind::Cloud { org } => {
            let (org_segment, project) = if authority == "dev.azure.com" {
                if segments.len() != 4 || segments[2] != "_git" {
                    return None;
                }
                (&segments[0], &segments[1])
            } else if authority == "ssh.dev.azure.com" {
                if segments.len() != 4 || segments[0] != "v3" {
                    return None;
                }
                (&segments[1], &segments[2])
            } else {
                return None;
            };
            if !plain_segment(org_segment) || !plain_segment(project) {
                return None;
            }
            if let Some(host_org) = org
                && !host_org.eq_ignore_ascii_case(org_segment)
            {
                return None;
            }
            Some(format!("{org_segment}/{project}"))
        }
        Kind::Visual {
            org,
            authority: wanted,
        } => {
            if authority != wanted || segments.len() != 3 || segments[1] != "_git" {
                return None;
            }
            let project = &segments[0];
            if !plain_segment(project) {
                return None;
            }
            Some(format!("{org}/{project}"))
        }
    }
}

/// The work item ids one `/workitems?ids=` read may name, in order.
fn id_batches(ids: &[u64]) -> Vec<&[u64]> {
    ids.chunks(PAGE_LIMIT).collect()
}

/// What binds a cursor to its scope and filters: a cursor made for other ones does not verify.
fn cursor_tag(scope: &str, query: &IssueQuery, offset: u32) -> String {
    let state = match query.state {
        StateFilter::Open => "open",
        StateFilter::Closed => "closed",
        StateFilter::All => "all",
    };
    let labels = query.labels.join("\u{1f}");
    let assignee = format!("{:?}", query.assignee);
    let limit = query.limit.to_string();
    let offset = offset.to_string();
    // The order is part of the filter: a cursor issued for one order is not accepted for another.
    let sort = format!("{:?}", query.sort);
    let parts: [&str; 9] = [
        "azure-cursor",
        scope,
        state,
        &labels,
        &assignee,
        &query.text,
        &sort,
        &limit,
        &offset,
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

fn cursor_for(scope: &str, query: &IssueQuery, offset: u32) -> String {
    format!("{offset}.{}", cursor_tag(scope, query, offset))
}

/// The offset this listing asks from: zero for the first page, or the one its cursor names. A cursor that was
/// not issued for this scope, these filters and this page size is refused, and so is one past the cap.
fn offset_of(scope: &str, query: &IssueQuery) -> Result<u32> {
    let Some(cursor) = query.cursors.get(&format!("azure_devops {scope}")) else {
        return Ok(0);
    };
    let refused = || {
        CoreError::Invalid("That page cursor was not issued for this project and filter.".into())
    };
    let (offset, tag) = cursor.split_once('.').ok_or_else(refused)?;
    let offset: u32 = offset.parse().map_err(|_| refused())?;
    let page = query.limit.clamp(1, PAGE_LIMIT) as u32;
    if offset == 0
        || offset > MAX_OFFSET
        || !offset.is_multiple_of(page)
        || tag != cursor_tag(scope, query, offset)
    {
        return Err(refused());
    }
    Ok(offset)
}

/// A filter value as WIQL text: length-capped, with each single quote doubled, so it cannot leave the string
/// literal. The route already caps text at 200 bytes, a label at 200 and a login at 320 (`query.rs`); the
/// character cap here is a second guard for a source called directly. A control character is refused because
/// no filter this side builds carries one.
fn filter_value(value: &str, max: usize, what: &str) -> Result<String> {
    if value.chars().any(char::is_control) {
        return Err(CoreError::Invalid(format!(
            "The {what} has control characters."
        )));
    }
    Ok(value
        .chars()
        .take(max)
        .collect::<String>()
        .replace('\'', "''"))
}

/// The WIQL for one listing: fixed text plus the filters, never a fragment from the tracker.
fn wiql(query: &IssueQuery) -> Result<String> {
    let state = match query.state {
        StateFilter::Open => {
            let mut clauses = Vec::with_capacity(CLOSED_STATES.len());
            for closed in CLOSED_STATES {
                clauses.push(format!("[System.State] <> '{closed}'"));
            }
            format!(" AND ({})", clauses.join(" AND "))
        }
        StateFilter::Closed => {
            let mut clauses = Vec::with_capacity(CLOSED_STATES.len());
            for closed in CLOSED_STATES {
                clauses.push(format!("[System.State] = '{closed}'"));
            }
            format!(" AND ({})", clauses.join(" OR "))
        }
        StateFilter::All => String::new(),
    };
    let mut rest = String::new();
    if !query.text.is_empty() {
        let text = filter_value(&query.text, MAX_TEXT, "search text")?;
        // `System.Title` is a String field, so word search (`CONTAINS WORDS`) does not apply.
        rest.push_str(&format!(" AND [System.Title] CONTAINS '{text}'"));
    }
    for label in &query.labels {
        if label.trim().is_empty() {
            return Err(CoreError::Invalid(
                "An Azure DevOps label filter is empty.".into(),
            ));
        }
        let label = filter_value(label, MAX_LABEL, "label")?;
        rest.push_str(&format!(" AND [System.Tags] CONTAINS '{label}'"));
    }
    match &query.assignee {
        AssigneeFilter::Any => {}
        AssigneeFilter::Me => rest.push_str(" AND [System.AssignedTo] = @Me"),
        AssigneeFilter::Login(login) => {
            let login = filter_value(login, MAX_ASSIGNEE, "assignee")?;
            rest.push_str(&format!(" AND [System.AssignedTo] = '{login}'"));
        }
        AssigneeFilter::NoOne => {
            return Err(CoreError::Invalid(
                "Azure DevOps cannot filter for unassigned work items.".into(),
            ));
        }
    }
    Ok(format!("{HEAD}{state}{rest}{ORDER}"))
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

/// The login of a plain-string identity: `Name <email>` keeps only the address, and anything else is kept as
/// it is.
fn address_of(text: &str) -> &str {
    let text = text.trim();
    if let Some(open) = text.rfind('<')
        && let Some(close) = text[open..].find('>').map(|end| open + end)
        && open + 1 < close
    {
        let inside = text[open + 1..close].trim();
        if !inside.is_empty() {
            return inside;
        }
    }
    text
}

/// The identity in an Azure DevOps field: `uniqueName` (the login), `displayName` as its fallback, or a plain
/// string on an older or proxied answer. An empty `uniqueName` falls back too.
fn actor(value: &Value) -> Option<IssueActor> {
    let raw = if let Some(text) = value.as_str() {
        address_of(text)
    } else {
        value
            .get("uniqueName")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
            .or_else(|| value.get("displayName").and_then(Value::as_str))?
    };
    (!raw.is_empty()).then(|| IssueActor {
        login: clip(raw, MAX_LOGIN),
    })
}

/// The state as the beta shows it: trimmed, the category from the documented names, and the tracker's own state
/// name beside it. An unknown name is open and keeps its name.
fn state_of(name: &str) -> IssueState {
    let name = name.trim();
    let name = if name.is_empty() { "Open" } else { name };
    let closed = CLOSED_STATES
        .iter()
        .any(|closed| closed.eq_ignore_ascii_case(name));
    IssueState {
        category: if closed {
            IssueStateCategory::Closed
        } else {
            IssueStateCategory::Open
        },
        name: clip(name, MAX_STATE),
        close_reason: None,
    }
}

fn labels(fields: &Value) -> Vec<IssueLabel> {
    fields
        .get("System.Tags")
        .and_then(Value::as_str)
        .map(|tags| {
            tags.split(';')
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .take(MAX_LABELS)
                .map(|tag| IssueLabel {
                    name: clip(tag, MAX_LABEL),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn assignees(fields: &Value) -> Vec<IssueActor> {
    // Azure DevOps has one `System.AssignedTo`, not a list.
    fields
        .get("System.AssignedTo")
        .and_then(actor)
        .into_iter()
        .collect()
}

/// A work item object with exactly the fields the reader uses: a numeric `id` and a `fields` object. Anything
/// else is refused, so a wrong shape is not half read.
fn checked_item(value: &Value) -> Result<&Value> {
    if value.get("id").and_then(Value::as_u64).is_none()
        || !value.get("fields").is_some_and(Value::is_object)
    {
        return Err(unreadable());
    }
    Ok(value)
}

fn summary(host: &str, base: &str, scope: &str, value: &Value) -> Result<IssueSummary> {
    let item = checked_item(value)?;
    let id = item
        .get("id")
        .and_then(Value::as_u64)
        .ok_or_else(unreadable)?;
    let fields = item.get("fields").ok_or_else(unreadable)?;
    let state = fields
        .get("System.State")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let own_url = format!("{host}{base}/_workitems/edit/{id}");
    let web_url = item
        .get("_links")
        .and_then(|links| links.get("html"))
        .and_then(|html| html.get("href"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    // An address the tracker gives is used only when it stays on the connection's host.
    let url = if web_url.len() <= MAX_URL && web_url.starts_with(&format!("{host}/")) {
        web_url.to_owned()
    } else {
        own_url
    };
    Ok(IssueSummary {
        tracker: TrackerKind::AzureDevops,
        scope: scope.to_owned(),
        key: id.to_string(),
        display_key: format!("#{id}"),
        title: clip(&text(fields, "System.Title"), MAX_TITLE),
        url,
        author: fields.get("System.CreatedBy").and_then(actor),
        state: state_of(state),
        created_at: clip(&text(fields, "System.CreatedDate"), MAX_DATE),
        updated_at: clip(&text(fields, "System.ChangedDate"), MAX_DATE),
        closed_at: Some(clip(
            &text(fields, "Microsoft.VSTS.Common.ClosedDate"),
            MAX_DATE,
        ))
        .filter(|closed| !closed.is_empty()),
        assignees: assignees(fields),
        labels: labels(fields),
        comment_count: 0,
    })
}

/// The people's comments from this answer, first 50. `text` is plain text or markdown, not HTML, so it is
/// kept as it is and only cut; `id` falls back to `commentId` in the preview shape.
fn comments_from(value: &Value) -> Result<Vec<IssueComment>> {
    let rows = value
        .get("comments")
        .and_then(Value::as_array)
        .ok_or_else(unreadable)?;
    Ok(rows
        .iter()
        .take(MAX_COMMENTS)
        .map(|comment| IssueComment {
            id: comment
                .get("id")
                .and_then(Value::as_u64)
                .or_else(|| comment.get("commentId").and_then(Value::as_u64))
                .map(|id| id.to_string())
                .unwrap_or_default(),
            author: comment.get("createdBy").and_then(actor),
            body: cut(
                comment
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                super::frame::MAX_BODY_BYTES,
            )
            .to_owned(),
            created_at: clip(&text(comment, "createdDate"), MAX_DATE),
        })
        .collect())
}

/// The tag name of one `<...>` body, lower case, without attributes or a closing slash.
fn tag_name(tag: &str) -> String {
    tag.trim_start_matches('/')
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase()
}

fn decode_entities(text: &str, out: &mut String) {
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp..];
        // Look ahead at most ten bytes for the `;`; never rescan the rest of the answer.
        let bytes = after.as_bytes();
        let limit = (bytes.len() - 1).min(10);
        let decoded = (1..=limit).find(|at| bytes[*at] == b';').and_then(|end| {
            let ch = match &after[1..end] {
                "amp" => '&',
                "lt" => '<',
                "gt" => '>',
                "quot" => '"',
                "apos" | "#39" | "#x27" => '\'',
                "nbsp" => ' ',
                _ => return None,
            };
            Some((end, ch))
        });
        match decoded {
            Some((end, ch)) => {
                out.push(ch);
                rest = &after[end + 1..];
            }
            None => {
                out.push('&');
                rest = &after[1..];
            }
        }
    }
    out.push_str(rest);
}

/// A small, safe HTML-to-text reduction for descriptions: tags are dropped, `<br>` and the ends of blocks
/// become line breaks, `script`/`style` bodies and comments are dropped, and the few entities above are decoded.
/// It never interprets the result again, so an entity that spells `<script` stays text. A `<` that is not
/// followed by an ASCII letter, `/` or `!` is text (so `1 < 2 and 3 > 0` survives); a `<` without a `>` is
/// text too. The scan is linear: the input is lowercased once, entity lookahead is capped, and each search
/// advances past the part it consumed.
fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len().min(4096));
    // ASCII lowercasing keeps byte offsets, so this copy can be searched while `html` is still read.
    let lower = html.to_ascii_lowercase();
    let mut rest = html;
    let mut from = 0;
    while let Some(offset) = rest.find('<') {
        decode_entities(&rest[..offset], &mut out);
        let at = from + offset;
        let after = &rest[offset..];
        let starts_tag = after
            .as_bytes()
            .get(1)
            .is_some_and(|next| next.is_ascii_alphabetic() || matches!(next, b'/' | b'!'));
        if !starts_tag {
            out.push('<');
            rest = &after[1..];
            from = at + 1;
            continue;
        }
        if let Some(comment) = after.strip_prefix("<!--") {
            match comment.find("-->") {
                Some(end) => {
                    rest = &comment[end + 3..];
                    from = at + 4 + end + 3;
                }
                None => return finish_text(out),
            }
            continue;
        }
        let Some(gt) = after.find('>') else {
            decode_entities(after, &mut out);
            return finish_text(out);
        };
        let tag = &after[1..gt];
        let name = tag_name(tag);
        if name == "script" || name == "style" {
            let close = if name == "script" {
                "</script"
            } else {
                "</style"
            };
            if let Some(start) = lower[at..].find(close)
                && let Some(end) = lower[at + start..].find('>')
            {
                rest = &html[at + start + end + 1..];
                from = at + start + end + 1;
                continue;
            }
            return finish_text(out);
        }
        let line_break = name == "br"
            || matches!(
                name.as_str(),
                "p" | "div" | "li" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
            ) && tag.trim_start().starts_with('/');
        if line_break {
            out.push('\n');
        }
        rest = &after[gt + 1..];
        from = at + gt + 1;
    }
    decode_entities(rest, &mut out);
    finish_text(out)
}

fn finish_text(mut out: String) -> String {
    while out.ends_with('\n') {
        out.pop();
    }
    out.trim().to_owned()
}

struct Fetched {
    body: Vec<u8>,
}

/// The types a new work item is filed as, in the order a project that has them is preferred.
const CREATE_TYPES: [&str; 3] = ["Issue", "Task", "Bug"];

/// The reactions Azure DevOps has, in the contract's words: it has six of the eight, on comments only.
const REACTION_SET: &str = "thumbs_up, thumbs_down, laugh, hooray, confused, heart";

/// Azure DevOps' own reaction name for one contract reaction, when it has it. None is mapped to a near one.
fn reaction_type(reaction: IssueReaction) -> Option<&'static str> {
    match reaction {
        IssueReaction::ThumbsUp => Some("like"),
        IssueReaction::ThumbsDown => Some("dislike"),
        IssueReaction::Laugh => Some("smile"),
        IssueReaction::Hooray => Some("hooray"),
        IssueReaction::Confused => Some("confused"),
        IssueReaction::Heart => Some("heart"),
        IssueReaction::Rocket | IssueReaction::Eyes => None,
    }
}

/// An Azure DevOps comment id: a 64-bit numeric id, up to 19 digits, because that number is what its reaction
/// path takes.
fn comment_id_ok(comment: &str) -> Result<()> {
    if comment.is_empty()
        || comment.len() > 19
        || !comment.chars().all(|c| c.is_ascii_digit())
        || comment.parse::<u64>().is_err()
    {
        return Err(CoreError::Invalid(
            "That comment id is not an Azure DevOps comment.".into(),
        ));
    }
    Ok(())
}

/// The person's text as an Azure DevOps HTML description: every character is text, so it is escaped and its line
/// breaks become `<br>`; the tracker renders it, this side never does.
fn html_body(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("<br>"),
            '\r' => {}
            other => out.push(other),
        }
    }
    out
}

/// The comment a comment-create reply stored: its numeric id, the person who wrote it, the text Azure kept and when.
fn stored_comment(value: &Value) -> Result<IssueComment> {
    let id = value
        .get("id")
        .and_then(Value::as_u64)
        .map(|id| id.to_string())
        .ok_or_else(unreadable)?;
    let author = value.get("createdBy").and_then(|who| {
        who.get("uniqueName")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
            .or_else(|| who.get("displayName").and_then(Value::as_str))
            .filter(|name| !name.is_empty())
            .map(|name| IssueActor {
                login: clip(address_of(name), MAX_LOGIN),
            })
    });
    Ok(IssueComment {
        id,
        author,
        body: cut(&text(value, "text"), MAX_BODY_BYTES).to_owned(),
        created_at: text(value, "createdDate"),
    })
}

/// A failed status in the service's own words. The tracker's body is never passed on.
fn failure(status: StatusCode, headers: &HeaderMap, what: &str) -> CoreError {
    match status.as_u16() {
        401 => CoreError::TrackerNotConnected(
            "Azure DevOps did not accept the saved token. Connect again with a new one.".into(),
        ),
        403 => CoreError::TrackerForbidden("Azure DevOps refused access to this project.".into()),
        404 => CoreError::NotFound(what.to_owned()),
        429 => CoreError::TrackerRateLimited(format!(
            "Azure DevOps asked us to slow down. Try again {}.",
            wait_words(retry_after(headers))
        )),
        _ => unavailable(),
    }
}

/// Seconds to wait, from an integer `Retry-After`. An HTTP-date or anything else is unknown.
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

/// The host shape `connect_issue_tracker` checks before it saves an Azure DevOps connection.
pub(crate) fn check_connect_host(host: &str) -> Result<()> {
    host_shape(host).map(|_| ())
}

/// The scope shape `connect_issue_tracker` checks before it saves a scope on an Azure DevOps connection.
pub(crate) fn check_connect_scope(scope: &str) -> Result<()> {
    scope_parts(scope).map(|_| ())
}

fn key_ok(key: &str) -> Result<()> {
    if key.is_empty() || key.len() > 10 || !key.chars().all(|c| c.is_ascii_digit()) {
        return Err(CoreError::Invalid(
            "An Azure DevOps work item key is a number.".into(),
        ));
    }
    Ok(())
}

fn parse(body: &[u8]) -> Result<Value> {
    serde_json::from_slice(body).map_err(|_| unreadable())
}

impl Azure {
    /// A reader of one connection. A host that is not `https://` is refused here, before any request.
    pub(crate) fn new(host: &str, token: Zeroizing<String>) -> Result<Self> {
        let host = validate_host(host)?;
        let (host, host_org, legacy) = host_shape(&host)?;
        Ok(Self {
            host,
            host_org,
            legacy,
            token,
            client: shared_client()?,
            timeout: TIMEOUT,
        })
    }

    /// A reader of a local fake server over plain http, which only tests may build.
    #[cfg(test)]
    pub(crate) fn for_test(base: &str, token: &str, timeout: Duration) -> Self {
        Self::for_test_host(base, None, false, token, timeout)
    }

    /// A test reader with the host shape `Azure::new` would give a real host.
    #[cfg(test)]
    pub(crate) fn for_test_host(
        base: &str,
        host_org: Option<&str>,
        legacy: bool,
        token: &str,
        timeout: Duration,
    ) -> Self {
        Self {
            host: base.trim_end_matches('/').to_owned(),
            host_org: host_org.map(str::to_owned),
            legacy,
            token: Zeroizing::new(token.to_owned()),
            client: build_client(false).unwrap(),
            timeout,
        }
    }

    /// `/{org}/{project}` on every `dev.azure.com` host, whether or not the host itself names the organisation,
    /// or `/{project}` on a `{org}.visualstudio.com` host. The scope's organisation must be the one the host
    /// names, when it names one.
    fn project_base(&self, org: &str, project: &str) -> Result<String> {
        if let Some(host_org) = &self.host_org
            && !host_org.eq_ignore_ascii_case(org)
        {
            return Err(CoreError::Invalid(
                "That scope is not part of this connection.".into(),
            ));
        }
        let project = encode_segment(project);
        if self.legacy {
            Ok(format!("/{project}"))
        } else {
            Ok(format!("/{}/{}", encode_segment(org), project))
        }
    }

    /// One read of the API. `what` names the thing a 404 means is missing.
    async fn fetch(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<String>,
        what: &str,
    ) -> Result<Fetched> {
        self.send(method, path, query, body, "application/json", what)
            .await
    }

    /// One request of the API, read or write. The token, host, redirect, timeout and size rules are the read's; a
    /// JSON body travels in the body, never in the address, and the content type names its shape (a work item create
    /// is a JSON Patch document).
    async fn send(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<String>,
        content_type: &str,
        what: &str,
    ) -> Result<Fetched> {
        let encoded =
            base64::engine::general_purpose::STANDARD.encode(format!(":{}", self.token.as_str()));
        let mut token = HeaderValue::from_str(&format!("Basic {encoded}")).map_err(|_| {
            CoreError::TrackerNotConnected(
                "The saved Azure DevOps token cannot be used. Connect again with a new one.".into(),
            )
        })?;
        token.set_sensitive(true);
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.host))
            .query(query)
            .header(AUTHORIZATION, token)
            .header(ACCEPT, "application/json")
            .timeout(self.timeout);
        if let Some(body) = body {
            request = request.header(CONTENT_TYPE, content_type).body(body);
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
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
            if body.len() + chunk.len() > MAX_RESPONSE {
                return Err(too_big());
            }
            body.extend_from_slice(&chunk);
        }
        Ok(Fetched { body })
    }

    /// The ids of one WIQL answer, asked to hold every id through the page's offset plus its size.
    async fn ids(&self, base: &str, query: &str, top: u32) -> Result<Vec<u64>> {
        let body = serde_json::json!({ "query": query }).to_string();
        let got = self
            .fetch(
                Method::POST,
                &format!("{base}/_apis/wit/wiql"),
                &[("api-version", "7.1".to_owned()), ("$top", top.to_string())],
                Some(body),
                "project",
            )
            .await?;
        let value = parse(&got.body)?;
        let rows = value
            .get("workItems")
            .and_then(Value::as_array)
            .ok_or_else(unreadable)?;
        rows.iter()
            .map(|row| row.get("id").and_then(Value::as_u64).ok_or_else(unreadable))
            .collect()
    }

    /// The work item objects for these ids, in the order asked, at most 50 ids per request and only the fields
    /// a listing maps.
    async fn fetch_items(&self, base: &str, ids: &[u64]) -> Result<Vec<Value>> {
        let mut found: HashMap<u64, Value> = HashMap::new();
        for batch in id_batches(ids) {
            let joined = batch
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(",");
            let got = self
                .fetch(
                    Method::GET,
                    &format!("{base}/_apis/wit/workitems"),
                    &[
                        ("ids", joined),
                        ("fields", LIST_FIELDS.to_owned()),
                        ("api-version", "7.1".to_owned()),
                    ],
                    None,
                    "project",
                )
                .await?;
            let value = parse(&got.body)?;
            let rows = value
                .get("value")
                .and_then(Value::as_array)
                .ok_or_else(unreadable)?;
            for row in rows {
                let item = checked_item(row)?;
                let id = item
                    .get("id")
                    .and_then(Value::as_u64)
                    .ok_or_else(unreadable)?;
                found.insert(id, item.clone());
            }
        }
        Ok(ids.iter().filter_map(|id| found.get(id).cloned()).collect())
    }

    /// The type a new work item is filed as: the project's own first of Issue, Task and Bug, else its first enabled
    /// type. Azure names a work item by its type, so the create cannot leave it out.
    async fn work_item_type(&self, base: &str) -> Result<String> {
        let got = self
            .fetch(
                Method::GET,
                &format!("{base}/_apis/wit/workitemtypes"),
                &[("api-version", "7.1".to_owned())],
                None,
                "project",
            )
            .await?;
        let value = parse(&got.body)?;
        let rows = value
            .get("value")
            .and_then(Value::as_array)
            .ok_or_else(unreadable)?;
        let names: Vec<&str> = rows
            .iter()
            .filter(|row| row.get("isDisabled").and_then(Value::as_bool) != Some(true))
            .filter_map(|row| row.get("name").and_then(Value::as_str))
            .filter(|name| !name.is_empty())
            .collect();
        CREATE_TYPES
            .iter()
            .find(|wanted| names.iter().any(|name| name.eq_ignore_ascii_case(wanted)))
            .map(|name| (*name).to_owned())
            .or_else(|| names.first().map(|name| (*name).to_owned()))
            .ok_or_else(|| {
                CoreError::Invalid("Azure DevOps offers no work item type to file.".into())
            })
    }
}

#[async_trait]
impl IssueSource for Azure {
    fn can_write(&self) -> bool {
        true
    }

    async fn create(&self, scope: &str, new: &NewIssue<'_>) -> Result<IssueDetail> {
        let (org, project) = scope_parts(scope)?;
        let base = self.project_base(&org, &project)?;
        if new.assignees.len() > 1 {
            return Err(CoreError::Invalid(
                "Azure DevOps assigns one person to a work item.".into(),
            ));
        }
        let mut fields = vec![json!({
            "op": "add",
            "path": "/fields/System.Title",
            "value": new.title,
        })];
        if !new.body.is_empty() {
            fields.push(json!({
                "op": "add",
                "path": "/fields/System.Description",
                "value": html_body(new.body),
            }));
        }
        if !new.labels.is_empty() {
            if new.labels.iter().any(|label| label.contains(';')) {
                return Err(CoreError::Invalid(
                    "Azure DevOps cannot use a label with a semicolon in it.".into(),
                ));
            }
            fields.push(json!({
                "op": "add",
                "path": "/fields/System.Tags",
                "value": new.labels.join("; "),
            }));
        }
        if let Some(assignee) = new.assignees.first() {
            fields.push(json!({
                "op": "add",
                "path": "/fields/System.AssignedTo",
                "value": assignee,
            }));
        }
        let kind = self.work_item_type(&base).await?;
        let got = self
            .send(
                Method::POST,
                // Azure DevOps 7.1 addresses a create by the literal `$` and the type: `/workitems/$Task`.
                &format!("{base}/_apis/wit/workitems/${}", encode_segment(&kind)),
                &[("api-version", "7.1".to_owned())],
                Some(Value::Array(fields).to_string()),
                "application/json-patch+json",
                "project",
            )
            .await
            .map_err(|error| super::possibly_created("Azure DevOps", error))?;
        let value = parse(&got.body)?;
        let summary = summary(&self.host, &base, scope, &value)?;
        let fields = value.get("fields").ok_or_else(unreadable)?;
        // A new work item has no comments: its own reply is the whole answer, so a successful create is never
        // reported as a failure by a second read.
        Ok(IssueDetail {
            summary,
            body: cut(
                &html_to_text(
                    fields
                        .get("System.Description")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                ),
                MAX_BODY_BYTES,
            )
            .to_owned(),
            comments: Vec::new(),
            linked_pull_requests: Vec::new(),
            linked_threads: Vec::new(),
            activity: Vec::new(),
            reactions: None,
            states: None,
        })
    }

    async fn comment(&self, scope: &str, key: &str, body: &str) -> Result<IssueComment> {
        let (org, project) = scope_parts(scope)?;
        key_ok(key)?;
        let base = self.project_base(&org, &project)?;
        let got = self
            .send(
                Method::POST,
                &format!("{base}/_apis/wit/workItems/{key}/comments"),
                &[
                    ("format", "markdown".to_owned()),
                    ("api-version", "7.1-preview.4".to_owned()),
                ],
                Some(json!({ "text": body }).to_string()),
                "application/json",
                "work item",
            )
            .await?;
        stored_comment(&parse(&got.body)?)
    }

    async fn react(
        &self,
        scope: &str,
        key: &str,
        comment: Option<&str>,
        reaction: IssueReaction,
        on: bool,
    ) -> Result<IssueDetail> {
        let (org, project) = scope_parts(scope)?;
        key_ok(key)?;
        let base = self.project_base(&org, &project)?;
        // A work item itself takes no reaction: Azure DevOps reacts on its comments only.
        let Some(comment) = comment else {
            return Err(CoreError::Invalid(
                "Azure DevOps reactions go on comments only.".into(),
            ));
        };
        comment_id_ok(comment)?;
        let Some(kind) = reaction_type(reaction) else {
            return Err(CoreError::Invalid(format!(
                "Azure DevOps has no such reaction. It has: {REACTION_SET}."
            )));
        };
        // Azure DevOps 7.1-preview.1 nests the reaction under the work item's own comment, not a flat resource.
        let path = format!("{base}/_apis/wit/workItems/{key}/comments/{comment}/reactions/{kind}");
        let method = if on { Method::PUT } else { Method::DELETE };
        match self
            .send(
                method,
                &path,
                &[("api-version", "7.1-preview.1".to_owned())],
                None,
                "application/json",
                "work item",
            )
            .await
        {
            Ok(_) => {}
            // There is no reaction of that kind left to take back: a removal is idempotent at the answer.
            Err(CoreError::NotFound(_)) if !on => {}
            Err(error) => return Err(error),
        }
        let mut detail = self.get(scope, key).await?;
        // Azure DevOps reports reactions on comments, not on the work item, so the issue carries none.
        detail.reactions = None;
        Ok(detail)
    }

    async fn list(&self, scope: &str, query: &IssueQuery) -> Result<ScopeList> {
        let (org, project) = scope_parts(scope)?;
        let base = self.project_base(&org, &project)?;
        let offset = offset_of(scope, query)?;
        let per_page = query.limit.clamp(1, PAGE_LIMIT);
        let top = offset + per_page as u32;
        let query_text = wiql(query)?;
        let ids = self.ids(&base, &query_text, top).await?;
        let ids = &ids[..ids.len().min(top as usize)];
        let page: Vec<u64> = ids
            .iter()
            .skip(offset as usize)
            .take(per_page)
            .copied()
            .collect();
        let items = self.fetch_items(&base, &page).await?;
        let mut issues = Vec::with_capacity(items.len());
        for item in &items {
            issues.push(summary(&self.host, &base, scope, item)?);
        }
        let full = page.len() == per_page;
        let next = (full && offset + per_page as u32 <= MAX_OFFSET)
            .then(|| cursor_for(scope, query, offset + per_page as u32));
        Ok(ScopeList {
            issues,
            counts: None,
            next,
        })
    }

    async fn get(&self, scope: &str, key: &str) -> Result<IssueDetail> {
        let (org, project) = scope_parts(scope)?;
        key_ok(key)?;
        let base = self.project_base(&org, &project)?;
        let fetched = self
            .fetch(
                Method::GET,
                &format!("{base}/_apis/wit/workitems/{key}"),
                &[("api-version", "7.1".to_owned())],
                None,
                "work item",
            )
            .await?;
        let value = parse(&fetched.body)?;
        // Linked pull requests are Planned, so relations are not expanded. A 404 on the preview comments read
        // means the endpoint is not there, not that the work item is missing: the comments are empty.
        let comments = match self
            .fetch(
                Method::GET,
                &format!("{base}/_apis/wit/workItems/{key}/comments"),
                &[
                    ("$top", "50".to_owned()),
                    ("order", "asc".to_owned()),
                    ("api-version", "7.1-preview.4".to_owned()),
                ],
                None,
                "work item",
            )
            .await
        {
            Ok(fetched) => comments_from(&parse(&fetched.body)?)?,
            Err(CoreError::NotFound(_)) => Vec::new(),
            Err(error) => return Err(error),
        };
        let mut summary = summary(&self.host, &base, scope, &value)?;
        summary.comment_count = u32::try_from(comments.len()).unwrap_or(u32::MAX);
        let fields = value.get("fields").ok_or_else(unreadable)?;
        Ok(IssueDetail {
            summary,
            body: cut(
                &html_to_text(
                    fields
                        .get("System.Description")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                ),
                MAX_BODY_BYTES,
            )
            .to_owned(),
            comments,
            linked_pull_requests: Vec::new(),
            linked_threads: Vec::new(),
            activity: Vec::new(),
            reactions: None,
            states: None,
        })
    }
}

#[cfg(test)]
#[path = "azure_tests.rs"]
mod tests;
