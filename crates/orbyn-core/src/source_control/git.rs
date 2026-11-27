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
    let mut command = command(cwd);
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

/// The refusal for a purpose that cannot be a worktree folder name.
pub(crate) const PURPOSE_RULE: &str =
    "Use lowercase letters, digits and hyphens for the worktree name.";

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

/// Whether `purpose` already names a registered worktree beside `repo`. Creation finds
/// and reuses it, so a naming option that spends an engine call must not run for it.
pub(crate) async fn reuses_worktree(repo: &Path, purpose: &str) -> Result<bool> {
    Ok(linked_worktrees(repo).await?.iter().any(|registered| {
        registered.path.exists()
            && registered.path.file_name() == Some(std::ffi::OsStr::new(purpose))
    }))
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

/// Clears the registration git keeps for a checkout whose folder is gone. A folder a person deleted
/// by hand leaves its registration behind, and git then refuses every `worktree add` at that path
/// ("missing but already registered worktree"), so each retry fails the same way. Only that one
/// registration is cleared, and only once git calls it prunable: a folder that still exists is never
/// touched, and neither is a locked registration, which its person owns. Best effort — the add that
/// follows reports whatever is still in the way.
pub(crate) async fn clear_missing_worktree(repo: &Path, path: &Path) {
    let Ok(records) = worktree_records(repo).await else {
        return;
    };
    if !records
        .iter()
        .any(|record| record.prunable && same_checkout_path(&record.path, path))
    {
        return;
    }
    let path_text = path.to_string_lossy().into_owned();
    if let Err(error) = git(repo, &["worktree", "remove", "--force", &path_text]).await {
        tracing::warn!(%error, "could not clear a deleted worktree's registration");
    }
}

/// Whether two spellings name the same checkout folder. `path` may be gone, so the folders that hold
/// them are what the filesystem resolves: git's Windows spelling differs from ours.
fn same_checkout_path(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    let (Some(left_holder), Some(right_holder)) = (
        left.parent()
            .and_then(|parent| crate::util::canonicalize(parent).ok()),
        right
            .parent()
            .and_then(|parent| crate::util::canonicalize(parent).ok()),
    ) else {
        return false;
    };
    left.file_name() == right.file_name() && left_holder == right_holder
}

/// Finds or creates the purpose's worktree at `<repo>-wt/<purpose>`. A registered
/// worktree already at that path is reused; its base is the base this call admits,
/// as a new one records, and where it left the primary clone's HEAD when it admits
/// none.
pub(crate) async fn create_worktree(
    repo: &Path,
    purpose: &str,
    options: &WorktreeOptions,
) -> Result<Worktree> {
    if !valid_purpose(purpose) {
        return Err(CoreError::Invalid(PURPOSE_RULE.into()));
    }
    let path = worktree_path(&primary_checkout(repo).await?, purpose)?;
    let path_text = path.to_string_lossy().into_owned();
    let head = text(&git(repo, &["rev-parse", "HEAD"]).await?);
    for registered in linked_worktrees(repo).await? {
        // Keep already registered worktrees in place, including earlier layouts.
        if registered.path.exists()
            && registered.path.file_name() == Some(std::ffi::OsStr::new(purpose))
        {
            // The base this call admits, exactly as a new worktree records it: the run's repository
            // checks are read at that commit, so the verifier must read that same one. With none
            // named, the reused branch's fork point is where its work began.
            let base = match worktree_start(repo, options).await {
                Some(start) => start,
                None => text(&git(repo, &["merge-base", "HEAD", &registered.revision]).await?),
            };
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
    // the request as a ref. Semantic naming and a written custom name read the project's
    // local branches first: semantic naming numbers a taken slug, and a custom name that
    // is already a branch is unusable, so the purpose stays its complete name.
    let existing = if options.naming.mode == NamingMode::Semantic || options.naming.custom.is_some()
    {
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
    // A folder deleted by hand still holds this path in git's records, and the add below would be
    // refused for it: that registration goes first.
    clear_missing_worktree(repo, &path).await;
    git(
        repo,
        &["worktree", "add", "-b", &branch, &path_text, &start],
    )
    .await?;
    populate_submodules(&path, &options.submodules).await;
    Ok(Worktree {
        path: path_text,
        branch,
        base: start,
    })
}

/// Populates a worktree's submodules the way the `submodules` mode asks (`recursive`, `top-level`, or
/// none). Every path that makes a worktree calls it, so a checkout recovered from a branch an earlier
/// attempt left out has the same contents as a new one. A submodule that cannot be fetched leaves the
/// worktree usable.
pub(crate) async fn populate_submodules(path: &Path, mode: &str) {
    let submodules: &[&str] = match mode {
        "none" | "skip" => &[],
        "top-level" | "top_level" => &["submodule", "update", "--init"],
        _ => &["submodule", "update", "--init", "--recursive"],
    };
    if submodules.is_empty() || !path.join(".gitmodules").exists() {
        return;
    }
    if let Err(error) = git(path, submodules).await {
        tracing::warn!(%error, "could not populate submodules");
    }
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
    let paths: Vec<_> = listing
        .split(|byte| *byte == 0)
        .filter(|p| !p.is_empty())
        .collect();
    let attributes = untracked_diff_attributes(cwd, &listing, &paths).await?;
    for (path, use_git) in paths.into_iter().zip(attributes) {
        let path = String::from_utf8_lossy(path);
        files.push(if use_git {
            git_untracked_diff(cwd, &path).await?
        } else {
            untracked_diff(cwd, &path).await?
        });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

/// Check every name in one process; custom drivers and -diff need Git's own classification.
async fn untracked_diff_attributes(
    cwd: &Path,
    listing: &[u8],
    paths: &[&[u8]],
) -> Result<Vec<bool>> {
    use tokio::io::AsyncWriteExt;
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    // check-attr prints the same words for boolean states and drivers literally named
    // "set" or "unspecified". If either driver configures binary classification,
    // conservatively let Git resolve the ambiguous value.
    let drivers = git_raw(
        cwd,
        &[
            "config",
            "--name-only",
            "--get-regexp",
            r"^diff\.(set|unspecified)\.binary$",
        ],
    )
    .await?;
    if !drivers.status.success() && drivers.status.code() != Some(1) {
        return Err(CoreError::Git(text(&drivers.stderr)));
    }
    let configured = String::from_utf8_lossy(&drivers.stdout);
    let named_set = configured.lines().any(|name| name == "diff.set.binary");
    let named_unspecified = configured
        .lines()
        .any(|name| name == "diff.unspecified.binary");
    let mut cmd = command(cwd);
    cmd.args(SAFE_CONFIG)
        .args(["check-attr", "-z", "--stdin", "diff"])
        .stdin(Stdio::piped());
    let mut child = crate::host::process::ManagedChild::spawn(cmd)
        .map_err(|error| CoreError::Git(format!("Could not run git: {error}")))?;
    let mut stdin = child
        .child_mut()
        .stdin
        .take()
        .ok_or_else(|| CoreError::Git("Could not open git stdin".into()))?;
    let write = async move {
        let result = stdin.write_all(listing).await;
        drop(stdin);
        result
    };
    let (written, output) = tokio::join!(write, child.output());
    written.map_err(|error| CoreError::Git(format!("Could not write git input: {error}")))?;
    let output = output.map_err(|error| CoreError::Git(format!("Could not run git: {error}")))?;
    if !output.status.success() {
        return Err(CoreError::Git(text(&output.stderr)));
    }
    let fields: Vec<_> = output.stdout.split(|byte| *byte == 0).collect();
    if fields.len() != paths.len() * 3 + 1 || fields.last() != Some(&&b""[..]) {
        return Err(CoreError::Git("Invalid git check-attr output".into()));
    }
    paths
        .iter()
        .zip(fields.as_chunks::<3>().0)
        .map(|(path, fields)| {
            if fields[0] != *path || fields[1] != b"diff" {
                return Err(CoreError::Git("Invalid git check-attr output".into()));
            }
            Ok(match fields[2] {
                b"unspecified" => named_unspecified,
                b"set" => named_set,
                _ => true,
            })
        })
        .collect()
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
    // One call carries every file's patch. Its headers line up with the status entries only when
    // both reads saw the same files in the same order, so a count or a header that does not match
    // falls back to one call per file, the read this replaces.
    let mut patch_args = vec!["diff", "--no-ext-diff", "--no-textconv"];
    patch_args.extend(range);
    let together = git(cwd, &patch_args).await?;
    let chunks = split_git_patch(&together);
    let entries = parse_name_status(&statuses);
    let batched = chunks.len() == entries.len()
        && entries
            .iter()
            .zip(chunks.iter().copied())
            .all(|((status, path), chunk)| {
                matches!(status.as_str(), "renamed" | "copied") || patch_chunk_is_for(chunk, path)
            });
    let mut files = Vec::with_capacity(entries.len());
    for (at, (status, path)) in entries.into_iter().enumerate() {
        // A rename or copy reads differently when Git limits the diff to the new path, and that is
        // the chart this read has always handed the UI; keep the one call those entries need.
        let renamed = matches!(status.as_str(), "renamed" | "copied");
        let patch = if batched && !renamed {
            String::from_utf8_lossy(chunks[at]).into_owned()
        } else {
            per_file_patch(cwd, range, &path).await?
        };
        let (additions, deletions) = counts.get(&path).copied().unwrap_or((0, 0));
        files.push(FileDiff {
            path,
            status,
            additions,
            deletions,
            patch,
            repo_id: None,
        });
    }
    Ok(files)
}

/// One file's patch, the call the batched read replaces: a rename or copy keeps it, and so does
/// any read whose combined patch could not be lined up with the status entries.
async fn per_file_patch(cwd: &Path, range: &[&str], path: &str) -> Result<String> {
    let mut patch_args = vec!["diff", "--no-ext-diff", "--no-textconv"];
    patch_args.extend(range);
    let literal = format!(":(top,literal){path}");
    patch_args.extend(["--", &literal]);
    Ok(String::from_utf8_lossy(&git(cwd, &patch_args).await?).into_owned())
}

/// A combined patch cut back into one chunk per file, each keeping its own diff --git header.
/// Every hunk line starts with a space, '+', '-' or '\', so only a header starts a line this way.
fn split_git_patch(patch: &[u8]) -> Vec<&[u8]> {
    let mut chunks = Vec::new();
    let mut start = None;
    let mut at = 0;
    while at < patch.len() {
        let end = patch[at..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(patch.len(), |offset| at + offset + 1);
        if patch[at..end].starts_with(b"diff --git ") {
            if let Some(begin) = start {
                chunks.push(&patch[begin..at]);
            }
            start = Some(at);
        }
        at = end;
    }
    if let Some(begin) = start {
        chunks.push(&patch[begin..]);
    }
    chunks
}

/// Whether a chunk's header names path on its new side: the default "a/<path> b/<path>", a
/// mnemonic "c/<path> w/<path>", or the no-prefix "<path> <path>" form. The whole header is
/// compared, so the trailing " b/x" of a header for "folder b/x" cannot stand in for "x"; a header
/// that reads two ways is left to the per-file fallback. Renames and copies never reach here, so
/// both sides of the header name the same path.
fn patch_chunk_is_for(chunk: &[u8], path: &str) -> bool {
    let Some(header) = chunk.split(|byte| *byte == b'\n').next() else {
        return false;
    };
    let Some(rest) = header.strip_prefix(b"diff --git ") else {
        return false;
    };
    let path = path.as_bytes();
    if rest.first() == Some(&b'"') {
        // The quoted form keeps the path's spaces inside the quotes; unescape it, then compare
        // both complete fields.
        return quoted_header_fields(header).is_some_and(|(source, destination)| {
            header_fields_are_for(&source, &destination, path)
        });
    }
    // An unquoted path can hold spaces, so its two fields cannot be split at a space. Their widths
    // follow from the path itself, and a space has to separate them.
    let fields = if rest.len() == 2 * path.len() + 5 && rest[2 + path.len()] == b' ' {
        Some((&rest[..2 + path.len()], &rest[3 + path.len()..]))
    } else if rest.len() == 2 * path.len() + 1 && rest[path.len()] == b' ' {
        Some((&rest[..path.len()], &rest[path.len() + 1..]))
    } else {
        None
    };
    fields.is_some_and(|(source, destination)| header_fields_are_for(source, destination, path))
}

/// Whether a header's two complete fields name path: both on their own for "diff.noprefix", or both
/// behind a one-byte prefix, the default "a/" and "b/" or a mnemonic pair such as "c/" and "w/".
/// Two equal prefixes can be read as either form, so they are left to the per-file fallback; Git
/// never writes them for a non-rename.
fn header_fields_are_for(source: &[u8], destination: &[u8], path: &[u8]) -> bool {
    if source == path && destination == path {
        return true;
    }
    let (Some((source_prefix, source_path)), Some((destination_prefix, destination_path))) = (
        strip_git_path_prefix(source),
        strip_git_path_prefix(destination),
    ) else {
        return false;
    };
    source_prefix != destination_prefix && source_path == path && destination_path == path
}

/// The one-byte Git diff prefix ("a/", "b/", "c/", "w/") and the path behind it; None when the
/// field carries no such prefix.
fn strip_git_path_prefix(field: &[u8]) -> Option<(u8, &[u8])> {
    (field.len() >= 3 && field[0].is_ascii_alphanumeric() && field[1] == b'/')
        .then(|| (field[0], &field[2..]))
}

/// The two complete paths a quoted diff --git header names, with Git's C-style quoting undone; None
/// when the header is not the quoted form.
fn quoted_header_fields(header: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let rest = header.strip_prefix(b"diff --git ")?;
    let (source, rest) = unquote_git_path(rest)?;
    let rest = rest.strip_prefix(b" ")?;
    if rest.first() == Some(&b'"') {
        let (destination, rest) = unquote_git_path(rest)?;
        return rest.is_empty().then_some((source, destination));
    }
    Some((source, rest.to_vec()))
}

/// Git's quoted path syntax: a leading quote, C-style escapes and octal bytes, then a closing
/// quote. Returns the unescaped bytes and the input after the closing quote.
fn unquote_git_path(input: &[u8]) -> Option<(Vec<u8>, &[u8])> {
    let mut rest = input.strip_prefix(b"\"")?;
    let mut path = Vec::new();
    loop {
        let byte = *rest.first()?;
        rest = &rest[1..];
        match byte {
            b'"' => return Some((path, rest)),
            b'\\' => {
                let escape = *rest.first()?;
                rest = &rest[1..];
                match escape {
                    b'n' => path.push(b'\n'),
                    b't' => path.push(b'\t'),
                    b'a' => path.push(0x07),
                    b'b' => path.push(0x08),
                    b'f' => path.push(0x0c),
                    b'r' => path.push(b'\r'),
                    b'v' => path.push(0x0b),
                    b'"' => path.push(b'"'),
                    b'\\' => path.push(b'\\'),
                    b'0'..=b'7' => {
                        let mut value = u32::from(escape - b'0');
                        for _ in 0..2 {
                            let Some(next) = rest.first() else { break };
                            if !(b'0'..=b'7').contains(next) {
                                break;
                            }
                            value = value * 8 + u32::from(next - b'0');
                            rest = &rest[1..];
                        }
                        path.push(u8::try_from(value).ok()?);
                    }
                    _ => return None,
                }
            }
            byte => path.push(byte),
        }
    }
}

async fn untracked_diff(cwd: &Path, path: &str) -> Result<FileDiff> {
    let Some(patch) = synthesized_untracked_patch(cwd, path) else {
        // A binary, symlinked, oddly named or unreadable file keeps the one Git call this read
        // has always made, so its patch stays byte for byte the one Git writes.
        return git_untracked_diff(cwd, path).await;
    };
    let additions = patch
        .lines()
        .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
        .count() as u32;
    Ok(FileDiff {
        path: path.to_owned(),
        status: "added".into(),
        additions,
        deletions: 0,
        patch,
        repo_id: None,
    })
}

/// Files larger than this keep Git's own patch: the file and its patch would otherwise both sit
/// in memory at file size, and a patch that large is unfit for the Changes read anyway. One Git
/// call answers byte for byte.
const MAX_SYNTHESIZED_UNTRACKED_BYTES: u64 = 8 * 1024 * 1024;

/// The patch Git writes for a new file, built from the file's own bytes instead of a run of Git
/// per file. None when Git should answer: a binary file, a symlink, a path Git would quote, a file
/// over the synthesis limit, or a file that cannot be read. Raw bytes go in unchanged, so line
/// endings stay the file's own; the index line Git prints is left out because its hash covers the
/// converted content.
fn synthesized_untracked_patch(cwd: &Path, path: &str) -> Option<String> {
    if !path
        .bytes()
        .all(|byte| (b' '..=b'~').contains(&byte) && byte != b'"' && byte != b'\\')
    {
        return None;
    }
    let file = cwd.join(path);
    let metadata = std::fs::symlink_metadata(&file).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_SYNTHESIZED_UNTRACKED_BYTES {
        return None;
    }
    let content = std::fs::read(&file).ok()?;
    if content.contains(&0) {
        return None;
    }
    let mut patch = Vec::new();
    patch.extend_from_slice(b"diff --git a/");
    patch.extend_from_slice(path.as_bytes());
    patch.extend_from_slice(b" b/");
    patch.extend_from_slice(path.as_bytes());
    patch.push(b'\n');
    patch.extend_from_slice(b"new file mode ");
    patch.extend_from_slice(untracked_file_mode(&metadata).as_bytes());
    patch.push(b'\n');
    if content.is_empty() {
        return Some(String::from_utf8_lossy(&patch).into_owned());
    }
    let lines = count_file_lines(&content);
    patch.extend_from_slice(b"--- /dev/null\n+++ b/");
    patch.extend_from_slice(path.as_bytes());
    // Git separates a path that contains a space from the rest of the +++ line with a tab.
    if path.contains(' ') {
        patch.push(b'\t');
    }
    patch.push(b'\n');
    if lines == 1 {
        patch.extend_from_slice(b"@@ -0,0 +1 @@\n");
    } else {
        patch.extend_from_slice(format!("@@ -0,0 +1,{lines} @@\n").as_bytes());
    }
    for line in content.split_inclusive(|byte| *byte == b'\n') {
        patch.push(b'+');
        patch.extend_from_slice(line);
    }
    if !content.ends_with(b"\n") {
        // The marker is a line of its own after the added line's terminator.
        patch.push(b'\n');
        patch.extend_from_slice(b"\\ No newline at end of file\n");
    }
    Some(String::from_utf8_lossy(&patch).into_owned())
}

fn count_file_lines(content: &[u8]) -> usize {
    if content.is_empty() {
        return 0;
    }
    let newlines = content.iter().filter(|byte| **byte == b'\n').count();
    newlines + usize::from(!content.ends_with(b"\n"))
}

/// The file mode Git records for a new file: only the owner's execute bit changes it.
#[cfg(unix)]
fn untracked_file_mode(metadata: &std::fs::Metadata) -> &'static str {
    use std::os::unix::fs::PermissionsExt;
    if metadata.permissions().mode() & 0o100 != 0 {
        "100755"
    } else {
        "100644"
    }
}

#[cfg(not(unix))]
fn untracked_file_mode(_metadata: &std::fs::Metadata) -> &'static str {
    "100644"
}

/// One new file through Git, for the cases the synthesis cannot reproduce byte for byte.
async fn git_untracked_diff(cwd: &Path, path: &str) -> Result<FileDiff> {
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
    let additions = patch
        .lines()
        .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
        .count() as u32;
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

#[cfg(test)]
mod tests {
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

    /// The name rule must never accept a name real Git refuses to create: a written
    /// custom name that passes it would become `invalid_input` instead of falling back.
    #[tokio::test]
    async fn the_branch_name_rule_never_accepts_a_name_git_refuses() {
        let dir = TempDir::new();
        let repo = repo(dir.path()).await;
        let long = "x".repeat(65);
        let names = [
            "HEAD",
            "head",
            "Head",
            "HEAD/next",
            "main",
            "release/login-42",
            "v1.2",
            "-leading",
            "foo.",
            "foo..bar",
            "foo//bar",
            "foo.lock",
            "a b",
            "a~b",
            "a^b",
            "a:b",
            "a?b",
            "a*b",
            "a[b",
            "a\\b",
            long.as_str(),
        ];
        for name in names {
            if branch_names::is_valid_branch_name(name) {
                assert!(
                    git(&repo, &["check-ref-format", "--branch", name])
                        .await
                        .is_ok(),
                    "the rule accepted {name:?}, which Git refuses"
                );
            }
        }
        // The reported gap: HEAD passes Git's general ref rules, but no branch may be HEAD.
        assert!(!branch_names::is_valid_branch_name("HEAD"));
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

    /// The read the batched tracked diff replaces: one Git call per file. It proves the entries
    /// keep their order, status, counts and exact patches.
    async fn per_file_tracked_diff(cwd: &Path, range: &[&str]) -> Vec<FileDiff> {
        let root = checkout_root(cwd).await.unwrap();
        let cwd = root.as_path();
        let mut statuses_args = vec![
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--name-status",
            "-z",
        ];
        statuses_args.extend(range);
        let statuses = git(cwd, &statuses_args).await.unwrap();
        let mut numstat_args = vec!["diff", "--no-ext-diff", "--no-textconv", "--numstat", "-z"];
        numstat_args.extend(range);
        let counts = parse_numstat(&git(cwd, &numstat_args).await.unwrap());
        let mut files = Vec::new();
        for (status, path) in parse_name_status(&statuses) {
            let (additions, deletions) = counts.get(&path).copied().unwrap_or((0, 0));
            files.push(FileDiff {
                patch: per_file_patch(cwd, range, &path).await.unwrap(),
                path,
                status,
                additions,
                deletions,
                repo_id: None,
            });
        }
        files
    }

    #[tokio::test]
    async fn tracked_patches_stay_identical_to_one_git_call_per_file() {
        let dir = TempDir::new();
        let repo = repo(dir.path()).await;
        std::fs::create_dir_all(repo.join("dir with space")).unwrap();
        for (path, content) in [
            ("plain.txt", "one\n"),
            ("binary.dat", "not yet"),
            ("unicodé.txt", "u\n"),
            ("gone.txt", "bye\n"),
            ("rename-src.txt", "alpha\nbravo\ncharlie\n"),
            ("dir with space/nested.txt", "n\n"),
            ("splitter.txt", "start\n"),
            ("noeol.txt", "one\ntwo"),
            ("empty.txt", "e\n"),
        ] {
            std::fs::write(repo.join(path), content).unwrap();
        }
        std::fs::write(repo.join("binary.dat"), [0u8, 1, 2, 3, 0, 255, 65]).unwrap();
        git(&repo, &["add", "."]).await.unwrap();
        git(&repo, &["commit", "-qm", "before"]).await.unwrap();

        std::fs::write(repo.join("plain.txt"), "one\ntwo\n").unwrap();
        std::fs::write(repo.join("binary.dat"), [0u8, 9, 9, 0, 1]).unwrap();
        std::fs::remove_file(repo.join("gone.txt")).unwrap();
        // The new line looks like a file header but starts with '+' inside the hunk.
        std::fs::write(
            repo.join("splitter.txt"),
            "start\ndiff --git a/fake b/fake\n",
        )
        .unwrap();
        std::fs::write(repo.join("noeol.txt"), "one\ntwo\n").unwrap();
        std::fs::write(repo.join("empty.txt"), "").unwrap();
        std::fs::write(repo.join("dir with space/nested.txt"), "n\nmore\n").unwrap();
        std::fs::write(repo.join("unicodé.txt"), "u\nmore\n").unwrap();
        git(&repo, &["mv", "rename-src.txt", "rename-dst.txt"])
            .await
            .unwrap();
        std::fs::write(repo.join("staged-add.txt"), "added\n").unwrap();
        git(&repo, &["add", "staged-add.txt"]).await.unwrap();

        let batched = tracked_diff(&repo, &["HEAD"]).await.unwrap();
        let expected = per_file_tracked_diff(&repo, &["HEAD"]).await;
        assert_eq!(batched, expected);
        assert!(
            batched.iter().any(|file| file.status == "renamed"),
            "the fixture must exercise the rename fallback"
        );
        assert!(
            batched.iter().any(|file| file.path == "unicodé.txt"),
            "the fixture must exercise a quoted header"
        );
    }

    #[tokio::test]
    async fn a_diff_read_spawns_the_same_number_of_commands_for_one_file_and_many() {
        let dir = TempDir::new();
        let repo = repo(dir.path()).await;
        std::fs::create_dir_all(repo.join("tracked")).unwrap();
        for at in 0..20 {
            std::fs::write(repo.join(format!("tracked/file-{at:02}.txt")), "one\n").unwrap();
        }
        git(&repo, &["add", "tracked"]).await.unwrap();
        git(&repo, &["commit", "-qm", "tracked"]).await.unwrap();

        std::fs::write(repo.join("tracked/file-00.txt"), "two\n").unwrap();
        std::fs::write(repo.join("single-untracked.txt"), "one\n").unwrap();
        crate::host::process::reset_spawned_on_this_thread();
        let few = diff(&repo, None).await.unwrap();
        let few_spawns = crate::host::process::spawned_on_this_thread();
        assert_eq!(few.len(), 2);

        std::fs::create_dir_all(repo.join("bulk")).unwrap();
        for at in 0..40 {
            std::fs::write(
                repo.join(format!("bulk/file-{at:03}.txt")),
                format!("marker {at}\n"),
            )
            .unwrap();
        }
        for at in 1..20 {
            std::fs::write(repo.join(format!("tracked/file-{at:02}.txt")), "two\n").unwrap();
        }
        crate::host::process::reset_spawned_on_this_thread();
        let many = diff(&repo, None).await.unwrap();
        let many_spawns = crate::host::process::spawned_on_this_thread();

        assert_eq!(many.len(), 1 + 20 + 40);
        let bulk = many
            .iter()
            .find(|file| file.path == "bulk/file-000.txt")
            .unwrap();
        assert_eq!(bulk.status, "added");
        assert_eq!(bulk.additions, 1);
        assert!(bulk.patch.contains("+marker 0"));
        assert_eq!(
            many_spawns, few_spawns,
            "the diff read must not start more programs as the change grows"
        );
        assert!(
            many_spawns <= 8,
            "the diff read spawned {many_spawns} programs for 60 files"
        );
    }

    #[tokio::test]
    async fn untracked_patches_stay_identical_to_git_apart_from_the_index_line() {
        let dir = TempDir::new();
        let repo = repo(dir.path()).await;
        // Read the files as raw bytes, which is what Git does without line-ending conversion.
        crate::testing::run_git(&repo, &["config", "core.autocrlf", "false"]);
        for (path, content) in [
            ("plain.txt", b"one\ntwo\n".to_vec()),
            ("single.txt", b"only\n".to_vec()),
            ("noeol.txt", b"alpha\nbeta".to_vec()),
            ("empty.txt", Vec::new()),
            ("blank-lines.txt", b"a\n\n\nb\n".to_vec()),
            ("crlf.txt", b"one\r\ntwo\r\n".to_vec()),
            ("space name.txt", b"space\n".to_vec()),
            ("splitter.txt", b"x\ndiff --git a/fake b/fake\n".to_vec()),
            ("plusplus.txt", b"++line\n".to_vec()),
        ] {
            std::fs::write(repo.join(path), content).unwrap();
        }
        let files = diff(&repo, None).await.unwrap();
        // A developer's global Git config changes the reference patch without changing the
        // file: diff.noprefix drops the a//b/ prefixes that synthesis always writes, and
        // color.ui=always adds escape bytes. Set both on this repository so the reference
        // command below has to pin the compared shape itself.
        crate::testing::run_git(&repo, &["config", "diff.noprefix", "true"]);
        crate::testing::run_git(&repo, &["config", "color.ui", "always"]);
        for (path, content) in [
            ("plain.txt", b"one\ntwo\n".to_vec()),
            ("single.txt", b"only\n".to_vec()),
            ("noeol.txt", b"alpha\nbeta".to_vec()),
            ("empty.txt", Vec::new()),
            ("blank-lines.txt", b"a\n\n\nb\n".to_vec()),
            ("crlf.txt", b"one\r\ntwo\r\n".to_vec()),
            ("space name.txt", b"space\n".to_vec()),
            ("splitter.txt", b"x\ndiff --git a/fake b/fake\n".to_vec()),
            ("plusplus.txt", b"++line\n".to_vec()),
        ] {
            let file = files.iter().find(|file| file.path == path).unwrap();
            let output = git_raw(
                &repo,
                &[
                    "diff",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--no-color",
                    "--src-prefix=a/",
                    "--dst-prefix=b/",
                    "--no-index",
                    "--",
                    null_device(),
                    path,
                ],
            )
            .await
            .unwrap();
            assert_eq!(output.status.code(), Some(1), "git --no-index {path}");
            assert_eq!(
                file.patch.as_bytes(),
                without_index_line(&output.stdout),
                "{path}"
            );
            if path != "plusplus.txt" {
                assert_eq!(
                    file.additions as usize,
                    count_file_lines(&content),
                    "{path}"
                );
            }
        }
    }

    #[tokio::test]
    async fn untracked_diff_respects_binary_diff_attributes() {
        let dir = TempDir::new();
        let repo = repo(dir.path()).await;
        std::fs::write(
            repo.join(".gitattributes"),
            "*.bin -diff\n*.custom diff=opaque\n*.namedset diff=set\n*.namedunspecified diff=unspecified\n",
        )
        .unwrap();
        crate::testing::run_git(&repo, &["config", "diff.opaque.binary", "true"]);
        crate::testing::run_git(&repo, &["config", "diff.set.binary", "true"]);
        crate::testing::run_git(&repo, &["config", "diff.unspecified.binary", "true"]);
        for path in [
            "plain.bin",
            "named.custom",
            "x.namedset",
            "x.namedunspecified",
            "ordinary.txt",
        ] {
            std::fs::write(repo.join(path), "plain text without NUL\n").unwrap();
        }
        let files = diff(&repo, None).await.unwrap();
        for path in [
            "plain.bin",
            "named.custom",
            "x.namedset",
            "x.namedunspecified",
        ] {
            let file = files.iter().find(|file| file.path == path).unwrap();
            let expected = git_untracked_diff(&repo, path).await.unwrap();
            assert!(expected.patch.contains("Binary files"), "{path}");
            assert_eq!(file.patch, expected.patch, "{path}");
            assert_eq!(file.additions, 0, "{path}");
        }
        assert!(
            files
                .iter()
                .find(|file| file.path == "ordinary.txt")
                .unwrap()
                .patch
                .contains("+plain text")
        );
    }

    #[tokio::test]
    async fn binary_and_quoted_untracked_files_keep_gits_own_patch() {
        let dir = TempDir::new();
        let repo = repo(dir.path()).await;
        crate::testing::run_git(&repo, &["config", "core.autocrlf", "false"]);
        std::fs::write(repo.join("binary.dat"), [0u8, 1, 2, 3, 0, 255]).unwrap();
        std::fs::write(repo.join("unicodé.txt"), "u\n").unwrap();
        let files = diff(&repo, None).await.unwrap();
        for path in ["binary.dat", "unicodé.txt"] {
            let file = files.iter().find(|file| file.path == path).unwrap();
            let output = git_raw(
                &repo,
                &[
                    "diff",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--no-index",
                    "--",
                    null_device(),
                    path,
                ],
            )
            .await
            .unwrap();
            assert_eq!(
                file.patch,
                String::from_utf8_lossy(&output.stdout),
                "{path} must keep Git's own patch"
            );
        }
    }

    #[test]
    fn an_untracked_file_over_the_synthesis_limit_keeps_gits_own_patch() {
        let dir = TempDir::new();
        let file = dir.path().join("large.txt");
        std::fs::write(&file, vec![b'a'; MAX_SYNTHESIZED_UNTRACKED_BYTES as usize]).unwrap();
        assert!(
            synthesized_untracked_patch(dir.path(), "large.txt").is_some(),
            "a file at the limit is still synthesized"
        );
        let mut append = std::fs::OpenOptions::new()
            .append(true)
            .open(&file)
            .unwrap();
        std::io::Write::write_all(&mut append, b"a").unwrap();
        assert!(
            synthesized_untracked_patch(dir.path(), "large.txt").is_none(),
            "a file over the limit keeps Git's own patch"
        );
    }

    fn without_index_line(patch: &[u8]) -> Vec<u8> {
        patch
            .split_inclusive(|byte| *byte == b'\n')
            .filter(|line| !line.starts_with(b"index "))
            .flatten()
            .copied()
            .collect()
    }

    fn null_device() -> &'static str {
        if cfg!(windows) { "NUL" } else { "/dev/null" }
    }

    #[test]
    fn a_combined_patch_splits_only_at_file_headers() {
        let patch = b"diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+diff --git a/fake b/fake\ndiff --git \"a/unicod\\303\\251.txt\" \"b/unicod\\303\\251.txt\"\nnew file mode 100644\n";
        let chunks = split_git_patch(patch);
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].ends_with(b"+diff --git a/fake b/fake\n"));
        assert!(patch_chunk_is_for(chunks[0], "a.txt"));
        assert!(patch_chunk_is_for(chunks[1], "unicodé.txt"));
        assert!(!patch_chunk_is_for(chunks[1], "a.txt"));
    }

    #[test]
    fn a_header_must_name_the_whole_path() {
        let header = |line: &str| format!("{line}\n").into_bytes();
        // The trailing " b/x" belongs to the path "folder b/x", not to "x".
        let spaced = header("diff --git a/folder b/x b/folder b/x");
        assert!(patch_chunk_is_for(&spaced, "folder b/x"));
        assert!(!patch_chunk_is_for(&spaced, "x"));
        // A one-letter first component is not a prefix when the header carries none.
        let bare = header("diff --git x/file x/file");
        assert!(patch_chunk_is_for(&bare, "x/file"));
        assert!(!patch_chunk_is_for(&bare, "file"));
        // Behind the default prefixes the same path still matches.
        assert!(patch_chunk_is_for(
            &header("diff --git a/x/file b/x/file"),
            "x/file"
        ));
        // Mnemonic prefixes are understood, and both sides must name the path.
        assert!(patch_chunk_is_for(
            &header("diff --git c/plain.txt w/plain.txt"),
            "plain.txt"
        ));
        assert!(!patch_chunk_is_for(
            &header("diff --git c/plain.txt w/other.txt"),
            "plain.txt"
        ));
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
}
