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
//! the repository the first remote names, and what the answer rests on (the work tree's
//! `.git` link, the git directory's `HEAD`, `config`, `config.worktree` and `commondir`,
//! and the `objects` and `refs` directories Git requires before it accepts any of it as a
//! repository, watched where `commondir` names them). `HEAD` is watched although the
//! answer never contains it, so a commit or a branch switch is observed rather than
//! ignored, and `objects` and `refs` are watched for what Git finds at them rather than
//! for their contents, so renaming either, or a directory Git can no longer search, is a
//! repository Git no longer finds, and one whose signature is not a directory at all is
//! answered once rather than kept. The
//! work tree's `.git` is kept by the location it resolved to, because Git names the
//! directory it resolved: a link retargeted at another repository leaves those files
//! as they were. What the answer rests on is watched before the configuration is read
//! and confirmed after it, so facts are never kept against a later state than the one
//! they were read from. Any file that differs, any folder discovery passed that now
//! holds a `.git` or reads as a repository of its own (a bare repository has no `.git`,
//! and Git stops at it instead of passing to the parent), and any resolution that
//! changed makes the next read observe the folder again: a folder that was deleted,
//! moved, made a repository, made a bare repository, or given another remote is never
//! answered from what it used to be. An observation that cannot be watched (a file too
//! large to read twice or not a regular file, a `.git` link that resolved elsewhere, a
//! Git too old to name its directory) is answered once and not kept.

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

/// What an answer rests on: the folders Git passed and the paths it found in the directory it
/// resolved.
#[derive(Clone)]
pub(super) struct Watch {
    /// Folders discovery looked in and passed, which must still be passed.
    passed: Vec<Passed>,
    /// What the answer rests on: the bytes of the files Git read, and the directories it found.
    files: Vec<Snapshot>,
}

/// The names Git reads in a folder to decide that the folder itself is a repository, which
/// it does before it passes to the parent: a bare repository has no `.git` at all.
const REPOSITORY_NAMES: [&str; 3] = ["HEAD", "objects", "refs"];

/// The signatures Git accepts in the common directory before it calls any of it a repository:
/// it checks that both can be accessed, and reads neither. Renaming either ends the repository,
/// however unchanged the files it read from there are.
const REPOSITORY_SIGNATURES: [&str; 2] = ["objects", "refs"];

/// One folder discovery looked in and passed: it held no `.git`, and nothing at it read as a
/// repository of its own.
#[derive(Clone)]
struct Passed {
    folder: PathBuf,
    /// What Git reads at the folder itself to stop there, which must read the same.
    repository: [Snapshot; REPOSITORY_NAMES.len()],
}

impl Passed {
    fn read(folder: PathBuf) -> Self {
        let repository = REPOSITORY_NAMES.map(|name| Snapshot::read(folder.join(name)));
        Self { folder, repository }
    }

    /// Whether discovery still passes the folder: no `.git` appeared, and nothing the folder
    /// could now hold as a repository changed.
    fn holds(&self) -> bool {
        no_git(&self.folder) && self.repository.iter().all(Snapshot::holds)
    }
}

impl Watch {
    /// The watch for a work tree: the folders discovery passed below the root, the
    /// work tree's `.git` link, and the files in the git directory Git read.
    pub(super) fn work_tree(real: &Path, root: &Path, git_dir: &Path) -> Option<Self> {
        // A root that does not contain the folder names another view of the same
        // checkout, which this module does not watch, so the answer is not kept.
        let passed = passed_folders(real, root)?
            .into_iter()
            .map(Passed::read)
            .collect();
        let link = Snapshot::read(root.join(".git"));
        // A `.git` directory other than the one discovery named is a link retargeted while the
        // answer was read: the files watched below belong to the directory it named before, so the
        // answer is not kept.
        if let Seen::Directory(resolved) = &link.seen
            && crate::util::canonicalize(git_dir).ok().as_deref() != Some(resolved.as_path())
        {
            return None;
        }
        let mut files = vec![link];
        for name in ["HEAD", "config", "config.worktree", "commondir"] {
            files.push(Snapshot::read(git_dir.join(name)));
        }
        // Git accepts the directory it resolved only while the common directory holds these
        // signatures, which it checks for access and never reads. Renaming either one is a
        // repository Git no longer finds, however unchanged everything above is.
        let common = common_directory(git_dir)?;
        for name in REPOSITORY_SIGNATURES {
            // Git asks only that it can access the signature, which an executable regular file
            // satisfies too. What this watch holds at a file is its bytes, which cannot tell such a
            // file from one whose execute permission was dropped, so a signature that is not a
            // directory leaves the observation unkept rather than vouched for twice.
            let signature = common.join(name);
            if !std::fs::metadata(&signature).is_ok_and(|meta| meta.is_dir()) {
                return None;
            }
            files.push(Snapshot::read(signature));
        }
        // A linked worktree keeps the shared configuration beside its `commondir`.
        if common.as_path() != git_dir {
            for name in ["config", "config.worktree"] {
                files.push(Snapshot::read(common.join(name)));
            }
        }
        Some(Self { passed, files })
    }

