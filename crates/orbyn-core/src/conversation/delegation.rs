//! Delegation: handing work to a child thread in the parent's folder.

use orbyn_contracts::{DelegateTask, ModelSelection, Thread};

use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::store::NewThread;

impl Core {
    /// Hands work to a child thread in the same folder. It reports back through the
    /// parent's queue when it ends.
    pub fn delegate(&self, parent: &str, task: &DelegateTask, actor: &str) -> Result<Thread> {
        self.delegate_then(parent, task, actor, |_| Ok(()))
    }

    /// `delegate`, with `before_start` run on the new thread before its first message is
    /// queued, so a caller can record what it is for before the engine can answer.
    pub(crate) fn delegate_then(
        &self,
        parent: &str,
        task: &DelegateTask,
        actor: &str,
        before_start: impl FnOnce(&Thread) -> Result<()>,
    ) -> Result<Thread> {
        let owner = self.store.thread(parent)?;
        if !self.store.project(&owner.project_id)?.trusted {
            return Err(CoreError::FolderUntrusted);
        }
        let engine = task.engine.clone().unwrap_or_else(|| owner.engine.clone());
        self.engines.driver(&engine)?;
        self.audit_task(actor, "thread.delegate", parent)?;
        // A selection means nothing to another engine, so only the same one inherits.
        let same_engine = engine == owner.engine;
        let inherited = |own: &Option<String>| own.clone().filter(|_| same_engine);
        let child = self.store.create_thread(NewThread {
            project_id: owner.project_id.clone(),
            title: task
                .title
                .clone()
                .unwrap_or_else(|| "Delegated task".to_owned()),
            engine,
            model: task.model.clone().or(inherited(&owner.model)),
            cwd: owner.cwd.clone(),
            worktree: owner.worktree.clone(),
            skill_profile: owner.skill_profile.clone(),
            parent_thread_id: Some(owner.id.clone()),
            relation: Some("subagent".to_owned()),
            scratch: owner.scratch,
        })?;
        let mut options = if same_engine {
            owner.options.clone()
        } else {
            std::collections::BTreeMap::new()
        };
        options.extend(task.options.clone());
        let wanted = ModelSelection {
            model: None,
            effort: task.effort.clone().or(inherited(&owner.effort)),
            options,
        };
        before_start(&child)?;
        self.runs
            .enqueue_with(&child.id, &task.prompt, &[], &wanted)?;
        self.store.thread(&child.id)
    }
}
