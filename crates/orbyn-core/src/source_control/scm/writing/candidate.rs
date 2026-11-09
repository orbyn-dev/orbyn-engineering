//! Reading the checkout for a writing request: bounded, owned Git reads and the change they add up to.

use super::invalid;
use crate::error::{CoreError, Result};
use crate::host::process::ManagedChild;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

pub(super) const CONTEXT_BYTES: usize = 96 * 1024;

pub(super) const FILE_BYTES: u64 = 2 * 1024 * 1024;

pub(super) const READ_TIMEOUT: Duration = Duration::from_secs(20);

/// Reads have a process-tree owner, deadline and hard output cap. Neither local
/// attributes nor repository configuration can execute diff programs.
pub(super) async fn read_git(cwd: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let attributes = if args.first() == Some(&"diff") {
        let info = utf8(
            read_git_bounded(cwd, &["rev-parse", "--git-path", "info/attributes"], None).await?,
        )?;
        let info = cwd.join(info.trim_end_matches(['\r', '\n']));
        match tokio::fs::metadata(info).await {
            Ok(meta) if meta.len() != 0 => {
                return Err(invalid(
                    "Local Git attributes may run filters. Enter text manually.",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Some(
            utf8(read_git_bounded(cwd, &["hash-object", "-t", "tree", "--stdin"], None).await?)?
                .trim()
                .to_owned(),
        )
    } else {
        None
    };
    read_git_bounded(cwd, args, attributes.as_deref()).await
}

pub(super) async fn read_git_bounded(
    cwd: &Path,
    args: &[&str],
    attributes: Option<&str>,
) -> Result<Vec<u8>> {
    let mut command = Command::new("git");
    command
        .args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "diff.external=",
            "-c",
            "core.pager=cat",
            "-c",
            "core.attributesFile=",
        ])
        .args(args)
        .current_dir(cwd)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ATTR_NOSYSTEM", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(attributes) = attributes {
        command.env("GIT_ATTR_SOURCE", attributes);
    }
    crate::host::child_env::for_tool(command.as_std_mut());
    let mut child = ManagedChild::spawn(command)?;
    let stdout = child
        .child_mut()
        .stdout
        .take()
        .ok_or_else(|| invalid("Missing Git output."))?;
    let stderr = child
        .child_mut()
        .stderr
        .take()
        .ok_or_else(|| invalid("Missing Git error output."))?;
    let work = async {
        let mut output = Vec::new();
        let mut error = Vec::new();
        let mut stdout = stdout.take((CONTEXT_BYTES + 1) as u64);
        let mut stderr = stderr.take(2049);
        // Either stream over its cap ends the read at once: Git would otherwise
        // block on the unread rest of the pipe until the deadline.
        let too_large =
            || invalid("This change is too large to generate text. Enter text manually.");
        tokio::try_join!(
            async {
                stdout.read_to_end(&mut output).await?;
                if output.len() > CONTEXT_BYTES {
                    return Err(too_large());
                }
                Ok(())
            },
            async {
                stderr.read_to_end(&mut error).await?;
                if error.len() > 2048 {
                    return Err(too_large());
                }
                Ok::<_, CoreError>(())
            }
        )?;
        if !child.wait().await?.success() {
            return Err(CoreError::Git(
                String::from_utf8_lossy(&error).trim().into(),
            ));
        }
        Ok(output)
    };
    tokio::time::timeout(READ_TIMEOUT, work)
        .await
        .map_err(|_| invalid("Reading this change timed out. Enter text manually."))?
}

/// The major and minor version in `git version` output, such as
/// `git version 2.39.3 (Apple Git-146)` or `git version 2.45.1.windows.1`.
pub(super) fn git_version(output: &str) -> Option<(u32, u32)> {
    let version = output.trim().strip_prefix("git version ")?;
    let mut parts = version.split(|c: char| !c.is_ascii_digit());
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

/// `GIT_ATTR_SOURCE` arrived in Git 2.40. Older Git ignores it, so the working
/// tree's `.gitattributes` could still apply clean filters from local config
/// while the diff is read; generation is refused there instead.
pub(super) async fn require_attr_source(cwd: &Path) -> Result<()> {
    let output = utf8(read_git_bounded(cwd, &["version"], None).await?)?;
    match git_version(&output) {
        Some(version) if version >= (2, 40) => Ok(()),
        _ => Err(invalid(
            "Generating text needs Git 2.40 or later. Enter text manually.",
        )),
    }
}

pub(super) fn utf8(bytes: Vec<u8>) -> Result<String> {
    String::from_utf8(bytes)
        .map_err(|_| invalid("This change contains unsupported text. Enter text manually."))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Candidate {
    pub(super) identity: Vec<u8>,
    pub(super) context: String,
}

pub(super) async fn head(cwd: &Path) -> Result<Option<String>> {
    match read_git(cwd, &["rev-parse", "--verify", "--quiet", "HEAD"]).await {
        Ok(value) => Ok(Some(utf8(value)?.trim().to_owned())),
        Err(CoreError::Git(message)) if message.is_empty() => {
            let target = utf8(read_git(cwd, &["symbolic-ref", "HEAD"]).await?)?;
            let target = target.trim();
            read_git(cwd, &["check-ref-format", target]).await?;
            match read_git(cwd, &["show-ref", "--verify", "--quiet", target]).await {
                Err(CoreError::Git(message)) if message.is_empty() => {
                    let path = utf8(read_git(cwd, &["rev-parse", "--git-path", target]).await?)?;
                    match tokio::fs::symlink_metadata(cwd.join(path.trim_end_matches(['\r', '\n'])))
                        .await
                    {
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                        Err(error) => Err(error.into()),
                        Ok(_) => Err(invalid(
                            "HEAD points to an unreadable branch reference. Enter text manually.",
                        )),
                    }
                }
                Err(error) => Err(error),
                Ok(_) => Err(invalid("HEAD could not be resolved. Enter text manually.")),
            }
        }
        Err(error) => Err(error),
    }
}

/// Canonical Git-effective pending modes, independent of staged/unstaged state.
/// With rename detection off every raw record has exactly one NUL-delimited path.
pub(super) async fn pending_modes(cwd: &Path, parent: &str) -> Result<BTreeMap<String, String>> {
    let mut args = vec![
        "diff",
        "--raw",
        "-z",
        "--no-abbrev",
        "--no-relative",
        "--no-renames",
        "--no-ext-diff",
        "--no-textconv",
    ];
    if parent.is_empty() {
        args.push("--cached");
    } else {
        args.push(parent);
    }
    args.push("--");
    let raw = utf8(read_git(cwd, &args).await?)?;
    let mut fields = raw.split_terminator('\0');
    let mut modes = BTreeMap::new();
    while let Some(header) = fields.next() {
        let mode = header
            .split_whitespace()
            .nth(1)
            .ok_or_else(|| invalid("Invalid Git mode output."))?;
        let path = fields
            .next()
            .ok_or_else(|| invalid("Invalid Git path output."))?;
        modes.insert(path.to_owned(), mode.to_owned());
    }
    Ok(modes)
}

/// The symbolic branch name, or a fixed marker on a detached HEAD, where
/// `symbolic-ref --quiet` exits 1 with no output.
pub(super) async fn symbolic_head(cwd: &Path) -> Result<Vec<u8>> {
    match read_git(cwd, &["symbolic-ref", "--quiet", "HEAD"]).await {
        Err(CoreError::Git(message)) if message.is_empty() => Ok(b"detached HEAD".to_vec()),
        other => other,
    }
}

/// Fingerprint exactly the changed working files, including untracked files and
/// staged-only changes that add -A will replace. Never follows a symlink.
pub(super) async fn candidate(cwd: &Path) -> Result<Candidate> {
    let root = utf8(read_git(cwd, &["rev-parse", "--show-toplevel"]).await?)?;
    let cwd = Path::new(root.trim_end_matches(['\r', '\n']));
    let parent = head(cwd).await?.unwrap_or_default();
    let modes = pending_modes(cwd, &parent).await?;
    #[cfg(unix)]
    let filemode = utf8(
        read_git(
            cwd,
            &[
                "config",
                "--type=bool",
                "--default",
                "true",
                "--get",
                "core.filemode",
            ],
        )
        .await?,
    )?
    .trim()
        == "true";
    let paths = utf8(read_git(cwd, &["ls-files", "--others", "--exclude-standard", "-z"]).await?)?;
    let untracked: BTreeSet<&str> = paths.split_terminator('\0').collect();
    // The raw, rename-disabled records already name every tracked pending
    // change. Repeating a name-only diff and a full index listing adds no
    // identity evidence. Untracked files are the only extra names to merge.
    let mut names: BTreeSet<String> = modes.keys().cloned().collect();
    names.extend(untracked.iter().map(|path| (*path).to_owned()));
    let mut hash = Sha256::new();
    hash.update(parent.as_bytes());
    hash.update(symbolic_head(cwd).await?);
    let mut context = String::new();
    let mut total = 0u64;
    for name in names {
        let path = cwd.join(&name);
        hash.update(name.len().to_le_bytes());
        hash.update(name.as_bytes());
        match tokio::fs::symlink_metadata(&path).await {
            Ok(meta) => {
                if !meta.is_file() || meta.file_type().is_symlink() {
                    return Err(invalid(
                        "Generate text manually for symlinks or submodules.",
                    ));
                }
                total = total.saturating_add(meta.len());
                if total > FILE_BYTES {
                    return Err(invalid(
                        "This change is too large to generate text. Enter text manually.",
                    ));
                }
                let scope = crate::governance::approvals::WriteScope {
                    checkout: cwd.into(),
                    engine_cwd: cwd.into(),
                    excluded: Vec::new(),
                };
                let full = crate::project::files::inside(&scope, &name)?;
                let (file, _) = crate::project::files::open_inside(&scope, &name, &full)?;
                let mut bytes = Vec::new();
                tokio::fs::File::from_std(file)
                    .take(FILE_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .await?;
                if bytes.len() as u64 > FILE_BYTES {
                    return Err(invalid("This file grew while reading. Try again."));
                }
                hash.update([1]);
                let default_mode = {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        if filemode && meta.permissions().mode() & 0o100 != 0 {
                            "100755"
                        } else {
                            "100644"
                        }
                    }
                    #[cfg(not(unix))]
                    {
                        "100644"
                    }
                };
                let indexed_mode = modes.get(&name).map(String::as_str).unwrap_or(default_mode);
                // An unborn --cached diff describes the index, not the working
                // file that add -A will stage. Git trusts the native owner execute
                // bit on Unix when core.filemode is enabled.
                #[cfg(unix)]
                let mode = if parent.is_empty() && filemode {
                    default_mode
                } else {
                    indexed_mode
                };
                #[cfg(not(unix))]
                let mode = indexed_mode;
                hash.update(mode.as_bytes());
                context.push_str(&format!("\nPending file {} mode {mode}\n", json!(name)));
                hash.update(bytes.len().to_le_bytes());
                hash.update(&bytes);
                // Untracked files do not appear in a normal diff. Include their
                // full text once; tracked files get their actual diff below.
                if parent.is_empty() || untracked.contains(name.as_str()) {
                    context.push_str(&format!("\nNew file {}:\n", json!(name)));
                    if bytes.contains(&0) {
                        context.push_str("[binary file]\n");
                    } else {
                        context.push_str(std::str::from_utf8(&bytes).map_err(|_| {
                            invalid("Unsupported file encoding. Enter text manually.")
                        })?);
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                hash.update([0]);
            }
            Err(error) => return Err(error.into()),
        }
        if context.len() > CONTEXT_BYTES {
            return Err(invalid(
                "This change is too large to generate text. Enter text manually.",
            ));
        }
    }
    if !parent.is_empty() {
        context.push_str(&utf8(
            read_git(
                cwd,
                &[
                    "diff",
                    "--no-relative",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--no-renames",
                    &parent,
                    "--",
                ],
            )
            .await?,
        )?);
    }
    if context.trim().is_empty() {
        return Err(invalid(
            "There is no text change to describe. Enter text manually.",
        ));
    }
    if context.len() > CONTEXT_BYTES {
        return Err(invalid(
            "This change is too large to generate text. Enter text manually.",
        ));
    }
    Ok(Candidate {
        identity: hash.finalize().to_vec(),
        context,
    })
}

/// The revision that names the base: the upstream remote's tracking ref when it
/// exists (the local branch can be stale), else the name as given.
pub(super) async fn base_spec(cwd: &Path, base: &str) -> Result<String> {
    if !base.starts_with("refs/") {
        let branch = read_git(cwd, &["symbolic-ref", "--quiet", "--short", "HEAD"])
            .await
            .ok()
            .and_then(|value| String::from_utf8(value).ok())
            .map(|value| value.trim().to_owned())
            .unwrap_or_default();
        let mut remote = "origin".to_owned();
        if !branch.is_empty()
            && let Ok(value) = read_git(
                cwd,
                &["config", "--get", &format!("branch.{branch}.remote")],
            )
            .await
            && let Ok(value) = String::from_utf8(value)
        {
            let value = value.trim();
            if !value.is_empty() && value != "." && !value.starts_with('-') {
                remote = value.to_owned();
            }
        }
        let tracking = format!("refs/remotes/{remote}/{base}^{{commit}}");
        if read_git(cwd, &["rev-parse", "--verify", "--quiet", &tracking])
            .await
            .is_ok()
        {
            return Ok(tracking);
        }
    }
    Ok(format!("{base}^{{commit}}"))
}
