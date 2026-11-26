//! Git through the installed CLI. Worktrees live in the sibling `<repo>-wt`
//! root, never inside the repository. Repository config can name helpers that
//! run code, so every read disables them.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use orbyn_contracts::{CommitInfo, FileDiff, Worktree};
use tokio::process::Command;

use crate::error::{CoreError, Result};

use super::branch_names::{self, MAX_EXISTING_BRANCHES, Naming, NamingMode};

/// Git's well-known empty tree, the parent side of a root commit's diff.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// Refuses repository-configured programs on read paths.
const SAFE_CONFIG: &[&str] = &[
    "-c",
    "core.fsmonitor=false",
    "-c",
    "core.hooksPath=/dev/null",
    "-c",
    "diff.external=",
];

/// Checkpoints hold the files' own bytes. Line-ending settings and attributes would
/// rewrite them on the way in or out, so neither applies to a snapshot or a restore.
const BYTE_EXACT: &[&str] = &[
    "-c",
    "core.autocrlf=false",
    "-c",
    "core.safecrlf=false",
    "-c",
    "core.attributesFile=",
];

/// Crew's review checks compare bytes. Line-ending conversion and the untracked cache could
/// normalize a change away or hide a new file, so only those checks add these on top of `git_hardened`.
const CREW_BYTE_EXACT: &[&str] = &[
    "-c",
    "core.autocrlf=false",
    "-c",
    "core.safecrlf=false",
    "-c",
    "core.untrackedCache=false",
];

/// The Git release that started honouring `GIT_ATTR_SOURCE`. Older Git reads the checkout's own
/// `.gitattributes` instead, which could normalize a byte change away from a byte-exact read.
const ATTR_SOURCE_FROM: (u32, u32) = (2, 40);

/// The repository's own empty tree. Reading attributes from it hides `.gitattributes`.
async fn no_attributes(cwd: &Path) -> Result<String> {
    Ok(text(
        &git(cwd, &["hash-object", "-t", "tree", "--stdin"]).await?,
    ))
}

