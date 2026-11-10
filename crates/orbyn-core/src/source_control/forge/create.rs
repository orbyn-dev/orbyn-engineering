//! Opening a pull request and publishing a repository through the forge CLI.

use super::Forge;
use super::parse::id_of;
use super::process::call;
use crate::error::{CoreError, Result};
use serde_json::Value;
use std::path::Path;

pub(super) fn check_branch_operand(name: &str) -> Result<()> {
    if name.trim().is_empty() || name.starts_with('-') || name.chars().any(char::is_control) {
        return Err(CoreError::Invalid(
            "That is not a valid branch name.".into(),
        ));
    }
    Ok(())
}

pub async fn create_pull(
    forge: Forge,
    cwd: &Path,
    branch: &str,
    title: &str,
    body: &str,
    base: Option<&str>,
) -> Result<String> {
    check_branch_operand(branch)?;
    if let Some(base) = base {
        check_branch_operand(base)?;
    }
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
