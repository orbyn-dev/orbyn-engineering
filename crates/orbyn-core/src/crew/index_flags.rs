//! Git index flags that hide an edit. After `git update-index --assume-unchanged f` or `--skip-worktree f`, a change to `f`
//! does not show in `git diff`, so every git-based check (`file_scope`, `diff_size`) is blind to it. The core records the
//! flagged entries (`git ls-files -v`: a lowercase tag, or `S`) before admission and every verification. Any flag refuses
//! admission or fails verification, including flags from a sparse checkout. Cost is proportional to the tracked files,
//! with no file reads. A write outside the checkout remains outside these checks.

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
