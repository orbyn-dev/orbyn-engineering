//! Forge reads and writes through each forge's own CLI, so a login stays the
//! user's and Orbyn holds no credential. No shell parses an argument. GitHub and
//! GitLab cover everything their APIs offer through `gh api` and `glab api`, Azure
//! DevOps and Forgejo cover the basics their CLIs have, and Bitbucket has no CLI to
//! go through, so it reports that instead of asking for a token.

mod create;

mod merge;

mod review;

mod pulls;

mod details;

mod gitlab;

mod github;

mod parse;

mod process;

pub(crate) mod cache;

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

use crate::error::{CoreError, Result};
use crate::host::process::resolve;
use crate::source_control::git::{git, text};
pub use create::{create_pull, number_from_url, publish};
pub use details::{
    DETAIL_FIELDS, DETAILS_ROW_LIMIT, DetailsOutcome, github_details, github_graphql_reset,
    graphql_reset,
};
pub use merge::{
    check_head, current_head, head_changed, head_in, head_precheck_needed, merge, pull_action,
    strip_agent_credits, update_pull,
};
pub use parse::parse_pull;
use parse::person;
pub use process::{call, no_such_pull};
use process::{call_json, spawn};
pub use pulls::{
    PullFilter, approve_workflow, conversation, fetch_pull, list_pulls, pull_checks, pull_detail,
    pull_files, pull_revisions, revision_files, timeline,
};
pub use review::{
    add_comment, edit_comment, label_candidates, react, reviewer_candidates, set_labels,
    set_reviewers, set_thread_resolved, submit_review,
};

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

#[cfg(test)]
mod revision_tests;

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
