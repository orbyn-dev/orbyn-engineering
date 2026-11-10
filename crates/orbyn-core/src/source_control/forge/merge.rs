//! Changing and merging a pull request, and the head check that keeps a merge on the head the person saw.

use super::Forge;
use super::create::check_branch_operand;
use super::process::{call, call_json};
use crate::error::{CoreError, Result};
use orbyn_contracts::{MergePull, UpdatePull};
use serde_json::Value;
use std::path::Path;

pub async fn update_pull(forge: Forge, cwd: &Path, number: u64, update: &UpdatePull) -> Result<()> {
    let id = number.to_string();
    if let Some(base) = update.base.as_deref() {
        check_branch_operand(base)?;
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
pub(super) fn merge_args(
    forge: Forge,
    id: &str,
    options: &MergePull,
    body: Option<&str>,
) -> Vec<String> {
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

pub(super) fn unreadable_head() -> CoreError {
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
pub(super) fn head_moved(forge: Forge, error: CoreError) -> CoreError {
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
