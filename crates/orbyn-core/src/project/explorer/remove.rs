//! A person's removal of one worktree Orbyn made. Only the sibling `<repo>-wt` root is
//! treated as Orbyn's own. Git's non-forced `git worktree remove` refuses modified and
//! untracked files, but it deletes IGNORED files (`.env`, local config, build output),
//! so the removal first refuses when `git status --porcelain --ignored` lists anything.
//! A worktree with nothing to lose is removed; commits and branches are never touched.

use std::path::{Path, PathBuf};

use serde_json::json;

use super::{Core, CoreError, Result, Target, member, plain, write::finish};
use crate::source_control::git::{
    git, git_hardened, primary_checkout, worktree_records_hardened, worktree_root,
};
use crate::store::AuditRecord;

impl Core {
    /// Removes the worktree `id` of `project_id`. A linked worktree under the
    /// project's sibling `<repo>-wt` root with no thread working in it is removed;
    /// the primary checkout, a locked or foreign worktree, and one a thread still
    /// uses are refused.
    pub async fn remove_worktree(&self, project_id: &str, id: &str, actor: &str) -> Result<()> {
        let _writer = self.explorer.writer.clone().lock_owned().await;
        let audit = AuditRecord {
            actor: actor.to_owned(),
            action: "worktree.remove".into(),
            target: project_id.to_owned(),
            decision: None,
            source: "user".into(),
            thread_id: None,
            run_id: None,
            detail: json!({"worktree_id": id}),
        };
        // A durable attempt is required before any filesystem effect.
        self.store.append_audit(&audit)?;
        let prepared = async {
            let project = self.explorer_project(project_id)?;
            if !project.is_git {
                return Err(CoreError::Invalid(
                    "This folder is not a Git checkout.".into(),
                ));
            }
            if !super::well_formed(id) {
                return Err(CoreError::NotFound("worktree".into()));
            }
            // Fresh membership: a kept list, even an in-flight refresh, is not removal
            // authority.
            let path = PathBuf::from(&project.path);
            let records = super::status::bounded(
                super::status::DEADLINE,
                worktree_records_hardened(&path),
            )
            .await?;
            if crate::util::canonicalize(&path).is_ok_and(|now| now != path) {
                return Err(CoreError::NotFound("worktree".into()));
            }
            let target = member::resolve(records, &plain(&path))?
                .into_iter()
                .map(|entry| Target::new(&project, entry))
                .find(|target| target.id == id)
                .ok_or_else(|| CoreError::NotFound("worktree".into()))?;
            if target.is_main {
                return Err(CoreError::Invalid(
                    "The primary checkout is not a worktree to remove.".into(),
                ));
            }
            if target.missing || !target.member || target.record.prunable {
                return Err(CoreError::NotFound("worktree".into()));
            }
            if target.record.locked {
                return Err(CoreError::PolicyDenied(
                    "This worktree is locked. Unlock it first.".into(),
                ));
            }
            let primary = primary_checkout(Path::new(&project.path)).await?;
            let root = crate::util::canonicalize(&worktree_root(&primary)?)
                .map_err(|_| CoreError::Git("The worktree folder is gone.".into()))?;
            if target.path == root || !target.path.starts_with(&root) {
                return Err(CoreError::PolicyDenied(
                    "Only a worktree Orbyn made can be removed here.".into(),
                ));
            }
            if thread_in(&self.store, &target.path)?.is_some() {
                return Err(CoreError::PolicyDenied(
                    "A thread still uses this worktree. Delete the thread first.".into(),
                ));
            }
            Ok((primary, target))
        }
        .await;
        let (primary, target) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => return finish(&self.store, audit, Err(error)),
        };
        let (store, workspace, explorer) = (
            self.store.clone(),
            self.workspace.clone(),
            self.explorer.clone(),
        );
        let project_id = project_id.to_owned();
        let runtime = tokio::runtime::Handle::current();
        // Move the serialization permit into the blocking task: dropping an HTTP future
        // cannot let a second action race a still-running filesystem call.
        let permit = _writer;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            // Recheck and effect run inside the admission fence, so a thread created
            // meanwhile is seen, and no run starts between the check and the removal.
            let result = workspace.admit_with(|| {
                if thread_in(&store, &target.path)?.is_some() {
                    return Err(CoreError::PolicyDenied(
                        "A thread still uses this worktree. Delete the thread first.".into(),
                    ));
                }
                runtime.block_on(async {
                    // Git's own removal deletes ignored files without asking.
                    let status = git_hardened(
                        &target.path,
                        &[
                            "status",
                            "--porcelain",
                            "--ignored",
                            "--untracked-files=all",
                            "-z",
                        ],
                    )
                    .await
                    .map_err(|_| {
                        CoreError::Git("The worktree could not be checked for local files.".into())
                    })?;
                    if !status.is_empty() {
                        return Err(CoreError::PolicyDenied(
                            "This worktree has local files that would be lost. Remove or commit them first."
                                .into(),
                        ));
                    }
                    let path = target.path.to_string_lossy().into_owned();
                    git(&primary, &["worktree", "remove", &path]).await.map(|_| ())
                })
            });
            if result.is_ok() {
                explorer.changed(&project_id);
            }
            finish(&store, audit, result)
        })
        .await
        .map_err(|_| CoreError::Internal("The removal ended unexpectedly.".into()))?
    }
}

/// The first thread whose folder is inside `path`, if any. Archived threads count
/// too: a thread row still names its checkout, and an archived thread can come back.
fn thread_in(store: &crate::store::Store, path: &Path) -> Result<Option<orbyn_contracts::Thread>> {
    for project in store.list_projects()? {
        for thread in store.list_all_threads(&project.id)? {
            let cwd = plain(Path::new(&thread.cwd));
            let real = crate::util::canonicalize(&cwd).unwrap_or_else(|_| cwd.clone());
            if [&cwd, &real].iter().any(|cwd| cwd.starts_with(path)) {
                return Ok(Some(thread));
            }
        }
    }
    Ok(None)
}
