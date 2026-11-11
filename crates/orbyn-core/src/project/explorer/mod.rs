//! The left explorer's backend: a project's worktrees and a read-only look at their
//! files. A client names a project and a worktree by id, never by path, and a path
//! inside a worktree passes [`guard`]. Reads that cost a process answer from the last
//! known value and refresh in the background ([`ExplorerReads`]); nothing here writes.

mod guard;
mod member;
mod status;
mod tree;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use orbyn_contracts::{
    ExplorerChange, ExplorerChanges, ExplorerFile, ExplorerFileKind, ExplorerReadOnlyReason,
    ExplorerThread, ExplorerTree, ExplorerWorktree, ExplorerWorktrees, Project,
};
use tokio::sync::Semaphore;

use self::guard::Guard;
use self::status::Snapshot;
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::source_control::git::{WorktreeRecord, worktree_records_hardened};
use crate::util::swr::{Notify, Swr, key};
use crate::util::{sha256_hex, strip_verbatim};

/// The worktree list is served as it was for this long before a refresh starts...
pub const LIST_FRESH: Duration = Duration::from_secs(10);
/// ...and is never shown past this age.
pub const LIST_MAX_AGE: Duration = Duration::from_secs(60);
/// A worktree's status likewise.
pub const STATUS_FRESH: Duration = Duration::from_secs(5);
pub const STATUS_MAX_AGE: Duration = Duration::from_secs(30);
/// A folder read with no status kept waits this long for Git, then answers without letters.
const COLD_WAIT: Duration = Duration::from_millis(400);
/// Worktrees whose status a listing starts reading; the rest show no summary.
const WARM_LIMIT: usize = 32;
/// The text of a file the explorer returns. Larger is reported by size only.
pub const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

/// Statuses one project reads at once, so a project whose Git hangs cannot hold the others.
const STATUS_READS: usize = 2;
/// Gates kept, one per project that read a status; idle ones are dropped past this.
const MAX_GATES: usize = 64;
/// After a well-formed id the list did not hold, the list is not read again for this long.
const MISS_RETRY: Duration = Duration::from_secs(5);
/// A topic is announced at once, then at most once more per this long.
const ANNOUNCE_GAP: Duration = Duration::from_millis(200);

/// The kept answers: each project's worktree list and each worktree's status.
pub struct ExplorerReads {
    lists: Swr<Vec<WorktreeRecord>>,
    statuses: Swr<Arc<Snapshot>>,
    gates: Mutex<HashMap<String, Arc<Semaphore>>>,
    misses: Mutex<HashMap<String, Instant>>,
}

impl Default for ExplorerReads {
    fn default() -> Self {
        Self {
            lists: Swr::new(LIST_FRESH, LIST_MAX_AGE, 32),
            statuses: Swr::new(STATUS_FRESH, STATUS_MAX_AGE, 128),
            gates: Mutex::default(),
            misses: Mutex::default(),
        }
    }
}

fn topic(project_id: &str) -> String {
    format!("explorer:{project_id}")
}

fn locked<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Passes a topic on at once, and folds the ones that follow within `ANNOUNCE_GAP` into one
/// more, so a listing that starts many status reads sends a handful of messages, not one each.
fn coalescing(notify: Notify) -> Notify {
    let held: Arc<Mutex<HashMap<String, bool>>> = Arc::default();
    Arc::new(move |topic: &str| {
        {
            let mut held = locked(&held);
            if let Some(again) = held.get_mut(topic) {
                *again = true;
                return;
            }
            held.insert(topic.to_owned(), false);
        }
        notify(topic);
        let (later, notify, name) = (held.clone(), notify.clone(), topic.to_owned());
        let settle = async move {
            tokio::time::sleep(ANNOUNCE_GAP).await;
            if locked(&later).remove(&name) == Some(true) {
                notify(&name);
            }
        };
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => drop(runtime.spawn(settle)),
            Err(_) => drop(locked(&held).remove(topic)),
        }
    })
}

impl ExplorerReads {
    pub fn set_notify(&self, notify: &Notify) {
        let notify = coalescing(notify.clone());
        self.lists.set_notify(notify.clone());
        self.statuses.set_notify(notify);
    }

    /// Forgets everything kept, without telling the windows: every write request calls
    /// it, and the window that wrote reads again by itself.
    pub fn invalidate(&self) {
        self.lists.invalidate_all();
        self.statuses.invalidate_all();
        locked(&self.misses).clear();
    }

