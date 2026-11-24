//! Review state and check counts for a whole list of pull requests in one GraphQL request per
//! batch, and only a few follow-up requests for a review connection that names a page after its
//! first.

use super::Forge;
use super::process::{call, spawn};
use crate::util::json::{field, optional};
use orbyn_contracts::{PullDetails, RequestedReviewer};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Duration;

/// The whole details read, first page and follow-ups, must not hold a refresh up for long.
pub(super) const DETAILS_TIMEOUT: Duration = Duration::from_secs(20);

/// Pull requests asked in one GraphQL query, to stay inside its node and cost limits.
pub(super) const DETAILS_BATCH: usize = 25;

/// Rows one details request may ask about, the size of a project's list.
pub const DETAILS_ROW_LIMIT: usize = 100;

/// Reviews counted in one bounded response. More reviews leave the count unknown.
const DETAILS_REVIEW_LIMIT: usize = 100;

/// Entries one review connection page carries.
pub(super) const DETAILS_PAGE_SIZE: usize = 50;

/// Pages read per review connection after its first page. A connection that still has a page
/// after that is withheld, never reported short, so at most
/// `(1 + DETAILS_PAGE_ROUNDS) * DETAILS_PAGE_SIZE` reviewers or reviews are read per pull
/// request.
pub(super) const DETAILS_PAGE_ROUNDS: usize = 3;

/// The `PullDetails` fields one batched request supplies, by wire name.
pub const DETAIL_FIELDS: [&str; 6] = [
    "created_at",
    "requested_reviewers",
    "viewer_review_state",
    "comment_count",
    "approval_count",
    "mergeable",
];

/// When GitHub's GraphQL limit resets, as seconds since the epoch, read from `gh api rate_limit`.
pub async fn github_graphql_reset(cwd: &Path) -> Option<u64> {
    let output = call(Forge::GitHub, cwd, &["api", "rate_limit"])
        .await
        .ok()?;
    graphql_reset(&output)
}

/// The GraphQL reset time in a `rate_limit` answer.
pub fn graphql_reset(answer: &str) -> Option<u64> {
    let value: Value = serde_json::from_str(answer).ok()?;
    value.pointer("/resources/graphql/reset")?.as_u64()
}

/// How the batched details request ended.
#[derive(Debug, PartialEq)]
pub enum DetailsOutcome {
    Details(HashMap<u64, PullDetails>),
    /// GitHub's GraphQL limit is spent, so no detail may be reported.
    RateLimited,
    Unavailable,
}

/// A review connection that reported a page the batched request did not read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum PageKind {
    Requests,
    Reviews,
}

impl PageKind {
    fn field(self) -> &'static str {
        match self {
            Self::Requests => "reviewRequests",
            Self::Reviews => "latestReviews",
        }
    }

    fn of_field(field: &str) -> Option<Self> {
        match field {
            "reviewRequests" => Some(Self::Requests),
            "latestReviews" => Some(Self::Reviews),
            _ => None,
        }
    }

    fn query(self, after: &str) -> String {
        match self {
            Self::Requests => review_requests(Some(after)),
            Self::Reviews => latest_reviews(Some(after)),
        }
    }
}

/// One review connection page still to read, and the cursor that reads it.
#[derive(Debug)]
pub(super) struct PendingPage {
    pub(super) number: u64,
    pub(super) kind: PageKind,
    pub(super) cursor: String,
}

/// The `reviewRequests` connection of one pull request: users and teams asked to review.
fn review_requests(after: Option<&str>) -> String {
    format!(
        "reviewRequests(first:{DETAILS_PAGE_SIZE}{}){{pageInfo{{hasNextPage endCursor}} \
nodes{{requestedReviewer{{__typename ...on User{{login}} ...on Mannequin{{login}} \
...on Team{{slug}}}}}}}}",
        after_clause(after)
    )
}

/// The `latestReviews` connection of one pull request: one review per reviewer.
fn latest_reviews(after: Option<&str>) -> String {
    format!(
        "latestReviews(first:{DETAILS_PAGE_SIZE}{}){{pageInfo{{hasNextPage endCursor}} \
nodes{{state author{{login}}}}}}",
        after_clause(after)
    )
}

/// The argument that reads a page after this cursor. The cursor is opaque, so it is sent as a
/// GraphQL string literal with its own escaping.
fn after_clause(after: Option<&str>) -> String {
    after.map_or_else(String::new, |cursor| {
        format!(",after:{}", Value::String(cursor.to_owned()))
    })
}