pub(crate) async fn git(cwd: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = git_raw(cwd, args).await?;
    if !output.status.success() {
        return Err(CoreError::Git(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    Ok(output.stdout)
}

pub(crate) async fn git_raw(cwd: &Path, args: &[&str]) -> Result<std::process::Output> {
    git_raw_env(cwd, &[], args).await
}

/// `git_raw` with extra environment, which the batched untracked read needs for its
/// throwaway index: `GIT_INDEX_FILE` moves the index a read compares against without
/// touching the checkout's own.
async fn git_raw_env(
    cwd: &Path,
    env: &[(&str, &Path)],
    args: &[&str],
) -> Result<std::process::Output> {
    let mut command = command(cwd);
    for (name, value) in env {
        command.env(name, value);
    }
    command.args(SAFE_CONFIG).args(args);
    let output = async {
        crate::host::process::ManagedChild::spawn(command)?
            .output()
            .await
    }
    .await;
    output.map_err(|error| CoreError::Git(format!("Could not run git: {error}")))
}

/// The `-c name=` overrides that switch off every filter and diff converter the repository's own config names: a clean filter
/// can make an edit read as the original bytes, and each of them runs a program the worker chose.
async fn converter_overrides(cwd: &Path) -> Vec<String> {
    let Ok(out) = git_raw(
        cwd,
        &[
            "config",
            "--name-only",
            "--get-regexp",
            r"^(filter\..*\.(clean|smudge|process)|diff\..*\.(textconv|command))$",
        ],
    )
    .await
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .flat_map(|name| ["-c".to_owned(), format!("{name}=")])
        .collect()
}

/// Git for the checks that read what a worker changed. Anything the worker could set in the repository to hide an edit, or to
/// make the core run a program of its choosing, is off: repository filters and diff converters, attributes (from the tree, the
/// global file and the system), and replace refs. A caller that relies on a changed file showing in a diff uses this.
pub(crate) async fn git_hardened(cwd: &Path, args: &[&str]) -> Result<Vec<u8>> {
    hardened(cwd, &[], args).await
}

/// `git_hardened` plus the settings Crew's byte-exact review checks need: no line-ending conversion and no untracked cache.
/// Only those checks call this; every other caller keeps `git_hardened` as is.
pub(crate) async fn git_byte_exact(cwd: &Path, args: &[&str]) -> Result<Vec<u8>> {
    hardened(cwd, CREW_BYTE_EXACT, args).await
}

async fn hardened_command(cwd: &Path, extra: &[&str], args: &[&str]) -> Result<Command> {
    let overrides = converter_overrides(cwd).await;
    let attributes = no_attributes(cwd).await?;
    let mut command = command(cwd);
    command
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env("GIT_ATTR_SOURCE", &attributes)
        .args(SAFE_CONFIG)
        .args(["-c", "core.attributesFile="])
        .args(extra)
        .args(&overrides)
        .args(args);
    Ok(command)
}

async fn hardened(cwd: &Path, extra: &[&str], args: &[&str]) -> Result<Vec<u8>> {
    let command = hardened_command(cwd, extra, args).await?;
    let output = async {
        crate::host::process::ManagedChild::spawn(command)?
            .output()
            .await
    }
    .await
    .map_err(|error| CoreError::Git(format!("Could not run git: {error}")))?;
    if !output.status.success() {
        return Err(CoreError::Git(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    Ok(output.stdout)
}

/// The major and minor version in `git version` output, such as `git version 2.39.3 (Apple Git-146)` or
/// `git version 2.45.1.windows.1`.
pub(crate) fn git_version(output: &str) -> Option<(u32, u32)> {
    let version = output.trim().strip_prefix("git version ")?;
    let mut parts = version.split(|c: char| !c.is_ascii_digit());
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

/// Whether this Git honours `GIT_ATTR_SOURCE`, which keeps the checkout's own `.gitattributes` out of a read.
/// A byte-exact read needs that: otherwise a text attribute could normalize a byte change away.
pub(crate) async fn reads_attributes_from_source(cwd: &Path) -> bool {
    let Ok(out) = git_raw(cwd, &["version"]).await else {
        return false;
    };
    git_version(&text(&out.stdout)).is_some_and(|version| version >= ATTR_SOURCE_FROM)
}

/// `git_hardened` that never holds more than `cap` bytes of output: past the cap the process tree is stopped and the call is an
/// error. For read paths whose output size a repository controls (messages, file lists, blame). Dropping the future stops git too.
pub(crate) async fn git_hardened_capped(cwd: &Path, args: &[&str], cap: usize) -> Result<Vec<u8>> {
    let (status, bytes) = git_hardened_output_capped(cwd, args, cap).await?;
    if !status.success() {
        return Err(CoreError::Git("git failed".into()));
    }
    Ok(bytes)
}

/// `git_hardened_capped`, with the exit status kept for a caller that reads a non-zero one as an answer rather than a failure:
/// a merge dry run reports a conflict as status 1. The output is still capped and stderr still drained.
pub(crate) async fn git_hardened_output_capped(
    cwd: &Path,
    args: &[&str],
    cap: usize,
) -> Result<(std::process::ExitStatus, Vec<u8>)> {
    use tokio::io::AsyncReadExt;
    let command = hardened_command(cwd, &[], args).await?;
    let mut child = crate::host::process::ManagedChild::spawn(command)
        .map_err(|error| CoreError::Git(format!("Could not run git: {error}")))?;
    let mut stdout = child.child_mut().stdout.take();
    let mut stderr = child.child_mut().stderr.take();
    drop(child.child_mut().stdin.take());
    let read = async {
        let mut bytes = Vec::new();
        if let Some(out) = stdout.as_mut() {
            out.take(cap as u64 + 1).read_to_end(&mut bytes).await?;
        }
        Ok::<_, std::io::Error>(bytes)
    };
    let drain = async {
        if let Some(err) = stderr.as_mut() {
            tokio::io::copy(err, &mut tokio::io::sink()).await?;
        }
        Ok::<_, std::io::Error>(())
    };
    // stdout and stderr are drained together: a chatty stderr must not stop git from finishing its stdout. The cap is checked as
    // soon as stdout ends (at the cap or at EOF), whatever stderr is doing; an over-cap call drops the child, which stops the tree.
    tokio::pin!(read, drain);
    let mut drained = false;
    let bytes = loop {
        tokio::select! {
            bytes = &mut read => {
                break bytes.map_err(|error| CoreError::Git(format!("Could not read git: {error}")))?;
            }
            result = &mut drain, if !drained => {
                result.map_err(|error| CoreError::Git(format!("Could not read git: {error}")))?;
                drained = true;
            }
        }
    };
    if bytes.len() > cap {
        return Err(CoreError::Git("git output is too large".into()));
    }
    let finish = async {
        if !drained {
            let _ = (&mut drain).await;
        }
    };
    let (_, status) = tokio::join!(finish, child.wait());
    let status = status.map_err(|error| CoreError::Git(format!("Could not run git: {error}")))?;
    Ok((status, bytes))
}

pub(crate) fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_owned()
}

/// Builds a Git process for `cwd`. Every Git Orbyn runs starts here, so its
/// environment, streams and drop behaviour are set once.
pub(crate) fn command(cwd: &Path) -> Command {
    #[cfg(test)]
    git_spawns::record(cwd);
    let mut command = Command::new("git");
    command
        .current_dir(cwd)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    crate::host::child_env::for_tool(command.as_std_mut());
    command
}

/// Filesystem-only check, so adding an untrusted folder runs nothing in it.
pub fn is_git_repo(path: &Path) -> bool {
    path.join(".git").exists()
}

/// Git coordinates belong to this checkout, independently of a selected project
/// subfolder. Strip only Git's line terminator; spaces can belong to the path.
pub(crate) async fn checkout_root(cwd: &Path) -> Result<PathBuf> {
    let output = git(cwd, &["rev-parse", "--show-toplevel"]).await?;
    let path = String::from_utf8_lossy(&output);
    let path = path.strip_suffix('\n').unwrap_or(&path);
    crate::util::canonicalize(Path::new(path)).map_err(Into::into)
}

/// Git lists the primary checkout first, including when invoked from a linked
/// worktree's subfolder or a repository with a separate Git directory.
pub(crate) async fn primary_checkout(cwd: &Path) -> Result<PathBuf> {
    let output = git(cwd, &["worktree", "list", "--porcelain", "-z"]).await?;
    let first = output.split(|byte| *byte == 0).next().unwrap_or_default();
    let first = String::from_utf8_lossy(first);
    let path = first
        .strip_prefix("worktree ")
        .ok_or_else(|| CoreError::Git("The primary checkout is unavailable.".into()))?;
    crate::util::canonicalize(Path::new(path)).map_err(Into::into)
}

pub(crate) fn valid_purpose(purpose: &str) -> bool {
    !purpose.is_empty()
        && purpose.len() <= 64
        && purpose
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !purpose.starts_with('-')
}

/// `<repo>-wt` beside the primary clone: where Orbyn makes its worktrees.
pub(crate) fn worktree_root(repo: &Path) -> Result<PathBuf> {
    let name = repo
        .file_name()
        .ok_or_else(|| CoreError::Git("This repository has no folder name.".into()))?;
    let mut root = name.to_owned();
    root.push("-wt");
    Ok(repo.with_file_name(root))
}

/// `<repo>-wt/<purpose>` beside the primary clone.
pub(crate) fn worktree_path(repo: &Path, purpose: &str) -> Result<PathBuf> {
    Ok(worktree_root(repo)?.join(purpose))
}

/// A registered worktree of a primary clone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisteredWorktree {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub revision: String,
}

/// One entry of `git worktree list`, with the flags that say whether it can be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WorktreeRecord {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub revision: String,
    pub detached: bool,
    pub locked: bool,
    pub prunable: bool,
    /// The entry is a bare repository, not a checkout.
    pub bare: bool,
}

/// Every worktree of the repository, the primary checkout first.
pub(crate) async fn worktree_records(repo: &Path) -> Result<Vec<WorktreeRecord>> {
    let listing = git(repo, &["worktree", "list", "--porcelain", "-z"]).await?;
    Ok(parse_worktrees(&listing))
}

/// Like [`worktree_records`] for a reader that shows what it finds to a person: through
/// the hardened runner, so no repository setting can start a program.
pub(crate) async fn worktree_records_hardened(repo: &Path) -> Result<Vec<WorktreeRecord>> {
    let listing = git_hardened(repo, &["worktree", "list", "--porcelain", "-z"]).await?;
    Ok(parse_worktrees(&listing))
}

fn parse_worktrees(listing: &[u8]) -> Vec<WorktreeRecord> {
    let mut worktrees = Vec::new();
    let mut current: Option<WorktreeRecord> = None;
    for field in listing.split(|byte| *byte == 0) {
        let line = String::from_utf8_lossy(field);
        if let Some(path) = line.strip_prefix("worktree ") {
            worktrees.extend(current.take());
            current = Some(WorktreeRecord {
                path: PathBuf::from(path),
                branch: None,
                revision: String::new(),
                detached: false,
                locked: false,
                prunable: false,
                bare: false,
            });
        } else if let Some(entry) = current.as_mut() {
            if let Some(head) = line.strip_prefix("HEAD ") {
                entry.revision = head.to_owned();
            } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
                entry.branch = Some(branch.to_owned());
            } else if line == "detached" {
                entry.detached = true;
            } else if line == "bare" {
                entry.bare = true;
            } else if line == "locked" || line.starts_with("locked ") {
                entry.locked = true;
            } else if line == "prunable" || line.starts_with("prunable ") {
                entry.prunable = true;
            }
        }
    }
    worktrees.extend(current);
    worktrees
}

/// Linked worktrees, without the primary checkout.
pub(crate) async fn linked_worktrees(repo: &Path) -> Result<Vec<RegisteredWorktree>> {
    Ok(worktree_records(repo)
        .await?
        .into_iter()
        .skip(1)
        .map(|record| RegisteredWorktree {
            path: record.path,
            branch: record.branch,
            revision: record.revision,
        })
        .collect())
}

/// How a new worktree starts, from the settings and the project file.
#[derive(Clone, Debug)]
pub struct WorktreeOptions {
    pub naming: Naming,
    pub base: Option<String>,
    pub from_origin: bool,
    /// `recursive`, `top-level` or `none`; legacy spellings remain readable.
    pub submodules: String,
    /// The thread's real title, when the creation request had one; semantic naming uses it
    /// before the purpose.
    pub seed: Option<String>,
}

impl Default for WorktreeOptions {
    fn default() -> Self {
        Self {
            naming: Naming::default(),
            base: None,
            from_origin: false,
            submodules: "none".to_owned(),
            seed: None,
        }
    }
}

/// Finds or creates the purpose's worktree at `<repo>-wt/<purpose>`. A registered
/// worktree already at that path is reused; its base is where it left the
/// primary clone's HEAD.
pub(crate) async fn create_worktree(
    repo: &Path,
    purpose: &str,
    options: &WorktreeOptions,
) -> Result<Worktree> {
    if !valid_purpose(purpose) {
        return Err(CoreError::Invalid(
            "Use lowercase letters, digits and hyphens for the worktree name.".into(),
        ));
    }
    let path = worktree_path(&primary_checkout(repo).await?, purpose)?;
    let path_text = path.to_string_lossy().into_owned();
    let head = text(&git(repo, &["rev-parse", "HEAD"]).await?);
    for registered in linked_worktrees(repo).await? {
        // Keep already registered worktrees in place, including earlier layouts.
        if registered.path.exists()
            && registered.path.file_name() == Some(std::ffi::OsStr::new(purpose))
        {
            let base = text(&git(repo, &["merge-base", "HEAD", &registered.revision]).await?);
            // Git's spelling of a path differs on Windows, so keep ours when it is the same place.
            let same = crate::util::canonicalize(&registered.path).ok().as_ref()
                == crate::util::canonicalize(&path).ok().as_ref();
            return Ok(Worktree {
                path: if same {
                    path_text
                } else {
                    registered.path.to_string_lossy().into_owned()
                },
                branch: registered.branch.unwrap_or_else(|| "HEAD".to_owned()),
                base,
            });
        }
    }
    if path.exists() {
        return Err(CoreError::Invalid(format!(
            "{} already exists.",
            path.display()
        )));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // The branch name is chosen from the thread and the naming mode, never taken from
    // the request as a ref: semantic naming reads the project's local branches first, so
    // the same title twice makes two branches, and custom naming uses the purpose itself.
    let existing = if options.naming.mode == NamingMode::Semantic {
        local_branch_names(repo).await?
    } else {
        Vec::new()
    };
    let branch =
        branch_names::branch_name(&options.naming, options.seed.as_deref(), purpose, &existing);
    if git(repo, &["check-ref-format", "--branch", &branch])
        .await
        .is_err()
    {
        return Err(CoreError::Invalid("That branch name is not valid.".into()));
    }
    let start = worktree_start(repo, options).await.unwrap_or(head);
    git(
        repo,
        &["worktree", "add", "-b", &branch, &path_text, &start],
    )
    .await?;
    let submodules: &[&str] = match options.submodules.as_str() {
        "none" | "skip" => &[],
        "top-level" | "top_level" => &["submodule", "update", "--init"],
        _ => &["submodule", "update", "--init", "--recursive"],
    };
    // A submodule that cannot be fetched leaves the worktree usable.
    if !submodules.is_empty()
        && path.join(".gitmodules").exists()
        && let Err(error) = git(&path, submodules).await
    {
        tracing::warn!(%error, "could not populate submodules");
    }
    Ok(Worktree {
        path: path_text,
        branch,
        base: start,
    })
}

/// The project's local branches, capped. Semantic naming avoids them; a listing past the
/// cap is cut rather than read unbounded, and a name past it is left to Git to refuse.
async fn local_branch_names(repo: &Path) -> Result<Vec<String>> {
    let listing = git(
        repo,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
    )
    .await?;
    let listing = String::from_utf8_lossy(&listing);
    Ok(listing
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .take(MAX_EXISTING_BRANCHES)
        .map(str::to_owned)
        .collect())
}

/// A chosen base, or the remote's head when asked for. `None` means this clone's HEAD.
async fn worktree_start(repo: &Path, options: &WorktreeOptions) -> Option<String> {
    if let Some(base) = &options.base {
        if base.starts_with('-') {
            return None;
        }
        return Some(text(
            &git(repo, &["rev-parse", "--verify", base]).await.ok()?,
        ));
    }
    if !options.from_origin {
        return None;
    }
    let default = git(
        repo,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    )
    .await
    .ok()?;
    let default = text(&default);
    let branch = default.strip_prefix("origin/")?;
    git(repo, &["fetch", "-q", "origin", branch]).await.ok()?;
    Some(text(
        &git(repo, &["rev-parse", "--verify", &default]).await.ok()?,
    ))
}

/// Refs and index entries only: a snapshot leaves the real index and every working
/// file alone. Ignored files are not captured.
pub(crate) async fn snapshot(cwd: &Path, reference: Option<&str>, index: &Path) -> Result<String> {
    let index_text = index.to_string_lossy().into_owned();
    let attributes = no_attributes(cwd).await?;
    let with_index = |args: Vec<String>| {
        let index_text = index_text.clone();
        let attributes = attributes.clone();
        let cwd = cwd.to_path_buf();
        async move {
            let mut command = command(&cwd);
            command
                .args(SAFE_CONFIG)
                .args(BYTE_EXACT)
                .args(["-c", "user.name=Orbyn", "-c", "user.email=orbyn@localhost"])
                .args(&args)
                .env("GIT_INDEX_FILE", &index_text)
                .env("GIT_ATTR_SOURCE", &attributes);
            let output = command
                .output()
                .await
                .map_err(|error| CoreError::Git(format!("Could not run git: {error}")))?;
            if !output.status.success() {
                return Err(CoreError::Git(text(&output.stderr)));
            }
            Ok(text(&output.stdout))
        }
    };
    let _ = std::fs::remove_file(index);
    let head = git(cwd, &["rev-parse", "--verify", "-q", "HEAD"])
        .await
        .ok();
    if let Some(head) = &head {
        with_index(vec!["read-tree".into(), text(head)]).await?;
    }
    // A selected ignored directory can still contain tracked files. Preserve
    // those without forcing ignored additions into its checkpoint.
    let ignored = git_raw(cwd, &["check-ignore", "--no-index", "-q", "."]).await?;
    let stage = match ignored.status.code() {
        Some(0) => "-u",
        Some(1) => "-A",
        _ => return Err(CoreError::Git(text(&ignored.stderr))),
    };
    with_index(vec!["add".into(), stage.into(), "--".into(), ".".into()]).await?;
    let tree = with_index(vec!["write-tree".into()]).await?;
    let mut commit = vec![
        "commit-tree".to_owned(),
        tree,
        "-m".into(),
        "checkpoint".into(),
    ];
    if let Some(head) = &head {
        commit.extend(["-p".to_owned(), text(head)]);
    }
    let sha = with_index(commit).await?;
    let _ = std::fs::remove_file(index);
    if let Some(reference) = reference {
        git(cwd, &["update-ref", reference, &sha]).await?;
    }
    Ok(sha)
}

/// What changed between two commits.
pub(crate) async fn diff_between(cwd: &Path, from: &str, to: &str) -> Result<Vec<FileDiff>> {
    tracked_diff(cwd, &[from, to]).await
}

/// The changes one commit made.
pub(crate) async fn commit_diff(cwd: &Path, sha: &str) -> Result<Vec<FileDiff>> {
    if !is_revision(sha) {
        return Err(CoreError::Invalid("That is not a commit.".into()));
    }
    let parent = format!("{sha}^");
    let base = match git(cwd, &["rev-parse", "--verify", "-q", &parent]).await {
        Ok(found) => text(&found),
        Err(_) => EMPTY_TREE.to_owned(),
    };
    diff_between(cwd, &base, sha).await
}

/// Everything since this branch left `base`, untracked work included.
pub(crate) async fn branch_diff(cwd: &Path, base: &str) -> Result<Vec<FileDiff>> {
    if base.starts_with('-') || base.is_empty() {
        return Err(CoreError::Invalid("That is not a branch.".into()));
    }
    let shared = text(&git(cwd, &["merge-base", base, "HEAD"]).await?);
    diff(cwd, Some(&shared)).await
}

pub(crate) fn is_revision(text: &str) -> bool {
    (7..=64).contains(&text.len()) && text.chars().all(|c| c.is_ascii_hexdigit())
}

pub(crate) async fn recent_commits(cwd: &Path, limit: u32) -> Result<Vec<CommitInfo>> {
    let limit = format!("-{}", limit.min(100));
    let Ok(log) = git(cwd, &["log", &limit, "--format=%H%x09%an%x09%aI%x09%s"]).await else {
        // A repository with no commits has no history to show.
        return Ok(Vec::new());
    };
    Ok(String::from_utf8_lossy(&log)
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(4, '\t');
            Some(CommitInfo {
                sha: fields.next()?.to_owned(),
                author: fields.next()?.to_owned(),
                date: fields.next()?.to_owned(),
                subject: fields.next()?.to_owned(),
            })
        })
        .collect())
}

/// Makes the working files match `target`, leaving HEAD, the index and ignored
/// files alone. `current` is a snapshot of the files as they are now.
pub(crate) async fn restore_worktree(cwd: &Path, current: &str, target: &str) -> Result<()> {
    let root = checkout_root(cwd).await?;
    let added = git(
        cwd,
        &[
            "diff",
            "--no-renames",
            "--name-only",
            "-z",
            "--diff-filter=A",
            target,
            current,
            "--",
            ".",
        ],
    )
    .await?;
    for path in added
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let path = root.join(String::from_utf8_lossy(path).as_ref());
        std::fs::remove_file(path)?;
    }
    // A selected subtree can be absent from the target tree entirely. Its
    // added files are already removed above; there is no restore path to match.
    if git(
        cwd,
        &["ls-tree", "-r", "--name-only", "-z", target, "--", "."],
    )
    .await?
    .is_empty()
    {
        return Ok(());
    }
    let mut command = command(cwd);
    command
        .args(SAFE_CONFIG)
        .args(BYTE_EXACT)
        .args([
            "restore",
            &format!("--source={target}"),
            "--worktree",
            "--",
            ".",
        ])
        .env("GIT_ATTR_SOURCE", no_attributes(cwd).await?);
    let output = command
        .output()
        .await
        .map_err(|error| CoreError::Git(format!("Could not run git: {error}")))?;
    if !output.status.success() {
        return Err(CoreError::Git(text(&output.stderr)));
    }
    Ok(())
}

/// Changes against `base` (or HEAD), including untracked files.
pub(crate) async fn diff(cwd: &Path, base: Option<&str>) -> Result<Vec<FileDiff>> {
    let root = checkout_root(cwd).await?;
    let cwd = root.as_path();
    let base = base.unwrap_or("HEAD");
    let mut files = tracked_diff(cwd, &[base]).await?;
    let listing = git(cwd, &["ls-files", "--others", "--exclude-standard", "-z"]).await?;
    let untracked: Vec<String> = listing
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8_lossy(path).into_owned())
        .collect();
    files.extend(untracked_diffs(cwd, &untracked).await?);
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

async fn tracked_diff(cwd: &Path, range: &[&str]) -> Result<Vec<FileDiff>> {
    let root = checkout_root(cwd).await?;
    let cwd = root.as_path();
    let mut statuses_args = vec![
        "diff",
        "--no-ext-diff",
        "--no-textconv",
        "--name-status",
        "-z",
    ];
    statuses_args.extend(range);
    let statuses = git(cwd, &statuses_args).await?;
    let mut numstat_args = vec!["diff", "--no-ext-diff", "--no-textconv", "--numstat", "-z"];
    numstat_args.extend(range);
    let numstat = git(cwd, &numstat_args).await?;
    let counts = parse_numstat(&numstat);
    let mut files = Vec::new();
    for (status, path) in parse_name_status(&statuses) {
        let mut patch_args = vec!["diff", "--no-ext-diff", "--no-textconv"];
        patch_args.extend(range);
        let literal = format!(":(top,literal){path}");
        patch_args.extend(["--", &literal]);
        let patch = git(cwd, &patch_args).await?;
        let (additions, deletions) = counts.get(&path).copied().unwrap_or((0, 0));
        files.push(FileDiff {
            path,
            status,
            additions,
            deletions,
            patch: String::from_utf8_lossy(&patch).into_owned(),
            repo_id: None,
        });
    }
    Ok(files)
}

/// The rows for a worktree's untracked files. The read this replaced started one `git
/// diff` per file; a throwaway index now carries every path as intent-to-add and one
/// `git diff` writes all of their new-file patches. Git still writes them, so the bytes,
/// binary detection, filters, line endings and quoting stay the ones it always produced.
/// A listing git will not take falls back to the per-file read, which is also the only
/// one that can report its error.
async fn untracked_diffs(cwd: &Path, paths: &[String]) -> Result<Vec<FileDiff>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    if let Some(files) = batched_untracked_diffs(cwd, paths).await {
        return Ok(files);
    }
    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        files.push(untracked_diff(cwd, path).await?);
    }
    Ok(files)
}

