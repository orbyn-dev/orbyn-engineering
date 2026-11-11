//! The protected paths (SPEC.md H11, N7): the files the checks use. When a task starts, before
//! its worker exists, the core records every protected file as it is in the checkout: its type,
//! whether it is executable and a SHA-256 of its exact bytes (of the link target, for a
//! symlink, which is never followed). The verifier records them again and compares. It reads
//! the files and asks git nothing, so nothing the worker can write in the repository
//! (`.gitignore`, `update-index` flags, `.git/config`, `refs/replace`, loose objects,
//! `objects/info/alternates`) can hide a change. Bytes are compared exactly: a CR added or
//! removed is a change. A `.git` entry at any depth, such as a submodule's, is left out.

use std::collections::BTreeMap;
use std::path::Path;

use orbyn_contracts::CrewPathPattern;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// One protected file as recorded.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// `file` or `symlink`.
    pub kind: String,
    pub executable: bool,
    pub sha256: String,
}

/// Every protected file under `root`, by `/` path.
pub type Snapshot = BTreeMap<String, Entry>;

fn entry(path: &Path, meta: &std::fs::Metadata) -> Option<Entry> {
    let kind = meta.file_type();
    let (name, bytes) = if kind.is_symlink() {
        let target = std::fs::read_link(path).ok()?;
        (
            "symlink",
            target.to_string_lossy().into_owned().into_bytes(),
        )
    } else if kind.is_file() {
        ("file", std::fs::read(path).ok()?)
    } else {
        return None;
    };
    #[cfg(unix)]
    let executable = {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    };
    #[cfg(not(unix))]
    let executable = false;
    Some(Entry {
        kind: name.to_owned(),
        executable,
        sha256: crate::util::hex(&Sha256::digest(&bytes)),
    })
}

fn walk(dir: &Path, rel: &str, out: &mut Snapshot) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for item in entries.flatten() {
        let name = item.file_name().to_string_lossy().into_owned();
        if name == ".git" {
            continue;
        }
        let path = format!("{rel}/{name}");
        // `symlink_metadata`: a link is recorded as a link, and never followed.
        let Ok(meta) = std::fs::symlink_metadata(item.path()) else {
            continue;
        };
        if meta.is_dir() {
            walk(&item.path(), &path, out);
        } else if let Some(found) = entry(&item.path(), &meta) {
            out.insert(path, found);
        }
    }
}

/// Records the protected files under `root`.
pub fn snapshot(root: &Path, patterns: &[CrewPathPattern]) -> Snapshot {
    let mut found = Snapshot::new();
    for pattern in patterns {
        let (base, tree) = pattern.parts();
        let at = root.join(base);
        let Ok(meta) = std::fs::symlink_metadata(&at) else {
            continue;
        };
        if meta.is_dir() {
            if tree {
                walk(&at, base, &mut found);
            }
        } else if let Some(file) = entry(&at, &meta) {
            found.insert(base.to_owned(), file);
        }
    }
    found
}

/// The protected files that are new, missing or different since `before`, sorted.
pub fn protected_changes(
    before: &Snapshot,
    root: &Path,
    patterns: &[CrewPathPattern],
) -> Vec<String> {
    let now = snapshot(root, patterns);
    let mut changed: Vec<String> = before
        .keys()
        .chain(now.keys())
        .filter(|path| before.get(*path) != now.get(*path))
        .cloned()
        .collect();
    changed.sort();
    changed.dedup();
    changed
}
