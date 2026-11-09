//! Forge reads and writes through each forge's own CLI, so a login stays the
//! user's and Orbyn holds no credential. No shell parses an argument. GitHub and
//! GitLab cover everything their APIs offer through `gh api` and `glab api`, Azure
//! DevOps and Forgejo cover the basics their CLIs have, and Bitbucket has no CLI to
//! go through, so it reports that instead of asking for a token.

mod process;

pub(crate) mod cache;

use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use orbyn_contracts::{
    AddPullComment, MergePull, PullCandidate, PullCheck, PullChecks, PullComment, PullConversation,
    PullDetails, PullFile, PullRequest, PullRequestDetail, PullRevision, PullThread,
    PullTimelineEvent, RequestedReviewer, ReviewLineComment, SetPullPeople, SubmitReview,
    UpdatePull,
};
use serde_json::{Value, json};
use tokio::process::Command;

use crate::error::{CoreError, Result};
use crate::host::process::resolve;
use crate::source_control::git::{git, text};
pub use process::{call, no_such_pull};
use process::{call_input, call_json, call_pages, spawn};

const LIST_LIMIT: &str = "100";
/// The details request must not hold a refresh up for long.
const DETAILS_TIMEOUT: Duration = Duration::from_secs(20);
/// Pull requests asked in one GraphQL query, to stay inside its node and cost limits.
const DETAILS_BATCH: usize = 25;
/// Rows one details request may ask about, the size of a project's list.
pub const DETAILS_ROW_LIMIT: usize = 100;

impl crate::app::Core {
    /// A fixed, read-only product-repository check through the person's own CLI.
    /// Missing sign-in and unavailable tools are not a reason to interrupt a send.
    pub async fn repository_star_status(&self) -> orbyn_contracts::RepositoryStarStatus {
        let starred = async {
            let resolved = resolve("gh")?;
            let mut command = Command::new(resolved.program);
            command
                .args(resolved.prefix)
                .args([
                    "api",
                    "--method",
                    "GET",
                    "--hostname",
                    "github.com",
                    "user/starred/orbyn-dev/orbyn-engineering",
                    "--silent",
                ])
                .current_dir(&self.config.data_dir)
                .env("GH_PROMPT_DISABLED", "1")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true);
            crate::host::child_env::for_tool(command.as_std_mut());
            let mut child = crate::host::process::ManagedChild::spawn(command).ok()?;
            match tokio::time::timeout(Duration::from_secs(3), child.wait()).await {
                Ok(Ok(status)) => Some(status.success()),
                _ => {
                    child.terminate();
                    let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
                    None
                }
            }
        }
        .await
        .unwrap_or(false);
        orbyn_contracts::RepositoryStarStatus { starred }
    }
}
const GH_LIST_FIELDS: &str = "number,title,url,state,headRefName,baseRefName,author,isDraft,\
updatedAt,labels,reviewDecision,statusCheckRollup";
const GH_VIEW_FIELDS: &str = "number,title,url,state,headRefName,baseRefName,author,isDraft,\
updatedAt,labels,reviewDecision,statusCheckRollup,body,comments,reviews,mergeable,reviewRequests,\
headRefOid,baseRefOid,additions,deletions,changedFiles,maintainerCanModify,autoMergeRequest";
/// Failing check names kept per pull request, so a list stays small.
const MAX_FAILING_NAMES: usize = 20;
const THREADS_QUERY: &str = "query($owner:String!,$repo:String!,$number:Int!){\
repository(owner:$owner,name:$repo){pullRequest(number:$number){reviewThreads(first:100){nodes{\
id isResolved path line comments(first:100){nodes{databaseId body createdAt author{login}}}}}}}}";
const RESOLVE_MUTATION: &str =
    "mutation($id:ID!){resolveReviewThread(input:{threadId:$id}){thread{id}}}";
const UNRESOLVE_MUTATION: &str =
    "mutation($id:ID!){unresolveReviewThread(input:{threadId:$id}){thread{id}}}";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Forge {
    GitHub,
    GitLab,
    Azure,
    Forgejo,
    Bitbucket,
}

pub const ALL_FORGES: [Forge; 5] = [
    Forge::GitHub,
    Forge::GitLab,
    Forge::Azure,
    Forge::Forgejo,
    Forge::Bitbucket,
];

impl Forge {
    /// Reads the host from a remote URL. A word in the path does not count. An
    /// unknown host is GitHub, so GitHub Enterprise works through `gh`.
    pub fn of_remote(url: &str) -> Self {
        let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
        let rest = rest.rsplit_once('@').map_or(rest, |(_, rest)| rest);
        let host = rest.split(['/', ':']).next().unwrap_or("").to_lowercase();
        if host.contains("gitlab") {
            Self::GitLab
        } else if host == "dev.azure.com"
            || host.ends_with(".dev.azure.com")
            || host.ends_with(".visualstudio.com")
        {
            Self::Azure
        } else if host.contains("bitbucket") {
            Self::Bitbucket
        } else if host.contains("codeberg") || host.contains("forgejo") || host.contains("gitea") {
            Self::Forgejo
        } else {
            Self::GitHub
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::GitHub => "github",
            Self::GitLab => "gitlab",
            Self::Azure => "azure",
            Self::Forgejo => "forgejo",
            Self::Bitbucket => "bitbucket",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::GitHub => "GitHub",
            Self::GitLab => "GitLab",
            Self::Azure => "Azure DevOps",
            Self::Forgejo => "Forgejo",
            Self::Bitbucket => "Bitbucket",
        }
    }

    pub fn program(self) -> Option<&'static str> {
        match self {
            Self::GitHub => Some("gh"),
            Self::GitLab => Some("glab"),
            Self::Azure => Some("az"),
            Self::Forgejo => Some("tea"),
            Self::Bitbucket => None,
        }
    }

    fn unsupported(self, what: &str) -> CoreError {
        CoreError::Unsupported(format!("{} cannot {what} from Orbyn.", self.name()))
    }
}

pub async fn forge_of(cwd: &Path) -> Result<Forge> {
    let url = git(cwd, &["remote", "get-url", "origin"])
        .await
        .map_err(|_| CoreError::Invalid("This project has no origin remote.".into()))?;
    Ok(Forge::of_remote(&text(&url)))
}

fn field<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn optional(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

fn id_of(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::String(text)) => text.clone(),
        _ => String::new(),
    }
}