/// One GraphQL request for every listed pull request, aliased `p<number>`: when it was
/// opened, whether it merges cleanly, who is asked to review, the viewer's latest review
/// (the viewer comes from the same request), and conversation comments plus the comments
/// inside each review. Both review connections name their next page, so a busy pull request
/// can be read on instead of withheld.
pub(super) fn details_query(numbers: &[u64]) -> String {
    let aliases: String = numbers
        .iter()
        .map(|number| {
            format!(
                "p{number}:pullRequest(number:{number}){{createdAt mergeable {} {} \
latestOpinionatedReviews(first:100,writersOnly:true){{totalCount pageInfo{{hasNextPage}} nodes{{state}}}} \
comments{{totalCount}} reviews(first:{DETAILS_REVIEW_LIMIT}){{totalCount pageInfo{{hasNextPage}} nodes{{comments{{totalCount}}}}}}}}",
                review_requests(None),
                latest_reviews(None),
            )
        })
        .collect();
    format!(
        "query($owner:String!,$repo:String!){{viewer{{login}} \
repository(owner:$owner,name:$repo){{{aliases}}}}}"
    )
}

/// One GraphQL request reading the next page of every pending connection, one alias per pull
/// request and only the connections that still name a page.
pub(super) fn page_query(pending: &[PendingPage]) -> String {
    let mut aliases = String::new();
    for group in pending.chunk_by(|left, right| left.number == right.number) {
        let number = group[0].number;
        let connections: String = group
            .iter()
            .map(|page| page.kind.query(&page.cursor))
            .collect();
        aliases.push_str(&format!(
            "p{number}:pullRequest(number:{number}){{{connections}}}"
        ));
    }
    format!(
        "query($owner:String!,$repo:String!){{repository(owner:$owner,name:$repo){{{aliases}}}}}"
    )
}

/// A count is known only when every review and every component count is present.
/// Do not fetch more pages or let malformed nodes become a partial sum.
fn comment_count(pull: &Value) -> Option<u32> {
    let reviews = pull.get("reviews")?;
    let nodes = reviews.get("nodes")?.as_array()?;
    if nodes.len() > DETAILS_REVIEW_LIMIT
        || reviews.pointer("/pageInfo/hasNextPage")?.as_bool()?
        || reviews.get("totalCount")?.as_u64()? != u64::try_from(nodes.len()).ok()?
    {
        return None;
    }
    let total =
        |value: &Value| u32::try_from(value.pointer("/comments/totalCount")?.as_u64()?).ok();
    nodes
        .iter()
        .try_fold(total(pull)?, |sum, review| sum.checked_add(total(review)?))
}

/// Whether a connection came back whole. One that may continue past its first page, or
/// that does not say, cannot be read as the whole list.
fn whole(connection: &Value) -> bool {
    connection
        .pointer("/pageInfo/hasNextPage")
        .and_then(Value::as_bool)
        == Some(false)
}

