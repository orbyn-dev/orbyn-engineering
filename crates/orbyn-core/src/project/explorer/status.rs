//! One `git status` per worktree, read once and shared: the change letters, the
//! ignored paths and the dirty summary all come from it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Duration;

use orbyn_contracts::{ExplorerChange, ExplorerDirty};

use crate::error::{CoreError, Result};
use crate::source_control::git::git_hardened;

/// Paths one snapshot keeps. Past it the counts go on and the letters stop, so a
/// worktree with a vendored tree of untracked files cannot grow the cache without bound.
const MAX_PATHS: usize = 50_000;

/// What `git status` said about one worktree. Paths are relative to its root, with `/`.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Snapshot {
    pub changes: BTreeMap<String, ExplorerChange>,
    /// Untracked folders, which Git reports once instead of file by file.
    pub untracked_dirs: BTreeSet<String>,
    /// Ignored files and folders; a folder stands for everything below it.
    pub ignored: BTreeSet<String>,
    pub dirty: ExplorerDirty,
    /// Some letters were dropped because the worktree changed more than `MAX_PATHS` paths.
    pub capped: bool,
}

/// What a folder passes down to everything inside it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Inherited {
    pub untracked: bool,
    pub ignored: bool,
}

/// A status that has not answered by now is given up on; its process is killed when
/// the read is dropped.
pub(super) const DEADLINE: Duration = Duration::from_secs(20);

/// Runs `read`, giving up after `limit`.
pub(super) async fn bounded<T>(
    limit: Duration,
    read: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    tokio::time::timeout(limit, read)
        .await
        .unwrap_or_else(|_| Err(CoreError::Git("Git took too long to answer.".into())))
}

/// Reads the status of the worktree at `root` through the shared Git runner. Status
/// runs a repository's clean filters on files whose timestamps moved, so this is the
/// hardened runner: no repository filter, converter or attribute can start a program.
pub(super) async fn read(root: &Path) -> Result<Snapshot> {
    bounded(DEADLINE, read_unbounded(root)).await
}

async fn read_unbounded(root: &Path) -> Result<Snapshot> {
    let output = git_hardened(
        root,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=normal",
            "--ignored=matching",
        ],
    )
    .await?;
    Ok(parse(&output))
}

/// The letter for an `XY` pair of porcelain v1.
fn change_of(x: u8, y: u8) -> ExplorerChange {
    let conflicted = x == b'U' || y == b'U' || (x == b'A' && y == b'A') || (x == b'D' && y == b'D');
    if y == b'D' {
        ExplorerChange::Deleted
    } else if conflicted {
        ExplorerChange::Conflicted
    } else if x == b'D' {
        ExplorerChange::Deleted
    } else if x == b'R' || y == b'R' {
        ExplorerChange::Renamed
    } else if x == b'A' || y == b'A' || x == b'C' || y == b'C' {
        ExplorerChange::Added
    } else {
        ExplorerChange::Modified
    }
}

/// `git status --porcelain=v1 -z`: `XY path`, and for a rename or copy the origin as
/// the next field, which is skipped.
pub(super) fn parse(output: &[u8]) -> Snapshot {
    let mut snapshot = Snapshot::default();
    let mut fields = output.split(|byte| *byte == 0);
    while let Some(field) = fields.next() {
        if field.len() < 4 || field[2] != b' ' {
            continue;
        }
        let (x, y) = (field[0], field[1]);
        let path = String::from_utf8_lossy(&field[3..]).into_owned();
        if x == b'R' || x == b'C' || y == b'R' || y == b'C' {
            fields.next();
        }
        let keep = snapshot.changes.len() + snapshot.untracked_dirs.len() + snapshot.ignored.len()
            < MAX_PATHS;
        if !keep {
            snapshot.capped = true;
        }
        match (x, y) {
            (b'!', b'!') => {
                if keep {
                    snapshot
                        .ignored
                        .insert(path.trim_end_matches('/').to_owned());
                }
            }
            (b'?', b'?') => {
                snapshot.dirty.untracked += 1;
                if keep {
                    match path.strip_suffix('/') {
                        Some(dir) => snapshot.untracked_dirs.insert(dir.to_owned()),
                        None => snapshot
                            .changes
                            .insert(path, ExplorerChange::Untracked)
                            .is_some(),
                    };
                }
            }
            _ => {
                let change = change_of(x, y);
                if change == ExplorerChange::Conflicted {
                    snapshot.dirty.conflicted += 1;
                } else {
                    snapshot.dirty.changed += 1;
                }
                if keep {
                    snapshot.changes.insert(path, change);
                }
            }
        }
    }
    snapshot
}

/// The ancestors of `path` and `path` itself, outermost first: `a/b` gives `a`, `a/b`.
fn chain(path: &str) -> impl Iterator<Item = &str> {
    path.match_indices('/')
        .map(|(at, _)| &path[..at])
        .chain((!path.is_empty()).then_some(path))
}

