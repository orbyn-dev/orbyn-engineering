//! The git behind the merge queue and the watch on main. Every operation happens in a scratch worktree of
//! its own, a detached checkout under the data folder, so a person's checkout is never touched. The target
//! branch moves only by a compare-and-swap `update-ref`, and only when no worktree has it checked out. Nothing here
//! fetches, pushes or contacts a remote: the merge is local.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{CoreError, Result};
use crate::source_control::git::{git, git_hardened, git_raw, text};
use crate::util::new_id;

/// The identity of a merge or revert commit the crew makes.
const IDENTITY: [&str; 4] = [
    "-c",
    "user.name=Orbyn Crew",
    "-c",
    "user.email=crew@orbyn.invalid",
];

/// A detached checkout used for one operation. `finish` removes it; a crash leaves a folder `git worktree prune` clears.
pub struct Scratch {
    pub dir: PathBuf,
    repo: PathBuf,
    removed: bool,
}

/// A scratch checkout is removed on every path: `finish` on the normal ones, and this when the owner is dropped without it,
/// as when a task panics or is cancelled. Removal here is synchronous and best effort.
impl Drop for Scratch {
    fn drop(&mut self) {
        if self.removed {
            return;
        }
        let _ = std::process::Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(&self.dir)
            .current_dir(&self.repo)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Scratch {
    pub async fn at(repo: &Path, root: &Path, rev: &str) -> Result<Self> {
        std::fs::create_dir_all(root)?;
        let dir = root.join(new_id());
        let _ = git(repo, &["worktree", "prune"]).await;
        git(
            repo,
            &["worktree", "add", "--detach", &dir.to_string_lossy(), rev],
        )
        .await?;
        Ok(Self {
            dir,
            repo: repo.to_owned(),
            removed: false,
        })
    }

    pub async fn finish(mut self) {
        self.removed = true;
        let _ = git(
            &self.repo,
            &["worktree", "remove", "--force", &self.dir.to_string_lossy()],
        )
        .await;
        let _ = std::fs::remove_dir_all(&self.dir);
    }

    /// Moves this checkout to another commit, detached.
    pub async fn checkout(&self, rev: &str) -> Result<()> {
        git(&self.dir, &["checkout", "--detach", "-q", "--force", rev]).await?;
        Ok(())
    }

    pub async fn head(&self) -> Result<String> {
        rev(&self.dir, "HEAD").await
    }
}

/// The commit a name points at.
pub async fn rev(repo: &Path, name: &str) -> Result<String> {
    Ok(text(
        &git(
            repo,
            &["rev-parse", "--verify", "-q", &format!("{name}^{{commit}}")],
        )
        .await?,
    ))
}

/// Whether `commit` is part of `branch`'s history now.
pub async fn contains(repo: &Path, branch: &str, commit: &str) -> Result<bool> {
    let out = git_raw(
        repo,
        &[
            "merge-base",
            "--is-ancestor",
            commit,
            &format!("refs/heads/{branch}"),
        ],
    )
    .await?;
    Ok(out.status.success())
}

/// The folder that has `branch` checked out, if any.
pub async fn checked_out(repo: &Path, branch: &str) -> Result<Option<String>> {
    let listing = text(&git(repo, &["worktree", "list", "--porcelain"]).await?);
    let want = format!("branch refs/heads/{branch}");
    let mut folder = String::new();
    for line in listing.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            folder = path.to_owned();
        } else if line == want {
            return Ok(Some(folder));
        }
    }
    Ok(None)
}

/// Why a merge or a revert did not happen.
#[derive(Debug)]
pub enum Refused {
    Conflict,
    Other(String),
}

/// `git merge --no-ff` of `candidate` into the scratch checkout. The merge commit is the result.
pub async fn merge(
    scratch: &Scratch,
    candidate: &str,
    message: &str,
) -> std::result::Result<String, Refused> {
    let mut args = IDENTITY.to_vec();
    args.extend(["merge", "--no-ff", "--no-edit", "-m", message, candidate]);
    let out = git_raw(&scratch.dir, &args)
        .await
        .map_err(|e| Refused::Other(e.public_message()))?;
    if out.status.success() {
        return scratch
            .head()
            .await
            .map_err(|e| Refused::Other(e.public_message()));
    }
    let _ = git(&scratch.dir, &["merge", "--abort"]).await;
    Err(Refused::Conflict)
}

/// `git revert -m 1` of a merge commit in the scratch checkout. The revert commit is the result.
pub async fn revert(scratch: &Scratch, merge_commit: &str) -> std::result::Result<String, Refused> {
    let mut args = IDENTITY.to_vec();
    args.extend(["revert", "-m", "1", "--no-edit", merge_commit]);
    let out = git_raw(&scratch.dir, &args)
        .await
        .map_err(|e| Refused::Other(e.public_message()))?;
    if out.status.success() {
        return scratch
            .head()
            .await
            .map_err(|e| Refused::Other(e.public_message()));
    }
    let _ = git(&scratch.dir, &["revert", "--abort"]).await;
    Err(Refused::Conflict)
}

