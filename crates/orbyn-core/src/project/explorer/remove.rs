//! A person's removal of one worktree Orbyn made. Only the sibling `<repo>-wt` root is
//! treated as Orbyn's own, and Git itself keeps uncommitted work: the removal runs
//! `git worktree remove` with no force, so a dirty worktree is refused by Git.

use std::path::Path;

use serde_json::json;

use super::{Core, CoreError, Result, Target, plain, write::finish};
use crate::source_control::git::{git, primary_checkout, worktree_root};
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
            // Fresh membership: a kept list, even a refreshing one, is not removal
            // authority.
            let target = self
                .explorer_members(&project, true)
                .await?
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
            if self.thread_in(&target.path)?.is_some() {
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
        // Recheck just before the effect: a thread can have been created meanwhile.
        if self.thread_in(&target.path)?.is_some() {
            return finish(
                &self.store,
                audit,
                Err(CoreError::PolicyDenied(
                    "A thread still uses this worktree. Delete the thread first.".into(),
                )),
            );
        }
        let path = target.path.to_string_lossy().into_owned();
        let outcome = git(&primary, &["worktree", "remove", &path]).await;
        if outcome.is_ok() {
            self.explorer.changed(project_id);
        }
        finish(&self.store, audit, outcome.map(|_| ()))
    }

    /// The first thread whose folder is inside `path`, if any. Archived threads count
    /// too: a thread row still names its checkout, and an archived thread can come back.
    fn thread_in(&self, path: &Path) -> Result<Option<orbyn_contracts::Thread>> {
        for project in self.store.list_projects()? {
            for thread in self.store.list_all_threads(&project.id)? {
                let cwd = plain(Path::new(&thread.cwd));
                let real = crate::util::canonicalize(&cwd).unwrap_or_else(|_| cwd.clone());
                if [&cwd, &real].iter().any(|cwd| cwd.starts_with(path)) {
                    return Ok(Some(thread));
                }
            }
        }
        Ok(None)
    }
}
