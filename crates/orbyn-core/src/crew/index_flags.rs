//! Git index flags that hide an edit. After `git update-index --assume-unchanged f` or `--skip-worktree f`, a change to `f`
//! does not show in `git diff`, so every git-based check (`file_scope`, `diff_size`) is blind to it. The core records the
//! flagged entries (`git ls-files -v`: a lowercase tag, or `S`) when a task starts and fails the verification if the worker
//! added one. Cost: one `git ls-files` at the start and one at each verification, proportional to the tracked files and with no
//! file reads. A sparse checkout's `S` entries are there at the start, so they are not a failure.
//!
//! Not covered, and listed as limits: a path hidden by `.gitignore` or `.git/info/exclude`, and a write outside the checkout.

use std::collections::BTreeSet;
use std::path::Path;

/// The entries the index flags as hiding changes, or `None` when git cannot say.
pub async fn flagged(root: &Path) -> Option<Vec<String>> {
    let out = crate::source_control::git::git_hardened(root, &["ls-files", "-v", "-z"])
        .await
        .ok()?;
    let mut paths: Vec<String> = out
        .split(|b| *b == 0)
        .filter_map(|entry| {
            let text = String::from_utf8_lossy(entry);
            let (tag, path) = text.split_once(' ')?;
            let tag = tag.chars().next()?;
            (tag.is_lowercase() || tag == 'S').then(|| path.to_owned())
        })
        .collect();
    paths.sort();
    paths.dedup();
    Some(paths)
}

/// The entries in `now` that `before` did not have.
pub fn added(before: &[String], now: &[String]) -> Vec<String> {
    let had: BTreeSet<&String> = before.iter().collect();
    now.iter().filter(|p| !had.contains(p)).cloned().collect()
}