    /// A run ended or files moved in this project: forgets its answers and tells the
    /// windows showing it, since they did not make the change.
    pub fn changed(&self, project_id: &str) {
        let prefix = key(&[project_id]);
        self.lists.invalidate(&prefix);
        self.statuses.invalidate(&prefix);
        locked(&self.misses).remove(project_id);
        self.lists.announce(&topic(project_id));
    }

    fn gate(&self, project_id: &str) -> Arc<Semaphore> {
        let mut gates = locked(&self.gates);
        if gates.len() >= MAX_GATES {
            // Only gates some read still holds are kept.
            gates.retain(|_, gate| Arc::strong_count(gate) > 1);
        }
        gates
            .entry(project_id.to_owned())
            .or_insert_with(|| Arc::new(Semaphore::new(STATUS_READS)))
            .clone()
    }
}

/// A worktree of one project, as the routes resolve it.
struct Target {
    project: Project,
    record: WorktreeRecord,
    id: String,
    is_main: bool,
    path: PathBuf,
    missing: bool,
    /// Git's own records and the folder agree it is this repository's worktree. A
    /// redirected `gitdir` file, a bare repository or a prunable entry is not one, and
    /// is never read.
    member: bool,
    /// The project's folder is inside a checkout and is the only root shown.
    subfolder: bool,
}

impl Target {
    fn new(project: &Project, entry: member::Entry) -> Self {
        let path = plain(&entry.record.path);
        Self {
            project: project.clone(),
            id: worktree_id(&path),
            is_main: entry.is_main,
            missing: !path.is_dir(),
            member: entry.member && !entry.record.prunable && !entry.record.bare,
            subfolder: entry.subfolder,
            path,
            record: entry.record,
        }
    }

    fn reason(&self) -> Option<ExplorerReadOnlyReason> {
        if self.missing || self.record.prunable {
            Some(ExplorerReadOnlyReason::Missing)
        } else if self.record.locked {
            Some(ExplorerReadOnlyReason::Locked)
        } else {
            None
        }
    }

    fn key(&self) -> String {
        key(&[&self.project.id, &self.path.to_string_lossy()])
    }
}

/// The id of a worktree: a digest of where it is, so a client cannot name another place.
fn worktree_id(path: &std::path::Path) -> String {
    let text = path.to_string_lossy();
    format!("wt-{}", &sha256_hex(text.as_bytes())[..16])
}

