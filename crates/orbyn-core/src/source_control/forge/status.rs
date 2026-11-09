//! Whether a forge CLI is installed and signed in, and who the signed-in person is.

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