/// Moves `branch` from `old` to `new` only if it still is at `old`.
pub async fn move_branch(repo: &Path, branch: &str, new: &str, old: &str) -> Result<()> {
    git(
        repo,
        &[
            "update-ref",
            "-m",
            "orbyn crew",
            &format!("refs/heads/{branch}"),
            new,
            old,
        ],
    )
    .await?;
    Ok(())
}

/// The first-parent commits after `from` up to `to`, oldest first.
pub async fn first_parents(repo: &Path, from: &str, to: &str) -> Result<Vec<String>> {
    let out = text(
        &git(
            repo,
            &[
                "rev-list",
                "--first-parent",
                "--reverse",
                &format!("{from}..{to}"),
            ],
        )
        .await?,
    );
    Ok(out.lines().map(str::to_owned).collect())
}

/// The first parent of a commit.
pub async fn parent(repo: &Path, commit: &str) -> Result<String> {
    rev(repo, &format!("{commit}^1")).await
}

/// The paths a candidate changes against where it left `branch`, and the lines added and removed.
pub async fn diff_facts(repo: &Path, branch: &str, candidate: &str) -> Result<(Vec<String>, u32)> {
    let out = git_hardened(
        repo,
        &[
            "diff",
            "--numstat",
            "--no-renames",
            "--no-ext-diff",
            "-z",
            &format!("{branch}...{candidate}"),
        ],
    )
    .await?;
    let mut paths = Vec::new();
    let mut lines = 0u32;
    for entry in String::from_utf8_lossy(&out)
        .split('\0')
        .filter(|e| !e.is_empty())
    {
        let mut parts = entry.splitn(3, '\t');
        let (added, removed, path) = (
            parts.next().unwrap_or("0"),
            parts.next().unwrap_or("0"),
            parts.next().unwrap_or(""),
        );
        // A binary file shows `-`: it counts as one line, so it is never free.
        lines = lines
            .saturating_add(added.parse().unwrap_or(1))
            .saturating_add(removed.parse().unwrap_or(0));
        if !path.is_empty() {
            paths.push(path.to_owned());
        }
    }
    Ok((paths, lines))
}

#[derive(Default, Deserialize)]
struct Rules {
    #[serde(default)]
    checks: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    run: String,
}

/// The `checks` of `.orbyn/crew.json` at `commit`: the repository's own checks, trusted at a commit the crew did not write.
/// A missing file or key is none; a file that cannot be read is an error, never an empty list.
pub async fn repo_checks(repo: &Path, commit: &str) -> Result<Vec<String>> {
    let file = crate::crew::RULES_FILE;
    if git_hardened(repo, &["ls-tree", commit, "--", file])
        .await?
        .is_empty()
    {
        return Ok(Vec::new());
    }
    let shown = git_hardened(repo, &["show", &format!("{commit}:{file}")]).await?;
    if shown.len() > 64 * 1024 {
        return Err(CoreError::Invalid(format!("{file} is over 64 KiB")));
    }
    let rules: Rules = serde_json::from_str(&text(&shown))
        .map_err(|e| CoreError::Invalid(format!("{file} is not valid: {e}")))?;
    Ok(rules.checks.into_iter().map(|c| c.run).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::run_git;

    fn repo(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("orbyn-scratch-{name}-{}", new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        run_git(&dir, &["init", "-q"]);
        run_git(
            &dir,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.com",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "first",
            ],
        );
        dir
    }

    #[tokio::test]
    async fn a_scratch_checkout_is_removed_by_finish_and_when_its_owner_panics() {
        let repo = repo("drop");
        let root = repo.join("scratch-root");
        let scratch = Scratch::at(&repo, &root, "HEAD").await.unwrap();
        let dir = scratch.dir.clone();
        assert!(dir.exists());
        scratch.finish().await;
        assert!(!dir.exists());
        // An owner that panics, as a task that is torn down would.
        let (r, root2) = (repo.clone(), root.clone());
        let held = tokio::spawn(async move {
            let scratch = Scratch::at(&r, &root2, "HEAD").await.unwrap();
            let dir = scratch.dir.clone();
            assert!(dir.exists());
            let _ = std::fs::write(root2.join("held"), dir.to_string_lossy().as_bytes());
            let _keep = &scratch;
            panic!("the operation failed");
        })
        .await;
        assert!(held.is_err(), "the task panicked");
        let held_dir = std::fs::read_to_string(root.join("held")).unwrap();
        assert!(!Path::new(&held_dir).exists(), "the panic left no checkout");
        let listing = text(
            &git(&repo, &["worktree", "list", "--porcelain"])
                .await
                .unwrap(),
        );
        assert!(!listing.contains(&held_dir), "git forgot it");
        let _ = std::fs::remove_dir_all(&repo);
    }
}