/// Every untracked path's patch from one `git diff`, or `None` when the throwaway index
/// was refused or git answered in a shape this read does not recognise, so the caller can
/// read the files one at a time instead.
async fn batched_untracked_diffs(cwd: &Path, paths: &[String]) -> Option<Vec<FileDiff>> {
    let scratch = Scratch::new(paths).ok()?;
    let indexed = [("GIT_INDEX_FILE", scratch.index.as_path())];
    let listing = format!("--pathspec-from-file={}", scratch.pathspec.display());
    let added = git_raw_env(
        cwd,
        &indexed,
        &["add", "--intent-to-add", &listing, "--pathspec-file-nul"],
    )
    .await
    .ok()?;
    if !added.status.success() {
        return None;
    }
    let output = git_raw_env(cwd, &indexed, &["diff", "--no-ext-diff", "--no-textconv"])
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    // The throwaway index holds nothing but these entries, so git writes their patches in
    // its index order, which is the byte order of their paths.
    let mut expected = paths.to_vec();
    expected.sort_unstable();
    let patch = String::from_utf8_lossy(&output.stdout);
    let blocks = patches_at_headers(&patch);
    if blocks.len() != expected.len() {
        return None;
    }
    let mut files = Vec::with_capacity(blocks.len());
    for (block, path) in blocks.iter().zip(&expected) {
        if !header_names(block, path) {
            return None;
        }
        files.push(FileDiff {
            path: path.clone(),
            status: "added".into(),
            additions: added_lines(block),
            deletions: 0,
            patch: (*block).to_owned(),
            repo_id: None,
        });
    }
    Some(files)
}

