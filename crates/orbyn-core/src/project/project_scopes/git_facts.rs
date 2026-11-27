//! The Git identity of one project folder, kept between catalog reads.
//!
//! A settings page reads the project scope catalog several times: once for the scope
//! list and once for each preference question beside it. Every read spawned
//! `git config` and `git rev-parse` for every registered project, which is the cost
//! this module removes: the answer is kept, and a later read reuses it while every
//! file that decided it still holds the same bytes.
//!
//! What is kept is what Git reported and what it read to report it: the folder as it
//! resolved, the folders discovery passed on the way to the work tree root, the root,
//! the repository the first remote names, and a digest of each file the answer rests
//! on (the work tree's `.git` link, and the git directory's `HEAD`, `config`,
//! `config.worktree` and `commondir`). `HEAD` is watched although the answer never
//! contains it, so a commit or a branch switch is observed rather than ignored. The
//! work tree's `.git` is kept by the location it resolved to, because Git names the
//! directory it resolved: a link retargeted at another repository leaves those files
//! as they were. What the answer rests on is watched before the configuration is read
//! and confirmed after it, so facts are never kept against a later state than the one
//! they were read from. Any file that differs, any folder that now holds a `.git`
//! where discovery found none, and any resolution that changed makes the next read
//! observe the folder again: a folder that was deleted, moved, made a repository, or
//! given another remote is never answered from what it used to be. An observation that
//! cannot be watched (a file too large to read twice, a Git too old to name its
//! directory) is answered once and not kept.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::util::sha256_hex;

use super::Repository;

/// Most folders kept. The catalog itself refuses more registered projects than this.
const MAX_FOLDERS: usize = 512;

/// Largest file read for its digest. Git configuration is small, and one larger than
/// this is answered but never read again on every settings read.
const MAX_WATCHED_BYTES: u64 = 64 * 1024;

/// What one observation of a folder's Git identity reported.
#[derive(Clone)]
pub(super) struct GitFacts {
    /// The folder as it resolved, so a symlink or a moved folder is not reused.
    real: PathBuf,
    /// The work tree root Git named, or `None` when it named none and the folder is
    /// not registered as a repository.
    root: Option<PathBuf>,
    /// The repository the first remote names, when it names a non-local one.
    repository: Option<Repository>,
    /// What has to still hold for these facts to describe the folder again.
    watch: Option<Watch>,
}

impl GitFacts {
    /// What Git reported for the work tree rooted at `root`, read from the files `watch` covers.
    /// The watch is taken before the configuration is read and confirmed after it, so the facts are
    /// never assembled from bytes the watch does not cover.
    pub(super) fn work_tree(
        real: &Path,
        root: PathBuf,
        watch: Option<Watch>,
        repository: Option<Repository>,
    ) -> Self {
        Self {
            real: real.to_path_buf(),
            root: Some(root),
            repository,
            watch,
        }
    }

    /// What Git reported for a folder it found no work tree for. The caller only
    /// reaches this for a folder that is not registered as a repository, so a
    /// registration that now says otherwise observes the folder again.
    pub(super) fn no_work_tree(real: &Path) -> Self {
        Self {
            real: real.to_path_buf(),
            root: None,
            repository: None,
            watch: Some(Watch::no_work_tree(real)),
        }
    }

    /// The work tree root Git named, when it named one.
    pub(super) fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// The repository the first remote names, when it names a non-local one.
    pub(super) fn repository(&self) -> Option<&Repository> {
        self.repository.as_ref()
    }

    /// Whether these facts still describe the folder: the same resolution, no `.git`
    /// where discovery passed, and the same bytes in every file the answer read.
    fn still_describes(&self, real: &Path) -> bool {
        self.real == real && self.watch.as_ref().is_some_and(Watch::holds)
    }
}

/// What an answer rests on: the folders Git passed and the files it read.
#[derive(Clone)]
pub(super) struct Watch {
    /// Folders discovery looked in and found no `.git`, which must still hold none.
    passed: Vec<PathBuf>,
    /// The files whose bytes decided what Git reported.
    files: Vec<Snapshot>,
}

impl Watch {
    /// The watch for a work tree: the folders discovery passed below the root, the
    /// work tree's `.git` link, and the files in the git directory Git read.
    pub(super) fn work_tree(real: &Path, root: &Path, git_dir: &Path) -> Option<Self> {
        // A root that does not contain the folder names another view of the same
        // checkout, which this module does not watch, so the answer is not kept.
        let passed = passed_folders(real, root)?;
        let mut files = vec![Snapshot::read(root.join(".git"))];
        for name in ["HEAD", "config", "config.worktree", "commondir"] {
            files.push(Snapshot::read(git_dir.join(name)));
        }
        // A linked worktree keeps the shared configuration beside its `commondir`.
        if let Ok(common) = std::fs::read_to_string(git_dir.join("commondir")) {
            let common = common.trim();
            if !common.is_empty() {
                let common = git_dir.join(common);
                for name in ["config", "config.worktree"] {
                    files.push(Snapshot::read(common.join(name)));
                }
            }
        }
        Some(Self { passed, files })
    }

