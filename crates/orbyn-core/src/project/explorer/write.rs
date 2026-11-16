//! User file actions. Resolve fresh membership, then fence admission and effects.

pub(super) mod platform;

use std::path::{Path, PathBuf};

use orbyn_contracts::{ExplorerFileAction, PolicyEffect};
use serde_json::json;

use super::{Core, CoreError, Result, Target, guard::Guard, member, plain};
use crate::source_control::git::worktree_records_hardened;
use crate::store::AuditRecord;

impl Core {
    /// A person's action, scoped by project and opaque worktree id, never a thread.
    /// All active runs in the target block writes; no caller can claim their identity.
    pub async fn explorer_file_action(
        &self,
        project_id: &str,
        id: &str,
        action: &ExplorerFileAction,
        actor: &str,
    ) -> Result<()> {
        // The route is behind the Preview flag until #441 is fixed; the core fails closed too, so
        // a second caller cannot reach it with the flag off.
        if !self.config.crew_preview {
            return Err(CoreError::NotFound("file actions".into()));
        }
        let _writer = self.explorer.writer.clone().lock_owned().await;
        let audit = AuditRecord {
            actor: actor.to_owned(),
            action: "explorer.file_action".into(),
            target: project_id.to_owned(),
            decision: None,
            source: "user".into(),
            thread_id: None,
            run_id: None,
            detail: json!({"worktree_id": id, "action": kind(action)}),
        };
        // A durable attempt is required before any filesystem effect. No contents or
        // arbitrary client paths are copied to the audit record.
        self.store.append_audit(&audit)?;
        let prepared = async {
            let project = self.explorer_project(project_id)?;
            if !super::well_formed(id) {
                return Err(CoreError::NotFound("worktree".into()));
            }
            let entries = if project.is_git {
                // A SWR answer, even an in-flight refresh, is not write authority.
                let path = PathBuf::from(&project.path);
                let records = super::status::bounded(
                    super::status::DEADLINE,
                    worktree_records_hardened(&path),
                )
                .await?;
                if crate::util::canonicalize(&path).is_ok_and(|now| now != path) {
                    return Err(CoreError::NotFound("worktree".into()));
                }
                member::resolve(records, &plain(&path))?
            } else {
                self.explorer_members(&project, true).await?
            };
            let target = entries
                .into_iter()
                .map(|entry| Target::new(&project, entry))
                .find(|target| target.id == id)
                .ok_or_else(|| CoreError::NotFound("worktree".into()))?;
            if target.reason().is_some() || !target.member {
                return Err(CoreError::PolicyDenied(
                    "This worktree is read-only.".into(),
                ));
            }
            Ok(target)
        }
        .await;
        let target = match prepared {
            Ok(prepared) => prepared,
            Err(error) => return finish(&self.store, audit, Err(error)),
        };
        let (store, workspace) = (self.store.clone(), self.workspace.clone());
        let explorer = self.explorer.clone();
        let data_dir = self.config.data_dir.clone();
        let terminals = self.terminals.clone();
        let (project_id, action) = (project_id.to_owned(), action.clone());
        // Move the serialization permit into the blocking task: dropping an HTTP
        // future cannot let a second action race a still-running filesystem call.
        let permit = _writer;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let result = workspace.admit_with(|| {
                if !store.project(&project_id)?.trusted {
                    return Err(CoreError::FolderUntrusted);
                }
                let guard = Guard::with_library(&target.path, &data_dir, workspace.root().ok())?;
                // Inspect all live threads, including other trusted project
                // registrations over this checkout. Admission uses this same fence.
                // A thread counts when a run is going or its engine console is open: a
                // console runs an agent without a run record. A person's own terminal does
                // not. Folders are compared by the name the project was registered under
                // and by the real one, so another spelling cannot slip past the fence.
                let roots = [target.path.clone(), guard.root().to_path_buf()];
                if store.list_threads(None, false)?.iter().any(|thread| {
                    let cwd = plain(Path::new(&thread.cwd));
                    let real = crate::util::canonicalize(&cwd).unwrap_or_else(|_| cwd.clone());
                    let inside = roots.iter().any(|root| {
                        [&cwd, &real]
                            .iter()
                            .any(|cwd| cwd.starts_with(root) || root.starts_with(cwd))
                    });
                    inside
                        && (thread.active_run.is_some()
                            || crate::agents::console::console_alive(&terminals, &thread.id))
                }) {
                    return Err(CoreError::PolicyDenied(
                        "This worktree has an active session.".into(),
                    ));
                }
                apply(&guard, &action)
            });
            if result.is_ok() {
                explorer.changed(&project_id);
            }
            finish(&store, audit, result)
        })
        .await
        .map_err(|_| CoreError::Internal("The file action ended unexpectedly.".into()))?
    }
}

fn finish(store: &crate::store::Store, audit: AuditRecord, result: Result<()>) -> Result<()> {
    let mut detail = audit.detail.clone();
    detail["error"] = json!(result.as_ref().err().map(CoreError::code));
    store.append_audit(&AuditRecord {
        action: "explorer.file_action_result".into(),
        decision: Some(if result.is_ok() {
            PolicyEffect::Allow
        } else {
            PolicyEffect::Deny
        }),
        detail,
        ..audit
    })?;
    result
}

fn kind(action: &ExplorerFileAction) -> &'static str {
    match action {
        ExplorerFileAction::CreateFile { .. } => "create_file",
        ExplorerFileAction::CreateDirectory { .. } => "create_directory",
        ExplorerFileAction::Move { .. } => "move",
        ExplorerFileAction::Delete { .. } => "delete",
    }
}

