//! Reading pull requests: the list, one pull request, its checks, conversation, timeline, files and revisions.

use super::Forge;
use super::github::{
    GH_LIST_FIELDS, GH_VIEW_FIELDS, MARK_VIEWED_MUTATION, PULL_ID_QUERY, THREADS_QUERY,
    UNMARK_VIEWED_MUTATION, VIEWED_FILES_QUERY, github_comment, github_threads, parse_github_event,
    parse_github_file, parse_github_revision, parse_github_viewed_page, raw_github_checks,
};
use super::gitlab::{gitlab_conversation, parse_gitlab_file, parse_gitlab_revision};
use super::parse::{check_state, id_of, parse_pull, person};
use super::process::{call, call_json, call_pages};
use crate::error::{CoreError, Result};
use crate::util::json::{field, optional};
use orbyn_contracts::{
    PullCheck, PullConversation, PullFile, PullRequest, PullRequestDetail, PullRevision,
    PullTimelineEvent,
};
use serde_json::{Value, json};
use std::path::Path;

pub(super) const LIST_LIMIT: &str = "100";

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

pub(super) async fn forgejo_pull(cwd: &Path, number: u64) -> Result<Value> {
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

/// The run a GitHub check's link names: the digits after `/actions/runs/`, at most
/// fifteen of them. A link without one names no run.
pub fn run_id_of(url: &str) -> Option<u64> {
    let rest = url.split_once("/actions/runs/")?.1;
    let digits = rest.split(['/', '?', '#']).next()?;
    if digits.is_empty() || digits.len() > 15 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// The failed jobs' log of one run.
pub async fn check_log(forge: Forge, cwd: &Path, run_id: u64) -> Result<String> {
    if forge != Forge::GitHub {
        return Err(forge.unsupported("read a check log"));
    }
    let id = run_id.to_string();
    call(forge, cwd, &["run", "view", &id, "--log-failed"]).await
}

/// Re-runs the failed jobs of one run.
pub async fn rerun_failed(forge: Forge, cwd: &Path, run_id: u64) -> Result<()> {
    if forge != Forge::GitHub {
        return Err(forge.unsupported("re-run a check"));
    }
    let id = run_id.to_string();
    call(forge, cwd, &["run", "rerun", &id, "--failed"])
        .await
        .map(|_| ())
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

/// Pages read for one pull request's viewed state. A pull request over the cap is an error, never
/// a short answer, so no file is reported unviewed because GitHub was not read that far.
const VIEWED_PAGE_LIMIT: usize = 20;

/// GitHub's own viewed state for each changed file path, and the pull request's node id for the
/// mark mutations. Only GitHub keeps a viewed state to read.
pub async fn pull_viewed(
    forge: Forge,
    cwd: &Path,
    number: u64,
) -> Result<(String, std::collections::HashMap<String, bool>)> {
    if forge != Forge::GitHub {
        return Err(forge.unsupported("read viewed files"));
    }
    let number_field = format!("number={number}");
    let mut states = std::collections::HashMap::new();
    let mut after: Option<String> = None;
    for _ in 0..VIEWED_PAGE_LIMIT {
        let query = format!("query={VIEWED_FILES_QUERY}");
        let after_field = after.as_ref().map(|cursor| format!("after={cursor}"));
        let mut args = vec![
            "api",
            "graphql",
            "-F",
            "owner={owner}",
            "-F",
            "repo={repo}",
            "-F",
            number_field.as_str(),
            "-f",
            query.as_str(),
        ];
        if let Some(field) = after_field.as_deref() {
            args.push("-f");
            args.push(field);
        }
        let reply = call_json(forge, cwd, &args).await?;
        let page = parse_github_viewed_page(&reply).ok_or_else(|| {
            CoreError::Git("GitHub did not answer the viewed state of the files.".into())
        })?;
        states.extend(page.states);
        match page.next {
            Some(cursor) => after = Some(cursor),
            // Every page names the same pull request; the last page's id is the read's own.
            None => return Ok((page.pull_id, states)),
        }
    }
    Err(CoreError::Git(format!(
        "GitHub's viewed state has more than {VIEWED_PAGE_LIMIT} pages of files."
    )))
}

/// The pull request's node id, read on its own when the page has no files read to take it from.
pub async fn pull_node_id(forge: Forge, cwd: &Path, number: u64) -> Result<String> {
    if forge != Forge::GitHub {
        return Err(forge.unsupported("read a pull request's id"));
    }
    let number_field = format!("number={number}");
    let query = format!("query={PULL_ID_QUERY}");
    let reply = call_json(
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
    reply
        .pointer("/data/repository/pullRequest/id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| CoreError::Git("GitHub did not name the pull request.".into()))
}

/// Marks one file viewed or unviewed in the viewer's own GitHub state, through the mutation the
/// forge CLI runs. Only GitHub keeps a viewed state to write.
pub async fn set_file_viewed(
    forge: Forge,
    cwd: &Path,
    pull_id: &str,
    path: &str,
    viewed: bool,
) -> Result<()> {
    if forge != Forge::GitHub {
        return Err(forge.unsupported("mark files viewed"));
    }
    let mutation = if viewed {
        MARK_VIEWED_MUTATION
    } else {
        UNMARK_VIEWED_MUTATION
    };
    let query = format!("query={mutation}");
    let id = format!("id={pull_id}");
    let path = format!("path={path}");
    call(
        forge,
        cwd,
        &["api", "graphql", "-f", &query, "-f", &id, "-f", &path],
    )
    .await
    .map(|_| ())
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
