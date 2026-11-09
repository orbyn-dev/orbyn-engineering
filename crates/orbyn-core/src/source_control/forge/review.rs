//! Reviewing a pull request: comments, reactions, review submissions, reviewers and labels.

use super::Forge;
use super::github::{RESOLVE_MUTATION, UNRESOLVE_MUTATION};
use super::gitlab::gitlab_emoji;
use super::parse::{field, optional};
use super::process::{call, call_input, call_json, call_pages};
use super::pulls::pull_detail;
use crate::error::{CoreError, Result};
use orbyn_contracts::{
    AddPullComment, PullCandidate, ReviewLineComment, SetPullPeople, SubmitReview,
};
use serde_json::{Value, json};
use std::path::Path;

pub(super) fn is_github_review(kind: &str) -> bool {
    kind == "review"
}

pub(super) async fn head_sha(forge: Forge, cwd: &Path, number: u64) -> Result<(String, Value)> {
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
pub(super) fn review_payload(event: &str, body: &str, comments: &[ReviewLineComment]) -> Value {
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

pub(super) fn joined(names: &[String]) -> Option<String> {
    (!names.is_empty()).then(|| names.join(","))
}

pub(super) fn valid_names(names: &[String]) -> Result<()> {
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