fn invalid(message: &str) -> CoreError {
    CoreError::Invalid(message.into())
}

/// Every component is opened without following links. Windows holds ancestors
/// without delete sharing; Unix effects use the checked parent descriptor.
struct Location {
    path: PathBuf,
    parents: Vec<(PathBuf, std::fs::File)>,
}

impl Location {
    fn new(guard: &Guard, raw: &str) -> Result<Self> {
        let resolved = guard.mutation_path(raw)?;
        let mut current = guard.root().to_path_buf();
        let mut parents = vec![(current.clone(), guard.write_dir(&current)?)];
        let parts: Vec<_> = resolved.rel.split('/').collect();
        for part in &parts[..parts.len() - 1] {
            current.push(part);
            no_link(&current)?;
            parents.push((current.clone(), guard.write_dir(&current)?));
        }
        let path = current.join(parts[parts.len() - 1]);
        match std::fs::symlink_metadata(&path) {
            Ok(_) => {
                no_link(&path)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let location = Self { path, parents };
        location.confirm(guard)?;
        Ok(location)
    }

    #[cfg(unix)]
    fn parent(&self) -> &std::fs::File {
        &self.parents.last().expect("root parent is always held").1
    }

    fn confirm(&self, guard: &Guard) -> Result<()> {
        for (path, handle) in &self.parents {
            guard.confirm_write_dir(handle, path)?;
        }
        guard.mutation_path(
            &self
                .path
                .strip_prefix(guard.root())
                .map_err(|_| invalid("That path left the worktree."))?
                .to_string_lossy()
                .replace('\\', "/"),
        )?;
        Ok(())
    }
}

fn no_link(path: &Path) -> Result<std::fs::Metadata> {
    let meta = std::fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            CoreError::NotFound("file or folder".into())
        } else {
            error.into()
        }
    })?;
    #[cfg(windows)]
    let link = {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let link = meta.is_symlink();
    if link || !(meta.is_file() || meta.is_dir()) {
        return Err(invalid("File actions refuse links and special files."));
    }
    Ok(meta)
}

/// Moving a folder must not carry protected entries or links with it. Bound the
/// inspection, refuse oversized trees, and never recursively delete anything.
fn movable_tree(guard: &Guard, root: &Path) -> Result<()> {
    let mut pending = vec![root.to_path_buf()];
    let mut seen = 0;
    while let Some(path) = pending.pop() {
        seen += 1;
        if seen > 20_000 {
            return Err(invalid("This folder is too large to move here."));
        }
        let relative = path
            .strip_prefix(guard.root())
            .map_err(|_| invalid("Invalid folder."))?;
        guard.mutation_path(&relative.to_string_lossy().replace('\\', "/"))?;
        if no_link(&path)?.is_dir() {
            let held = guard.write_dir(&path)?;
            for entry in std::fs::read_dir(&path)? {
                pending.push(entry?.path());
                if pending.len() > 20_000 {
                    return Err(invalid("This folder is too large to move here."));
                }
            }
            guard.confirm_write_dir(&held, &path)?;
        }
    }
    Ok(())
}

fn apply(guard: &Guard, action: &ExplorerFileAction) -> Result<()> {
    let raw = match action {
        ExplorerFileAction::CreateFile { path }
        | ExplorerFileAction::CreateDirectory { path }
        | ExplorerFileAction::Move { path, .. }
        | ExplorerFileAction::Delete { path } => path,
    };
    let source = Location::new(guard, raw)?;
    match action {
        ExplorerFileAction::CreateFile { .. } => platform::create_file(&source),
        ExplorerFileAction::CreateDirectory { .. } => platform::create_directory(&source),
        ExplorerFileAction::Move { destination, .. } => {
            no_link(&source.path)?;
            let dest = Location::new(guard, destination)?;
            if dest.path.starts_with(&source.path) {
                return Err(invalid("A folder cannot move inside itself."));
            }
            // Where names fold case, `Dir` into `dir/x` is the same folder under another
            // spelling; the rename refuses it and the answer must say why.
            let inside_by_case = dest
                .path
                .to_string_lossy()
                .to_lowercase()
                .starts_with(&source.path.to_string_lossy().to_lowercase());
            movable_tree(guard, &source.path)?;
            source.confirm(guard)?;
            dest.confirm(guard)?;
            platform::move_entry(&source, &dest).map_err(|error| {
                if inside_by_case && error.kind() == std::io::ErrorKind::Unsupported {
                    std::io::Error::from(std::io::ErrorKind::InvalidInput)
                } else {
                    error
                }
            })
        }
        ExplorerFileAction::Delete { .. } => {
            let directory = no_link(&source.path)?.is_dir();
            source.confirm(guard)?;
            platform::delete(&source, directory)
        }
    }
    .map_err(|error| match error.kind() {
        std::io::ErrorKind::AlreadyExists => invalid("The destination already exists."),
        std::io::ErrorKind::NotFound => CoreError::NotFound("file or folder".into()),
        std::io::ErrorKind::DirectoryNotEmpty => invalid("Only empty folders can be deleted."),
        std::io::ErrorKind::InvalidInput => invalid("A folder cannot move inside itself."),
        std::io::ErrorKind::Unsupported => {
            invalid("This folder's file system cannot move files safely.")
        }
        _ => error.into(),
    })
}