fn person(value: &Value) -> Option<String> {
    value
        .get("login")
        .or_else(|| value.get("username"))
        .or_else(|| value.get("name"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn check_state(status: &str, conclusion: &str) -> &'static str {
    match (
        status.to_uppercase().as_str(),
        conclusion.to_uppercase().as_str(),
    ) {
        (_, "SUCCESS" | "PASS" | "PASSED") | ("SUCCESS" | "PASS" | "PASSED", _) => "pass",
        (_, "FAILURE" | "FAIL" | "FAILED" | "TIMED_OUT" | "CANCELLED" | "CANCELED" | "ERROR")
        | ("FAILURE" | "FAIL" | "FAILED" | "ERROR", _) => "fail",
        (_, "SKIPPED" | "SKIPPING") | ("SKIPPED" | "SKIPPING", _) => "skipped",
        (_, "NEUTRAL") => "neutral",
        _ => "pending",
    }
}

/// What one entry of GitHub's check rollup counts as.
#[derive(Debug, PartialEq, Eq)]
enum CheckBucket {
    Pass,
    Fail,
    Running,
    /// Skipped, neutral or stale: only in the total.
    Other,
}

/// A check run is running until it is `COMPLETED`; then its conclusion decides. A
/// commit status has only a `state`, where `EXPECTED` and `PENDING` are still to come.
fn check_bucket(entry: &Value) -> CheckBucket {
    let status = field(entry, "status").to_uppercase();
    let conclusion = field(entry, "conclusion").to_uppercase();
    if status.is_empty() && conclusion.is_empty() {
        return match field(entry, "state").to_uppercase().as_str() {
            "SUCCESS" => CheckBucket::Pass,
            "FAILURE" | "ERROR" => CheckBucket::Fail,
            _ => CheckBucket::Running,
        };
    }
    if !status.is_empty() && status != "COMPLETED" {
        return CheckBucket::Running;
    }
    match conclusion.as_str() {
        "SUCCESS" => CheckBucket::Pass,
        "FAILURE" | "CANCELLED" | "TIMED_OUT" | "ACTION_REQUIRED" | "STARTUP_FAILURE" => {
            CheckBucket::Fail
        }
        _ => CheckBucket::Other,
    }
}

/// One word for the counts: a failure wins, then anything unfinished.
fn github_rollup(value: &Value) -> Option<String> {
    let checks = github_checks(value)?;
    Some(
        if checks.total == 0 {
            "none"
        } else if checks.failing > 0 {
            "fail"
        } else if checks.running > 0 {
            "pending"
        } else {
            "pass"
        }
        .to_owned(),
    )
}

/// Per-state counts over GitHub's check rollup, which holds check runs and commit
/// statuses. Skipped, neutral and stale checks count only in the total.
fn github_checks(value: &Value) -> Option<PullChecks> {
    let entries = value.get("statusCheckRollup")?.as_array()?;
    let mut checks = PullChecks {
        total: u32::try_from(entries.len()).unwrap_or(u32::MAX),
        passing: 0,
        failing: 0,
        running: 0,
        failing_names: Vec::new(),
    };
    for entry in entries {
        match check_bucket(entry) {
            CheckBucket::Pass => checks.passing += 1,
            CheckBucket::Fail => {
                checks.failing += 1;
                let name = [field(entry, "name"), field(entry, "context")].concat();
                if !name.is_empty() && checks.failing_names.len() < MAX_FAILING_NAMES {
                    checks.failing_names.push(name);
                }
            }
            CheckBucket::Running => checks.running += 1,
            CheckBucket::Other => {}
        }
    }
    Some(checks)
}

fn review_word(decision: &str) -> String {
    match decision.to_uppercase().as_str() {
        "APPROVED" => "approved",
        "CHANGES_REQUESTED" => "changes_requested",
        "REVIEW_REQUIRED" => "review_required",
        _ => "none",
    }
    .to_owned()
}

/// Reads any forge's JSON into one shape. States are lowercase and `opened` is
/// GitLab's word for open.
pub fn parse_pull(value: &Value) -> Option<PullRequest> {
    let number = value
        .get("number")
        .or_else(|| value.get("iid"))
        .or_else(|| value.get("index"))
        .or_else(|| value.get("pullRequestId"))?
        .as_u64()?;
    let state = [field(value, "state"), field(value, "status")]
        .concat()
        .to_lowercase();
    let strip = |name: &str| name.trim_start_matches("refs/heads/").to_owned();
    let head = [
        field(value, "headRefName"),
        field(value, "source_branch"),
        field(value, "head"),
        field(value, "sourceRefName"),
    ]
    .concat();
    let base = [
        field(value, "baseRefName"),
        field(value, "target_branch"),
        field(value, "base"),
        field(value, "targetRefName"),
    ]
    .concat();
    let url = [field(value, "url"), field(value, "web_url")].concat();
    let author = value
        .get("author")
        .and_then(person)
        .or_else(|| {
            value
                .pointer("/createdBy/displayName")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .or_else(|| optional(value, "poster"));
    let labels = value
        .get("labels")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|label| {
            label
                .as_str()
                .map(str::to_owned)
                .or_else(|| optional(label, "name"))
        })
        .collect();
    let draft = ["isDraft", "draft"]
        .iter()
        .any(|key| value.get(*key).and_then(Value::as_bool) == Some(true));
    Some(PullRequest {
        number,
        title: field(value, "title").to_owned(),
        url,
        state: match state.as_str() {
            "opened" | "active" => "open".to_owned(),
            "completed" => "merged".to_owned(),
            "abandoned" => "closed".to_owned(),
            _ => state,
        },
        head: strip(&head),
        base: strip(&base),
        author,
        draft,
        updated_at: optional(value, "updatedAt").or_else(|| optional(value, "updated_at")),
        labels,
        review_state: value
            .get("reviewDecision")
            .map(|decision| review_word(decision.as_str().unwrap_or_default())),
        checks_state: github_rollup(value),
        checks: github_checks(value),
    })
}

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
fn details_query(numbers: &[u64]) -> String {
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
fn parse_details(response: &Value, project_id: &str, numbers: &[u64]) -> HashMap<u64, PullDetails> {
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

fn classify_details(
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

async fn details_batch(
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

pub async fn fetch_pull(forge: Forge, cwd: &Path, number: u64) -> Result<PullRequest> {
    let id = number.to_string();
    let value = match forge {
        Forge::GitHub => {
            call_json(forge, cwd, &["pr", "view", &id, "--json", GH_LIST_FIELDS]).await?
        }
        Forge::GitLab => call_json(forge, cwd, &["mr", "view", &id, "--output", "json"]).await?,
        Forge::Azure => {
            call_json(
                forge,
                cwd,
                &[
                    "repos", "pr", "show", "--id", &id, "--detect", "true", "--output", "json",
                ],
            )
            .await?
        }
        Forge::Forgejo => forgejo_pull(cwd, number).await?,
        Forge::Bitbucket => return Err(forge.unsupported("read pull requests")),
    };
    parse_pull(&value).ok_or_else(|| CoreError::Git("The forge returned no pull request.".into()))
}

async fn forgejo_pull(cwd: &Path, number: u64) -> Result<Value> {
    let listed = call_json(
        Forge::Forgejo,
        cwd,
        &[
            "pulls",
            "list",
            "--state",
            "all",
            "--output",
            "json",
            "--fields",
            "index,title,state,author,url,head,base,labels,updated",
        ],
    )
    .await?;
    listed
        .as_array()
        .into_iter()
        .flatten()
        .find(|pull| id_of(pull, "index") == number.to_string())
        .cloned()
        .ok_or_else(|| CoreError::NotFound("pull request".into()))
}

/// Filters for a list. Empty fields do not narrow it.
#[derive(Clone, Debug, Default)]
pub struct PullFilter {
    pub state: String,
    pub search: Option<String>,
    pub author: Option<String>,
    pub label: Option<String>,
    pub head: Option<String>,
}

pub async fn list_pulls(forge: Forge, cwd: &Path, filter: &PullFilter) -> Result<Vec<PullRequest>> {
    let mut args: Vec<&str>;
    match forge {
        Forge::GitHub => {
            args = vec![
                "pr",
                "list",
                "--limit",
                LIST_LIMIT,
                "--state",
                &filter.state,
                "--json",
                GH_LIST_FIELDS,
            ];
            for (flag, value) in [
                ("--search", &filter.search),
                ("--author", &filter.author),
                ("--label", &filter.label),
                ("--head", &filter.head),
            ] {
                if let Some(value) = value {
                    args.extend([flag, value.as_str()]);
                }
            }
        }
        Forge::GitLab => {
            args = vec!["mr", "list", "--per-page", LIST_LIMIT, "--output", "json"];
            match filter.state.as_str() {
                "closed" => args.push("--closed"),
                "merged" => args.push("--merged"),
                "all" => args.push("--all"),
                _ => {}
            }
            for (flag, value) in [
                ("--search", &filter.search),
                ("--author", &filter.author),
                ("--label", &filter.label),
                ("--source-branch", &filter.head),
            ] {
                if let Some(value) = value {
                    args.extend([flag, value.as_str()]);
                }
            }
        }
        Forge::Azure => {
            let status = match filter.state.as_str() {
                "open" => "active",
                "merged" => "completed",
                "closed" => "abandoned",
                _ => "all",
            };
            args = vec![
                "repos", "pr", "list", "--detect", "true", "--status", status, "--top", LIST_LIMIT,
                "--output", "json",
            ];
        }
        Forge::Forgejo => {
            let state = if filter.state == "merged" {
                "closed"
            } else {
                filter.state.as_str()
            };
            args = vec![
                "pulls",
                "list",
                "--state",
                state,
                "--output",
                "json",
                "--limit",
                LIST_LIMIT,
                "--fields",
                "index,title,state,author,url,head,base,labels,updated",
            ];
        }
        Forge::Bitbucket => return Err(forge.unsupported("list pull requests")),
    }
    let values: Vec<Value> = serde_json::from_str(&call(forge, cwd, &args).await?)?;
    let mut pulls: Vec<PullRequest> = values.iter().filter_map(parse_pull).collect();
    // Azure and Forgejo have no such flags, so they are applied here.
    if matches!(forge, Forge::Azure | Forge::Forgejo) {
        if let Some(needle) = &filter.search {
            let needle = needle.to_lowercase();
            pulls.retain(|pull| pull.title.to_lowercase().contains(&needle));
        }
        if let Some(head) = &filter.head {
            pulls.retain(|pull| pull.head == *head);
        }
        if let Some(author) = &filter.author {
            pulls.retain(|pull| pull.author.as_deref() == Some(author));
        }
        if let Some(label) = &filter.label {
            pulls.retain(|pull| pull.labels.contains(label));
        }
    }
    Ok(pulls)
}

pub async fn pull_detail(forge: Forge, cwd: &Path, number: u64) -> Result<PullRequestDetail> {
    let id = number.to_string();
    match forge {
        Forge::GitHub => {
            let view =
                call_json(forge, cwd, &["pr", "view", &id, "--json", GH_VIEW_FIELDS]).await?;
            let pull = parse_pull(&view)
                .ok_or_else(|| CoreError::Git("The forge returned no pull request.".into()))?;
            let reviewers = view
                .get("reviewRequests")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(person)
                .collect();
            Ok(PullRequestDetail {
                pull,
                body: field(&view, "body").to_owned(),
                checks: raw_github_checks(cwd, &id).await,
                comments: json!({ "comments": view.get("comments"), "reviews": view.get("reviews") }),
                mergeable: optional(&view, "mergeable").map(|word| word.to_lowercase()),
                reviewers,
                head_sha: optional(&view, "headRefOid"),
                base_sha: optional(&view, "baseRefOid"),
                additions: view.get("additions").and_then(Value::as_u64),
                deletions: view.get("deletions").and_then(Value::as_u64),
                changed_files: view.get("changedFiles").and_then(Value::as_u64),
                maintainer_can_modify: view.get("maintainerCanModify").and_then(Value::as_bool),
                auto_merge: view
                    .get("autoMergeRequest")
                    .is_some_and(|auto| !auto.is_null()),
            })
        }
        Forge::GitLab => {
            let view = call_json(forge, cwd, &["mr", "view", &id, "--output", "json"]).await?;
            let pull = parse_pull(&view)
                .ok_or_else(|| CoreError::Git("The forge returned no pull request.".into()))?;
            Ok(PullRequestDetail {
                pull,
                body: field(&view, "description").to_owned(),
                checks: view.get("head_pipeline").cloned().unwrap_or(json!(null)),
                comments: view.get("notes").cloned().unwrap_or(json!([])),
                mergeable: optional(&view, "detailed_merge_status").map(|word| {
                    if word == "conflict" {
                        "conflicting".to_owned()
                    } else if word == "mergeable" {
                        word
                    } else {
                        "unknown".to_owned()
                    }
                }),
                reviewers: view
                    .get("reviewers")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(person)
                    .collect(),
                head_sha: optional(&view, "sha"),
                base_sha: view
                    .pointer("/diff_refs/base_sha")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                additions: None,
                deletions: None,
                changed_files: view
                    .get("changes_count")
                    .and_then(Value::as_str)
                    .and_then(|n| n.parse().ok()),
                maintainer_can_modify: view.get("allow_collaboration").and_then(Value::as_bool),
                auto_merge: view
                    .get("merge_when_pipeline_succeeds")
                    .and_then(Value::as_bool)
                    == Some(true),
            })
        }
        Forge::Azure | Forge::Forgejo => Ok(PullRequestDetail {
            pull: fetch_pull(forge, cwd, number).await?,
            body: String::new(),
            checks: json!([]),
            comments: json!([]),
            mergeable: None,
            reviewers: Vec::new(),
            head_sha: None,
            base_sha: None,
            additions: None,
            deletions: None,
            changed_files: None,
            maintainer_can_modify: None,
            auto_merge: false,
        }),
        Forge::Bitbucket => Err(forge.unsupported("read pull requests")),
    }
}

/// `gh pr checks` exits non-zero while checks fail or wait, and still prints them.
async fn raw_github_checks(cwd: &Path, id: &str) -> Value {
    let args = [
        "pr",
        "checks",
        id,
        "--json",
        "name,state,link,workflow,description,bucket",
    ];
    match spawn(Forge::GitHub, cwd, &args, None).await {
        Ok(output) => serde_json::from_str(&output.stdout).unwrap_or(json!([])),
        Err(_) => json!([]),
    }
}

pub async fn pull_checks(forge: Forge, cwd: &Path, number: u64) -> Result<Vec<PullCheck>> {
    let id = number.to_string();
    match forge {
        Forge::GitHub => Ok(raw_github_checks(cwd, &id)
            .await
            .as_array()
            .into_iter()
            .flatten()
            .map(|check| PullCheck {
                name: field(check, "name").to_owned(),
                state: check_state(field(check, "state"), field(check, "bucket")).to_owned(),
                url: optional(check, "link"),
                workflow: optional(check, "workflow"),
                description: optional(check, "description"),
            })
            .collect()),
        Forge::GitLab => {
            let path = format!("projects/:id/merge_requests/{id}/pipelines");
            let pipelines = call_pages(forge, cwd, &["api", &path]).await?;
            Ok(pipelines
                .iter()
                .take(1)
                .map(|pipeline| PullCheck {
                    name: format!("Pipeline {}", id_of(pipeline, "id")),
                    state: check_state(field(pipeline, "status"), "").to_owned(),
                    url: optional(pipeline, "web_url"),
                    workflow: None,
                    description: None,
                })
                .collect())
        }
        _ => Ok(Vec::new()),
    }
}

/// Lets a fork's waiting workflow run.
pub async fn approve_workflow(forge: Forge, cwd: &Path, run_id: u64) -> Result<()> {
    if forge != Forge::GitHub {
        return Err(forge.unsupported("approve a workflow"));
    }
    let path = format!("repos/{{owner}}/{{repo}}/actions/runs/{run_id}/approve");
    call(forge, cwd, &["api", "-X", "POST", &path])
        .await
        .map(|_| ())
}

fn github_comment(value: &Value, kind: &str) -> PullComment {
    PullComment {
        id: id_of(value, "id"),
        kind: kind.to_owned(),
        author: value
            .get("user")
            .or_else(|| value.get("author"))
            .and_then(person),
        body: field(value, "body").to_owned(),
        created_at: optional(value, "created_at").or_else(|| optional(value, "createdAt")),
        path: optional(value, "path"),
        line: value.get("line").and_then(Value::as_u64),
    }
}

fn gitlab_comment(note: &Value) -> PullComment {
    PullComment {
        id: id_of(note, "id"),
        kind: "note".to_owned(),
        author: note.get("author").and_then(person),
        body: field(note, "body").to_owned(),
        created_at: optional(note, "created_at"),
        path: note
            .pointer("/position/new_path")
            .and_then(Value::as_str)
            .map(str::to_owned),
        line: note.pointer("/position/new_line").and_then(Value::as_u64),
    }
}

/// Splits a GraphQL `reviewThreads` reply into threads.
fn github_threads(reply: &Value) -> Vec<PullThread> {
    let nodes = reply.pointer("/data/repository/pullRequest/reviewThreads/nodes");
    nodes
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|thread| PullThread {
            id: field(thread, "id").to_owned(),
            resolved: thread.get("isResolved").and_then(Value::as_bool) == Some(true),
            path: optional(thread, "path"),
            line: thread.get("line").and_then(Value::as_u64),
            comments: thread
                .pointer("/comments/nodes")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|comment| PullComment {
                    id: id_of(comment, "databaseId"),
                    kind: "review".to_owned(),
                    author: comment.get("author").and_then(person),
                    body: field(comment, "body").to_owned(),
                    created_at: optional(comment, "createdAt"),
                    path: optional(thread, "path"),
                    line: thread.get("line").and_then(Value::as_u64),
                })
                .collect(),
        })
        .collect()
}

/// A discussion with one system note is activity, not a conversation.
fn gitlab_conversation(discussions: &[Value]) -> PullConversation {
    let mut conversation = PullConversation {
        comments: Vec::new(),
        threads: Vec::new(),
    };
    for discussion in discussions {
        let notes: Vec<&Value> = discussion
            .get("notes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|note| note.get("system").and_then(Value::as_bool) != Some(true))
            .collect();
        let Some(first) = notes.first() else { continue };
        if first.get("resolvable").and_then(Value::as_bool) == Some(true) {
            conversation.threads.push(PullThread {
                id: field(discussion, "id").to_owned(),
                resolved: first.get("resolved").and_then(Value::as_bool) == Some(true),
                path: first
                    .pointer("/position/new_path")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                line: first.pointer("/position/new_line").and_then(Value::as_u64),
                comments: notes.iter().map(|note| gitlab_comment(note)).collect(),
            });
        } else {
            conversation
                .comments
                .extend(notes.iter().map(|note| gitlab_comment(note)));
        }
    }
    conversation
}

pub async fn conversation(forge: Forge, cwd: &Path, number: u64) -> Result<PullConversation> {
    let id = number.to_string();
    match forge {
        Forge::GitHub => {
            let path = format!("repos/{{owner}}/{{repo}}/issues/{id}/comments");
            let comments = call_pages(forge, cwd, &["api", "--paginate", &path]).await?;
            let number_field = format!("number={id}");
            let query = format!("query={THREADS_QUERY}");
            let threads = call_json(
                forge,
                cwd,
                &[
                    "api",
                    "graphql",
                    "-F",
                    "owner={owner}",
                    "-F",
                    "repo={repo}",
                    "-F",
                    &number_field,
                    "-f",
                    &query,
                ],
            )
            .await?;
            Ok(PullConversation {
                comments: comments
                    .iter()
                    .map(|value| github_comment(value, "issue"))
                    .collect(),
                threads: github_threads(&threads),
            })
        }
        Forge::GitLab => {
            let path = format!("projects/:id/merge_requests/{id}/discussions?per_page=100");
            let discussions = call_pages(forge, cwd, &["api", "--paginate", &path]).await?;
            Ok(gitlab_conversation(&discussions))
        }
        Forge::Azure | Forge::Forgejo => Ok(PullConversation {
            comments: Vec::new(),
            threads: Vec::new(),
        }),
        Forge::Bitbucket => Err(forge.unsupported("read comments")),
    }
}

pub async fn timeline(forge: Forge, cwd: &Path, number: u64) -> Result<Vec<PullTimelineEvent>> {
    let id = number.to_string();
    match forge {
        Forge::GitHub => {
            let path = format!("repos/{{owner}}/{{repo}}/issues/{id}/timeline");
            let events = call_pages(forge, cwd, &["api", "--paginate", &path]).await?;
            Ok(events.iter().map(parse_github_event).collect())
        }
        Forge::GitLab => {
            let path = format!("projects/:id/merge_requests/{id}/notes?per_page=100&sort=asc");
            let notes = call_pages(forge, cwd, &["api", "--paginate", &path]).await?;
            Ok(notes
                .iter()
                .map(|note| PullTimelineEvent {
                    id: id_of(note, "id"),
                    kind: if note.get("system").and_then(Value::as_bool) == Some(true) {
                        "system".to_owned()
                    } else {
                        "commented".to_owned()
                    },
                    actor: note.get("author").and_then(person),
                    created_at: optional(note, "created_at"),
                    text: optional(note, "body"),
                })
                .collect())
        }
        Forge::Azure | Forge::Forgejo => Ok(Vec::new()),
        Forge::Bitbucket => Err(forge.unsupported("read activity")),
    }
}

/// One GitHub timeline event. A `committed` event has no `id`, `actor` or `created_at`: it carries the commit's `sha`,
/// its `author` and `committer` (a name and a date) and its `message`, so those stand in for them.
fn parse_github_event(event: &Value) -> PullTimelineEvent {
    let dated = |who: &str| event.get(who).and_then(|person| optional(person, "date"));
    let first_line = |text: String| text.lines().next().unwrap_or_default().to_owned();
    PullTimelineEvent {
        id: Some(id_of(event, "id"))
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| id_of(event, "sha")),
        kind: field(event, "event").to_owned(),
        actor: event
            .get("actor")
            .or_else(|| event.get("user"))
            .or_else(|| event.get("author"))
            .and_then(person),
        created_at: optional(event, "created_at")
            .or_else(|| optional(event, "submitted_at"))
            .or_else(|| dated("committer"))
            .or_else(|| dated("author")),
        text: optional(event, "body")
            .or_else(|| optional(event, "state"))
            .or_else(|| optional(event, "message").map(first_line)),
    }
}

fn parse_github_file(file: &Value) -> PullFile {
    let status = field(file, "status");
    PullFile {
        path: field(file, "filename").to_owned(),
        previous_path: optional(file, "previous_filename"),
        status: match status {
            "removed" => "deleted",
            "changed" => "modified",
            other => other,
        }
        .to_owned(),
        additions: file.get("additions").and_then(Value::as_u64).unwrap_or(0),
        deletions: file.get("deletions").and_then(Value::as_u64).unwrap_or(0),
        patch: optional(file, "patch"),
        viewed: false,
    }
}

fn parse_gitlab_file(file: &Value) -> PullFile {
    let flag = |key: &str| file.get(key).and_then(Value::as_bool) == Some(true);
    let (mut additions, mut deletions) = (0, 0);
    let patch = optional(file, "diff");
    for line in patch.as_deref().unwrap_or_default().lines() {
        if line.starts_with('+') {
            additions += 1;
        } else if line.starts_with('-') {
            deletions += 1;
        }
    }
    PullFile {
        path: field(file, "new_path").to_owned(),
        previous_path: Some(field(file, "old_path").to_owned())
            .filter(|old| *old != field(file, "new_path")),
        status: if flag("new_file") {
            "added"
        } else if flag("deleted_file") {
            "deleted"
        } else if flag("renamed_file") {
            "renamed"
        } else {
            "modified"
        }
        .to_owned(),
        additions,
        deletions,
        patch,
        viewed: false,
    }
}

pub async fn pull_files(forge: Forge, cwd: &Path, number: u64) -> Result<Vec<PullFile>> {
    match forge {
        Forge::GitHub => {
            let path = format!("repos/{{owner}}/{{repo}}/pulls/{number}/files?per_page=100");
            let files = call_pages(forge, cwd, &["api", "--paginate", &path]).await?;
            Ok(files.iter().map(parse_github_file).collect())
        }
        Forge::GitLab => {
            let path = format!("projects/:id/merge_requests/{number}/diffs?per_page=100");
            let files = call_pages(forge, cwd, &["api", "--paginate", &path]).await?;
            Ok(files.iter().map(parse_gitlab_file).collect())
        }
        _ => Err(forge.unsupported("list changed files")),
    }
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or_default().to_owned()
}

fn parse_github_revision(commit: &Value) -> PullRevision {
    let detail = commit.get("commit").unwrap_or(&Value::Null);
    PullRevision {
        sha: optional(commit, "sha").unwrap_or_default(),
        message: first_line(&optional(detail, "message").unwrap_or_default()),
        author: commit
            .get("author")
            .and_then(|author| optional(author, "login"))
            .or_else(|| {
                detail
                    .get("author")
                    .and_then(|author| optional(author, "name"))
            }),
        committed_at: detail
            .get("committer")
            .and_then(|committer| optional(committer, "date")),
    }
}

fn parse_gitlab_revision(commit: &Value) -> PullRevision {
    PullRevision {
        sha: optional(commit, "id").unwrap_or_default(),
        message: first_line(&optional(commit, "title").unwrap_or_default()),
        author: optional(commit, "author_name"),
        committed_at: optional(commit, "committed_date"),
    }
}

/// The commits of a pull request, oldest first.
pub async fn pull_revisions(forge: Forge, cwd: &Path, number: u64) -> Result<Vec<PullRevision>> {
    match forge {
        Forge::GitHub => {
            let path = format!("repos/{{owner}}/{{repo}}/pulls/{number}/commits?per_page=100");
            let commits = call_pages(forge, cwd, &["api", "--paginate", &path]).await?;
            Ok(commits.iter().map(parse_github_revision).collect())
        }
        Forge::GitLab => {
            let path = format!("projects/:id/merge_requests/{number}/commits?per_page=100");
            let commits = call_pages(forge, cwd, &["api", "--paginate", &path]).await?;
            // GitLab lists the newest first.
            Ok(commits.iter().rev().map(parse_gitlab_revision).collect())
        }
        _ => Err(forge.unsupported("list commits")),
    }
}

/// What one commit changed. The hash comes from the page, so it is checked here.
pub async fn revision_files(forge: Forge, cwd: &Path, sha: &str) -> Result<Vec<PullFile>> {
    if sha.is_empty() || !sha.chars().all(|letter| letter.is_ascii_hexdigit()) {
        return Err(CoreError::Invalid("That is not a commit.".into()));
    }
    match forge {
        Forge::GitHub => {
            let path = format!("repos/{{owner}}/{{repo}}/commits/{sha}");
            let commit: Value = serde_json::from_str(&call(forge, cwd, &["api", &path]).await?)
                .map_err(|error| CoreError::Git(error.to_string()))?;
            let files = commit.get("files").and_then(Value::as_array);
            Ok(files.into_iter().flatten().map(parse_github_file).collect())
        }
        Forge::GitLab => {
            let path = format!("projects/:id/repository/commits/{sha}/diff?per_page=100");
            let files = call_pages(forge, cwd, &["api", "--paginate", &path]).await?;
            Ok(files.iter().map(parse_gitlab_file).collect())
        }
        _ => Err(forge.unsupported("list changed files")),
    }
}

#[cfg(test)]
mod revision_tests;

fn is_github_review(kind: &str) -> bool {
    kind == "review"
}

async fn head_sha(forge: Forge, cwd: &Path, number: u64) -> Result<(String, Value)> {
    let detail = pull_detail(forge, cwd, number).await?;
    let sha = detail
        .head_sha
        .ok_or_else(|| CoreError::Git("The forge gave no head commit.".into()))?;
    Ok((sha, json!({ "base_sha": detail.base_sha })))
}

pub async fn add_comment(
    forge: Forge,
    cwd: &Path,
    number: u64,
    request: &AddPullComment,
) -> Result<()> {
    let id = number.to_string();
    let body_field = format!("body={}", request.body);
    match (forge, &request.reply_to, &request.path, request.line) {
        (Forge::GitHub, Some(reply), _, _) => {
            let path = format!("repos/{{owner}}/{{repo}}/pulls/{id}/comments/{reply}/replies");
            call(forge, cwd, &["api", "-X", "POST", &path, "-f", &body_field]).await?;
        }
        (Forge::GitHub, None, Some(file), Some(line)) => {
            let (sha, _) = head_sha(forge, cwd, number).await?;
            let path = format!("repos/{{owner}}/{{repo}}/pulls/{id}/comments");
            let commit = format!("commit_id={sha}");
            let file = format!("path={file}");
            let line = format!("line={line}");
            call(
                forge,
                cwd,
                &[
                    "api",
                    "-X",
                    "POST",
                    &path,
                    "-f",
                    &body_field,
                    "-f",
                    &commit,
                    "-f",
                    &file,
                    "-F",
                    &line,
                    "-f",
                    "side=RIGHT",
                ],
            )
            .await?;
        }
        (Forge::GitHub, None, _, _) => {
            call(forge, cwd, &["pr", "comment", &id, "--body", &request.body]).await?;
        }
        (Forge::GitLab, Some(reply), _, _) => {
            let path = format!("projects/:id/merge_requests/{id}/discussions/{reply}/notes");
            call(forge, cwd, &["api", "-X", "POST", &path, "-f", &body_field]).await?;
        }
        (Forge::GitLab, None, Some(file), Some(line)) => {
            let (sha, refs) = head_sha(forge, cwd, number).await?;
            let path = format!("projects/:id/merge_requests/{id}/discussions");
            let base = format!("position[base_sha]={}", field(&refs, "base_sha"));
            let head = format!("position[head_sha]={sha}");
            let start = format!("position[start_sha]={}", field(&refs, "base_sha"));
            let new_path = format!("position[new_path]={file}");
            let new_line = format!("position[new_line]={line}");
            call(
                forge,
                cwd,
                &[
                    "api",
                    "-X",
                    "POST",
                    &path,
                    "-f",
                    &body_field,
                    "-f",
                    "position[position_type]=text",
                    "-f",
                    &base,
                    "-f",
                    &head,
                    "-f",
                    &start,
                    "-f",
                    &new_path,
                    "-F",
                    &new_line,
                ],
            )
            .await?;
        }
        (Forge::GitLab, None, _, _) => {
            call(forge, cwd, &["mr", "note", &id, "--message", &request.body]).await?;
        }
        (Forge::Azure | Forge::Forgejo | Forge::Bitbucket, ..) => {
            return Err(forge.unsupported("comment"));
        }
    }
    Ok(())
}

pub async fn edit_comment(
    forge: Forge,
    cwd: &Path,
    number: u64,
    comment: &str,
    kind: &str,
    body: &str,
) -> Result<()> {
    let body_field = format!("body={body}");
    let path = match forge {
        Forge::GitHub if is_github_review(kind) => {
            format!("repos/{{owner}}/{{repo}}/pulls/comments/{comment}")
        }
        Forge::GitHub => format!("repos/{{owner}}/{{repo}}/issues/comments/{comment}"),
        Forge::GitLab => format!("projects/:id/merge_requests/{number}/notes/{comment}"),
        _ => return Err(forge.unsupported("edit comments")),
    };
    let method = if forge == Forge::GitHub {
        "PATCH"
    } else {
        "PUT"
    };
    call(forge, cwd, &["api", "-X", method, &path, "-f", &body_field])
        .await
        .map(|_| ())
}

pub async fn set_thread_resolved(
    forge: Forge,
    cwd: &Path,
    number: u64,
    thread: &str,
    resolved: bool,
) -> Result<()> {
    match forge {
        Forge::GitHub => {
            let mutation = if resolved {
                RESOLVE_MUTATION
            } else {
                UNRESOLVE_MUTATION
            };
            let query = format!("query={mutation}");
            let id = format!("id={thread}");
            call(forge, cwd, &["api", "graphql", "-f", &query, "-f", &id])
                .await
                .map(|_| ())
        }
        Forge::GitLab => {
            let path = format!("projects/:id/merge_requests/{number}/discussions/{thread}");
            let value = format!("resolved={resolved}");
            call(forge, cwd, &["api", "-X", "PUT", &path, "-F", &value])
                .await
                .map(|_| ())
        }
        _ => Err(forge.unsupported("resolve conversations")),
    }
}

fn gitlab_emoji(content: &str) -> &str {
    match content {
        "+1" => "thumbsup",
        "-1" => "thumbsdown",
        "laugh" => "laughing",
        "hooray" => "tada",
        other => other,
    }
}

pub async fn react(
    forge: Forge,
    cwd: &Path,
    number: u64,
    comment: &str,
    kind: &str,
    content: &str,
) -> Result<()> {
    const REACTIONS: &[&str] = &[
        "+1", "-1", "laugh", "hooray", "confused", "heart", "rocket", "eyes",
    ];
    if !REACTIONS.contains(&content) {
        return Err(CoreError::Invalid("Choose a known reaction.".into()));
    }
    match forge {
        Forge::GitHub => {
            let scope = if is_github_review(kind) {
                "pulls"
            } else {
                "issues"
            };
            let path = format!("repos/{{owner}}/{{repo}}/{scope}/comments/{comment}/reactions");
            let value = format!("content={content}");
            call(forge, cwd, &["api", "-X", "POST", &path, "-f", &value])
                .await
                .map(|_| ())
        }
        Forge::GitLab => {
            let path = format!("projects/:id/merge_requests/{number}/notes/{comment}/award_emoji");
            let value = format!("name={}", gitlab_emoji(content));
            call(forge, cwd, &["api", "-X", "POST", &path, "-f", &value])
                .await
                .map(|_| ())
        }
        _ => Err(forge.unsupported("react")),
    }
}

/// The body of a GitHub review with line comments.
fn review_payload(event: &str, body: &str, comments: &[ReviewLineComment]) -> Value {
    json!({
        "event": event.to_uppercase(),
        "body": body,
        "comments": comments
            .iter()
            .map(|comment| json!({
                "path": comment.path,
                "line": comment.line,
                "side": "RIGHT",
                "body": comment.body,
            }))
            .collect::<Vec<_>>(),
    })
}

pub async fn submit_review(
    forge: Forge,
    cwd: &Path,
    number: u64,
    review: &SubmitReview,
) -> Result<()> {
    let id = number.to_string();
    let event = review.event.as_str();
    if !matches!(event, "approve" | "request_changes" | "comment") {
        return Err(CoreError::Invalid(
            "That review action is not available.".into(),
        ));
    }
    match forge {
        Forge::GitHub if review.comments.is_empty() => {
            let flag = format!("--{}", event.replace('_', "-"));
            call(
                forge,
                cwd,
                &["pr", "review", &id, &flag, "--body", &review.body],
            )
            .await?;
        }
        Forge::GitHub => {
            let path = format!("repos/{{owner}}/{{repo}}/pulls/{id}/reviews");
            let payload = review_payload(event, &review.body, &review.comments).to_string();
            call_input(
                forge,
                cwd,
                &["api", "-X", "POST", &path, "--input", "-"],
                &payload,
            )
            .await?;
        }
        Forge::GitLab => {
            for comment in &review.comments {
                let request = AddPullComment {
                    body: comment.body.clone(),
                    path: Some(comment.path.clone()),
                    line: Some(comment.line),
                    reply_to: None,
                };
                add_comment(forge, cwd, number, &request).await?;
            }
            match event {
                "approve" => {
                    call(forge, cwd, &["mr", "approve", &id]).await?;
                }
                "comment" if !review.body.is_empty() => {
                    call(forge, cwd, &["mr", "note", &id, "--message", &review.body]).await?;
                }
                "comment" => {}
                _ => {
                    return Err(CoreError::Invalid(
                        "That review action is not available.".into(),
                    ));
                }
            }
        }
        _ => return Err(forge.unsupported("review")),
    }
    Ok(())
}

fn joined(names: &[String]) -> Option<String> {
    (!names.is_empty()).then(|| names.join(","))
}

fn valid_names(names: &[String]) -> Result<()> {
    if names
        .iter()
        .any(|name| name.is_empty() || name.starts_with('-') || name.contains(','))
    {
        return Err(CoreError::Invalid("That name is not valid.".into()));
    }
    Ok(())
}

pub async fn set_reviewers(
    forge: Forge,
    cwd: &Path,
    number: u64,
    change: &SetPullPeople,
) -> Result<()> {
    valid_names(&change.add)?;
    valid_names(&change.remove)?;
    let id = number.to_string();
    match forge {
        Forge::GitHub => {
            let mut args = vec!["pr", "edit", id.as_str()];
            let (add, remove) = (joined(&change.add), joined(&change.remove));
            if let Some(add) = &add {
                args.extend(["--add-reviewer", add]);
            }
            if let Some(remove) = &remove {
                args.extend(["--remove-reviewer", remove]);
            }
            call(forge, cwd, &args).await.map(|_| ())
        }
        Forge::GitLab => {
            let names: Vec<String> = change
                .add
                .iter()
                .map(|name| format!("+{name}"))
                .chain(change.remove.iter().map(|name| format!("-{name}")))
                .collect();
            let names = names.join(",");
            call(forge, cwd, &["mr", "update", &id, "--reviewer", &names])
                .await
                .map(|_| ())
        }
        _ => Err(forge.unsupported("change reviewers")),
    }
}

pub async fn set_labels(
    forge: Forge,
    cwd: &Path,
    number: u64,
    change: &SetPullPeople,
) -> Result<()> {
    valid_names(&change.add)?;
    valid_names(&change.remove)?;
    let id = number.to_string();
    let (add, remove) = (joined(&change.add), joined(&change.remove));
    let (program, add_flag, remove_flag) = match forge {
        Forge::GitHub => (["pr", "edit"], "--add-label", "--remove-label"),
        Forge::GitLab => (["mr", "update"], "--label", "--unlabel"),
        _ => return Err(forge.unsupported("change labels")),
    };
    let mut args = vec![program[0], program[1], id.as_str()];
    if let Some(add) = &add {
        args.extend([add_flag, add]);
    }
    if let Some(remove) = &remove {
        args.extend([remove_flag, remove]);
    }
    call(forge, cwd, &args).await.map(|_| ())
}

pub async fn reviewer_candidates(forge: Forge, cwd: &Path) -> Result<Vec<PullCandidate>> {
    let (args, name): (&[&str], &str) = match forge {
        Forge::GitHub => (
            &[
                "api",
                "--paginate",
                "repos/{owner}/{repo}/assignees?per_page=100",
            ],
            "login",
        ),
        Forge::GitLab => (
            &["api", "--paginate", "projects/:id/members/all?per_page=100"],
            "username",
        ),
        _ => return Err(forge.unsupported("list reviewers")),
    };
    Ok(call_pages(forge, cwd, args)
        .await?
        .iter()
        .map(|person| PullCandidate {
            name: field(person, name).to_owned(),
            description: optional(person, "name"),
        })
        .collect())
}

pub async fn label_candidates(forge: Forge, cwd: &Path) -> Result<Vec<PullCandidate>> {
    let value = match forge {
        Forge::GitHub => {
            call_json(
                forge,
                cwd,
                &[
                    "label",
                    "list",
                    "--limit",
                    "200",
                    "--json",
                    "name,description",
                ],
            )
            .await?
        }
        Forge::GitLab => call_json(forge, cwd, &["label", "list", "--output", "json"]).await?,
        _ => return Err(forge.unsupported("list labels")),
    };
    Ok(value
        .as_array()
        .into_iter()
        .flatten()
        .map(|label| PullCandidate {
            name: field(label, "name").to_owned(),
            description: optional(label, "description"),
        })
        .collect())
}

pub async fn update_pull(forge: Forge, cwd: &Path, number: u64, update: &UpdatePull) -> Result<()> {
    let id = number.to_string();
    if update
        .base
        .as_deref()
        .is_some_and(|base| base.starts_with('-'))
    {
        return Err(CoreError::Invalid(
            "That is not a valid branch name.".into(),
        ));
    }
    match forge {
        Forge::GitHub => {
            let mut args = vec!["pr", "edit", id.as_str()];
            for (flag, value) in [
                ("--title", &update.title),
                ("--body", &update.body),
                ("--base", &update.base),
            ] {
                if let Some(value) = value {
                    args.extend([flag, value.as_str()]);
                }
            }
            if args.len() > 3 {
                call(forge, cwd, &args).await?;
            }
            match update.draft {
                Some(true) => {
                    call(forge, cwd, &["pr", "ready", &id, "--undo"]).await?;
                }
                Some(false) => {
                    call(forge, cwd, &["pr", "ready", &id]).await?;
                }
                None => {}
            }
            if let Some(allowed) = update.maintainer_can_modify {
                let path = format!("repos/{{owner}}/{{repo}}/pulls/{id}");
                let value = format!("maintainer_can_modify={allowed}");
                call(forge, cwd, &["api", "-X", "PATCH", &path, "-F", &value]).await?;
            }
            Ok(())
        }
        Forge::GitLab => {
            let mut args = vec!["mr", "update", id.as_str(), "--yes"];
            for (flag, value) in [
                ("--title", &update.title),
                ("--description", &update.body),
                ("--target-branch", &update.base),
            ] {
                if let Some(value) = value {
                    args.extend([flag, value.as_str()]);
                }
            }
            match update.draft {
                Some(true) => args.push("--draft"),
                Some(false) => args.push("--ready"),
                None => {}
            }
            if update.maintainer_can_modify.is_some() {
                return Err(forge.unsupported("change maintainer edits"));
            }
            call(forge, cwd, &args).await.map(|_| ())
        }
        _ => Err(forge.unsupported("edit pull requests")),
    }
}

/// Azure's merge strategy words differ from everyone else's.
pub async fn merge(forge: Forge, cwd: &Path, number: u64, options: &MergePull) -> Result<()> {
    if !matches!(options.method.as_str(), "merge" | "squash" | "rebase") {
        return Err(CoreError::Invalid("Choose merge, squash or rebase.".into()));
    }
    // Where the forge cannot check the head itself, it is read just before the merge, uncached.
    if let Some(expected) = options.expected_head.as_deref()
        && head_precheck_needed(forge, &options.method)
    {
        check_head(expected, current_head(forge, cwd, number).await)?;
    }
    let id = number.to_string();
    let body = options.body.as_ref().map(|body| {
        if options.strip_credits {
            strip_agent_credits(body)
        } else {
            body.clone()
        }
    });
    match forge {
        Forge::GitHub | Forge::GitLab => {
            let args = merge_args(forge, &id, options, body.as_deref());
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            call(forge, cwd, &args)
                .await
                .map(|_| ())
                .map_err(|error| head_moved(forge, error))
        }
        Forge::Azure => {
            let squash = if options.method == "squash" {
                "true"
            } else {
                "false"
            };
            let delete = if options.delete_branch {
                "true"
            } else {
                "false"
            };
            call(
                forge,
                cwd,
                &[
                    "repos",
                    "pr",
                    "update",
                    "--id",
                    &id,
                    "--detect",
                    "true",
                    "--status",
                    "completed",
                    "--squash",
                    squash,
                    "--delete-source-branch",
                    delete,
                    "--output",
                    "json",
                ],
            )
            .await
            .map(|_| ())
        }
        Forge::Forgejo => call(
            forge,
            cwd,
            &["pulls", "merge", "--style", &options.method, &id],
        )
        .await
        .map(|_| ()),
        Forge::Bitbucket => Err(forge.unsupported("merge")),
    }
}

/// The merge command for the forges whose CLI takes the options as flags. The head the person saw
/// goes with it, so the forge refuses a branch that has moved on.
fn merge_args(forge: Forge, id: &str, options: &MergePull, body: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    let mut push = |words: &[&str]| args.extend(words.iter().map(|word| (*word).to_owned()));
    if forge == Forge::GitHub {
        push(&["pr", "merge", id, &format!("--{}", options.method)]);
        if options.auto {
            push(&["--auto"]);
        }
        if options.delete_branch {
            push(&["--delete-branch"]);
        }
        if let Some(subject) = &options.subject {
            push(&["--subject", subject]);
        }
        if let Some(body) = body {
            push(&["--body", body]);
        }
        if let Some(head) = &options.expected_head {
            push(&["--match-head-commit", head]);
        }
    } else {
        push(&["mr", "merge", id, "--yes"]);
        match options.method.as_str() {
            "squash" => push(&["--squash"]),
            "rebase" => push(&["--rebase"]),
            _ => {}
        }
        if options.auto {
            push(&["--auto-merge"]);
        }
        if options.delete_branch {
            push(&["--remove-source-branch"]);
        }
        if let Some(body) = body {
            push(&["--message", body]);
        }
        // glab rebases before it merges, which moves the head the person saw: with --sha every rebase
        // merge would be refused as "changed". Whether it checks first is not documented, so a rebase
        // goes without the check.
        if options.method != "rebase"
            && let Some(head) = &options.expected_head
        {
            push(&["--sha", head]);
        }
    }
    args
}

/// Whether the service must read the head itself before a merge. GitHub and GitLab (except a rebase,
/// which moves the head) refuse a moved head on their own; the other forges have no such flag.
pub fn head_precheck_needed(forge: Forge, method: &str) -> bool {
    match forge {
        Forge::GitHub => false,
        Forge::GitLab => method == "rebase",
        Forge::Azure | Forge::Forgejo | Forge::Bitbucket => true,
    }
}

/// The head commit in a forge's own pull request record.
pub fn head_in(forge: Forge, record: &Value) -> Option<String> {
    let at = match forge {
        Forge::GitHub => "/headRefOid",
        Forge::GitLab => "/sha",
        Forge::Azure => "/lastMergeSourceCommit/commitId",
        Forge::Forgejo => "/head/sha",
        Forge::Bitbucket => return None,
    };
    record
        .pointer(at)
        .and_then(Value::as_str)
        .filter(|sha| !sha.is_empty())
        .map(str::to_owned)
}

/// The pull request's head commit now, from a read that goes to the forge every time.
pub async fn current_head(forge: Forge, cwd: &Path, number: u64) -> Result<String> {
    let id = number.to_string();
    let record = match forge {
        Forge::GitHub => {
            call_json(forge, cwd, &["pr", "view", &id, "--json", "headRefOid"]).await?
        }
        Forge::GitLab => call_json(forge, cwd, &["mr", "view", &id, "--output", "json"]).await?,
        Forge::Azure => {
            call_json(
                forge,
                cwd,
                &[
                    "repos", "pr", "show", "--id", &id, "--detect", "true", "--output", "json",
                ],
            )
            .await?
        }
        // The list gives only the branch's name, so the one pull request is read through the API.
        Forge::Forgejo => {
            call_json(
                forge,
                cwd,
                &["api", &format!("repos/{{owner}}/{{repo}}/pulls/{id}")],
            )
            .await?
        }
        Forge::Bitbucket => return Err(forge.unsupported("merge")),
    };
    head_in(forge, &record).ok_or_else(unreadable_head)
}

fn unreadable_head() -> CoreError {
    CoreError::Git(
        "Couldn't read the pull request's current head, so it was not merged. Try again.".into(),
    )
}

/// Compares what the person saw with what the forge says now. A head that cannot be read refuses
/// the merge: it is never attempted unguarded.
pub fn check_head(expected: &str, current: Result<String>) -> Result<()> {
    match current {
        Ok(now) if now == expected => Ok(()),
        Ok(_) => Err(CoreError::PullHeadChanged),
        Err(CoreError::Unsupported(message)) => Err(CoreError::Unsupported(message)),
        Err(_) => Err(unreadable_head()),
    }
}

/// True when a forge's refusal says the head moved after the person looked, by the phrases that forge uses.
pub fn head_changed(forge: Forge, message: &str) -> bool {
    let message = message.to_lowercase();
    match forge {
        // GitHub: "Head branch was modified. Review and try the merge again."
        Forge::GitHub => message.contains("head branch was modified"),
        // GitLab: "SHA does not match HEAD of source branch"
        Forge::GitLab => message.contains("sha does not match head of source branch"),
        _ => false,
    }
}

/// A merge the forge refused because the head moved is its own error, not a generic failure.
fn head_moved(forge: Forge, error: CoreError) -> CoreError {
    match &error {
        CoreError::Git(message) if head_changed(forge, message) => CoreError::PullHeadChanged,
        _ => error,
    }
}

/// Removes the lines coding agents add to a message, and the blank tail they leave.
pub fn strip_agent_credits(message: &str) -> String {
    let is_credit = |line: &str| {
        let lower = line.trim().to_lowercase();
        let agent = [
            "claude",
            "codex",
            "copilot",
            "cursor",
            "openai",
            "anthropic",
            "orbyn",
        ]
        .iter()
        .any(|name| lower.contains(name));
        (lower.starts_with("co-authored-by:") && agent)
            || (lower.contains("generated with") && agent)
    };
    let kept: Vec<&str> = message.lines().filter(|line| !is_credit(line)).collect();
    kept.join("\n").trim_end().to_owned()
}

pub async fn pull_action(
    forge: Forge,
    cwd: &Path,
    number: u64,
    action: &str,
    head: &str,
) -> Result<()> {
    let id = number.to_string();
    let args: Vec<String> = match (forge, action) {
        (Forge::GitHub, "close") => vec!["pr".into(), "close".into(), id],
        (Forge::GitHub, "reopen") => vec!["pr".into(), "reopen".into(), id],
        (Forge::GitHub, "revert") => vec!["pr".into(), "revert".into(), id],
        (Forge::GitHub, "update_branch") => vec!["pr".into(), "update-branch".into(), id],
        (Forge::GitHub, "rebase") => {
            vec!["pr".into(), "update-branch".into(), id, "--rebase".into()]
        }
        (Forge::GitHub, "delete_branch") => vec![
            "api".into(),
            "-X".into(),
            "DELETE".into(),
            format!("repos/{{owner}}/{{repo}}/git/refs/heads/{head}"),
        ],
        (Forge::GitLab, "close") => vec!["mr".into(), "close".into(), id],
        (Forge::GitLab, "reopen") => vec!["mr".into(), "reopen".into(), id],
        (Forge::GitLab, "rebase") => vec!["mr".into(), "rebase".into(), id],
        (Forge::Azure, "close") => vec![
            "repos".into(),
            "pr".into(),
            "update".into(),
            "--id".into(),
            id,
            "--detect".into(),
            "true".into(),
            "--status".into(),
            "abandoned".into(),
        ],
        (Forge::Azure, "reopen") => vec![
            "repos".into(),
            "pr".into(),
            "update".into(),
            "--id".into(),
            id,
            "--detect".into(),
            "true".into(),
            "--status".into(),
            "active".into(),
        ],
        (Forge::Forgejo, "close") => vec!["pulls".into(), "close".into(), id],
        (Forge::Forgejo, "reopen") => vec!["pulls".into(), "reopen".into(), id],
        (_, "close" | "reopen" | "revert" | "update_branch" | "rebase" | "delete_branch") => {
            return Err(forge.unsupported(&format!("{} this", action.replace('_', " "))));
        }
        _ => return Err(CoreError::Invalid("Unknown pull request action.".into())),
    };
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    call(forge, cwd, &args).await.map(|_| ())
}

pub async fn create_pull(
    forge: Forge,
    cwd: &Path,
    branch: &str,
    title: &str,
    body: &str,
    base: Option<&str>,
) -> Result<String> {
    let mut args: Vec<&str> = match forge {
        Forge::GitHub => vec![
            "pr", "create", "--title", title, "--body", body, "--head", branch,
        ],
        Forge::GitLab => vec![
            "mr",
            "create",
            "--title",
            title,
            "--description",
            body,
            "--source-branch",
            branch,
            "--yes",
        ],
        Forge::Azure => vec![
            "repos",
            "pr",
            "create",
            "--detect",
            "true",
            "--title",
            title,
            "--description",
            body,
            "--source-branch",
            branch,
            "--output",
            "json",
        ],
        Forge::Forgejo => vec![
            "pulls",
            "create",
            "--title",
            title,
            "--description",
            body,
            "--head",
            branch,
        ],
        Forge::Bitbucket => return Err(forge.unsupported("open pull requests")),
    };
    if let Some(base) = base {
        args.push(match forge {
            Forge::GitHub | Forge::Forgejo => "--base",
            Forge::GitLab => "--target-branch",
            Forge::Azure | Forge::Bitbucket => "--target-branch",
        });
        args.push(base);
    }
    let output = call(forge, cwd, &args).await?;
    if forge == Forge::Azure {
        let value: Value = serde_json::from_str(&output)?;
        return Ok(format!("azure:{}", id_of(&value, "pullRequestId")));
    }
    Ok(output
        .lines()
        .rfind(|line| line.trim_start().starts_with("http"))
        .unwrap_or(&output)
        .trim()
        .to_owned())
}

pub fn number_from_url(url: &str) -> Option<u64> {
    url.trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()?
        .parse()
        .ok()
}

/// Creates a remote from a local repository with this forge's CLI and pushes.
pub async fn publish(forge: Forge, cwd: &Path, name: &str, private: bool) -> Result<()> {
    let visibility = if private { "--private" } else { "--public" };
    match forge {
        Forge::GitHub => call(
            forge,
            cwd,
            &[
                "repo", "create", name, visibility, "--source", ".", "--push",
            ],
        )
        .await
        .map(|_| ()),
        Forge::GitLab => call(
            forge,
            cwd,
            &[
                "repo",
                "create",
                name,
                visibility,
                "--defaultBranch",
                "main",
            ],
        )
        .await
        .map(|_| ()),
        _ => Err(forge.unsupported("publish a repository")),
    }
}

/// What the CLI says about itself, without touching a token.
pub async fn forge_status(forge: Forge) -> orbyn_contracts::ForgeStatus {
    let cwd = std::env::temp_dir();
    let mut status = orbyn_contracts::ForgeStatus {
        id: forge.id().to_owned(),
        name: forge.name().to_owned(),
        program: forge.program().map(str::to_owned),
        installed: false,
        version: None,
        signed_in: false,
        accounts: Vec::new(),
        viewer: None,
        problem: None,
    };
    let Some(program) = forge.program() else {
        status.problem =
            Some("Bitbucket has no command line tool, and Orbyn holds no forge tokens.".into());
        return status;
    };
    if resolve(program).is_none() {
        status.problem = Some(format!("Install {program} and sign in to it to use this."));
        return status;
    }
    status.installed = true;
    let version_flag: &[&str] = if forge == Forge::Azure {
        &["version", "--output", "json"]
    } else {
        &["--version"]
    };
    if let Ok(output) = spawn(forge, &cwd, version_flag, None).await {
        status.version = output.stdout.lines().next().map(str::to_owned);
    }
    let auth: &[&str] = match forge {
        Forge::GitHub => &["auth", "status"],
        Forge::GitLab => &["auth", "status"],
        Forge::Azure => &["account", "show", "--output", "json"],
        Forge::Forgejo => &["login", "list", "--output", "json"],
        Forge::Bitbucket => return status,
    };
    if let Ok(output) = spawn(forge, &cwd, auth, None).await {
        status.signed_in = output.success;
        status.accounts = auth_accounts(&format!("{}\n{}", output.stdout, output.stderr));
        if output.success {
            status.viewer = viewer(forge, &cwd).await;
        }
    }
    status
}

/// Who the CLI is signed in as, so a watch can skip your own comments.
pub async fn viewer(forge: Forge, cwd: &Path) -> Option<String> {
    let args: &[&str] = match forge {
        Forge::GitHub | Forge::GitLab => &["api", "user"],
        _ => return None,
    };
    person(&call_json(forge, cwd, args).await.ok()?)
}

/// Picks `account NAME` out of `gh auth status`; tokens are never printed by it.
fn auth_accounts(output: &str) -> Vec<String> {
    let mut accounts = Vec::new();
    for line in output.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        if let Some(index) = words.iter().position(|word| *word == "account")
            && let Some(name) = words.get(index + 1)
        {
            let name = name.trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '_');
            if !name.is_empty() && !accounts.iter().any(|known| known == name) {
                accounts.push(name.to_owned());
            }
        }
    }
    accounts
}

#[cfg(test)]
mod tests;