    /// The watch for a folder with no work tree: every folder Git passed while
    /// looking, which must still hold no `.git`.
    fn no_work_tree(real: &Path) -> Self {
        let mut passed = vec![real.to_path_buf()];
        let mut at = real;
        while let Some(parent) = at.parent() {
            passed.push(parent.to_path_buf());
            at = parent;
        }
        Self {
            passed,
            files: Vec::new(),
        }
    }

    /// Whether everything the answer rests on is still as it was.
    pub(super) fn holds(&self) -> bool {
        self.passed.iter().all(|folder| no_git(folder)) && self.files.iter().all(Snapshot::holds)
    }
}

/// The folders between `real` and `root`, which Git passed on its way up.
fn passed_folders(real: &Path, root: &Path) -> Option<Vec<PathBuf>> {
    let mut passed = Vec::new();
    let mut at = real;
    while at != root {
        passed.push(at.to_path_buf());
        at = at.parent()?;
    }
    Some(passed)
}

/// Whether `folder` holds no `.git` at all, which is what discovery relied on.
fn no_git(folder: &Path) -> bool {
    matches!(
        std::fs::metadata(folder.join(".git")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    )
}

/// One file the answer read, as it was then.
#[derive(Clone)]
struct Snapshot {
    path: PathBuf,
    seen: Seen,
}

impl Snapshot {
    fn read(path: PathBuf) -> Self {
        let seen = Seen::read(&path);
        Self { path, seen }
    }

    /// Whether the path holds what the answer read. A file that is unreadable now, or
    /// too large to read twice, never counts as the same.
    fn holds(&self) -> bool {
        self.seen.holds(&self.path)
    }
}

/// What was found at a watched path.
#[derive(Clone)]
enum Seen {
    /// Nothing is there, which is how a file Git would have read is remembered.
    Absent,
    /// A directory is there, which is what a work tree's `.git` is; it is remembered by where it
    /// resolved, because a link retargeted at another directory is otherwise that same directory,
    /// and Git names the repository it resolved to.
    Directory(PathBuf),
    /// A file of this length, hashing to this digest.
    File { len: u64, digest: String },
    /// A file too large to read on every settings read.
    Oversize,
}

impl Seen {
    fn read(path: &Path) -> Self {
        match std::fs::metadata(path) {
            Ok(meta) if meta.is_dir() => Self::Directory(
                crate::util::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()),
            ),
            Ok(meta) if meta.len() > MAX_WATCHED_BYTES => Self::Oversize,
            Ok(_) => match std::fs::read(path) {
                Ok(bytes) => Self::File {
                    len: bytes.len() as u64,
                    digest: sha256_hex(&bytes),
                },
                Err(_) => Self::Absent,
            },
            Err(_) => Self::Absent,
        }
    }

    fn holds(&self, path: &Path) -> bool {
        match self {
            Self::Oversize => false,
            Self::Absent => matches!(
                std::fs::metadata(path),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound
            ),
            Self::Directory(resolved) => {
                crate::util::canonicalize(path).is_ok_and(|now| now == *resolved)
            }
            Self::File { len, digest } => {
                std::fs::metadata(path).is_ok_and(|meta| !meta.is_dir() && meta.len() == *len)
                    && std::fs::read(path).is_ok_and(|bytes| sha256_hex(&bytes) == *digest)
            }
        }
    }
}

/// The Git identity the catalog keeps for the folders it has observed.
#[derive(Default)]
pub(crate) struct ScopeGitFacts {
    state: Mutex<State>,
}

impl ScopeGitFacts {
    /// The observation kept for `folder`, when it still describes the folder on disk.
    /// A stale one is dropped, so the next read observes the folder and keeps that.
    pub(super) fn kept(&self, folder: &str, real: &Path) -> Option<GitFacts> {
        let mut state = lock(&self.state);
        let used = state.next_use();
        let describes = state
            .folders
            .get(folder)
            .is_some_and(|kept| kept.facts.still_describes(real));
        if !describes {
            state.folders.remove(folder);
            return None;
        }
        let kept = state.folders.get_mut(folder)?;
        kept.used = used;
        Some(kept.facts.clone())
    }

    /// Keeps `facts` for `folder`, dropping the least recently read folder when full.
    pub(super) fn keep(&self, folder: &str, facts: GitFacts) {
        let mut state = lock(&self.state);
        let used = state.next_use();
        if !state.folders.contains_key(folder) && state.folders.len() >= MAX_FOLDERS {
            let least_recently_read = state
                .folders
                .iter()
                .min_by_key(|(_, kept)| kept.used)
                .map(|(folder, _)| folder.clone());
            if let Some(folder) = least_recently_read {
                state.folders.remove(&folder);
            }
        }
        state
            .folders
            .insert(folder.to_owned(), Kept { facts, used });
    }
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Default)]
struct State {
    folders: HashMap<String, Kept>,
    /// Orders the folders by last read, so the cap drops the least recently read.
    uses: u64,
}

impl State {
    fn next_use(&mut self) -> u64 {
        self.uses += 1;
        self.uses
    }
}

struct Kept {
    facts: GitFacts,
    used: u64,
}

#[cfg(test)]
mod tests;
