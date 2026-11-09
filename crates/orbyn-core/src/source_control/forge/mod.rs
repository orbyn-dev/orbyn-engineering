//! Forge reads and writes through each forge's own CLI, so a login stays the
//! user's and Orbyn holds no credential. No shell parses an argument. GitHub and
//! GitLab cover everything their APIs offer through `gh api` and `glab api`, Azure
//! DevOps and Forgejo cover the basics their CLIs have, and Bitbucket has no CLI to
//! go through, so it reports that instead of asking for a token.

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

use orbyn_contracts::{MergePull, UpdatePull};
use serde_json::Value;
use tokio::process::Command;

use crate::error::{CoreError, Result};
use crate::host::process::resolve;
use crate::source_control::git::{git, text};
pub use details::{
    DETAIL_FIELDS, DETAILS_ROW_LIMIT, DetailsOutcome, github_details, github_graphql_reset,
    graphql_reset,
};
pub use parse::parse_pull;
use parse::{id_of, person};
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

pub async fn update_pull(forge: Forge, cwd: &Path, number: u64, update: &UpdatePull) -> Result<()> {
    let id = number.to_string();
    if update
        .base
        .as_deref()
        .is_some_and(|base| base.starts_with('-'))
    {
        return Err(CoreError::Invalid(
            "That is not a valid branch name.".into(),
        ));
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
fn merge_args(forge: Forge, id: &str, options: &MergePull, body: Option<&str>) -> Vec<String> {
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

fn unreadable_head() -> CoreError {
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
fn head_moved(forge: Forge, error: CoreError) -> CoreError {
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

pub async fn create_pull(
    forge: Forge,
    cwd: &Path,
    branch: &str,
    title: &str,
    body: &str,
    base: Option<&str>,
) -> Result<String> {
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
