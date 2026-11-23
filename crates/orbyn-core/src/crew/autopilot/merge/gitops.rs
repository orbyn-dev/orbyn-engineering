//! The git behind the merge queue and the watch on main. Every operation happens in a scratch worktree of
//! its own, a detached checkout under the data folder, so a person's checkout is never touched. The target
//! branch moves only by a compare-and-swap `update-ref`, and only when no worktree has it checked out. Nothing here
//! fetches, pushes or contacts a remote: the merge is local.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{CoreError, Result};
use crate::source_control::git::{
    git, git_hardened, git_hardened_capped, git_hardened_output_capped, git_raw, text,
};
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
        // A crash can leave a registration for a folder that is gone; clear those before adding another checkout.
        let _ = git(repo, &["worktree", "prune"]).await;
        std::fs::create_dir_all(root)?;
        Self::named(repo, &root.join(new_id()), rev).await
    }

    pub(super) async fn named(repo: &Path, dir: &Path, rev: &str) -> Result<Self> {
        let dir = dir.to_owned();
        if dir.exists() {
            return Err(CoreError::Invalid(
                "the corrective checkout path already exists".into(),
            ));
        }
        if let Some(parent) = dir.parent() {
            std::fs::create_dir_all(parent)?;
        }
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

    /// Ownership passes to the durable correction binding while its review and queue remain active.
    pub(super) fn retain(mut self) {
        self.removed = true;
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

/// How far back a crashed merge is looked for: the first-parent commits at the tip of the branch.
pub const LANDED_WINDOW: usize = 256;
/// The most output the search reads.
const LOG_CAP: usize = 256 * 1024;

/// The merge commit the queue made for `candidate`, if it is among the last [`LANDED_WINDOW`] first-parent commits of `branch`:
/// a two-parent commit whose second parent is `candidate` and whose subject is `subject`. Returns `(merge commit, its first parent)`.
/// `None` means no such commit in that window, which does not say the candidate is absent from the branch: see [`contains`].
pub async fn landed_merge(
    repo: &Path,
    branch: &str,
    candidate: &str,
    subject: &str,
) -> Result<Option<(String, String)>> {
    let log = text(
        &git_hardened_capped(
            repo,
            &[
                "log",
                "--first-parent",
                "-n",
                &LANDED_WINDOW.to_string(),
                "--format=%H %P%x1f%s",
                &format!("refs/heads/{branch}"),
            ],
            LOG_CAP,
        )
        .await?,
    );
    for line in log.lines() {
        let Some((ids, subj)) = line.split_once('\u{1f}') else {
            continue;
        };
        let ids: Vec<&str> = ids.split(' ').collect();
        if ids.len() == 3 && ids[2] == candidate && subj == subject {
            return Ok(Some((ids[0].to_owned(), ids[1].to_owned())));
        }
    }
    Ok(None)
}

/// Whether `candidate` is already part of `branch`'s history (nothing of it is left to merge). Hardened, output-capped read.
pub async fn already_on(repo: &Path, branch: &str, candidate: &str) -> Result<bool> {
    let out = git_hardened_capped(
        repo,
        &[
            "rev-list",
            "-n",
            "1",
            candidate,
            "--not",
            &format!("refs/heads/{branch}"),
        ],
        4096,
    )
    .await?;
    Ok(text(&out).is_empty())
}

/// Whether `commit` is a merge of exactly `first` and `second`, in that order.
pub async fn is_merge_of(repo: &Path, commit: &str, first: &str, second: &str) -> Result<bool> {
    let out = text(
        &git_hardened_capped(repo, &["rev-list", "--parents", "-n", "1", commit], 4096).await?,
    );
    Ok(out.split(' ').collect::<Vec<_>>() == [commit, first, second])
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

/// A merge or revert runs any custom merge driver the repository configures, a program of its choosing, so the real merge
/// refuses where one is configured: the same rule the forecast dry run applies.
async fn refuse_merge_driver(scratch: &Scratch) -> std::result::Result<(), Refused> {
    if has_merge_driver(&scratch.repo).await {
        return Err(Refused::Other(
            "the repository configures a custom merge driver, which the crew does not run".into(),
        ));
    }
    Ok(())
}

/// `git merge --no-ff` of `candidate` into the scratch checkout. The merge commit is the result.
pub async fn merge(
    scratch: &Scratch,
    candidate: &str,
    message: &str,
) -> std::result::Result<String, Refused> {
    refuse_merge_driver(scratch).await?;
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
    refuse_merge_driver(scratch).await?;
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

/// The most output a merge dry run reads.
const MERGE_TREE_CAP: usize = 64 * 1024;
/// The most conflicting files one candidate reports; a merge can conflict in more, and the screen needs the first few.
pub const CONFLICT_PATHS_MAX: usize = 20;

/// Whether the repository configures any custom merge driver (`merge.<name>.driver`). A driver named by `info/attributes` runs a
/// program during a merge, so a dry run in such a repository is not made: the forecast leaves the candidate out. An unreadable
/// config counts as configured.
pub async fn has_merge_driver(repo: &Path) -> bool {
    match git_hardened_output_capped(
        repo,
        &["config", "--get-regexp", r"^merge\..*\.driver$"],
        MERGE_TREE_CAP,
    )
    .await
    {
        // 1 is no match.
        Ok((status, _)) => status.code() != Some(1),
        Err(_) => true,
    }
}

/// The files a dry-run merge of `candidate` onto `branch` would report as conflicting, from `git merge-tree`. The trial tree is
/// written as unreachable objects only: no ref, index or worktree moves. Empty means the merge is clean at this read.
pub async fn conflict_paths(repo: &Path, branch: &str, candidate: &str) -> Result<Vec<String>> {
    let (status, out) = git_hardened_output_capped(
        repo,
        &[
            "merge-tree",
            "--write-tree",
            "--name-only",
            "--no-messages",
            branch,
            candidate,
        ],
        MERGE_TREE_CAP,
    )
    .await?;
    match status.code() {
        Some(0) => Ok(Vec::new()),
        // 1 is a conflict; the first line names the trial tree and every line after it is a path.
        Some(1) => Ok(text(&out)
            .lines()
            .skip(1)
            .take(CONFLICT_PATHS_MAX)
            .map(str::to_owned)
            .collect()),
        _ => Err(CoreError::Git("the merge dry run did not finish".into())),
    }
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
            "--",
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
#[serde(deny_unknown_fields)]
struct Entry {
    run: String,
    /// Advertised by `docs/crew.md`: `checks` hold a `run` and a `when`. The condition is not implemented
    /// (issue #795), and a verdict that ignores it would run the check unconditionally, so it is read here
    /// only to refuse it by name rather than let it vanish.
    /// `Some(None)` is an explicit `"when": null`: declared, so refused like any other value.
    #[serde(default, deserialize_with = "declared")]
    when: Option<Option<serde_json::Value>>,
}

/// Keeps a present key apart from an absent one: serde reads `null` as `None`, which would let a declared `when` pass.
fn declared<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<Option<serde_json::Value>>, D::Error> {
    Option::<serde_json::Value>::deserialize(deserializer).map(Some)
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
    // A condition this version does not read is refused, never ignored: running the check regardless is the one
    // outcome the repository did not ask for, and it would look enforced.
    if rules.checks.iter().any(|entry| entry.when.is_some()) {
        return Err(CoreError::Invalid(format!(
            "{file}: checks[].when is not implemented yet, so a conditional check would run unconditionally; \
             remove the condition, or wait for the merge queue and the verifier to read it"
        )));
    }
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
    async fn full_commit_ranges_are_revisions_in_a_long_checkout() {
        let owned = crate::testing::TempDir::new();
        let parent_len = owned.path().to_string_lossy().chars().count();
        let leaf = if cfg!(windows) {
            // Keep ordinary repository files below MAX_PATH, while a possible
            // 83-character SHA...SHA pathname crosses it. No global Git settings.
            assert!(
                parent_len < 190,
                "use a shorter owned TEMP for the controlled repro"
            );
            "r".repeat(190 - parent_len - 1)
        } else {
            "repo".to_owned()
        };
        let repo = owned.path().join(leaf);
        std::fs::create_dir_all(&repo).unwrap();
        run_git(&repo, &["init", "-q", "-b", "main"]);
        run_git(&repo, &["config", "core.longpaths", "false"]);
        std::fs::write(repo.join("feature.txt"), "before\n").unwrap();
        run_git(&repo, &["add", "feature.txt"]);
        run_git(
            &repo,
            &[
                "-c",
                "user.name=fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-q",
                "-m",
                "before",
            ],
        );
        let before = rev(&repo, "HEAD").await.unwrap();
        std::fs::write(repo.join("feature.txt"), "after\n").unwrap();
        run_git(&repo, &["add", "feature.txt"]);
        run_git(
            &repo,
            &[
                "-c",
                "user.name=fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-q",
                "-m",
                "after",
            ],
        );
        let candidate = rev(&repo, "HEAD").await.unwrap();
        if cfg!(windows) {
            assert!(
                repo.to_string_lossy().chars().count() + 1 + before.len() + 3 + candidate.len()
                    >= 260
            );
        }
        let (paths, lines) = diff_facts(&repo, &before, &candidate).await.unwrap();
        assert_eq!(paths, ["feature.txt"]);
        assert_eq!(lines, 2);
    }

    /// Issue #795: a check that declares the `when` this version does not read is refused by name, never run
    /// unconditionally. The verifier and the merge queue both read this one function.
    #[tokio::test]
    async fn a_declared_check_condition_is_refused_rather_than_ignored() {
        let repo = repo("when");
        let rules = |text: &str| {
            std::fs::create_dir_all(repo.join(".orbyn")).unwrap();
            std::fs::write(repo.join(".orbyn/crew.json"), text).unwrap();
            run_git(&repo, &["add", ".orbyn/crew.json"]);
            run_git(
                &repo,
                &[
                    "-c",
                    "user.name=t",
                    "-c",
                    "user.email=t@example.com",
                    "commit",
                    "-q",
                    "-m",
                    "rules",
                ],
            );
        };
        rules(r#"{"checks":[{"run":"exit 0"}]}"#);
        let head = rev(&repo, "HEAD").await.unwrap();
        assert_eq!(repo_checks(&repo, &head).await.unwrap(), ["exit 0"]);
        // A condition is refused, with the reason a person can act on.
        rules(r#"{"checks":[{"run":"exit 0","when":{"paths":["src/**"]}}]}"#);
        let head = rev(&repo, "HEAD").await.unwrap();
        let why = repo_checks(&repo, &head)
            .await
            .unwrap_err()
            .public_message();
        assert!(why.contains("checks[].when"), "{why}");
        assert!(why.contains("not implemented"), "{why}");
        // An explicit null is a declared condition too; an absent one is fine.
        rules(r#"{"checks":[{"run":"exit 0","when":null}]}"#);
        let head = rev(&repo, "HEAD").await.unwrap();
        assert!(repo_checks(&repo, &head).await.is_err());
        // Any other field of a check is refused too: it is not silently read as a check with no condition.
        rules(r#"{"checks":[{"run":"exit 0","branch":"main"}]}"#);
        let head = rev(&repo, "HEAD").await.unwrap();
        assert!(repo_checks(&repo, &head).await.is_err());
        let _ = std::fs::remove_dir_all(&repo);
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

    #[tokio::test]
    async fn a_merge_or_revert_is_refused_where_a_custom_merge_driver_is_configured() {
        let repo = repo("driver");
        let root = repo.join("scratch-root");
        let base = rev(&repo, "HEAD").await.unwrap();
        run_git(&repo, &["checkout", "-q", "-b", "candidate"]);
        std::fs::write(repo.join("feature.txt"), "candidate\n").unwrap();
        run_git(&repo, &["add", "feature.txt"]);
        run_git(
            &repo,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.com",
                "commit",
                "-q",
                "-m",
                "candidate",
            ],
        );
        let candidate = rev(&repo, "HEAD").await.unwrap();
        // The repository names a driver for every path, as the issue's reproduction does.
        run_git(&repo, &["config", "merge.custom.driver", "true"]);
        std::fs::write(
            repo.join(".git").join("info").join("attributes"),
            "* merge=custom\n",
        )
        .unwrap();
        // Without the refusal this fast-forward would succeed, so the refusal is what the test checks.
        let scratch = Scratch::at(&repo, &root, &base).await.unwrap();
        assert!(
            matches!(
                merge(&scratch, &candidate, "merge candidate").await,
                Err(Refused::Other(_))
            ),
            "a merge runs no driver"
        );
        assert_eq!(
            scratch.head().await.unwrap(),
            base,
            "the refused merge moved nothing"
        );
        assert!(
            matches!(revert(&scratch, &candidate).await, Err(Refused::Other(_))),
            "a revert runs no driver"
        );
        scratch.finish().await;
        let _ = std::fs::remove_dir_all(&repo);
    }
}