/// Reads known facts independently. An incomplete comment count stays absent while
/// valid reviewer facts remain available. A missing/null or wholly unknown row is
/// omitted; the viewer's review state is absent when no viewer is named, and the facts a
/// truncated reviewer connection would undercount are absent rather than partial.
pub(super) fn parse_details(
    response: &Value,
    project_id: &str,
    numbers: &[u64],
) -> HashMap<u64, PullDetails> {
    let viewer = response
        .pointer("/data/viewer/login")
        .and_then(Value::as_str);
    numbers
        .iter()
        .filter_map(|number| {
            let pull = response.pointer(&format!("/data/repository/p{number}"))?;
            pull.as_object()?;
            let created_at = optional(pull, "createdAt");
            // `MERGEABLE`, `CONFLICTING` or `UNKNOWN`, in the words the rest of the contract uses.
            let mergeable = optional(pull, "mergeable").map(|word| word.to_lowercase());
            let comment_count = comment_count(pull);
            let approval_count = approval_count(pull);
            let requested_reviewers = pull
                .get("reviewRequests")
                .filter(|connection| whole(connection))
                .and_then(|connection| connection.get("nodes"))
                .and_then(Value::as_array)
                .map(|nodes| {
                    // Pages are merged in cursor order; a name a page repeats keeps its first place.
                    let mut seen = HashSet::new();
                    nodes
                        .iter()
                        .filter_map(|node| {
                            let who = node.get("requestedReviewer")?;
                            let team = optional(who, "slug");
                            let (name, kind) = match team {
                                Some(slug) => (slug, "team"),
                                None => (optional(who, "login")?, "user"),
                            };
                            Some(RequestedReviewer {
                                name,
                                kind: kind.into(),
                            })
                        })
                        .filter(|reviewer| {
                            seen.insert((reviewer.kind.clone(), reviewer.name.clone()))
                        })
                        .collect()
                });
            let viewer_review_state = viewer.and_then(|viewer| {
                let reviews = pull
                    .get("latestReviews")
                    .filter(|connection| whole(connection))?
                    .get("nodes")?
                    .as_array()?;
                let mine = reviews.iter().find(|review| {
                    review
                        .pointer("/author/login")
                        .and_then(Value::as_str)
                        .is_some_and(|login| login.eq_ignore_ascii_case(viewer))
                });
                Some(mine.map_or("none".to_owned(), |review| {
                    field(review, "state").to_lowercase()
                }))
            });
            if created_at.is_none()
                && requested_reviewers.is_none()
                && viewer_review_state.is_none()
                && comment_count.is_none()
                && approval_count.is_none()
                && mergeable.is_none()
            {
                return None;
            }
            Some((
                *number,
                PullDetails {
                    project_id: project_id.to_owned(),
                    number: *number,
                    created_at,
                    requested_reviewers,
                    comment_count,
                    approval_count,
                    viewer_review_state,
                    mergeable,
                },
            ))
        })
        .collect()
}

/// Approvals among the latest opinionated review of each user with write access (the
/// count GitHub's merge box uses). Never report a partial count.
fn approval_count(pull: &Value) -> Option<u32> {
    let connection = pull.get("latestOpinionatedReviews")?;
    let nodes = connection.get("nodes")?.as_array()?;
    let total = connection.get("totalCount")?.as_u64()?;
    if total != u64::try_from(nodes.len()).ok()?
        || connection.pointer("/pageInfo/hasNextPage")?.as_bool()?
    {
        return None;
    }
    let mut approved = 0;
    for node in nodes {
        match node.get("state")?.as_str()? {
            "APPROVED" => approved += 1,
            "CHANGES_REQUESTED" | "COMMENTED" | "DISMISSED" | "PENDING" => {}
            _ => return None,
        }
    }
    Some(approved)
}

/// The outcome of one raw `gh` answer: the batched read's own shape, kept for the tests that
/// pin the pause and the error rules.
#[cfg(test)]
pub(super) fn classify_details(
    _success: bool,
    stdout: &str,
    stderr: &str,
    project_id: &str,
    numbers: &[u64],
) -> DetailsOutcome {
    if rate_limited(stdout, stderr) {
        return DetailsOutcome::RateLimited;
    }
    // GraphQL can answer some pull requests and report errors for others, and `gh`
    // then exits non-zero; every row that came back whole is kept.
    let Ok(response) = serde_json::from_str::<Value>(stdout) else {
        return DetailsOutcome::Unavailable;
    };
    classify_response(response, project_id, numbers)
}

/// Whether the answer says GitHub's GraphQL limit is spent.
fn rate_limited(stdout: &str, stderr: &str) -> bool {
    let said = format!("{stderr}\n{stdout}").to_lowercase();
    said.contains("rate limit") || said.contains("rate_limited")
}

/// Reads a parsed answer. An error inside a pull request can null a node of an otherwise
/// complete row, which would leave a short reviewer list or an undercounted total, so the
/// row goes whole.
fn classify_response(response: Value, project_id: &str, numbers: &[u64]) -> DetailsOutcome {
    let mut found = parse_details(&response, project_id, numbers);
    for error in response
        .get("errors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some([Value::String(root), Value::String(alias), ..]) = error
            .get("path")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            && root == "repository"
            && let Some(number) = alias.strip_prefix('p').and_then(|n| n.parse().ok())
        {
            found.remove(&number);
        }
    }
    if found.is_empty() {
        DetailsOutcome::Unavailable
    } else {
        DetailsOutcome::Details(found)
    }
}

/// The pages read so far of one review connection. `next` is the cursor of the page still to
/// read; `nodes` is `None` when a page could not be read whole, so the connection is withheld
/// instead of reported short.
#[derive(Default)]
struct Pages {
    nodes: Option<Vec<Value>>,
    next: Option<String>,
}