fn well_formed(id: &str) -> bool {
    id.strip_prefix("wt-")
        .is_some_and(|hex| hex.len() == 16 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// The path as the product stores it. A folder that is gone keeps the same spelling as
/// before it went: the nearest folder that still exists is resolved (Windows short
/// names, links, drive-letter case) and the missing rest is appended as written, so
/// a worktree's id does not change when its folder is deleted.
fn plain(path: &std::path::Path) -> PathBuf {
    if let Ok(found) = crate::util::canonicalize(path) {
        return found;
    }
    let mut rest = Vec::new();
    let mut head = path;
    while let (Some(parent), Some(name)) = (head.parent(), head.file_name()) {
        rest.push(name);
        if let Ok(mut found) = crate::util::canonicalize(parent) {
            found.extend(rest.iter().rev());
            return found;
        }
        head = parent;
    }
    strip_verbatim(&path.to_string_lossy()).into()
}

impl Core {
    /// Explorer reads are file reads: the folder must be trusted.
    fn explorer_project(&self, project_id: &str) -> Result<Project> {
        let project = self.store.project(project_id)?;
        if !project.trusted {
            return Err(CoreError::FolderUntrusted);
        }
        Ok(project)
    }

    /// The worktrees of the project that Git's records and the folders agree on.
    async fn explorer_members(
        &self,
        project: &Project,
        refresh: bool,
    ) -> Result<Vec<member::Entry>> {
        let path = PathBuf::from(&project.path);
        if !project.is_git {
            return Ok(vec![member::Entry {
                is_main: true,
                member: true,
                subfolder: false,
                record: WorktreeRecord {
                    path,
                    branch: None,
                    revision: String::new(),
                    detached: false,
                    locked: false,
                    prunable: false,
                    bare: false,
                },
            }]);
        }
        let listed = path.clone();
        let records = self
            .explorer
            .lists
            .get(
                &key(&[&project.id, "list"]),
                &topic(&project.id),
                refresh,
                move || async move {
                    status::bounded(status::DEADLINE, worktree_records_hardened(&listed)).await
                },
            )
            .await?;
        member::resolve(records, &plain(&path))
    }

    /// The worktree `worktree_id` of `project_id`, or not found. A name outside the
    /// project's own Git list never resolves, whatever path it hashes from.
    async fn explorer_target(&self, project_id: &str, worktree_id: &str) -> Result<Target> {
        let project = self.explorer_project(project_id)?;
        if !well_formed(worktree_id) {
            return Err(CoreError::NotFound("worktree".into()));
        }
        // A worktree made a moment ago is not in a kept list, so one miss reads again, but
        // not again for a few seconds, so a made-up id cannot make Git list on every request.
        for refresh in [false, true] {
            if refresh {
                let mut misses = locked(&self.explorer.misses);
                if misses
                    .get(project_id)
                    .is_some_and(|at| at.elapsed() < MISS_RETRY)
                {
                    break;
                }
                if misses.len() >= 256 {
                    misses.clear();
                }
                misses.insert(project_id.to_owned(), Instant::now());
            }
            let found = self.explorer_members(&project, refresh).await?;
            for entry in found {
                let target = Target::new(&project, entry);
                if target.id == worktree_id {
                    return Ok(target);
                }
            }
        }
        Err(CoreError::NotFound("worktree".into()))
    }

    fn explorer_guard(&self, target: &Target) -> Result<Guard> {
        if target.missing || !target.member {
            return Err(CoreError::NotFound("worktree".into()));
        }
        // The library's `repo/` holds other projects and is never shown through a folder that
        // holds the library, whether that folder is the library or sits above it.
        let library_repo = self
            .workspace
            .root()
            .ok()
            .filter(|root| {
                crate::util::canonicalize(root).is_ok_and(|root| root.starts_with(&target.path))
            })
            .map(|root| root.join("repo"));
        Guard::new(&target.path, &self.config.data_dir, library_repo)
    }

    /// The status of a worktree, waiting for Git only when nothing is kept and only
    /// `wait` long. `Pending` means Git is still reading and a `data_changed` follows.
    async fn explorer_status(
        &self,
        target: &Target,
        refresh: bool,
        wait: Duration,
    ) -> (Option<Arc<Snapshot>>, ExplorerChanges) {
        // A status names paths from the repository's root, not from a subfolder project's.
        if !target.project.is_git || target.missing || !target.member || target.subfolder {
            return (None, ExplorerChanges::Unavailable);
        }
        let (root, gate) = (target.path.clone(), self.explorer.gate(&target.project.id));
        let read = move || async move {
            let _turn = gate
                .acquire_owned()
                .await
                .map_err(|_| CoreError::Internal("the status gate closed".into()))?;
            status::read(&root).await.map(Arc::new)
        };
        match self
            .explorer
            .statuses
            .get_within(
                &target.key(),
                &topic(&target.project.id),
                refresh,
                wait,
                read,
            )
            .await
        {
            Ok(Some(snapshot)) => (Some(snapshot), ExplorerChanges::Ready),
            Ok(None) => (None, ExplorerChanges::Pending),
            Err(_) => (None, ExplorerChanges::Unavailable),
        }
    }

    /// A project's worktrees with the threads in them and a dirty summary. The list and
    /// the summaries answer from what is kept; a summary not read yet is `None` and a
    /// `data_changed` for `explorer:<project>` follows. `for_thread` marks a worktree
    /// that another thread has a run going in as read-only for that thread.
    pub async fn explorer_worktrees(
        &self,
        project_id: &str,
        for_thread: Option<&str>,
        refresh: bool,
    ) -> Result<ExplorerWorktrees> {
        let project = self.explorer_project(project_id)?;
        let threads = self.store.list_threads(Some(&project.id), false)?;
        if let Some(asking) = for_thread
            && !threads.iter().any(|thread| thread.id == asking)
        {
            return Err(CoreError::NotFound("thread".into()));
        }
        let found = self.explorer_members(&project, refresh).await?;
        let targets: Vec<Target> = found
            .into_iter()
            .map(|entry| Target::new(&project, entry))
            .collect();
        let paths: Vec<PathBuf> = targets.iter().map(|target| target.path.clone()).collect();
        // A thread belongs to the deepest worktree its folder is in.
        let homes: Vec<Option<usize>> = threads
            .iter()
            .map(|thread| {
                let cwd = plain(std::path::Path::new(&thread.cwd));
                (0..paths.len())
                    .filter(|&at| cwd.starts_with(&paths[at]))
                    .max_by_key(|&at| paths[at].components().count())
            })
            .collect();
        let mut worktrees = Vec::with_capacity(targets.len());
        for (index, target) in targets.into_iter().enumerate() {
            let here: Vec<_> = threads
                .iter()
                .zip(&homes)
                .filter(|(_, home)| **home == Some(index))
                .map(|(thread, _)| thread)
                .collect();
            let reason = target.reason().or_else(|| {
                here.iter()
                    .any(|thread| {
                        thread.active_run.is_some() && for_thread.is_some_and(|id| id != thread.id)
                    })
                    .then_some(ExplorerReadOnlyReason::OtherSession)
            });
            let dirty = if index < WARM_LIMIT {
                self.explorer_status(&target, refresh, Duration::ZERO)
                    .await
                    .0
                    .map(|snapshot| snapshot.dirty.clone())
            } else {
                None
            };
            worktrees.push(ExplorerWorktree {
                name: target
                    .path
                    .file_name()
                    .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
                path: target.path.to_string_lossy().into_owned(),
                branch: target.record.branch.clone(),
                detached: target.record.detached,
                head: target.record.revision.chars().take(7).collect(),
                is_main: target.is_main,
                is_subfolder: target.subfolder,
                read_only: reason.is_some(),
                read_only_reason: reason,
                threads: here
                    .into_iter()
                    .map(|thread| ExplorerThread {
                        id: thread.id.clone(),
                        title: thread.title.clone(),
                        running: thread.active_run.is_some(),
                    })
                    .collect(),
                dirty,
                id: target.id,
            });
        }
        Ok(ExplorerWorktrees {
            project_id: project.id,
            git: project.is_git,
            worktrees,
        })
    }

    /// One folder level of a worktree, `path` empty for its root. `filter` keeps names
    /// containing it, ignoring case.
    pub async fn explorer_tree(
        &self,
        project_id: &str,
        worktree_id: &str,
        path: &str,
        filter: &str,
        refresh: bool,
    ) -> Result<ExplorerTree> {
        let target = self.explorer_target(project_id, worktree_id).await?;
        let guard = self.explorer_guard(&target)?;
        let wait = if refresh {
            Duration::from_secs(30)
        } else {
            COLD_WAIT
        };
        let (snapshot, changes) = self.explorer_status(&target, refresh, wait).await;
        let (raw, filter) = (path.to_owned(), filter.to_owned());
        let shown = snapshot.clone();
        let level = tokio::task::spawn_blocking(move || {
            tree::list(&guard, &raw, &filter, shown.as_deref())
        })
        .await
        .map_err(|_| CoreError::Internal("The read stopped.".into()))??;
        Ok(ExplorerTree {
            worktree_id: target.id.clone(),
            path: guard::clean(path, cfg!(windows))?,
            entries: level.entries,
            truncated: level.truncated,
            changes,
            read_only: target.reason().is_some(),
        })
    }

    /// One file of a worktree for the explorer's tab: its text up to a cap, or a marker
    /// for a binary or too large file.
    pub async fn explorer_file(
        &self,
        project_id: &str,
        worktree_id: &str,
        path: &str,
    ) -> Result<ExplorerFile> {
        let target = self.explorer_target(project_id, worktree_id).await?;
        let guard = self.explorer_guard(&target)?;
        let raw = path.to_owned();
        let (resolved, size, bytes) =
            tokio::task::spawn_blocking(move || guard.read(&raw, MAX_FILE_BYTES))
                .await
                .map_err(|_| CoreError::Internal("The read stopped.".into()))??;
        if resolved.rel.is_empty() {
            return Err(CoreError::Invalid("That path is not a file.".into()));
        }
        // Kept letters only: opening a file never waits for Git.
        let (snapshot, _) = self.explorer_status(&target, false, Duration::ZERO).await;
        let change = snapshot.and_then(|snapshot| snapshot.changes.get(&resolved.rel).copied());
        let (kind, content, revision) = match bytes {
            None => (ExplorerFileKind::TooLarge, String::new(), None),
            Some(bytes) => {
                let binary = bytes.contains(&0);
                match (binary, String::from_utf8(bytes)) {
                    (false, Ok(text)) => {
                        let revision = sha256_hex(text.as_bytes());
                        (ExplorerFileKind::Text, text, Some(revision))
                    }
                    _ => (ExplorerFileKind::Binary, String::new(), None),
                }
            }
        };
        Ok(ExplorerFile {
            worktree_id: target.id.clone(),
            path: resolved.rel,
            kind,
            content,
            size,
            revision,
            change: change.filter(|change| *change != ExplorerChange::Deleted),
            read_only: target.reason().is_some(),
        })
    }
}
