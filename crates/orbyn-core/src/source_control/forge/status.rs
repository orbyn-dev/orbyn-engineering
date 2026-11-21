//! Whether a forge CLI is installed and signed in, who the signed-in person is, and
//! what that person's own CLI says they starred.

use super::Forge;
use super::parse::person;
use super::process::{call_json, spawn};
use crate::host::process::resolve;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

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
pub(super) fn auth_accounts(output: &str) -> Vec<String> {
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

/// One page of the person's starred repositories: the most a read returns.
const STARRED_PAGE: usize = 100;
/// A starred-list read stays short: a CLI that hangs must not hold a request.
const STARRED_TIMEOUT: Duration = Duration::from_secs(10);

impl crate::app::Core {
    /// The signed-in person's starred GitHub repositories through their own CLI. This
    /// is the same `/user/starred` resource the fixed star check reads, one page of
    /// it. A missing tool, a signed-out login or a failed read is `unavailable`,
    /// never an empty list.
    pub async fn starred_repositories(&self) -> orbyn_contracts::StarredRepositories {
        starred_repositories(&self.config.data_dir).await
    }
}

async fn starred_repositories(cwd: &Path) -> orbyn_contracts::StarredRepositories {
    let page = format!("user/starred?per_page={STARRED_PAGE}");
    let args = ["api", page.as_str(), "--hostname", "github.com"];
    let output =
        match tokio::time::timeout(STARRED_TIMEOUT, spawn(Forge::GitHub, cwd, &args, None)).await {
            Err(_) => return unavailable("The GitHub CLI did not answer in time."),
            Ok(Err(error)) => {
                let reason = match error {
                    crate::error::CoreError::Unsupported(message)
                    | crate::error::CoreError::Git(message) => message,
                    _ => "The GitHub CLI could not start. Check that it is installed.".to_owned(),
                };
                return unavailable(&reason);
            }
            Ok(Ok(output)) if !output.success => {
                let reason = crate::util::tail(&output.stderr, 1024);
                let reason = reason.trim();
                return unavailable(if reason.is_empty() {
                    "The GitHub CLI could not read your starred repositories."
                } else {
                    reason
                });
            }
            Ok(Ok(output)) => output,
        };
    let rows = match serde_json::from_str::<serde_json::Value>(&output.stdout) {
        Ok(serde_json::Value::Array(rows)) => rows,
        _ => {
            return unavailable(
                "The GitHub CLI answered with something other than a starred list.",
            );
        }
    };
    // A full page may hide older stars: say so rather than present a partial list as all.
    let truncated = rows.len() >= STARRED_PAGE;
    orbyn_contracts::StarredRepositories {
        repositories: rows.iter().filter_map(starred_repository).collect(),
        truncated,
        unavailable: None,
    }
}

fn unavailable(reason: &str) -> orbyn_contracts::StarredRepositories {
    orbyn_contracts::StarredRepositories {
        repositories: Vec::new(),
        truncated: false,
        unavailable: Some(reason.to_owned()),
    }
}

/// One row of the forge's starred list. A row the CLI printed without the name or
/// address the app shows is left out rather than shown half empty.
fn starred_repository(row: &serde_json::Value) -> Option<orbyn_contracts::StarredRepository> {
    let full_name = row["full_name"].as_str().filter(|name| !name.is_empty())?;
    let (owner, name) = full_name.split_once('/')?;
    if owner.is_empty() || name.is_empty() || name.contains('/') {
        return None;
    }
    let url = row["html_url"].as_str().filter(|url| !url.is_empty())?;
    Some(orbyn_contracts::StarredRepository {
        owner: owner.to_owned(),
        name: name.to_owned(),
        full_name: full_name.to_owned(),
        url: url.to_owned(),
        description: row["description"].as_str().map(str::to_owned),
        private: row["private"].as_bool().unwrap_or_default(),
        archived: row["archived"].as_bool().unwrap_or_default(),
        default_branch: row["default_branch"].as_str().map(str::to_owned),
        language: row["language"].as_str().map(str::to_owned),
        stars: u32::try_from(row["stargazers_count"].as_u64().unwrap_or(0)).unwrap_or(u32::MAX),
    })
}

#[cfg(test)]
mod starred_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_starred_row_needs_the_name_and_address_the_app_shows() {
        assert!(starred_repository(&json!({"full_name": "o/r"})).is_none());
        assert!(
            starred_repository(&json!({"full_name": "o", "html_url": "https://github.com/o"}))
                .is_none()
        );
        let row = starred_repository(&json!({
            "full_name": "o/r",
            "html_url": "https://github.com/o/r",
            "description": null,
            "private": true,
            "archived": false,
            "default_branch": "main",
            "language": null,
            "stargazers_count": 9_000_000_000u64,
        }))
        .unwrap();
        assert_eq!(row.owner, "o");
        assert_eq!(row.name, "r");
        assert_eq!(row.stars, u32::MAX);
        assert!(row.private && !row.archived);
        assert!(row.description.is_none() && row.language.is_none());
        assert_eq!(row.default_branch.as_deref(), Some("main"));
    }
}
