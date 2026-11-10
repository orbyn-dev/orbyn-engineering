//! Review state and check counts for a whole list of pull requests in one GraphQL request per batch.

use super::Forge;
use super::process::{call, spawn};
use crate::util::json::{field, optional};
use orbyn_contracts::{PullDetails, RequestedReviewer};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

/// The details request must not hold a refresh up for long.
pub(super) const DETAILS_TIMEOUT: Duration = Duration::from_secs(20);

/// Pull requests asked in one GraphQL query, to stay inside its node and cost limits.
pub(super) const DETAILS_BATCH: usize = 25;

/// Rows one details request may ask about, the size of a project's list.
pub const DETAILS_ROW_LIMIT: usize = 100;

/// The `PullDetails` fields one batched request supplies, by wire name.
pub const DETAIL_FIELDS: [&str; 4] = [
    "created_at",
    "requested_reviewers",
    "viewer_review_state",
    "comment_count",
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

/// One GraphQL request for every listed pull request, aliased `p<number>`: when it was
/// opened, who is asked to review, the viewer's latest review (the viewer comes from
/// the same request), and conversation comments plus the comments inside each review.
pub(super) fn details_query(numbers: &[u64]) -> String {
    let aliases: String = numbers
        .iter()
        .map(|number| {
            format!(
                "p{number}:pullRequest(number:{number}){{createdAt \
reviewRequests(first:50){{nodes{{requestedReviewer{{__typename ...on User{{login}} \
...on Mannequin{{login}} ...on Team{{slug}}}}}}}} \
latestReviews(first:50){{nodes{{state author{{login}}}}}} \
comments{{totalCount}} reviews(first:100){{nodes{{comments{{totalCount}}}}}}}}"
            )
        })
        .collect();
    format!(
        "query($owner:String!,$repo:String!){{viewer{{login}} \
repository(owner:$owner,name:$repo){{{aliases}}}}}"
    )
}

/// Reads the response. A pull request whose comments cannot be totalled is left out
/// whole, so a row never carries a partial set. The viewer's review state is absent
/// when the response names no viewer.
pub(super) fn parse_details(
    response: &Value,
    project_id: &str,
    numbers: &[u64],
) -> HashMap<u64, PullDetails> {
    let viewer = response
        .pointer("/data/viewer/login")
        .and_then(Value::as_str);
    let total = |value: &Value| {
        value
            .pointer("/comments/totalCount")
            .and_then(Value::as_u64)
    };
    numbers
        .iter()
        .filter_map(|number| {
            let pull = response.pointer(&format!("/data/repository/p{number}"))?;
            let in_reviews: u64 = pull
                .pointer("/reviews/nodes")?
                .as_array()?
                .iter()
                .filter_map(total)
                .sum();
            let comments = total(pull)? + in_reviews;
            let requested_reviewers = pull
                .pointer("/reviewRequests/nodes")
                .and_then(Value::as_array)
                .map(|nodes| {
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
                        .collect()
                });
            let viewer_review_state = viewer.and_then(|viewer| {
                let reviews = pull.pointer("/latestReviews/nodes")?.as_array()?;
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
            Some((
                *number,
                PullDetails {
                    project_id: project_id.to_owned(),
                    number: *number,
                    created_at: optional(pull, "createdAt"),
                    requested_reviewers,
                    comment_count: Some(u32::try_from(comments).unwrap_or(u32::MAX)),
                    viewer_review_state,
                },
            ))
        })
        .collect()
}

pub(super) fn classify_details(
    _success: bool,
    stdout: &str,
    stderr: &str,
    project_id: &str,
    numbers: &[u64],
) -> DetailsOutcome {
    let said = format!("{stderr}\n{stdout}").to_lowercase();
    if said.contains("rate limit") || said.contains("rate_limited") {
        return DetailsOutcome::RateLimited;
    }
    // GraphQL can answer some pull requests and report errors for others, and `gh`
    // then exits non-zero; every row that came back whole is kept.
    let Ok(response) = serde_json::from_str::<Value>(stdout) else {
        return DetailsOutcome::Unavailable;
    };
    let mut found = parse_details(&response, project_id, numbers);
    // An error inside a pull request can null a node of an otherwise complete row, which
    // would leave a short reviewer list or an undercounted total, so the row goes whole.
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

/// Details for GitHub pull requests, in requests of at most `DETAILS_BATCH` aliases. A
/// batch that fails leaves only its own rows out; a spent limit ends the whole call.
/// Other forges have none.
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
    let query = format!("query={}", details_query(numbers));
    let args = [
        "api",
        "graphql",
        "-F",
        "owner={owner}",
        "-F",
        "repo={repo}",
        "-f",
        query.as_str(),
    ];
    let Ok(Ok(output)) =
        tokio::time::timeout(DETAILS_TIMEOUT, spawn(forge, cwd, &args, None)).await
    else {
        return DetailsOutcome::Unavailable;
    };
    classify_details(
        output.success,
        &output.stdout,
        &output.stderr,
        project_id,
        numbers,
    )
}
