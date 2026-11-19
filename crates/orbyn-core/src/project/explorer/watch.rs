//! Native watches shared by focused windows. A lease owns every OS handle and task.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, TryLockError, Weak};
use std::time::{Duration, Instant};

use notify::event::{CreateKind, ModifyKind, RemoveKind};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc};
use tokio_util::sync::CancellationToken;

use super::{Core, CoreError, Guard, Result, locked};

const MAX_WATCHES: usize = 64;
const MAX_DIRECTORIES: usize = 512;
/// Every watched directory costs one OS handle. The default Linux inotify limit is 8,192
/// per user, shared with every other program, so the whole service takes at most half.
const MAX_TOTAL_DIRECTORIES: usize = 4096;
const MAX_ENTRIES: usize = 20_000;
/// Directory handles held open at once while a level is registered and read.
const OPEN_CHUNK: usize = 64;
/// Directory-shaped changes rebuild the watched set at most this often. A change that
/// arrives inside the gap runs once more when it ends, so the last one is never lost.
const RECONCILE_GAP: Duration = Duration::from_secs(1);

pub(super) struct Watches {
    held: Mutex<HashMap<String, Weak<ExplorerWatch>>>,
    slots: Arc<Semaphore>,
    /// One permit per watched directory, across every watch.
    directories: Arc<Semaphore>,
}

impl Default for Watches {
    fn default() -> Self {
        Self {
            held: Mutex::default(),
            slots: Arc::new(Semaphore::new(MAX_WATCHES)),
            directories: Arc::new(Semaphore::new(MAX_TOTAL_DIRECTORIES)),
        }
    }
}

#[cfg(test)]
#[derive(Default)]
struct Stats {
    reconciles: usize,
    added: usize,
    removed: usize,
    commits: usize,
}

/// Which folder a watch was made for. A folder deleted and recreated under the same
/// name is another folder, and some backends drop their watch when the old one goes.
type Identity = (u64, u64);

struct Watched {
    _permit: OwnedSemaphorePermit,
    identity: Identity,
}

#[cfg(unix)]
fn identity(handle: &std::fs::File) -> Identity {
    use std::os::unix::fs::MetadataExt;
    handle
        .metadata()
        .map_or((0, 0), |meta| (meta.dev(), meta.ino()))
}

#[cfg(windows)]
fn identity(handle: &std::fs::File) -> Identity {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    // SAFETY: the handle stays owned by the caller; the struct is plain data the call fills.
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: a live handle and a writable struct of the expected type.
    if unsafe { GetFileInformationByHandle(handle.as_raw_handle(), &mut info) } == 0 {
        return (0, 0);
    }
    (
        u64::from(info.dwVolumeSerialNumber),
        u64::from(info.nFileIndexHigh) << 32 | u64::from(info.nFileIndexLow),
    )
}

#[cfg(not(any(unix, windows)))]
fn identity(_: &std::fs::File) -> Identity {
    (0, 0)
}

struct Resources {
    native: RecommendedWatcher,
    /// Each watched directory with the permit it holds on the service-wide budget.
    directories: BTreeMap<PathBuf, Watched>,
    /// Folders to watch again at the next scan; filled by the worker from native events.
    rewatch: BTreeSet<PathBuf>,
    budget: Arc<Semaphore>,
    _slot: OwnedSemaphorePermit,
    #[cfg(test)]
    stats: Stats,
}

impl Resources {
    fn new(native: RecommendedWatcher, slot: OwnedSemaphorePermit, budget: Arc<Semaphore>) -> Self {
        Self {
            native,
            directories: BTreeMap::new(),
            rewatch: BTreeSet::new(),
            budget,
            _slot: slot,
            #[cfg(test)]
            stats: Stats::default(),
        }
    }
}

/// Kept by the response stream. Its last drop stops the native watcher and worker.
pub struct ExplorerWatch {
    state: Arc<WatchState>,
    worker: Mutex<Option<tokio::task::AbortHandle>>,
}

struct WatchState {
    resources: Mutex<Option<Resources>>,
    stopped: CancellationToken,
}

impl ExplorerWatch {
    /// Resolves when authority or the native backend fails; the transport closes.
    pub async fn stopped(&self) {
        self.state.stopped.cancelled().await;
    }
}

impl WatchState {
    /// Cancels first: a scan holding the lock sees it and stops, and this never waits for
    /// one. When a scan is running it drops the handles itself once it stops.
    fn revoke(&self) {
        self.stopped.cancel();
        match self.resources.try_lock() {
            Ok(mut held) => drop(held.take()),
            Err(TryLockError::Poisoned(poisoned)) => drop(poisoned.into_inner().take()),
            Err(TryLockError::WouldBlock) => {}
        }
    }
}

impl Drop for ExplorerWatch {
    fn drop(&mut self) {
        self.state.revoke();
        if let Some(worker) = locked(&self.worker).take() {
            worker.abort();
        }
    }
}

fn unavailable() -> CoreError {
    CoreError::WatchUnavailable(
        "Explorer file watching is unavailable. Refresh to read changes.".into(),
    )
}

/// What the callback tells the worker. The flags survive a full wake-up channel.
#[derive(Default)]
struct Pending {
    /// A directory may have appeared, vanished or moved: the watched set needs a rebuild.
    structure: AtomicBool,
    failed: AtomicBool,
    /// Where a folder was removed or renamed. Some backends drop their own watch on such
    /// a folder, so the next rebuild watches it again whatever its identity says.
    removed: Mutex<BTreeSet<PathBuf>>,
}

/// More removed paths than this stand for "everything under the root".
const MAX_REMOVED: usize = 1024;