/// A throwaway index and the pathspec list that fills it, both under the system temp
/// folder so nothing this read writes lands in the checkout, and both removed when the
/// read ends. The index file is git's to create; this only names where.
struct Scratch {
    folder: PathBuf,
    index: PathBuf,
    pathspec: PathBuf,
}

impl Scratch {
    fn new(paths: &[String]) -> std::io::Result<Self> {
        let folder = std::env::temp_dir().join(format!("orbyn-diff-{}", crate::util::new_id()));
        std::fs::create_dir_all(&folder)?;
        let scratch = Self {
            index: folder.join("index"),
            pathspec: folder.join("pathspec"),
            folder,
        };
        let mut listing = Vec::with_capacity(paths.iter().map(|path| path.len() + 16).sum());
        for path in paths {
            // A path can spell pathspec magic, and this list must name each file as it is.
            listing.extend_from_slice(format!(":(top,literal){path}").as_bytes());
            listing.push(0);
        }
        std::fs::write(&scratch.pathspec, listing)?;
        Ok(scratch)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.folder);
    }
}

/// A combined patch split at the `diff --git` header that starts each block. No content
/// line can start one: git writes every content line behind a space, `+` or `-`.
fn patches_at_headers(patch: &str) -> Vec<&str> {
    let mut blocks = Vec::new();
    let mut start = 0;
    let mut offset = 0;
    for line in patch.split_inclusive('\n') {
        if line.starts_with("diff --git ") {
            if offset > start {
                blocks.push(&patch[start..offset]);
            }
            start = offset;
        }
        offset += line.len();
    }
    if start < patch.len() {
        blocks.push(&patch[start..]);
    }
    blocks
}

