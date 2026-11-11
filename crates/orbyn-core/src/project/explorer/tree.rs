//! One folder level of a worktree, read lazily.

use orbyn_contracts::{ExplorerChange, ExplorerEntry, ExplorerEntryKind};

use super::guard::Guard;
use super::status::Snapshot;
use crate::error::{CoreError, Result};

/// Entries one answer carries. More is `truncated`.
pub(super) const TREE_LIMIT: usize = 1_000;
/// Directory entries looked at before the read gives up on seeing the rest.
const SCAN_LIMIT: usize = 20_000;

pub(super) struct Level {
    pub entries: Vec<ExplorerEntry>,
    pub truncated: bool,
}

fn child(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_owned()
    } else {
        format!("{dir}/{name}")
    }
}

/// Lists the folder `raw`: folders first, then names, each case-insensitively, at most
/// `TREE_LIMIT` of those whose name contains `filter`. A link is listed as itself, never
/// followed. Without a snapshot the entries carry no letters and nothing is ignored.
pub(super) fn list(
    guard: &Guard,
    raw: &str,
    filter: &str,
    snapshot: Option<&Snapshot>,
) -> Result<Level> {
    let resolved = guard.resolve(raw)?;
    let dir = resolved.rel.as_str();
    let inherited = snapshot
        .map(|status| status.inherited(dir))
        .unwrap_or_default();
    let needle = filter.to_lowercase();
    let wanted = |name: &str| needle.is_empty() || name.to_lowercase().contains(&needle);
    let mut entries: Vec<ExplorerEntry> = Vec::new();
    let mut truncated = false;
    // Hold the folder open while it is read, and check it is still the one inside the
    // worktree afterwards, so a parent swapped for a link in between lists nothing.
    let held = match guard.open_dir(&resolved.real) {
        Ok(handle) => Some(handle),
        Err(CoreError::NotFound(_)) => None,
        Err(error) => return Err(error),
    };
    let reading = if held.is_some() {
        std::fs::read_dir(&resolved.real)
    } else {
        Err(std::io::ErrorKind::NotFound.into())
    };
    match reading {
        Ok(reader) => {
            for (seen, entry) in reader.flatten().enumerate() {
                if seen >= SCAN_LIMIT {
                    truncated = true;
                    break;
                }
                let Ok(name) = entry.file_name().into_string() else {
                    continue;
                };
                if !wanted(&name) || !guard.lists(&resolved.real, &name) {
                    continue;
                }
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                let (kind, size) = if kind.is_dir() {
                    (ExplorerEntryKind::Directory, None)
                } else if kind.is_file() {
                    (
                        ExplorerEntryKind::File,
                        entry.metadata().ok().map(|m| m.len()),
                    )
                } else if kind.is_symlink() {
                    (ExplorerEntryKind::Symlink, None)
                } else {
                    // Devices, pipes and sockets are not files anyone can open.
                    continue;
                };
                let path = child(dir, &name);
                let is_dir = kind == ExplorerEntryKind::Directory;
                let (mut change, mut has_changes, mut ignored) = (None, false, inherited.ignored);
                if let Some(status) = snapshot {
                    ignored |= status.ignored.contains(&path);
                    change = status.changes.get(&path).copied();
                    if inherited.untracked && change.is_none() {
                        change = Some(ExplorerChange::Untracked);
                    }
                    if is_dir {
                        has_changes = status.has_changes_below(&path);
                        if status.untracked_dirs.contains(&path) {
                            change = Some(ExplorerChange::Untracked);
                        }
                    }
                    has_changes |= change.is_some() && is_dir;
                }
                entries.push(ExplorerEntry {
                    name,
                    path,
                    kind,
                    size,
                    ignored,
                    change,
                    has_changes,
                });
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // A folder Git sees deleted is listed from its deleted files alone.
            if snapshot.is_none_or(|status| status.deleted_under(dir).is_empty()) {
                return Err(CoreError::NotFound("folder".into()));
            }
        }
        Err(_) => return Err(CoreError::NotFound("folder".into())),
    }
    if let Some(handle) = &held {
        guard.confirm_dir(handle, &resolved.real)?;
    }
    if let Some(status) = snapshot {
        for (name, is_dir) in status.deleted_under(dir) {
            if !wanted(&name) || entries.iter().any(|entry| entry.name == name) {
                continue;
            }
            entries.push(ExplorerEntry {
                path: child(dir, &name),
                name,
                kind: if is_dir {
                    ExplorerEntryKind::Directory
                } else {
                    ExplorerEntryKind::File
                },
                size: None,
                ignored: false,
                change: (!is_dir).then_some(ExplorerChange::Deleted),
                has_changes: is_dir,
            });
        }
    }
    entries.sort_by(|a, b| {
        let folder = |entry: &ExplorerEntry| entry.kind != ExplorerEntryKind::Directory;
        (folder(a), a.name.to_lowercase(), &a.name).cmp(&(
            folder(b),
            b.name.to_lowercase(),
            &b.name,
        ))
    });
    if entries.len() > TREE_LIMIT {
        entries.truncate(TREE_LIMIT);
        truncated = true;
    }
    Ok(Level { entries, truncated })
}
