//! GitHub specific: its field lists, GraphQL review thread queries and answer parsing.

use super::Forge;
use super::parse::{first_line, id_of, person};
use super::process::spawn;
use crate::util::json::{field, optional};
use orbyn_contracts::{PullComment, PullFile, PullRevision, PullThread, PullTimelineEvent};
use serde_json::{Value, json};
use std::path::Path;

pub(super) const GH_LIST_FIELDS: &str = "number,title,url,state,headRefName,baseRefName,author,isDraft,\
updatedAt,labels,reviewDecision,statusCheckRollup";

pub(super) const GH_VIEW_FIELDS: &str = "number,title,url,state,headRefName,baseRefName,author,isDraft,\
updatedAt,labels,reviewDecision,statusCheckRollup,body,comments,reviews,mergeable,reviewRequests,\
headRefOid,baseRefOid,additions,deletions,changedFiles,maintainerCanModify,autoMergeRequest";

pub(super) const THREADS_QUERY: &str = "query($owner:String!,$repo:String!,$number:Int!){\
repository(owner:$owner,name:$repo){pullRequest(number:$number){reviewThreads(first:100){nodes{\
id isResolved path line comments(first:100){nodes{databaseId body createdAt author{login}}}}}}}}";

/// A reply inside one review thread, named by its GraphQL node id. Its variable names are the
/// `-f` fields `gh api graphql` binds them from, so the declared names and the call must agree.
pub(super) const ADD_THREAD_REPLY_MUTATION: &str = "mutation($pullRequestReviewThreadId:ID!,$body:String!){\
addPullRequestReviewThreadReply(input:{pullRequestReviewThreadId:$pullRequestReviewThreadId,body:$body}){comment{id}}}";

pub(super) const RESOLVE_MUTATION: &str =
    "mutation($id:ID!){resolveReviewThread(input:{threadId:$id}){thread{id}}}";

pub(super) const UNRESOLVE_MUTATION: &str =
    "mutation($id:ID!){unresolveReviewThread(input:{threadId:$id}){thread{id}}}";

/// The pull request's own node id, which every mark mutation names.
pub(super) const PULL_ID_QUERY: &str = "query ViewedId($owner:String!,$repo:String!,$number:Int!){\
repository(owner:$owner,name:$repo){pullRequest(number:$number){id}}}";

/// One page of a pull request's own node id and its changed files' viewed state. `after` is
/// passed only for a later page, so the first request carries no cursor.
pub(super) const VIEWED_FILES_QUERY: &str = "query ViewedFiles($owner:String!,$repo:String!,$number:Int!,$after:String){\
repository(owner:$owner,name:$repo){pullRequest(number:$number){id files(first:100,after:$after){\
pageInfo{hasNextPage endCursor} nodes{path viewerViewedState}}}}}";

/// The viewer's own viewed mark for one file, set or cleared through GitHub.
pub(super) const MARK_VIEWED_MUTATION: &str = "mutation MarkFileAsViewed($id:ID!,$path:String!){\
markFileAsViewed(input:{pullRequestId:$id,path:$path}){pullRequest{id}}}";

pub(super) const UNMARK_VIEWED_MUTATION: &str = "mutation UnmarkFileAsViewed($id:ID!,$path:String!){\
unmarkFileAsViewed(input:{pullRequestId:$id,path:$path}){pullRequest{id}}}";

/// One page of GitHub's viewed state: the pull request's id, each file's path and whether the
/// viewer marked it viewed, and the cursor of a later page when GitHub names one.
pub(super) struct ViewedPage {
    pub(super) pull_id: String,
    pub(super) states: Vec<(String, bool)>,
    pub(super) next: Option<String>,
}

/// A reply that names no pull request, or no files connection, is not read as "nothing is viewed".
pub(super) fn parse_github_viewed_page(reply: &Value) -> Option<ViewedPage> {
    let pull = reply.pointer("/data/repository/pullRequest")?;
    let pull_id = optional(pull, "id")?;
    let files = pull.get("files")?;
    let nodes = files.get("nodes")?.as_array()?;
    let states = nodes
        .iter()
        .filter_map(|node| {
            let path = optional(node, "path")?;
            Some((path, field(node, "viewerViewedState") == "VIEWED"))
        })
        .collect();
    let more = files
        .pointer("/pageInfo/hasNextPage")
        .and_then(Value::as_bool)?;
    let next = if more {
        // A page GitHub says continues but names no cursor is a broken answer, not the last page.
        Some(
            files
                .pointer("/pageInfo/endCursor")
                .and_then(Value::as_str)?
                .to_owned(),
        )
    } else {
        None
    };
    Some(ViewedPage {
        pull_id,
        states,
        next,
    })
}

/// `gh pr checks` exits non-zero while checks fail or wait, and still prints them.
pub(super) async fn raw_github_checks(cwd: &Path, id: &str) -> Value {
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

pub(super) fn github_comment(value: &Value, kind: &str) -> PullComment {
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

/// Splits a GraphQL `reviewThreads` reply into threads.
pub(super) fn github_threads(reply: &Value) -> Vec<PullThread> {
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

/// One GitHub timeline event. A `committed` event has no `id`, `actor` or `created_at`: it carries the commit's `sha`,
/// its `author` and `committer` (a name and a date) and its `message`, so those stand in for them.
pub(super) fn parse_github_event(event: &Value) -> PullTimelineEvent {
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

pub(super) fn parse_github_file(file: &Value) -> PullFile {
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
        viewed_synced: false,
    }
}

pub(super) fn parse_github_revision(commit: &Value) -> PullRevision {
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