    /// The watch for a folder with no work tree: every folder Git passed while
    /// looking, which must still be passed.
    fn no_work_tree(real: &Path) -> Self {
        let mut passed = vec![Passed::read(real.to_path_buf())];
        let mut at = real;
        while let Some(parent) = at.parent() {
            passed.push(Passed::read(parent.to_path_buf()));
            at = parent;
        }
        Self {
            passed,
            files: Vec::new(),
        }
    }

    /// Whether everything the answer rests on is still as it was.
    pub(super) fn holds(&self) -> bool {
        self.passed.iter().all(Passed::holds) && self.files.iter().all(Snapshot::holds)
    }
}

/// Whether a directory is still there and one Git can search. Git accepts a repository's `objects`
/// and `refs` with `access(X_OK)` and never reads them, so a directory whose search permission was
/// dropped is one it no longer finds. Resolving `dir/.` asks for that search: stating the directory
/// itself needs none, and reading its entries needs read rather than search. Windows drops the
/// trailing `.`, where the same existence test Git makes there is the one this makes.
fn searchable(path: &Path) -> bool {
    std::fs::metadata(path.join(".")).is_ok()
}

/// The common directory Git resolved for `git_dir`: the path its `commondir` names, and the Git
/// directory itself when it names none. Git drops a trailing newline and carriage return from
/// that path and nothing else, so a name that ends in a space keeps it; trimming the path would
/// watch a configuration the repository does not read. `None` when the path cannot be watched,
/// as for any other file Git read.
fn common_directory(git_dir: &Path) -> Option<PathBuf> {
    let path = git_dir.join("commondir");
    let named = match std::fs::metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Some(git_dir.to_path_buf());
        }
        Ok(meta) if meta.is_file() && meta.len() <= MAX_WATCHED_BYTES => {
            std::fs::read(&path).ok()?
        }
        _ => return None,
    };
    let named = std::str::from_utf8(&named).ok()?;
    let named = named.trim_end_matches(['\r', '\n']);
    if named.is_empty() {
        return Some(git_dir.to_path_buf());
    }
    Some(git_dir.join(named))
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

/// Whether `folder` holds no `.git` at all, which is what discovery relied on to pass it.
/// Whether it also reads as a repository of its own is [`Passed`]'s other half.
fn no_git(folder: &Path) -> bool {
    matches!(
        std::fs::metadata(folder.join(".git")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    )
}

/// One path the answer rests on, as it was then.
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

    /// Whether the path holds what the answer read. A path that is unreadable now, or
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
    /// A directory is there, which is what a work tree's `.git` and Git's repository
    /// signatures are; it is remembered by where it resolved, because a link retargeted at
    /// another directory is otherwise that same directory, and Git names the repository it
    /// resolved to. It has to still be a directory: a signature replaced by a file is not one.
    Directory(PathBuf),
    /// A file of this length, hashing to this digest.
    File { len: u64, digest: String },
    /// A file too large to read on every settings read, or not a regular file at all: reading a
    /// pipe, socket or device can wait for a writer, so it is never read again.
    Unwatchable,
}

impl Seen {
    fn read(path: &Path) -> Self {
        match std::fs::metadata(path) {
            Ok(meta) if meta.is_dir() => Self::Directory(
                crate::util::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()),
            ),
            Ok(meta) if meta.len() > MAX_WATCHED_BYTES || !meta.is_file() => Self::Unwatchable,
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
            Self::Unwatchable => false,
            Self::Absent => matches!(
                std::fs::metadata(path),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound
            ),
            Self::Directory(resolved) => {
                std::fs::metadata(path).is_ok_and(|meta| meta.is_dir())
                    && searchable(path)
                    && crate::util::canonicalize(path).is_ok_and(|now| now == *resolved)
            }
            Self::File { len, digest } => {
                std::fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.len() == *len)
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
