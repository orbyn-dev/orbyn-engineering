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

pub(super) const RESOLVE_MUTATION: &str =
    "mutation($id:ID!){resolveReviewThread(input:{threadId:$id}){thread{id}}}";

pub(super) const UNRESOLVE_MUTATION: &str =
    "mutation($id:ID!){unresolveReviewThread(input:{threadId:$id}){thread{id}}}";

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
