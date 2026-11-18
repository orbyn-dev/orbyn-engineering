//! Native watches shared by focused windows. A lease owns every OS handle and task.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc};
use tokio_util::sync::CancellationToken;

use super::{Core, CoreError, Guard, Result, locked};

const MAX_WATCHES: usize = 64;
const MAX_DIRECTORIES: usize = 512;
const MAX_ENTRIES: usize = 20_000;

pub(super) struct Watches {
    held: Mutex<HashMap<String, Weak<ExplorerWatch>>>,
    slots: Arc<Semaphore>,
}

impl Default for Watches {
    fn default() -> Self {
        Self {
            held: Mutex::default(),
            slots: Arc::new(Semaphore::new(MAX_WATCHES)),
        }
    }
}

struct Resources {
    native: RecommendedWatcher,
    directories: BTreeSet<PathBuf>,
    _slot: OwnedSemaphorePermit,
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
    fn revoke(&self) {
        locked(&self.resources).take();
        self.stopped.cancel();
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
    CoreError::Unsupported("Explorer file watching is unavailable. Refresh to read changes.".into())
}

/// No recursive native registration: that could follow a link or traverse denied data.
/// Watch each allowed directory before listing it, closing the scan/register gap.
fn reconcile(resources: &mut Resources, guard: &Guard, root: &Path) -> Result<()> {
    // Release the previous set first: retaining old paths during a rebuild could
    // briefly double the handle bound after a large rename.
    for old in std::mem::take(&mut resources.directories) {
        let _ = resources.native.unwatch(&old);
    }
    let mut queue = VecDeque::from([root.to_owned()]);
    let mut entries = 0;
    while let Some(dir) = queue.pop_front() {
        if resources.directories.len() == MAX_DIRECTORIES {
            break;
        }
        let handle = guard.open_dir(&dir)?;
        resources
            .native
            .watch(&dir, RecursiveMode::NonRecursive)
            .map_err(|_| unavailable())?;
        resources.directories.insert(dir.clone());
        for entry in std::fs::read_dir(&dir)? {
            if entries == MAX_ENTRIES {
                break;
            }
            entries += 1;
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if guard.lists(&dir, name)
                && entry.file_type()?.is_dir()
                && !entry.file_type()?.is_symlink()
            {
                // Resolve before opening: Windows junctions can be directories too.
                let path = entry.path();
                if crate::util::canonicalize(&path).is_ok_and(|real| real == path)
                    && queue.len() + resources.directories.len() < MAX_DIRECTORIES
                {
                    queue.push_back(path);
                }
            }
        }
        guard.confirm_dir(&handle, &dir)?;
    }
    Ok(())
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
            let failed = Arc::new(AtomicBool::new(false));
            let callback_failed = failed.clone();
            let native = RecommendedWatcher::new(
                move |event: notify::Result<Event>| {
                    if event.is_err() {
                        callback_failed.store(true, Ordering::Release);
                    }
                    // Reading a folder must not wake its own watcher forever.
                    if !matches!(&event, Ok(event) if matches!(event.kind, EventKind::Access(_))) {
                        let _ = send.try_send(event.is_ok());
                    }
                },
                Config::default().with_follow_symlinks(false),
            )
            .map_err(|_| unavailable())?;
            let mut resources = Resources {
                native,
                directories: BTreeSet::new(),
                _slot: slot,
            };
            let root = target.path;
            let resources = tokio::task::spawn_blocking(move || {
                reconcile(&mut resources, &guard, &root)?;
                Ok::<_, CoreError>(resources)
            })
            .await
            .map_err(|_| unavailable())??;
            let watch = Arc::new(ExplorerWatch {
                state: Arc::new(WatchState {
                    resources: Mutex::new(Some(resources)),
                    stopped: CancellationToken::new(),
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
                failed,
            ));
            *locked(&watch.worker) = Some(worker.abort_handle());
            watches.push(watch);
        }
        // Recheck after registration and before handing leases to transport.
        for (project, _) in pairs {
            self.explorer_project(project)?;
            self.explorer.changed(project);
        }
        Ok(watches)
    }
}

async fn follow(
    core: Weak<Core>,
    lease: Weak<WatchState>,
    project: String,
    worktree: String,
    mut events: mpsc::Receiver<bool>,
    mut projects: tokio::sync::watch::Receiver<u64>,
    failed: Arc<AtomicBool>,
) {
    loop {
        let native = tokio::select! {
            event = events.recv() => match event { Some(ok) => Some(ok), None => return },
            change = projects.changed() => { if change.is_err() { return; } None },
        };
        if native.is_some() {
            tokio::time::sleep(super::ANNOUNCE_GAP).await;
            while events.try_recv().is_ok() {}
        }
        let (Some(core), Some(lease)) = (core.upgrade(), lease.upgrade()) else {
            return;
        };
        if native == Some(false) || failed.load(Ordering::Acquire) {
            core.explorer.changed(&project);
            lease.revoke();
            return;
        }
        // Never serve a cached membership after the event that could have changed it.
        core.explorer.lists.invalidate(&super::key(&[&project]));
        let checked = async {
            let target = core.explorer_target(&project, &worktree).await?;
            let guard = core.explorer_guard(&target)?;
            let lease = lease.clone();
            tokio::task::spawn_blocking(move || {
                let mut held = locked(&lease.resources);
                let resources = held.as_mut().ok_or_else(unavailable)?;
                reconcile(resources, &guard, &target.path)
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
        core.explorer.changed(&project);
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{core, entry, main_id, project, write};
    use super::*;
    use crate::testing::TempDir;
    use orbyn_contracts::{ExplorerChange, ExplorerReadOnlyReason, ShellEvent};
    use std::time::Duration;

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
        drop(first);
        assert!(locked(&second[0].state.resources).is_some());
        drop(second);
        assert_eq!(core.explorer.watches.slots.available_permits(), MAX_WATCHES);
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
        assert!(locked(&watches[0].state.resources).is_none());
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
        assert!(locked(&watches[0].state.resources).is_none());
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
                    .contains(&root.join("new folder"))
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
        assert!(
            core.explorer_watch(&[(project.id.clone(), id.clone())])
                .await
                .is_err()
        );
        drop(reserved);
        let watches = core.explorer_watch(&[(project.id, id)]).await.unwrap();
        assert_eq!(
            core.explorer.watches.slots.available_permits(),
            MAX_WATCHES - 1
        );
        drop(watches);
        assert_eq!(core.explorer.watches.slots.available_permits(), MAX_WATCHES);
    }

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
        let native = RecommendedWatcher::new(
            |_: notify::Result<Event>| {},
            Config::default().with_follow_symlinks(false),
        )
        .unwrap();
        let mut resources = Resources {
            native,
            directories: BTreeSet::new(),
            _slot: Arc::new(Semaphore::new(1)).try_acquire_owned().unwrap(),
        };
        reconcile(&mut resources, &guard, &root).unwrap();
        assert_eq!(resources.directories.len(), MAX_DIRECTORIES);
        for denied in ["data", "repo", ".git", "linked"] {
            assert!(
                !resources
                    .directories
                    .iter()
                    .any(|path| path.starts_with(root.join(denied)))
            );
        }
        assert!(guard.read(".env", 100).is_ok());
        let denied = Guard::new(&root.join("data"), &root.join("data"), None).unwrap();
        assert!(reconcile(&mut resources, &denied, &root.join("data")).is_err());
    }
}