/// On Windows one recursive handle covers the tree and events name paths inside it,
/// including ones the view never shows. They are dropped here, before they announce.
fn event_allowed(guard: &Guard, root: &Path, event: &Event) -> bool {
    event.paths.is_empty()
        || event.paths.iter().any(|path| {
            let Ok(inside) = path.strip_prefix(root) else {
                return true;
            };
            let mut dir = root.to_path_buf();
            for part in inside.components() {
                let Some(name) = part.as_os_str().to_str() else {
                    return false;
                };
                if !guard.lists(&dir, name) {
                    return false;
                }
                dir.push(part);
            }
            true
        })
}

/// Plain file content changes never alter which directories exist.
fn changes_structure(event: &Event, roots: &[&Path]) -> bool {
    matches!(
        event.kind,
        EventKind::Create(CreateKind::Folder | CreateKind::Any)
            | EventKind::Remove(RemoveKind::Folder | RemoveKind::Any)
            | EventKind::Modify(ModifyKind::Name(_))
    ) || event.need_rescan()
        || event
            .paths
            .iter()
            .any(|path| roots.contains(&path.as_path()))
}

/// Stops a scan that hit a failure under the root only; one folder that vanished or
/// cannot be read is skipped instead.
fn skipped(dir: &Path, root: &Path, error: impl Into<CoreError>) -> Result<()> {
    if dir == root {
        Err(error.into())
    } else {
        Ok(())
    }
}

/// Releases watches in one native restart. Some backends restart their stream on every
/// change to the set, so changes are always batched.
fn release(resources: &mut Resources, gone: &[PathBuf]) -> Result<()> {
    if gone.is_empty() {
        return Ok(());
    }
    let mut paths = resources.native.paths_mut();
    for old in gone {
        let _ = paths.remove(old);
        resources.directories.remove(old);
    }
    #[cfg(test)]
    {
        resources.stats.removed += gone.len();
        resources.stats.commits += 1;
    }
    paths.commit().map_err(|_| unavailable())
}

/// Watches the directories not yet watched, and those whose folder was replaced under the
/// same name, in one native restart; says which of the given directories are watched now.
/// Admission is decided first, so a tree past a bound costs no native call at all.
fn register(
    resources: &mut Resources,
    dirs: &[(&Path, Identity)],
    root: &Path,
    mode: RecursiveMode,
    marks: &BTreeSet<PathBuf>,
) -> Result<Vec<bool>> {
    let mut batch = Vec::new();
    let mut replaced = Vec::new();
    for (dir, identity) in dirs {
        match resources.directories.get(*dir) {
            Some(watched)
                if watched.identity == *identity
                    && !dir.ancestors().any(|above| marks.contains(above)) => {}
            Some(_) => {
                // Keep its permit: the replacement takes the place of the old watch.
                if let Some(old) = resources.directories.remove(*dir) {
                    replaced.push(*dir);
                    batch.push((*dir, *identity, old._permit));
                }
            }
            None => {
                if resources.directories.len() + batch.len() >= MAX_DIRECTORIES {
                    continue;
                }
                if let Ok(permit) = resources.budget.clone().try_acquire_owned() {
                    batch.push((*dir, *identity, permit));
                }
            }
        }
    }
    if !batch.is_empty() {
        let mut paths = resources.native.paths_mut();
        for dir in &replaced {
            let _ = paths.remove(dir);
        }
        for (dir, identity, permit) in batch {
            match paths.add(dir, mode) {
                Ok(()) => {
                    resources.directories.insert(
                        dir.to_path_buf(),
                        Watched {
                            _permit: permit,
                            identity,
                        },
                    );
                    #[cfg(test)]
                    {
                        resources.stats.added += 1;
                    }
                }
                Err(_) if dir == root => return Err(unavailable()),
                Err(_) => {}
            }
        }
        #[cfg(test)]
        {
            resources.stats.removed += replaced.len();
            resources.stats.commits += 1;
        }
        paths.commit().map_err(|_| unavailable())?;
    }
    Ok(dirs
        .iter()
        .map(|(dir, _)| resources.directories.contains_key(*dir))
        .collect())
}

/// No recursive native registration: that could follow a link or traverse denied data.
/// Each level is watched before it is listed, closing the scan/register gap. Only the
/// difference from the watched set reaches the backend, so a tree that did not change
/// costs no native call at all.
fn reconcile(
    resources: &mut Resources,
    guard: &Guard,
    root: &Path,
    cancel: &CancellationToken,
) -> Result<()> {
    // Children are compared with this spelling, so /var and /private/var agree.
    let root =
        crate::util::canonicalize(root).map_err(|_| CoreError::NotFound("worktree".into()))?;
    scan(resources, guard, &root, cancel, vec![root.clone()])
}