/// Whether a block's header names `path`. Git quotes the path when it holds a control
/// byte, a quote or a backslash, and also, unless `core.quotePath` is off, a byte over
/// ASCII, so the plain spelling and both quoted ones count.
fn header_names(block: &str, path: &str) -> bool {
    let Some(header) = block.lines().next() else {
        return false;
    };
    if header == format!("diff --git a/{path} b/{path}") {
        return true;
    }
    [true, false].into_iter().any(|escape_high| {
        let quoted = quoted_path(path, escape_high);
        header == format!("diff --git a/{quoted} b/{quoted}")
    })
}

/// Git's C-style quoting of one path as a header writes it. `escape_high` is the default
/// `core.quotePath` reading, which also escapes every byte over ASCII.
fn quoted_path(path: &str, escape_high: bool) -> String {
    let mut quoted = Vec::with_capacity(path.len() + 2);
    quoted.push(b'"');
    for &byte in path.as_bytes() {
        match byte {
            b'\n' => quoted.extend_from_slice(b"\\n"),
            b'\t' => quoted.extend_from_slice(b"\\t"),
            b'\r' => quoted.extend_from_slice(b"\\r"),
            b'\\' => quoted.extend_from_slice(b"\\\\"),
            b'"' => quoted.extend_from_slice(b"\\\""),
            0x20..=0x7e => quoted.push(byte),
            byte if byte < 0x20 || byte == 0x7f || escape_high => {
                quoted.extend_from_slice(format!("\\{byte:03o}").as_bytes());
            }
            byte => quoted.push(byte),
        }
    }
    quoted.push(b'"');
    String::from_utf8_lossy(&quoted).into_owned()
}

/// The added lines of one new-file patch, its `+++` header aside.
fn added_lines(patch: &str) -> u32 {
    patch
        .lines()
        .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
        .count() as u32
}

/// One untracked file's new-file patch from the `--no-index` call against the null
/// device. The batched read replaces this one; it stays as that read's fallback and as
/// the reference the equivalence test measures against.
async fn untracked_diff(cwd: &Path, path: &str) -> Result<FileDiff> {
    let null_device = if cfg!(windows) { "NUL" } else { "/dev/null" };
    // `--no-index` exits 1 when files differ, which is the expected case.
    let output = git_raw(
        cwd,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-index",
            "--",
            null_device,
            path,
        ],
    )
    .await?;
    if output.status.code() != Some(1) {
        return Err(CoreError::Git(text(&output.stderr)));
    }
    let patch = String::from_utf8_lossy(&output.stdout).into_owned();
    let additions = added_lines(&patch);
    Ok(FileDiff {
        path: path.to_owned(),
        status: "added".into(),
        additions,
        deletions: 0,
        patch,
        repo_id: None,
    })
}

fn parse_name_status(bytes: &[u8]) -> Vec<(String, String)> {
    let mut fields = bytes
        .split(|byte| *byte == 0)
        .map(|field| String::from_utf8_lossy(field).into_owned());
    let mut entries = Vec::new();
    while let Some(status) = fields.next() {
        if status.is_empty() {
            continue;
        }
        let renamed = status.starts_with('R') || status.starts_with('C');
        let Some(mut path) = fields.next() else {
            break;
        };
        if renamed && let Some(new_path) = fields.next() {
            path = new_path;
        }
        let name = match status.chars().next() {
            Some('A') => "added",
            Some('D') => "deleted",
            Some('R') => "renamed",
            Some('C') => "copied",
            _ => "modified",
        };
        entries.push((name.to_owned(), path));
    }
    entries
}