impl Snapshot {
    /// What the folder `dir` passes down: inside an untracked or ignored folder
    /// everything is untracked or ignored too.
    pub fn inherited(&self, dir: &str) -> Inherited {
        let mut found = Inherited::default();
        for ancestor in chain(dir) {
            found.untracked |= self.untracked_dirs.contains(ancestor);
            found.ignored |= self.ignored.contains(ancestor);
        }
        found
    }

    /// True when something changed at or below the folder `dir`.
    pub fn has_changes_below(&self, dir: &str) -> bool {
        let prefix = format!("{dir}/");
        self.untracked_dirs.contains(dir)
            || self
                .untracked_dirs
                .range(prefix.clone()..)
                .next()
                .is_some_and(|other| other.starts_with(&prefix))
            || self
                .changes
                .range(prefix.clone()..)
                .next()
                .is_some_and(|(path, _)| path.starts_with(&prefix))
    }

    /// The files Git sees deleted under `dir`, as the name of the first part below it
    /// and whether that part is a folder.
    pub fn deleted_under(&self, dir: &str) -> Vec<(String, bool)> {
        let prefix = if dir.is_empty() {
            String::new()
        } else {
            format!("{dir}/")
        };
        let mut found: BTreeMap<String, bool> = BTreeMap::new();
        for (path, change) in self.changes.range(prefix.clone()..) {
            if !path.starts_with(&prefix) {
                break;
            }
            if *change != ExplorerChange::Deleted {
                continue;
            }
            let rest = &path[prefix.len()..];
            match rest.split_once('/') {
                Some((first, _)) => found.insert(first.to_owned(), true),
                None => found.insert(rest.to_owned(), false),
            };
        }
        found.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_follow_the_porcelain_pairs() {
        let snapshot = parse(
            b" M mod.rs\0M  staged.rs\0A  new.rs\0 D gone.rs\0D  rm.rs\0R  new-name.rs\0old-name.rs\0\
              ?? loose.txt\0?? folder/\0UU clash.rs\0AA both.rs\0!! target/\0!! a.log\0",
        );
        let letter = |path: &str| snapshot.changes.get(path).copied();
        assert_eq!(letter("mod.rs"), Some(ExplorerChange::Modified));
        assert_eq!(letter("staged.rs"), Some(ExplorerChange::Modified));
        assert_eq!(letter("new.rs"), Some(ExplorerChange::Added));
        assert_eq!(letter("gone.rs"), Some(ExplorerChange::Deleted));
        assert_eq!(letter("rm.rs"), Some(ExplorerChange::Deleted));
        assert_eq!(letter("new-name.rs"), Some(ExplorerChange::Renamed));
        assert_eq!(letter("old-name.rs"), None, "the origin is not a change");
        assert_eq!(letter("loose.txt"), Some(ExplorerChange::Untracked));
        assert_eq!(letter("clash.rs"), Some(ExplorerChange::Conflicted));
        assert_eq!(letter("both.rs"), Some(ExplorerChange::Conflicted));
        assert!(snapshot.untracked_dirs.contains("folder"));
        assert!(snapshot.ignored.contains("target") && snapshot.ignored.contains("a.log"));
        assert_eq!(snapshot.dirty.conflicted, 2);
        assert_eq!(snapshot.dirty.untracked, 2);
        assert_eq!(snapshot.dirty.changed, 6);
    }

    #[test]
    fn a_folder_passes_its_state_down() {
        let snapshot = parse(b"?? new/\0!! build/\0 M src/a.rs\0 D src/old/x.rs\0");
        assert!(snapshot.inherited("new/deep").untracked);
        assert!(snapshot.inherited("build").ignored);
        assert!(!snapshot.inherited("src").ignored);
        assert!(snapshot.has_changes_below("src"));
        assert!(snapshot.has_changes_below("new"));
        assert!(!snapshot.has_changes_below("docs"));
        assert_eq!(
            snapshot.deleted_under("src"),
            vec![("old".to_owned(), true)]
        );
        assert_eq!(
            snapshot.deleted_under("src/old"),
            vec![("x.rs".to_owned(), false)]
        );
    }

    #[tokio::test]
    async fn a_read_that_never_answers_is_given_up_on() {
        let hung = bounded(Duration::from_millis(30), async {
            std::future::pending::<()>().await;
            Ok(())
        })
        .await;
        assert!(matches!(hung, Err(CoreError::Git(_))));
        assert_eq!(
            bounded(Duration::from_secs(5), async { Ok(7) })
                .await
                .unwrap(),
            7
        );
    }

    #[test]
    fn a_name_that_continues_another_is_not_below_it() {
        let snapshot = parse(b" M srcs/a.rs\0");
        assert!(!snapshot.has_changes_below("src"));
    }
}