fn scan(
    resources: &mut Resources,
    guard: &Guard,
    root: &Path,
    cancel: &CancellationToken,
    mut level: Vec<PathBuf>,
) -> Result<()> {
    #[cfg(test)]
    {
        resources.stats.reconciles += 1;
    }
    let marks = std::mem::take(&mut resources.rewatch);
    if cfg!(windows) {
        return scan_root(resources, guard, root, cancel, &marks);
    }
    // Free what is gone first: after a large rename the old names would otherwise
    // count against the bound next to the new ones.
    let gone: Vec<PathBuf> = resources
        .directories
        .keys()
        .filter(|dir| !std::fs::symlink_metadata(dir).is_ok_and(|meta| meta.is_dir()))
        .cloned()
        .collect();
    release(resources, &gone)?;
    let mut seen = BTreeSet::new();
    let mut entries = 0;
    while !level.is_empty() {
        let mut next = Vec::new();
        for chunk in level.chunks(OPEN_CHUNK) {
            if cancel.is_cancelled() {
                return Ok(());
            }
            let mut open = Vec::with_capacity(chunk.len());
            for dir in chunk {
                match guard.open_dir(dir) {
                    Ok(handle) => open.push((dir.as_path(), handle)),
                    Err(error) => skipped(dir, root, error)?,
                }
            }
            let dirs: Vec<(&Path, Identity)> = open
                .iter()
                .map(|(dir, handle)| (*dir, identity(handle)))
                .collect();
            let watched = register(resources, &dirs, root, RecursiveMode::NonRecursive, &marks)?;
            for ((dir, handle), watched) in open.into_iter().zip(watched) {
                if cancel.is_cancelled() {
                    return Ok(());
                }
                if !watched {
                    skipped(dir, root, unavailable())?;
                    continue;
                }
                let listing = match std::fs::read_dir(dir) {
                    Ok(listing) => listing,
                    Err(error) => {
                        skipped(dir, root, error)?;
                        continue;
                    }
                };
                let mut children = Vec::new();
                for entry in listing {
                    if entries == MAX_ENTRIES {
                        break;
                    }
                    entries += 1;
                    let Ok(entry) = entry else { continue };
                    let name = entry.file_name();
                    let Some(name) = name.to_str() else { continue };
                    let Ok(kind) = entry.file_type() else {
                        continue;
                    };
                    if guard.lists(dir, name) && kind.is_dir() && !kind.is_symlink() {
                        // Resolve before opening: Windows junctions can be directories too.
                        let path = entry.path();
                        if crate::util::canonicalize(&path).is_ok_and(|real| real == path) {
                            children.push(path);
                        }
                    }
                }
                match guard.confirm_dir(&handle, dir) {
                    Ok(()) => {
                        seen.insert(dir.to_path_buf());
                        next.extend(children);
                    }
                    Err(error) => skipped(dir, root, error)?,
                }
            }
        }
        level = next;
    }
    let stale: Vec<PathBuf> = resources
        .directories
        .keys()
        .filter(|dir| !seen.contains(*dir))
        .cloned()
        .collect();
    release(resources, &stale)
}

/// Windows reads a folder through a handle that blocks renaming or deleting it, so one
/// handle per folder would block moving any watched ancestor. One recursive handle on
/// the root covers the tree instead; only the root itself stays locked while watched.
fn scan_root(
    resources: &mut Resources,
    guard: &Guard,
    root: &Path,
    cancel: &CancellationToken,
    marks: &BTreeSet<PathBuf>,
) -> Result<()> {
    if cancel.is_cancelled() {
        return Ok(());
    }
    let handle = guard.open_dir(root)?;
    let watched = register(
        resources,
        &[(root, identity(&handle))],
        root,
        RecursiveMode::Recursive,
        marks,
    )?;
    if !watched[0] {
        return Err(unavailable());
    }
    guard.confirm_dir(&handle, root)?;
    let others: Vec<PathBuf> = resources
        .directories
        .keys()
        .filter(|dir| dir.as_path() != root)
        .cloned()
        .collect();
    release(resources, &others)
}

impl Core {
    /// Names are ids, never paths. At most 32 pairs belong to one window connection.
    pub async fn explorer_watch(
        self: &Arc<Self>,
        pairs: &[(String, String)],
    ) -> Result<Vec<Arc<ExplorerWatch>>> {
        if pairs.is_empty() || pairs.len() > 32 {
            return Err(CoreError::Invalid(
                "Watch between 1 and 32 worktrees.".into(),
            ));
        }
        let mut watches = Vec::new();
        for (project, worktree) in pairs.iter().collect::<BTreeSet<_>>() {
            let projects = self.store.project_changes();
            let target = self.explorer_target(project, worktree).await?;
            let guard = self.explorer_guard(&target)?;
            let key = super::key(&[project, worktree]);
            let existing = locked(&self.explorer.watches.held)
                .get(&key)
                .and_then(Weak::upgrade);
            if let Some(existing) = existing.filter(|watch| !watch.state.stopped.is_cancelled()) {
                watches.push(existing);
                continue;
            }
            let slot = self
                .explorer
                .watches
                .slots
                .clone()
                .try_acquire_owned()
                .map_err(|_| unavailable())?;
            let (send, receive) = mpsc::channel(1);
            let pending = Arc::new(Pending::default());
            let callback_pending = pending.clone();
            let root = target.path;
            let real_root = crate::util::canonicalize(&root).unwrap_or_else(|_| root.clone());
            let (callback_root, callback_real) = (root.clone(), real_root);
            let event_guard = guard.clone();
            let native = RecommendedWatcher::new(
                move |event: notify::Result<Event>| {
                    match &event {
                        Err(_) => callback_pending.failed.store(true, Ordering::Release),
                        // Reading a folder must not wake its own watcher forever.
                        Ok(event) if matches!(event.kind, EventKind::Access(_)) => return,
                        Ok(event)
                            if cfg!(windows)
                                && !event_allowed(&event_guard, &callback_real, event) =>
                        {
                            return;
                        }
                        Ok(event) => {
                            if changes_structure(
                                event,
                                &[callback_root.as_path(), callback_real.as_path()],
                            ) {
                                callback_pending.structure.store(true, Ordering::Release);
                                if event.need_rescan()
                                    || matches!(
                                        event.kind,
                                        EventKind::Remove(RemoveKind::Folder | RemoveKind::Any)
                                            | EventKind::Modify(ModifyKind::Name(_))
                                    )
                                {
                                    let mut removed = locked(&callback_pending.removed);
                                    if event.need_rescan() || removed.len() >= MAX_REMOVED {
                                        removed.insert(callback_real.clone());
                                    } else {
                                        removed.extend(event.paths.iter().cloned());
                                    }
                                }
                            }
                        }
                    }
                    let _ = send.try_send(());
                },
                Config::default().with_follow_symlinks(false),
            )
            .map_err(|_| unavailable())?;
            let mut resources =
                Resources::new(native, slot, self.explorer.watches.directories.clone());
            let cancel = CancellationToken::new();
            let scan_cancel = cancel.clone();
            let resources = tokio::task::spawn_blocking(move || {
                reconcile(&mut resources, &guard, &root, &scan_cancel)?;
                Ok::<_, CoreError>(resources)
            })
            .await
            .map_err(|_| unavailable())??;
            let watch = Arc::new(ExplorerWatch {
                state: Arc::new(WatchState {
                    resources: Mutex::new(Some(resources)),
                    stopped: cancel,
                }),
                worker: Mutex::default(),
            });
            // Another window may have registered while the blocking scan ran.
            {
                let mut held = locked(&self.explorer.watches.held);
                held.retain(|_, watch| watch.strong_count() > 0);
                if let Some(existing) = held
                    .get(&key)
                    .and_then(Weak::upgrade)
                    .filter(|watch| !watch.state.stopped.is_cancelled())
                {
                    watches.push(existing);
                    continue;
                }
                held.insert(key, Arc::downgrade(&watch));
            }
            let worker = tokio::spawn(follow(
                Arc::downgrade(self),
                Arc::downgrade(&watch.state),
                project.clone(),
                worktree.clone(),
                receive,
                projects,
                pending,
            ));
            *locked(&watch.worker) = Some(worker.abort_handle());
            watches.push(watch);
        }
        // Recheck after registration and before handing leases to transport. The stream
        // sends each window its ready frame, so nothing is announced here; kept answers
        // from before the watch existed are still forgotten.
        for (project, _) in pairs {
            self.explorer_project(project)?;
            self.explorer.forget(project);
        }
        Ok(watches)
    }
}

