//! Which of the folders `git worktree list` names may be shown. Git's records can be
//! rewritten by anything that writes inside a `.git` folder (a redirected `gitdir`, a
//! rewritten `commondir`, a rewritten `.git` file), so the list is trusted only where
//! the folders on disk agree with it, and a project whose own folder is not among the
//! members is refused whole, unless it is a plain subfolder of a member (no `.git` of its
//! own), which is then shown as one row rooted at that folder.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::error::{CoreError, Result};
use crate::source_control::git::WorktreeRecord;

/// A worktree the explorer may list.
pub(super) struct Entry {
    pub is_main: bool,
    /// Git's records and the folder agree it belongs to this repository. A linked entry
    /// that fails stays listed, read-only, and is never read.
    pub member: bool,
    /// The project's own folder lies inside a member checkout and holds no `.git`: the
    /// only row, rooted at that folder and reaching nothing above it.
    pub subfolder: bool,
    pub record: WorktreeRecord,
}

fn refused() -> CoreError {
    CoreError::NotFound("worktree".into())
}

fn canon(path: &Path) -> Option<PathBuf> {
    crate::util::canonicalize(path).ok()
}

/// Where a `.git` file points, canonicalized.
fn gitdir_of(dot: &Path) -> Option<PathBuf> {
    let mut text = String::new();
    std::fs::File::open(dot)
        .ok()?
        .take(4096)
        .read_to_string(&mut text)
        .ok()?;
    let target = Path::new(text.lines().next()?.strip_prefix("gitdir:")?.trim());
    let target = if target.is_absolute() {
        target.to_path_buf()
    } else {
        dot.parent()?.join(target)
    };
    canon(&target)
}

/// The `core.worktree` a Git directory's own config names, canonicalized: `None` when it
/// names none, `Some(None)` when it names a folder that cannot be resolved. Only the
/// repository's own `config` file is read; nothing it includes is.
fn configured_worktree(git_dir: &Path) -> Option<Option<PathBuf>> {
    let mut text = String::new();
    std::fs::File::open(git_dir.join("config"))
        .ok()?
        .take(64 * 1024)
        .read_to_string(&mut text)
        .ok()?;
    let mut in_core = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_core = line.to_ascii_lowercase().starts_with("[core]");
        } else if in_core
            && let Some((key, value)) = line.split_once('=')
            && key.trim().eq_ignore_ascii_case("worktree")
        {
            let value = Path::new(value.trim().trim_matches('"'));
            let value = if value.is_absolute() {
                value.to_path_buf()
            } else {
                git_dir.join(value)
            };
            return Some(canon(&value));
        }
    }
    None
}

/// A folder that is a Git directory rather than a checkout.
fn is_git_dir(path: &Path) -> bool {
    path.join("HEAD").is_file() && path.join("objects").is_dir()
}

/// True when `root` really is a linked worktree of the repository whose common Git
/// directory is `common`: its `.git` is a file pointing into `<common>/worktrees/`, and
/// that entry points back at this `.git`.
pub(super) fn belongs(root: &Path, common: &Path) -> bool {
    let dot = root.join(".git");
    if !std::fs::symlink_metadata(&dot).is_ok_and(|meta| meta.is_file()) {
        return false;
    }
    let (Some(entry), Some(common)) = (gitdir_of(&dot), canon(common)) else {
        return false;
    };
    if !entry.starts_with(common.join("worktrees")) {
        return false;
    }
    let mut back = String::new();
    let read = std::fs::File::open(entry.join("gitdir"))
        .and_then(|file| file.take(4096).read_to_string(&mut back));
    let back = Path::new(back.trim());
    let back = if back.is_absolute() {
        back.to_path_buf()
    } else {
        entry.join(back)
    };
    read.is_ok() && matches!((canon(&back), canon(&dot)), (Some(a), Some(b)) if a == b)
}

/// The members among `records`, the first of which Git calls the main worktree, for the
/// project whose folder is `project`. Fails closed: with no common Git directory to
/// check against, or with the project's own folder not among the members, nothing is
/// shown.
///
/// - A bare repository is no checkout and is left out.
/// - Git names the Git folder itself as the main worktree when the repository was made
///   with `--separate-git-dir` or kept as a `.git` file. That entry is no checkout and is
///   left out; the project's folder stands in for it when its `.git` file points there.
/// - The main checkout must have a `.git` that is the common directory, or a file
///   pointing at it, and not a linked worktree's own directory.
pub(super) fn resolve(records: Vec<WorktreeRecord>, project: &Path) -> Result<Vec<Entry>> {
    let first = records.first().ok_or_else(refused)?;
    let first_path = canon(&first.path).ok_or_else(refused)?;
    let (common, main) = if first.bare {
        (first_path, None)
    } else if is_git_dir(&first_path) {
        let main = gitdir_of(&project.join(".git"))
            .filter(|dir| *dir == first_path && project != first_path)
            // A Git folder that names its working tree (a submodule's does) must name this one.
            .filter(|dir| match configured_worktree(dir) {
                None => true,
                Some(worktree) => worktree.is_some_and(|worktree| worktree == project),
            })
            .map(|_| WorktreeRecord {
                path: project.to_path_buf(),
                ..first.clone()
            });
        (first_path, main)
    } else {
        let dot = first_path.join(".git");
        let common = match std::fs::symlink_metadata(&dot) {
            Ok(meta) if meta.is_dir() => canon(&dot),
            Ok(meta) if meta.is_file() => {
                gitdir_of(&dot).filter(|dir| !dir.join("commondir").exists())
            }
            _ => None,
        }
        .ok_or_else(refused)?;
        (common, Some(first.clone()))
    };
    let mut entries: Vec<Entry> = main
        .into_iter()
        .map(|record| Entry {
            is_main: true,
            member: !record.prunable,
            subfolder: false,
            record,
        })
        .collect();
    for record in records.into_iter().skip(1) {
        let member = !record.prunable
            && !record.bare
            && canon(&record.path).is_some_and(|path| belongs(&path, &common));
        entries.push(Entry {
            is_main: false,
            member,
            subfolder: false,
            record,
        });
    }
    // A project is a checkout root, so only its own folder will do: a member folder above it
    // (a rewritten `.git` file naming an ancestor repository) is not the project.
    let holds_project = entries
        .iter()
        .any(|entry| entry.member && canon(&entry.record.path).is_some_and(|path| path == project));
    if holds_project {
        return Ok(entries);
    }
    subfolder_of(&entries, project).ok_or_else(refused)
}

/// The single row for a project that is a plain subfolder of a member checkout. Only a
/// folder with no `.git` entry at all qualifies, so a rewritten `.git` file is still
/// refused; the branch and head are those of the checkout that holds it.
fn subfolder_of(entries: &[Entry], project: &Path) -> Option<Vec<Entry>> {
    let absent = matches!(
        std::fs::symlink_metadata(project.join(".git")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    );
    if !absent || !project.is_dir() {
        return None;
    }
    let holder = entries
        .iter()
        .filter(|entry| entry.member)
        .filter_map(|entry| Some((entry, canon(&entry.record.path)?)))
        .filter(|(_, path)| path != project && project.starts_with(path))
        .max_by_key(|(_, path)| path.components().count())?
        .0;
    Some(vec![Entry {
        is_main: false,
        member: true,
        subfolder: true,
        record: WorktreeRecord {
            path: project.to_path_buf(),
            ..holder.record.clone()
        },
    }])
}