impl Pages {
    /// The first page of a connection, when its nodes and page flag are readable.
    fn start(connection: Option<&Value>) -> Self {
        let mut pages = Self::default();
        if let Some(connection) = connection
            && let Some((nodes, next)) = page(connection)
        {
            pages.nodes = Some(nodes);
            pages.next = next;
        }
        pages
    }

    /// Reads one later page. A page that cannot be read whole, or whose read failed, leaves
    /// the whole connection unknown.
    fn absorb(&mut self, connection: Option<&Value>) {
        match connection.and_then(page) {
            Some((nodes, next)) => {
                if let Some(read) = self.nodes.as_mut() {
                    read.extend(nodes);
                    self.next = next;
                }
            }
            None => {
                self.nodes = None;
                self.next = None;
            }
        }
    }

    /// The whole list, only once no page remains and no page failed.
    fn complete(&self) -> Option<&[Value]> {
        if self.next.is_some() {
            return None;
        }
        self.nodes.as_deref()
    }
}

/// The review pages of one pull request.
#[derive(Default)]
struct PullPages {
    requests: Pages,
    reviews: Pages,
}

/// One readable connection page: its nodes and the cursor of the page after it.
fn page(connection: &Value) -> Option<(Vec<Value>, Option<String>)> {
    let nodes = connection.get("nodes")?.as_array()?;
    // A nulled node is an unread reviewer or review, so the page cannot stand for a whole one.
    if nodes.iter().any(|node| !node.is_object()) {
        return None;
    }
    let next = match connection.pointer("/pageInfo/hasNextPage")?.as_bool()? {
        false => None,
        true => {
            let cursor = connection.pointer("/pageInfo/endCursor")?.as_str()?;
            if cursor.is_empty() {
                return None;
            }
            Some(cursor.to_owned())
        }
    };
    Some((nodes.clone(), next))
}

/// Follows every review connection of the batched answer from its first page.
fn page_state(response: &Value, numbers: &[u64]) -> HashMap<u64, PullPages> {
    numbers
        .iter()
        .filter_map(|number| {
            let pull = response.pointer(&format!("/data/repository/p{number}"))?;
            let state = PullPages {
                requests: Pages::start(pull.get("reviewRequests")),
                reviews: Pages::start(pull.get("latestReviews")),
            };
            (state.requests.next.is_some() || state.reviews.next.is_some())
                .then_some((*number, state))
        })
        .collect()
}

/// The next page of every connection that still names one, in a stable order.
fn pending_pages(state: &HashMap<u64, PullPages>, numbers: &[u64]) -> Vec<PendingPage> {
    let mut pending = Vec::new();
    for number in numbers {
        let Some(pull) = state.get(number) else {
            continue;
        };
        for (kind, pages) in [
            (PageKind::Requests, &pull.requests),
            (PageKind::Reviews, &pull.reviews),
        ] {
            if let Some(cursor) = &pages.next {
                pending.push(PendingPage {
                    number: *number,
                    kind,
                    cursor: cursor.clone(),
                });
            }
        }
    }
    pending
}

/// Reads the follow-up answer into the page state. An error answer names the connections it
/// nulled a node of; those are withheld, while connections it answered stay usable.
fn absorb_pages(response: &Value, pending: &[PendingPage], state: &mut HashMap<u64, PullPages>) {
    let broken = broken_paths(response);
    for asked in pending {
        let Some(pull) = state.get_mut(&asked.number) else {
            continue;
        };
        let unusable = broken.contains(&(asked.number, None))
            || broken.contains(&(asked.number, Some(asked.kind)));
        let connection = response.pointer(&format!(
            "/data/repository/p{}/{}",
            asked.number,
            asked.kind.field()
        ));
        let pages = match asked.kind {
            PageKind::Requests => &mut pull.requests,
            PageKind::Reviews => &mut pull.reviews,
        };
        if unusable {
            pages.absorb(None);
        } else {
            pages.absorb(connection);
        }
    }
}

/// The connections an answer's errors touched, by pull request number and connection. An
/// error with no connection deeper in its path may concern the whole pull request.
fn broken_paths(response: &Value) -> HashSet<(u64, Option<PageKind>)> {
    let mut broken = HashSet::new();
    for error in response
        .get("errors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some([Value::String(root), Value::String(alias), rest @ ..]) = error
            .get("path")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
        else {
            continue;
        };
        if root != "repository" {
            continue;
        }
        let Some(number) = alias
            .strip_prefix('p')
            .and_then(|digits| digits.parse().ok())
        else {
            continue;
        };
        let kind = rest
            .first()
            .and_then(Value::as_str)
            .and_then(PageKind::of_field);
        broken.insert((number, kind));
    }
    broken
}