async fn follow(
    core: Weak<Core>,
    lease: Weak<WatchState>,
    project: String,
    worktree: String,
    mut events: mpsc::Receiver<()>,
    mut projects: tokio::sync::watch::Receiver<u64>,
    pending: Arc<Pending>,
) {
    let mut last_rebuild: Option<Instant> = None;
    loop {
        let native = tokio::select! {
            event = events.recv() => { if event.is_none() { return; } true },
            change = projects.changed() => { if change.is_err() { return; } false },
        };
        if native {
            tokio::time::sleep(super::ANNOUNCE_GAP).await;
            while events.try_recv().is_ok() {}
            // Build and install storms: rebuild once per gap, with a trailing run.
            if pending.structure.load(Ordering::Acquire)
                && let Some(wait) = last_rebuild
                    .map(|at| RECONCILE_GAP.saturating_sub(at.elapsed()))
                    .filter(|wait| !wait.is_zero())
            {
                tokio::time::sleep(wait).await;
                while events.try_recv().is_ok() {}
            }
        }
        let (Some(core), Some(lease)) = (core.upgrade(), lease.upgrade()) else {
            return;
        };
        let rebuild = pending.structure.swap(false, Ordering::AcqRel);
        let removed = std::mem::take(&mut *locked(&pending.removed));
        if pending.failed.load(Ordering::Acquire) {
            core.explorer.changed(&project);
            lease.revoke();
            return;
        }
        if !native || rebuild {
            // A project change only needs its authority checked; so does a rebuild.
            let checked = async {
                let target = core.explorer_target(&project, &worktree).await?;
                let guard = core.explorer_guard(&target)?;
                if !rebuild {
                    return Ok(());
                }
                let lease = lease.clone();
                tokio::task::spawn_blocking(move || {
                    let mut held = locked(&lease.resources);
                    let resources = held.as_mut().ok_or_else(unavailable)?;
                    resources.rewatch.extend(removed);
                    let done = reconcile(resources, &guard, &target.path, &lease.stopped);
                    if lease.stopped.is_cancelled() {
                        // The lease was dropped mid-scan; its handles go with this thread.
                        drop(held.take());
                        return Err(unavailable());
                    }
                    done
                })
                .await
                .map_err(|_| unavailable())?
            }
            .await;
            if checked.is_err() {
                core.explorer.changed(&project);
                lease.revoke();
                return;
            }
            if rebuild {
                last_rebuild = Some(Instant::now());
            }
        }
        if native {
            // Never serve a cached membership after the event that could have changed it.
            core.explorer.changed(&project);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{core, entry, main_id, project, write};
    use super::*;
    use crate::testing::TempDir;
    use orbyn_contracts::{ExplorerChange, ExplorerReadOnlyReason, ShellEvent};
    use std::time::Duration;

    fn unwatched(budget: &Arc<Semaphore>) -> Resources {
        let native = RecommendedWatcher::new(
            |_: notify::Result<Event>| {},
            Config::default().with_follow_symlinks(false),
        )
        .unwrap();
        Resources::new(
            native,
            Arc::new(Semaphore::new(1)).try_acquire_owned().unwrap(),
            budget.clone(),
        )
    }

    fn roomy() -> Arc<Semaphore> {
        Arc::new(Semaphore::new(MAX_TOTAL_DIRECTORIES))
    }

    /// The revoking thread drops the handles after the stop is visible, so wait for it.
    async fn released(watch: &ExplorerWatch) {
        tokio::time::timeout(Duration::from_secs(3), async {
            while locked(&watch.state.resources).is_some() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the handles are released after the stop");
    }

    fn reconciles(watch: &ExplorerWatch) -> usize {
        locked(&watch.state.resources)
            .as_ref()
            .unwrap()
            .stats
            .reconciles
    }

    async fn changed(events: &mut crate::host::shell::ShellWatch, project: &str) {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if matches!(events.events.recv().await.unwrap(), ShellEvent::DataChanged { what } if what == format!("explorer:{project}")) {
                    break;
                }
            }
        }).await.expect("native edit announces before the five-second freshness limit");
    }

    #[tokio::test]
    async fn external_edit_invalidates_fresh_status_and_pushes_without_polling() {
        let dir = TempDir::new();
        let core = Arc::new(core(&dir).await);
        let (project, root) = project(&core, &dir, "app").await;
        let id = main_id(&core, &project).await;
        let watches = core
            .explorer_watch(&[(project.id.clone(), id.clone())])
            .await
            .unwrap();
        let first = core
            .explorer_tree(&project.id, &id, "", "", true)
            .await
            .unwrap();
        assert_eq!(entry(&first.entries, "a.txt").change, None);
        let mut events = core.shell.watch();
        write(&root.join("a.txt"), "external edit\n");
        // Initial background reads can also announce. Only an event triggers a
        // reread, exactly as in the client; no freshness timer or forced read runs.
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                changed(&mut events, &project.id).await;
                let next = core
                    .explorer_tree(&project.id, &id, "", "", false)
                    .await
                    .unwrap();
                if entry(&next.entries, "a.txt").change == Some(ExplorerChange::Modified) {
                    break;
                }
            }
        })
        .await
        .expect("external edit becomes visible from pushed events");
        drop(watches);
        assert_eq!(core.explorer.watches.slots.available_permits(), MAX_WATCHES);
    }

    #[tokio::test]
    async fn windows_share_a_watch_and_the_last_disconnect_releases_every_handle() {
        let dir = TempDir::new();
        let core = Arc::new(core(&dir).await);
        let (project, _) = project(&core, &dir, "app").await;
        let id = main_id(&core, &project).await;
        let pairs = [(project.id, id)];
        let first = core.explorer_watch(&pairs).await.unwrap();
        let second = core.explorer_watch(&pairs).await.unwrap();
        assert!(Arc::ptr_eq(&first[0], &second[0]));
        assert_eq!(
            core.explorer.watches.slots.available_permits(),
            MAX_WATCHES - 1
        );
        assert!(core.explorer.watches.directories.available_permits() < MAX_TOTAL_DIRECTORIES);
        drop(first);
        assert!(locked(&second[0].state.resources).is_some());
        drop(second);
        assert_eq!(core.explorer.watches.slots.available_permits(), MAX_WATCHES);
        assert_eq!(
            core.explorer.watches.directories.available_permits(),
            MAX_TOTAL_DIRECTORIES
        );
        assert!(core.explorer_watch(&[]).await.is_err());
        assert!(
            core.explorer_watch(&vec![pairs[0].clone(); 33])
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn an_externally_removed_root_announces_and_releases_its_watch() {
        let dir = TempDir::new();
        let core = Arc::new(core(&dir).await);
        let folder = dir.path().join("plain");
        write(&folder.join("a.txt"), "one");
        let project = core.add_project(&folder.to_string_lossy()).await.unwrap();
        let project = core.store.trust_project(&project.id, "test").unwrap();
        assert!(
            !project.is_git,
            "no asynchronous status announcements can mask the hint"
        );
        let id = main_id(&core, &project).await;
        let watches = core
            .explorer_watch(&[(project.id.clone(), id)])
            .await
            .unwrap();
        let mut events = core.shell.watch();
        // The subscription's initial hint was held before this receiver existed.
        events.held.clear();
        std::fs::remove_dir_all(&project.path).unwrap();
        tokio::time::timeout(Duration::from_secs(3), watches[0].stopped())
            .await
            .expect("removing the root revokes its native watch");
        changed(&mut events, &project.id).await;
        released(&watches[0]).await;
        assert_eq!(core.explorer.watches.slots.available_permits(), MAX_WATCHES);
        let listed = core
            .explorer_worktrees(&project.id, None, false)
            .await
            .unwrap();
        assert_eq!(
            listed.worktrees[0].read_only_reason,
            Some(ExplorerReadOnlyReason::Missing)
        );
    }

    #[tokio::test]
    async fn project_removal_revokes_idle_watchers_without_a_filesystem_event() {
        let dir = TempDir::new();
        let core = Arc::new(core(&dir).await);
        let (project, _) = project(&core, &dir, "app").await;
        let id = main_id(&core, &project).await;
        let watches = core
            .explorer_watch(&[(project.id.clone(), id)])
            .await
            .unwrap();
        core.store
            .remove_project_keeping_threads(&project.id, "test")
            .unwrap();
        tokio::time::timeout(Duration::from_secs(3), watches[0].stopped())
            .await
            .unwrap();
        released(&watches[0]).await;
        assert_eq!(core.explorer.watches.slots.available_permits(), MAX_WATCHES);
    }

    #[tokio::test]
    async fn untrusted_and_foreign_worktree_ids_never_allocate_watches() {
        let dir = TempDir::new();
        let core = Arc::new(core(&dir).await);
        let (project, _) = project(&core, &dir, "app").await;
        let raw = super::super::tests::repo(&dir.path().join("raw"));
        let untrusted = core.add_project(&raw.to_string_lossy()).await.unwrap();
        let id = main_id(&core, &project).await;
        assert!(matches!(
            core.explorer_watch(&[(untrusted.id, id)]).await,
            Err(CoreError::FolderUntrusted)
        ));
        assert!(
            core.explorer_watch(&[(project.id.clone(), "wt-0000000000000000".into())])
                .await
                .is_err()
        );
        let (other, _) = super::super::tests::project(&core, &dir, "other").await;
        let foreign = main_id(&core, &other).await;
        assert!(
            core.explorer_watch(&[(project.id.clone(), foreign.clone())])
                .await
                .is_err()
        );
        assert!(
            core.explorer_watch(&[
                (other.id, foreign),
                (project.id, "wt-0000000000000000".into())
            ])
            .await
            .is_err()
        );
        assert_eq!(core.explorer.watches.slots.available_permits(), MAX_WATCHES);
    }

    #[tokio::test]
    async fn an_atomic_save_and_a_new_directory_remain_observable() {
        let dir = TempDir::new();
        let core = Arc::new(core(&dir).await);
        let (project, root) = project(&core, &dir, "app").await;
        let id = main_id(&core, &project).await;
        let watches = core
            .explorer_watch(&[(project.id.clone(), id.clone())])
            .await
            .unwrap();
        let mut events = core.shell.watch();
        write(&root.join("new folder/雪.txt"), "one");
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                changed(&mut events, &project.id).await;
                if locked(&watches[0].state.resources)
                    .as_ref()
                    .unwrap()
                    .directories
                    .contains_key(&root.join("new folder"))
                    || cfg!(windows)
                {
                    break;
                }
            }
        })
        .await
        .unwrap();
        write(&root.join("new folder/save.tmp"), "replacement");
        std::fs::rename(
            root.join("new folder/save.tmp"),
            root.join("new folder/雪.txt"),
        )
        .unwrap();
        changed(&mut events, &project.id).await;
        let file = core
            .explorer_file(&project.id, &id, "new folder/雪.txt")
            .await
            .unwrap();
        assert_eq!(file.content, "replacement");
    }

    #[tokio::test]
    async fn the_service_watch_limit_refuses_admission_and_recovers_after_release() {
        let dir = TempDir::new();
        let core = Arc::new(core(&dir).await);
        let (project, _) = project(&core, &dir, "app").await;
        let id = main_id(&core, &project).await;
        let reserved = core
            .explorer
            .watches
            .slots
            .clone()
            .acquire_many_owned(MAX_WATCHES as u32)
            .await
            .unwrap();
        let refused = core
            .explorer_watch(&[(project.id.clone(), id.clone())])
            .await
            .err()
            .expect("a full table refuses");
        assert_eq!(refused.status(), 503, "busy, not invalid: clients retry");
        drop(reserved);
        let watches = core.explorer_watch(&[(project.id, id)]).await.unwrap();
        assert_eq!(
            core.explorer.watches.slots.available_permits(),
            MAX_WATCHES - 1
        );
        drop(watches);
        assert_eq!(core.explorer.watches.slots.available_permits(), MAX_WATCHES);
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn guarded_roots_and_links_are_never_watched_and_directory_count_is_bounded() {
        let dir = TempDir::new();
        let root = crate::util::canonicalize(dir.path()).unwrap();
        write(&root.join("data/token"), "private");
        write(&root.join("repo/other/x"), "private");
        write(&root.join(".git/config"), "metadata");
        write(&root.join(".env"), "visible");
        for n in 0..(MAX_DIRECTORIES + 20) {
            std::fs::create_dir(root.join(format!("d{n}"))).unwrap();
        }
        let outside = TempDir::new();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), root.join("linked")).unwrap();
        #[cfg(windows)]
        assert!(
            std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(root.join("linked"))
                .arg(outside.path())
                .output()
                .unwrap()
                .status
                .success()
        );
        let guard = Guard::new(&root, &root.join("data"), Some(root.join("repo"))).unwrap();
        let mut resources = unwatched(&roomy());
        let cancel = CancellationToken::new();
        reconcile(&mut resources, &guard, &root, &cancel).unwrap();
        assert_eq!(resources.directories.len(), MAX_DIRECTORIES);
        let commits = resources.stats.commits;
        reconcile(&mut resources, &guard, &root, &cancel).unwrap();
        assert_eq!(
            resources.stats.commits, commits,
            "folders past the bound are never offered to the backend again"
        );
        for denied in ["data", "repo", ".git", "linked"] {
            assert!(
                !resources
                    .directories
                    .keys()
                    .any(|path| path.starts_with(root.join(denied)))
            );
        }
        assert!(guard.read(".env", 100).is_ok());
        let denied = Guard::new(&root.join("data"), &root.join("data"), None).unwrap();
        assert!(reconcile(&mut resources, &denied, &root.join("data"), &cancel).is_err());
    }

    #[cfg(not(windows))]
    #[test]
    fn an_unchanged_tree_costs_no_native_call_and_a_change_costs_one_restart() {
        let dir = TempDir::new();
        let root = crate::util::canonicalize(dir.path()).unwrap();
        for sub in ["a/b/c", "d"] {
            std::fs::create_dir_all(root.join(sub)).unwrap();
        }
        let guard = Guard::new(&root, &root.join("data"), None).unwrap();
        let mut resources = unwatched(&roomy());
        let cancel = CancellationToken::new();
        reconcile(&mut resources, &guard, &root, &cancel).unwrap();
        assert_eq!(resources.directories.len(), 5);
        // One restart per level, not one per directory.
        assert_eq!(
            (
                resources.stats.added,
                resources.stats.removed,
                resources.stats.commits
            ),
            (5, 0, 4)
        );
        reconcile(&mut resources, &guard, &root, &cancel).unwrap();
        assert_eq!(
            (
                resources.stats.added,
                resources.stats.removed,
                resources.stats.commits
            ),
            (5, 0, 4),
            "nothing changed, so the backend is never touched"
        );
        std::fs::create_dir(root.join("d/e")).unwrap();
        reconcile(&mut resources, &guard, &root, &cancel).unwrap();
        assert_eq!(
            (
                resources.stats.added,
                resources.stats.removed,
                resources.stats.commits
            ),
            (6, 0, 5)
        );
        std::fs::remove_dir_all(root.join("a")).unwrap();
        reconcile(&mut resources, &guard, &root, &cancel).unwrap();
        assert_eq!(
            (
                resources.stats.added,
                resources.stats.removed,
                resources.stats.commits
            ),
            (6, 3, 6)
        );
        assert_eq!(resources.directories.len(), 3);
    }

    #[cfg(not(windows))]
    #[test]
    fn a_folder_that_vanished_or_cannot_be_read_is_skipped_but_the_root_is_not() {
        let dir = TempDir::new();
        let root = crate::util::canonicalize(dir.path()).unwrap();
        std::fs::create_dir_all(root.join("kept/inner")).unwrap();
        let guard = Guard::new(&root, &root.join("data"), None).unwrap();
        let cancel = CancellationToken::new();
        let mut resources = unwatched(&roomy());
        // Queued while it existed and deleted before the scan reached it.
        scan(
            &mut resources,
            &guard,
            &root,
            &cancel,
            vec![root.clone(), root.join("ghost")],
        )
        .unwrap();
        assert!(resources.directories.contains_key(&root.join("kept/inner")));
        assert!(!resources.directories.contains_key(&root.join("ghost")));
        assert!(reconcile(&mut resources, &guard, &root.join("missing"), &cancel).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let locked_dir = root.join("locked");
            std::fs::create_dir(&locked_dir).unwrap();
            std::fs::set_permissions(&locked_dir, std::fs::Permissions::from_mode(0o000)).unwrap();
            let unreadable = std::fs::read_dir(&locked_dir).is_err();
            let result = reconcile(&mut resources, &guard, &root, &cancel);
            std::fs::set_permissions(&locked_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
            result.unwrap();
            assert!(resources.directories.contains_key(&root.join("kept")));
            if unreadable {
                assert!(!resources.directories.contains_key(&locked_dir));
            }
        }
    }

    #[tokio::test]
    async fn a_vanished_or_unreadable_subfolder_does_not_end_the_watch() {
        let dir = TempDir::new();
        let core = Arc::new(core(&dir).await);
        let (project, root) = project(&core, &dir, "app").await;
        let id = main_id(&core, &project).await;
        std::fs::create_dir_all(root.join("doomed/deeper")).unwrap();
        #[cfg(unix)]
        let locked_dir = {
            use std::os::unix::fs::PermissionsExt;
            let path = root.join("locked");
            std::fs::create_dir(&path).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
            path
        };
        let watches = core
            .explorer_watch(&[(project.id.clone(), id)])
            .await
            .unwrap();
        let mut events = core.shell.watch();
        std::fs::remove_dir_all(root.join("doomed")).unwrap();
        changed(&mut events, &project.id).await;
        write(&root.join("after.txt"), "still announced");
        changed(&mut events, &project.id).await;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&locked_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert!(!watches[0].state.stopped.is_cancelled());
        assert!(
            !locked(&watches[0].state.resources)
                .as_ref()
                .unwrap()
                .directories
                .contains_key(&root.join("doomed"))
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn the_service_wide_directory_budget_caps_every_watch_and_returns_on_release() {
        let dir = TempDir::new();
        let root = crate::util::canonicalize(dir.path()).unwrap();
        for n in 0..10 {
            std::fs::create_dir(root.join(format!("d{n}"))).unwrap();
        }
        let guard = Guard::new(&root, &root.join("data"), None).unwrap();
        let cancel = CancellationToken::new();
        let budget = Arc::new(Semaphore::new(3));
        let mut first = unwatched(&budget);
        reconcile(&mut first, &guard, &root, &cancel).unwrap();
        assert_eq!(first.directories.len(), 3);
        assert_eq!(budget.available_permits(), 0);
        let mut second = unwatched(&budget);
        assert!(
            reconcile(&mut second, &guard, &root, &cancel).is_err(),
            "no budget left for even the root"
        );
        drop(first);
        assert_eq!(budget.available_permits(), 3);
        reconcile(&mut second, &guard, &root, &cancel).unwrap();
        assert_eq!(second.directories.len(), 3);
    }

    #[test]
    fn a_cancelled_scan_stops_without_touching_the_backend() {
        let dir = TempDir::new();
        let root = crate::util::canonicalize(dir.path()).unwrap();
        std::fs::create_dir(root.join("a")).unwrap();
        let guard = Guard::new(&root, &root.join("data"), None).unwrap();
        let cancel = CancellationToken::new();
        cancel.cancel();
        let mut resources = unwatched(&roomy());
        reconcile(&mut resources, &guard, &root, &cancel).unwrap();
        assert_eq!(resources.stats.added, 0);
    }

    #[test]
    fn revoking_never_waits_for_a_scan_that_holds_the_handles() {
        let state = WatchState {
            resources: Mutex::new(Some(unwatched(&roomy()))),
            stopped: CancellationToken::new(),
        };
        let scanning = locked(&state.resources);
        state.revoke();
        assert!(state.stopped.is_cancelled(), "the scan can see the stop");
        assert!(scanning.is_some(), "the scan drops the handles itself");
        drop(scanning);
        state.revoke();
        assert!(locked(&state.resources).is_none());
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn a_burst_of_new_folders_rebuilds_at_most_twice_and_loses_no_folder() {
        let dir = TempDir::new();
        let core = Arc::new(core(&dir).await);
        let (project, root) = project(&core, &dir, "app").await;
        let id = main_id(&core, &project).await;
        let watches = core
            .explorer_watch(&[(project.id.clone(), id)])
            .await
            .unwrap();
        let mut events = core.shell.watch();
        let before = reconciles(&watches[0]);
        for n in 0..200 {
            std::fs::create_dir(root.join(format!("burst{n}"))).unwrap();
        }
        tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                changed(&mut events, &project.id).await;
                if locked(&watches[0].state.resources)
                    .as_ref()
                    .unwrap()
                    .directories
                    .contains_key(&root.join("burst199"))
                {
                    break;
                }
            }
        })
        .await
        .expect("the trailing rebuild picks up the last folder");
        tokio::time::sleep(RECONCILE_GAP + RECONCILE_GAP / 2).await;
        let rebuilds = reconciles(&watches[0]) - before;
        assert!((1..=2).contains(&rebuilds), "{rebuilds} rebuilds");
    }

    #[tokio::test]
    async fn plain_file_writes_announce_without_rebuilding_the_watched_set() {
        let dir = TempDir::new();
        let core = Arc::new(core(&dir).await);
        let (project, root) = project(&core, &dir, "app").await;
        let id = main_id(&core, &project).await;
        for n in 0..200 {
            write(&root.join(format!("file{n}.txt")), "one");
        }
        let watches = core
            .explorer_watch(&[(project.id.clone(), id)])
            .await
            .unwrap();
        // Let events from setup settle before counting.
        tokio::time::sleep(RECONCILE_GAP * 2).await;
        let mut events = core.shell.watch();
        let before = reconciles(&watches[0]);
        for n in 0..200 {
            write(&root.join(format!("file{n}.txt")), "two");
        }
        changed(&mut events, &project.id).await;
        tokio::time::sleep(RECONCILE_GAP + RECONCILE_GAP / 2).await;
        assert_eq!(reconciles(&watches[0]), before);
    }

    #[cfg(not(windows))]
    #[test]
    fn a_folder_the_backend_reported_removed_is_watched_again_even_with_the_same_identity() {
        let dir = TempDir::new();
        let root = crate::util::canonicalize(dir.path()).unwrap();
        std::fs::create_dir_all(root.join("dist/inner")).unwrap();
        let guard = Guard::new(&root, &root.join("data"), None).unwrap();
        let cancel = CancellationToken::new();
        let mut resources = unwatched(&roomy());
        reconcile(&mut resources, &guard, &root, &cancel).unwrap();
        let stats = |r: &Resources| (r.stats.added, r.stats.removed, r.stats.commits);
        let before = stats(&resources);
        // Deleted and recreated within one rebuild window: the inode may be reused, so
        // only the removal event says that the backend dropped its watch.
        std::fs::remove_dir_all(root.join("dist")).unwrap();
        std::fs::create_dir_all(root.join("dist/inner")).unwrap();
        resources.rewatch.insert(root.join("dist"));
        reconcile(&mut resources, &guard, &root, &cancel).unwrap();
        let after = stats(&resources);
        assert_eq!(
            after.0 - before.0,
            2,
            "dist and dist/inner are watched again"
        );
        assert_eq!(after.1 - before.1, 2, "their old watches were replaced");
        assert!(resources.rewatch.is_empty(), "the marks are used up");
        reconcile(&mut resources, &guard, &root, &cancel).unwrap();
        assert_eq!(stats(&resources), after, "settled");
    }

    #[test]
    fn a_recursive_root_watch_is_one_handle_and_costs_nothing_when_unchanged() {
        let dir = TempDir::new();
        let root = crate::util::canonicalize(dir.path()).unwrap();
        std::fs::create_dir_all(root.join("a/b/c")).unwrap();
        let guard = Guard::new(&root, &root.join("data"), None).unwrap();
        let cancel = CancellationToken::new();
        let budget = Arc::new(Semaphore::new(4));
        let mut resources = unwatched(&budget);
        let marks = BTreeSet::new();
        scan_root(&mut resources, &guard, &root, &cancel, &marks).unwrap();
        assert_eq!(resources.directories.len(), 1);
        assert_eq!(budget.available_permits(), 3);
        assert_eq!((resources.stats.added, resources.stats.commits), (1, 1));
        scan_root(&mut resources, &guard, &root, &cancel, &marks).unwrap();
        assert_eq!((resources.stats.added, resources.stats.commits), (1, 1));
        assert!(
            scan_root(
                &mut resources,
                &guard,
                &root.join("missing"),
                &cancel,
                &marks
            )
            .is_err()
        );
    }

    #[test]
    fn events_inside_denied_folders_are_not_announced_by_the_recursive_watch() {
        let dir = TempDir::new();
        let root = crate::util::canonicalize(dir.path()).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        let guard = Guard::new(&root, &root.join("data"), Some(root.join("repo"))).unwrap();
        let at =
            |path: &str| Event::new(EventKind::Modify(ModifyKind::Any)).add_path(root.join(path));
        assert!(event_allowed(&guard, &root, &at("src/a.rs")));
        assert!(event_allowed(&guard, &root, &at(".env")));
        for denied in [".git/index", "data/token", "repo/x/y", "src/.git/HEAD"] {
            assert!(!event_allowed(&guard, &root, &at(denied)), "{denied}");
        }
        assert!(event_allowed(&guard, &root, &Event::new(EventKind::Any)));
    }

    #[tokio::test]
    async fn a_fast_recreate_of_a_folder_keeps_announcing_writes_inside_it() {
        let dir = TempDir::new();
        let core = Arc::new(core(&dir).await);
        let (project, root) = project(&core, &dir, "app").await;
        let id = main_id(&core, &project).await;
        std::fs::create_dir(root.join("dist")).unwrap();
        let watches = core
            .explorer_watch(&[(project.id.clone(), id)])
            .await
            .unwrap();
        let mut events = core.shell.watch();
        let before = reconciles(&watches[0]);
        std::fs::remove_dir_all(root.join("dist")).unwrap();
        std::fs::create_dir(root.join("dist")).unwrap();
        tokio::time::timeout(Duration::from_secs(8), async {
            while reconciles(&watches[0]) == before {
                changed(&mut events, &project.id).await;
            }
        })
        .await
        .expect("the rebuild runs");
        tokio::time::sleep(Duration::from_millis(300)).await;
        let mut events = core.shell.watch();
        write(&root.join("dist/out.js"), "built");
        changed(&mut events, &project.id).await;
    }
}