/// `git diff --numstat -z`: a rename puts its two paths in separate fields.
fn parse_numstat(bytes: &[u8]) -> std::collections::HashMap<String, (u32, u32)> {
    let mut counts = std::collections::HashMap::new();
    let mut fields = bytes.split(|byte| *byte == 0);
    while let Some(field) = fields.next() {
        let line = String::from_utf8_lossy(field);
        let mut parts = line.splitn(3, '\t');
        let (Some(added), Some(deleted), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let added = added.parse().unwrap_or(0);
        let deleted = deleted.parse().unwrap_or(0);
        let path = if path.is_empty() {
            let _old = fields.next();
            fields
                .next()
                .map(|new| String::from_utf8_lossy(new).into_owned())
                .unwrap_or_default()
        } else {
            path.to_owned()
        };
        counts.insert(path, (added, deleted));
    }
    counts
}

/// Counts the Git processes a read starts, so a test shows how many rather than
/// trusting a number, and prints it beside the time the read took. Recording the
/// checkout each one ran in keeps a parallel test's processes out of another test's
/// count; the checkout is resolved first because Git's answer carries the resolved
/// path while a test may hold a path that only names it.
#[cfg(test)]
pub(crate) mod git_spawns {
    use std::path::Path;
    use std::sync::{Mutex, PoisonError};

    static SPAWNED: Mutex<Vec<String>> = Mutex::new(Vec::new());

    fn key(folder: &Path) -> String {
        crate::util::canonicalize(folder)
            .map(|resolved| resolved.to_string_lossy().into_owned())
            .unwrap_or_else(|_| folder.to_string_lossy().into_owned())
    }

    pub(crate) fn record(folder: &Path) {
        let mut spawned = SPAWNED.lock().unwrap_or_else(PoisonError::into_inner);
        spawned.push(key(folder));
    }

    /// The Git processes started in `folder` so far.
    pub(crate) fn count(folder: &Path) -> usize {
        let folder = key(folder);
        let spawned = SPAWNED.lock().unwrap_or_else(PoisonError::into_inner);
        spawned.iter().filter(|spawned| **spawned == folder).count()
    }
}

#[cfg(test)]
mod tests {
    use super::git_spawns;
    use super::*;
    use crate::testing::TempDir;

    #[tokio::test]
    async fn only_the_byte_exact_helper_carries_the_crew_flags() {
        let dir = TempDir::new();
        crate::testing::run_git(dir.path(), &["init", "-q"]);
        for (key, value) in [
            ("core.autocrlf", "true"),
            ("core.safecrlf", "true"),
            ("core.untrackedCache", "true"),
        ] {
            crate::testing::run_git(dir.path(), &["config", key, value]);
            let hardened = git_hardened(dir.path(), &["config", "--get", key])
                .await
                .unwrap();
            assert_eq!(text(&hardened), "true", "{key} must stay as configured");
            let exact = git_byte_exact(dir.path(), &["config", "--get", key])
                .await
                .unwrap();
            assert_eq!(text(&exact), "false", "{key} is off for byte-exact checks");
        }
    }

    #[test]
    fn the_byte_exact_checks_need_attr_source_from_git_2_40() {
        for (output, supported) in [
            ("git version 2.39.5 (Apple Git-154)\n", false),
            ("git version 2.40.0\n", true),
            ("git version 2.45.1.windows.1\n", true),
            ("git version 3.0.0\n", true),
            ("not git", false),
        ] {
            assert_eq!(
                git_version(output).is_some_and(|version| version >= ATTR_SOURCE_FROM),
                supported,
                "{output}"
            );
        }
    }

    #[tokio::test]
    async fn a_finished_git_leader_does_not_leave_helpers_holding_output() {
        let dir = TempDir::new();
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            git_raw(
                dir.path(),
                &[
                    "-c",
                    "alias.detach=!(sleep 1; echo escaped > escaped) &",
                    "detach",
                ],
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(output.status.success(), "{}", text(&output.stderr));
        assert!(
            !dir.path().join("escaped").exists(),
            "Git waited for a detached helper to finish"
        );
    }

    #[tokio::test]
    async fn cancelling_git_stops_its_helper_descendants() {
        let dir = TempDir::new();
        let root = dir.path().to_path_buf();
        let ready = root.join("ready");
        let escaped = root.join("escaped");
        let task = tokio::spawn(async move {
            // Git runs ! aliases through its shell on both supported platforms.
            // The descendant has a bounded lifetime even on the failing baseline.
            git_raw(
                &root,
                &[
                    "-c",
                    "alias.hold=!(sleep 1; echo escaped > escaped) & echo ready > ready; wait",
                    "hold",
                ],
            )
            .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while !ready.exists() {
                assert!(
                    !task.is_finished(),
                    "Git fixture exited before its handshake"
                );
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        let escaped = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while !escaped.exists() {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await;
        assert!(escaped.is_err(), "Git's descendant survived cancellation");
    }

    async fn repo(dir: &Path) -> PathBuf {
        let repo = dir.join("project");
        std::fs::create_dir_all(&repo).unwrap();
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.email", "t@example.com"],
            vec!["config", "user.name", "T"],
        ] {
            git(&repo, &args).await.unwrap();
        }
        std::fs::write(repo.join("a.txt"), "one\n").unwrap();
        git(&repo, &["add", "."]).await.unwrap();
        git(&repo, &["commit", "-q", "-m", "first"]).await.unwrap();
        repo
    }

    #[tokio::test]
    async fn subfolder_worktrees_stay_beside_the_primary_clone() {
        let dir = TempDir::new();
        let repo = repo(dir.path()).await;
        let child = repo.join("packages/web");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(child.join("kept.txt"), "one\n").unwrap();
        git(&repo, &["add", "."]).await.unwrap();
        git(&repo, &["commit", "-qm", "nested"]).await.unwrap();
        let first = create_worktree(&child, "first", &WorktreeOptions::default())
            .await
            .unwrap();
        assert_eq!(
            Path::new(&first.path),
            worktree_path(&repo, "first").unwrap()
        );
        let second = create_worktree(
            &Path::new(&first.path).join("packages/web"),
            "second",
            &WorktreeOptions::default(),
        )
        .await
        .unwrap();
        assert_eq!(
            Path::new(&second.path),
            worktree_path(&repo, "second").unwrap()
        );
        assert!(!repo.join("packages/web-wt").exists());
    }

    #[tokio::test]
    async fn subfolder_diffs_use_repository_paths_but_checkpoints_restore_only_the_selected_tree() {
        let dir = TempDir::new();
        let repo = repo(dir.path()).await;
        let child = repo.join("packages/web");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(child.join("kept.txt"), "one\n").unwrap();
        git(&repo, &["add", "."]).await.unwrap();
        git(&repo, &["commit", "-qm", "nested"]).await.unwrap();
        let before = snapshot(&child, None, &dir.path().join("before-index"))
            .await
            .unwrap();
        std::fs::write(child.join("kept.txt"), "inside-two\n").unwrap();
        std::fs::write(child.join("new.txt"), "inside-new\n").unwrap();
        std::fs::write(repo.join("a.txt"), "outside-two\n").unwrap();
        std::fs::write(repo.join("outside.txt"), "outside-new\n").unwrap();
        let files = diff(&child, None).await.unwrap();
        for (path, marker) in [
            ("packages/web/kept.txt", "inside-two"),
            ("packages/web/new.txt", "inside-new"),
            ("a.txt", "outside-two"),
            ("outside.txt", "outside-new"),
        ] {
            let file = files
                .iter()
                .find(|file| file.path == path)
                .unwrap_or_else(|| panic!("missing {path}: {files:?}"));
            assert!(
                file.patch.contains(marker),
                "empty or misaddressed patch for {path}"
            );
        }
        let after = snapshot(&child, None, &dir.path().join("after-index"))
            .await
            .unwrap();
        let changes = diff_between(&child, &before, &after).await.unwrap();
        assert_eq!(changes.len(), 2);
        assert!(
            changes
                .iter()
                .all(|file| file.path.starts_with("packages/web/"))
        );
        restore_worktree(&child, &after, &before).await.unwrap();
        assert_eq!(
            std::fs::read_to_string(child.join("kept.txt")).unwrap(),
            "one\n"
        );
        assert!(!child.join("new.txt").exists());
        assert_eq!(
            std::fs::read_to_string(repo.join("a.txt")).unwrap(),
            "outside-two\n"
        );
        assert_eq!(
            std::fs::read_to_string(repo.join("outside.txt")).unwrap(),
            "outside-new\n"
        );
    }

    #[tokio::test]
    async fn restoring_an_empty_subfolder_checkpoint_keeps_the_selected_folder() {
        let dir = TempDir::new();
        let repo = repo(dir.path()).await;
        let child = repo.join("empty");
        std::fs::create_dir_all(&child).unwrap();
        let before = snapshot(&child, None, &dir.path().join("before-index"))
            .await
            .unwrap();
        std::fs::write(child.join("added.txt"), "new child file\n").unwrap();
        let after = snapshot(&child, None, &dir.path().join("after-index"))
            .await
            .unwrap();
        let index = git(&repo, &["ls-files", "--stage", "-z"]).await.unwrap();
        restore_worktree(&child, &after, &before).await.unwrap();
        assert!(child.is_dir());
        assert!(!child.join("added.txt").exists());
        assert_eq!(
            git(&repo, &["ls-files", "--stage", "-z"]).await.unwrap(),
            index
        );
        assert!(repo.join("a.txt").is_file());
    }

    #[tokio::test]
    async fn subfolder_ignored_scope_checkpoints_keep_tracked_files() {
        let dir = TempDir::new();
        let repo = repo(dir.path()).await;
        let child = repo.join("ignored");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(child.join("tracked.txt"), "before\n").unwrap();
        git(&repo, &["add", "."]).await.unwrap();
        git(&repo, &["commit", "-qm", "tracked child"])
            .await
            .unwrap();
        std::fs::write(repo.join(".gitignore"), "ignored/\n").unwrap();
        std::fs::write(child.join("tracked.txt"), "after\n").unwrap();
        std::fs::write(child.join("ignored.txt"), "not captured\n").unwrap();
        let captured = snapshot(&child, None, &dir.path().join("ignored-index"))
            .await
            .unwrap();
        let files = diff_between(&child, "HEAD", &captured).await.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "ignored/tracked.txt");
        assert!(files[0].patch.contains("after"));
    }

    #[tokio::test]
    async fn an_existing_worktree_elsewhere_is_reused_without_moving_it() {
        let dir = TempDir::new();
        let repo = repo(dir.path()).await;
        let earlier = dir.path().join("elsewhere/before");
        std::fs::create_dir_all(earlier.parent().unwrap()).unwrap();
        git(
            &repo,
            &[
                "worktree",
                "add",
                "-b",
                "earlier",
                earlier.to_str().unwrap(),
            ],
        )
        .await
        .unwrap();
        std::fs::write(earlier.join("kept.txt"), "Uncommitted work\n").unwrap();
        let made = create_worktree(&repo, "fresh", &WorktreeOptions::default())
            .await
            .unwrap();
        assert_eq!(Path::new(&made.path), dir.path().join("project-wt/fresh"));
        let kept = create_worktree(&repo, "before", &WorktreeOptions::default())
            .await
            .unwrap();
        assert_eq!(
            crate::util::canonicalize(Path::new(&kept.path)).unwrap(),
            crate::util::canonicalize(&earlier).unwrap()
        );
        assert_eq!(
            std::fs::read_to_string(earlier.join("kept.txt")).unwrap(),
            "Uncommitted work\n"
        );
    }

    #[tokio::test]
    async fn worktree_is_a_sibling_and_diff_sees_tracked_and_untracked() {
        let dir = TempDir::new();
        let repo = repo(dir.path()).await;
        let worktree = create_worktree(&repo, "fix-bug", &WorktreeOptions::default())
            .await
            .unwrap();
        let path = PathBuf::from(&worktree.path);
        assert_eq!(path, dir.path().join("project-wt").join("fix-bug"));
        assert_eq!(worktree.branch, "feat/fix-bug");
        let again = create_worktree(&repo, "fix-bug", &WorktreeOptions::default())
            .await
            .unwrap();
        assert_eq!(again, worktree, "a registered worktree is reused");

        std::fs::write(path.join("a.txt"), "one\ntwo\n").unwrap();
        std::fs::write(path.join("new file.txt"), "fresh\nlines\n").unwrap();
        let files = diff(&path, Some(&worktree.base)).await.unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "a.txt");
        assert_eq!(files[0].status, "modified");
        assert_eq!((files[0].additions, files[0].deletions), (1, 0));
        assert!(files[0].patch.contains("+two"));
        assert_eq!(files[1].path, "new file.txt");
        assert_eq!(files[1].status, "added");
        assert_eq!(files[1].additions, 2);
        assert!(files[1].patch.contains("+fresh"));
    }

    #[tokio::test]
    async fn diff_includes_committed_work_since_the_base() {
        let dir = TempDir::new();
        let repo = repo(dir.path()).await;
        let base = text(&git(&repo, &["rev-parse", "HEAD"]).await.unwrap());
        std::fs::write(repo.join("b.txt"), "b\n").unwrap();
        git(&repo, &["add", "."]).await.unwrap();
        git(&repo, &["commit", "-q", "-m", "second"]).await.unwrap();
        let files = diff(&repo, Some(&base)).await.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].status, "added");
    }

    #[tokio::test]
    async fn repository_helpers_do_not_run_on_read() {
        let dir = TempDir::new();
        let repo = repo(dir.path()).await;
        let marker = dir.path().join("ran");
        let script = dir.path().join("helper.sh");
        std::fs::write(
            &script,
            format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let helper = script.to_string_lossy().into_owned();
        git(&repo, &["config", "diff.external", &helper])
            .await
            .unwrap();
        git(&repo, &["config", "core.fsmonitor", &helper])
            .await
            .unwrap();
        std::fs::write(repo.join("a.txt"), "changed\n").unwrap();
        let files = diff(&repo, None).await.unwrap();
        assert_eq!(files.len(), 1);
        assert!(!marker.exists());
    }

    #[test]
    fn purposes_are_slugs() {
        assert!(valid_purpose("fix-bug-2"));
        assert!(!valid_purpose("../escape"));
        assert!(!valid_purpose("-flag"));
        assert!(!valid_purpose(""));
        assert!(!valid_purpose("Upper"));
    }

    #[test]
    fn name_status_handles_renames() {
        let raw = b"M\0a.txt\0R100\0old.txt\0new.txt\0A\0c.txt\0";
        assert_eq!(
            parse_name_status(raw),
            [
                ("modified".to_owned(), "a.txt".to_owned()),
                ("renamed".to_owned(), "new.txt".to_owned()),
                ("added".to_owned(), "c.txt".to_owned()),
            ]
        );
    }

    #[test]
    fn a_combined_patch_splits_at_its_headers_and_nowhere_else() {
        let patch = concat!(
            "diff --git a/one.txt b/one.txt\n",
            "--- /dev/null\n",
            "+++ b/one.txt\n",
            "@@ -0,0 +1,2 @@\n",
            "+diff --git a/two.txt b/two.txt\n",
            "+-not a header\n",
            "diff --git a/two.txt b/two.txt\n",
            "--- /dev/null\n",
            "+++ b/two.txt\n",
            "@@ -0,0 +1 @@\n",
            "+two\n",
        );
        let blocks = patches_at_headers(patch);
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].starts_with("diff --git a/one.txt"));
        assert!(blocks[0].contains("+-not a header"));
        assert!(blocks[1].starts_with("diff --git a/two.txt"));
    }

    #[test]
    fn a_block_header_accepts_the_spellings_git_quotes_a_path_with() {
        let plain = "diff --git a/spaced name.txt b/spaced name.txt\n--- /dev/null\n";
        assert!(header_names(plain, "spaced name.txt"));
        assert!(!header_names(plain, "other.txt"));
        // A high byte is an octal escape unless `core.quotePath` is off, and a control
        // byte is always escaped.
        assert_eq!(
            quoted_path("\u{fc}\u{f1}\u{ef}.txt", true),
            "\"\\303\\274\\303\\261\\303\\257.txt\""
        );
        assert_eq!(
            quoted_path("\u{fc}\u{f1}\u{ef}.txt", false),
            "\"\u{fc}\u{f1}\u{ef}.txt\""
        );
        let escaped = quoted_path("\u{fc}\u{f1}\u{ef}.txt", true);
        assert!(header_names(
            &format!("diff --git a/{escaped} b/{escaped}\n"),
            "\u{fc}\u{f1}\u{ef}.txt"
        ));
        assert!(header_names(
            "diff --git a/\u{fc}\u{f1}\u{ef}.txt b/\u{fc}\u{f1}\u{ef}.txt\n",
            "\u{fc}\u{f1}\u{ef}.txt"
        ));
        let control = quoted_path("a\tb.txt", false);
        assert!(header_names(
            &format!("diff --git a/{control} b/{control}\n"),
            "a\tb.txt"
        ));
    }

    #[tokio::test]
    async fn a_diff_read_starts_one_git_process_per_untracked_batch_not_per_file() {
        let dir = TempDir::new();
        let repo = repo(dir.path()).await;
        std::fs::write(repo.join("new.txt"), "one\n").unwrap();
        let before = git_spawns::count(&repo);
        let started = std::time::Instant::now();
        let one = diff(&repo, None).await.unwrap();
        let one_took = started.elapsed();
        let one_file = git_spawns::count(&repo) - before;
        assert_eq!(one.len(), 1, "one untracked file is one row");

        for index in 0..129 {
            let name = format!("new-{index:03}.txt");
            std::fs::write(repo.join(&name), format!("line {index}\n")).unwrap();
        }
        let before = git_spawns::count(&repo);
        let started = std::time::Instant::now();
        let many = diff(&repo, None).await.unwrap();
        let many_took = started.elapsed();
        let many_files = git_spawns::count(&repo) - before;
        assert_eq!(many.len(), 130, "130 untracked files are 130 rows");
        println!(
            "1 untracked file: {one_file} git processes in {one_took:?}; \
             130 untracked files: {many_files} git processes in {many_took:?}"
        );
        assert_eq!(
            many_files, one_file,
            "the untracked side must not start a Git process per file"
        );
        assert!(
            many_files <= 8,
            "a diff read started {many_files} Git processes"
        );
    }

    /// The rows a worktree's untracked files produce must not move, so the batched read
    /// is compared with the one-call-per-file read it replaced. The cases are the ones
    /// that decide a patch's shape: empty, no trailing newline, binary, CRLF and
    /// attributes, quoting, and a path that is not quiet in a header.
    #[tokio::test]
    async fn untracked_patches_match_the_per_file_read_byte_for_byte() {
        let dir = TempDir::new();
        let repo = repo(dir.path()).await;
        for args in [
            vec!["config", "core.autocrlf", "true"],
            vec!["config", "core.safecrlf", "false"],
        ] {
            git(&repo, &args).await.unwrap();
        }
        for folder in ["nested", "attrs"] {
            std::fs::create_dir_all(repo.join(folder)).unwrap();
        }
        std::fs::write(repo.join("attrs/.gitattributes"), "*.bin binary\n").unwrap();
        let cases: [(&str, &[u8]); 10] = [
            ("plain.txt", b"one\ntwo\n"),
            ("crlf.txt", b"one\r\ntwo\r\n"),
            ("no-newline.txt", b"a line with no terminator"),
            ("empty.txt", b""),
            ("binary.dat", b"\x00\x01\x02binary\n"),
            ("spaced name.txt", b"spaces\n"),
            ("dash -name.txt", b"dash\n"),
            ("quoted \u{fc}n\u{ef}code.txt", b"unicode\n"),
            ("nested/deep.txt", b"deep\n"),
            ("attrs/x.bin", b"plain bytes that attributes call binary\n"),
        ];
        for (path, bytes) in cases {
            std::fs::write(repo.join(path), bytes).unwrap();
        }
        let files = diff(&repo, None).await.unwrap();
        let rows: std::collections::HashMap<&str, &FileDiff> = files
            .iter()
            .map(|file| (file.path.as_str(), file))
            .collect();
        for (path, _) in cases {
            let expected = untracked_diff(&repo, path).await.unwrap();
            let actual = rows
                .get(path)
                .unwrap_or_else(|| panic!("missing {path}: {files:?}"));
            assert_eq!(actual.patch, expected.patch, "{path} patch");
            assert_eq!(actual.additions, expected.additions, "{path} additions");
            assert_eq!(actual.status, expected.status, "{path} status");
            assert_eq!(actual.deletions, expected.deletions, "{path} deletions");
        }
    }
}
