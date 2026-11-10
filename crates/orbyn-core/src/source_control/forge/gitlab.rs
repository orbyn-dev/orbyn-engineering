//! GitLab specific: its discussion, file and revision parsing and emoji names.

use super::parse::{first_line, id_of, person};
use crate::util::json::{field, optional};
use orbyn_contracts::{PullComment, PullConversation, PullFile, PullRevision, PullThread};
use serde_json::Value;

pub(super) fn gitlab_comment(note: &Value) -> PullComment {
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

/// A discussion with one system note is activity, not a conversation.
pub(super) fn gitlab_conversation(discussions: &[Value]) -> PullConversation {
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

pub(super) fn parse_gitlab_file(file: &Value) -> PullFile {
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

pub(super) fn parse_gitlab_revision(commit: &Value) -> PullRevision {
    PullRevision {
        sha: optional(commit, "id").unwrap_or_default(),
        message: first_line(&optional(commit, "title").unwrap_or_default()),
        author: optional(commit, "author_name"),
        committed_at: optional(commit, "committed_date"),
    }
}

pub(super) fn gitlab_emoji(content: &str) -> &str {
    match content {
        "+1" => "thumbsup",
        "-1" => "thumbsdown",
        "laugh" => "laughing",
        "hooray" => "tada",
        other => other,
    }
}
