//! Running one forge CLI call: spawn, timeout and output.

use super::Forge;
use crate::error::{CoreError, Result};
use crate::host::process::resolve;
use crate::source_control::git::text;
use crate::util::tail;
use serde_json::Value;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

pub(super) const FORGE_TIMEOUT: Duration = Duration::from_secs(120);

/// What one CLI call produced. Some commands report through their exit code.
pub(super) struct Output {
    pub(super) success: bool,
    pub(super) stdout: String,
    pub(super) stderr: String,
}

pub(super) async fn spawn(
    forge: Forge,
    cwd: &Path,
    args: &[&str],
    input: Option<&str>,
) -> Result<Output> {
    let program = forge.program().ok_or_else(|| {
        CoreError::Unsupported(format!(
            "{} has no command line tool, and Orbyn does not hold forge tokens.",
            forge.name()
        ))
    })?;
    let command = resolve(program).ok_or_else(|| {
        CoreError::Unsupported(format!("Install {program} and sign in to it to use this."))
    })?;
    let mut builder = Command::new(&command.program);
    builder
        .args(&command.prefix)
        .args(args)
        .current_dir(cwd)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GLAB_NO_PROMPT", "1")
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    crate::host::child_env::for_tool(builder.as_std_mut());
    let output = tokio::time::timeout(FORGE_TIMEOUT, run_owned(builder, input))
        .await
        .map_err(|_| CoreError::Git(format!("{program} did not finish in time.")))?
        .map_err(|error| CoreError::Git(format!("Could not run {program}: {error}")))?;
    Ok(Output {
        success: output.status.success(),
        stdout: text(&output.stdout),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

/// Runs a forge CLI as the owner of its whole process tree. A CLI such as `az`
/// leaves helpers running after it exits; they end with it, and they end when
/// the service stops or the request is cancelled.
pub(super) async fn run_owned(
    builder: Command,
    input: Option<&str>,
) -> Result<std::process::Output> {
    let mut child = crate::host::process::ManagedChild::spawn(builder)?;
    if let (Some(input), Some(mut stdin)) = (input, child.child_mut().stdin.take()) {
        stdin.write_all(input.as_bytes()).await?;
    }
    child.output().await
}

/// True when a forge CLI's failure says the pull request does not exist, by the phrases that forge
/// uses for it. Anything else (auth, network, rate limit, a server error) is not matched.
pub fn no_such_pull(forge: Forge, message: &str) -> bool {
    let message = message.to_lowercase();
    let any = |phrases: &[&str]| phrases.iter().any(|phrase| message.contains(phrase));
    match forge {
        Forge::GitHub => any(&["could not resolve to a pullrequest with the number of"]),
        // A bare 404 may be a proxy's page; GitLab's own answer names the merge request it looked for.
        Forge::GitLab => {
            (message.contains("merge_requests/") && message.contains("404 not found"))
                || any(&["merge request not found"])
        }
        Forge::Azure => any(&["tf401180", "pull request does not exist"]),
        Forge::Forgejo | Forge::Bitbucket => false,
    }
}

pub async fn call(forge: Forge, cwd: &Path, args: &[&str]) -> Result<String> {
    let output = spawn(forge, cwd, args, None).await?;
    if output.success {
        Ok(output.stdout)
    } else {
        Err(CoreError::Git(tail(&output.stderr, 1024)))
    }
}

pub(super) async fn call_input(
    forge: Forge,
    cwd: &Path,
    args: &[&str],
    input: &str,
) -> Result<String> {
    let output = spawn(forge, cwd, args, Some(input)).await?;
    if output.success {
        Ok(output.stdout)
    } else {
        Err(CoreError::Git(tail(&output.stderr, 1024)))
    }
}

pub(super) async fn call_json(forge: Forge, cwd: &Path, args: &[&str]) -> Result<Value> {
    Ok(serde_json::from_str(&call(forge, cwd, args).await?)?)
}

/// `--paginate` prints one array per page, so they are read one after another.
pub(super) async fn call_pages(forge: Forge, cwd: &Path, args: &[&str]) -> Result<Vec<Value>> {
    let output = call(forge, cwd, args).await?;
    let mut items = Vec::new();
    for page in serde_json::Deserializer::from_str(&output).into_iter::<Value>() {
        match page? {
            Value::Array(list) => items.extend(list),
            other => items.push(other),
        }
    }
    Ok(items)
}
