//! The protected paths (SPEC.md H11, N7): the files the checks use. The verifier compares them
//! with the commit the worker started from by reading both directly, so nothing the worker can
//! edit in the repository (`.gitignore`, `update-index` flags, `.git/config`) can hide a change.
//! A file that is new, missing or different counts, and a rename is both a missing file and a
//! new one. Line endings are ignored, since a checkout may rewrite them.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use orbyn_contracts::CrewPathPattern;

use crate::error::Result;
use crate::source_control::git::git;

fn unix(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
            i += 1;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

fn covered(patterns: &[CrewPathPattern], path: &str) -> bool {
    patterns.iter().any(|p| super::checks::covers(p, path))
}

/// Every file under a pattern in the working tree, as `/` paths.
fn present(root: &Path, patterns: &[CrewPathPattern]) -> BTreeSet<String> {
    fn walk(dir: &Path, rel: &str, out: &mut BTreeSet<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if rel.is_empty() && name == ".git" {
                continue;
            }
            let path = if rel.is_empty() {
                name
            } else {
                format!("{rel}/{name}")
            };
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => walk(&entry.path(), &path, out),
                Ok(_) => {
                    out.insert(path);
                }
                Err(_) => {}
            }
        }
    }
    let mut found = BTreeSet::new();
    for pattern in patterns {
        let (base, tree) = pattern.parts();
        let at = root.join(base);
        if tree {
            walk(&at, base, &mut found);
        } else if at.symlink_metadata().is_ok_and(|m| !m.is_dir()) {
            found.insert(base.to_owned());
        }
    }
    found
}

/// The protected files that differ from `base`, sorted. Empty when none does.
pub async fn protected_changes(
    root: &Path,
    base: &str,
    patterns: &[CrewPathPattern],
) -> Result<Vec<String>> {
    let listing = git(root, &["ls-tree", "-r", "-z", base]).await?;
    // `<mode> <type> <oid>\t<path>`, NUL-terminated.
    let mut at_base: BTreeMap<String, String> = BTreeMap::new();
    for row in listing.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let row = String::from_utf8_lossy(row);
        if let Some((meta, path)) = row.split_once('\t')
            && covered(patterns, path)
            && let Some(oid) = meta.split(' ').nth(2)
        {
            at_base.insert(path.to_owned(), oid.to_owned());
        }
    }
    let now = present(root, patterns);
    let mut changed: BTreeSet<String> = BTreeSet::new();
    for path in &now {
        if !at_base.contains_key(path) {
            changed.insert(path.clone());
        }
    }
    for (path, oid) in &at_base {
        if !now.contains(path) {
            changed.insert(path.clone());
            continue;
        }
        let blob = git(root, &["cat-file", "blob", oid]).await?;
        match std::fs::read(root.join(path)) {
            Ok(file) if unix(&file) == unix(&blob) => {}
            _ => {
                changed.insert(path.clone());
            }
        }
    }
    Ok(changed.into_iter().collect())
}