/// Replaces each connection read to its end with the whole list, so the parse sees one
/// complete connection. One still at the page cap or failed keeps its first page flag and so
/// stays withheld.
fn merge_pages(mut response: Value, state: &HashMap<u64, PullPages>) -> Value {
    for (number, pull) in state {
        let Some(object) = response
            .pointer_mut(&format!("/data/repository/p{number}"))
            .and_then(Value::as_object_mut)
        else {
            continue;
        };
        for (kind, pages) in [
            (PageKind::Requests, &pull.requests),
            (PageKind::Reviews, &pull.reviews),
        ] {
            if let Some(nodes) = pages.complete() {
                object.insert(
                    kind.field().to_owned(),
                    json!({ "pageInfo": { "hasNextPage": false }, "nodes": nodes }),
                );
            }
        }
    }
    response
}

/// The answer of one GraphQL request.
enum PageAnswer {
    Data(Value),
    RateLimited,
    Failed,
}

/// One GraphQL request inside the batch's time budget. The words for a spent limit are the
/// same ones `classify_details` matches, so a follow-up cannot slip past a pause.
async fn ask(forge: Forge, cwd: &Path, query: &str, deadline: tokio::time::Instant) -> PageAnswer {
    let args = [
        "api",
        "graphql",
        "-F",
        "owner={owner}",
        "-F",
        "repo={repo}",
        "-f",
        query,
    ];
    let Ok(finished) = tokio::time::timeout_at(deadline, spawn(forge, cwd, &args, None)).await
    else {
        return PageAnswer::Failed;
    };
    let Ok(output) = finished else {
        return PageAnswer::Failed;
    };
    if rate_limited(&output.stdout, &output.stderr) {
        return PageAnswer::RateLimited;
    }
    match serde_json::from_str(&output.stdout) {
        Ok(response) => PageAnswer::Data(response),
        Err(_) => PageAnswer::Failed,
    }
}

/// Details for GitHub pull requests, in requests of at most `DETAILS_BATCH` aliases. A
/// batch that fails leaves only its own rows out; a spent limit ends the whole call. Other
/// forges have none.
pub async fn github_details(
    forge: Forge,
    cwd: &Path,
    project_id: &str,
    numbers: &[u64],
) -> DetailsOutcome {
    if forge != Forge::GitHub || numbers.is_empty() {
        return DetailsOutcome::Unavailable;
    }
    let mut all = HashMap::new();
    for batch in numbers.chunks(DETAILS_BATCH) {
        match details_batch(forge, cwd, project_id, batch).await {
            DetailsOutcome::Details(found) => all.extend(found),
            DetailsOutcome::RateLimited => return DetailsOutcome::RateLimited,
            DetailsOutcome::Unavailable => {}
        }
    }
    if all.is_empty() {
        DetailsOutcome::Unavailable
    } else {
        DetailsOutcome::Details(all)
    }
}

pub(super) async fn details_batch(
    forge: Forge,
    cwd: &Path,
    project_id: &str,
    numbers: &[u64],
) -> DetailsOutcome {
    // One budget for the first page and its follow-ups: a busy pull request must not hold a
    // refresh up for longer than any other details read.
    let deadline = tokio::time::Instant::now() + DETAILS_TIMEOUT;
    let first = format!("query={}", details_query(numbers));
    let response = match ask(forge, cwd, &first, deadline).await {
        PageAnswer::Data(response) => response,
        PageAnswer::RateLimited => return DetailsOutcome::RateLimited,
        PageAnswer::Failed => return DetailsOutcome::Unavailable,
    };
    let mut state = page_state(&response, numbers);
    for _ in 0..DETAILS_PAGE_ROUNDS {
        let pending = pending_pages(&state, numbers);
        if pending.is_empty() {
            break;
        }
        let query = format!("query={}", page_query(&pending));
        match ask(forge, cwd, &query, deadline).await {
            PageAnswer::Data(page) => absorb_pages(&page, &pending, &mut state),
            PageAnswer::RateLimited => return DetailsOutcome::RateLimited,
            // The first page's facts stand; a connection still naming a page is withheld.
            PageAnswer::Failed => break,
        }
    }
    classify_response(merge_pages(response, &state), project_id, numbers)
}
