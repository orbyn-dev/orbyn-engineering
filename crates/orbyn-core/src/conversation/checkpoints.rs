//! Diffs over checkpoints, and moving a thread back: restoring files, rewinding
//! the conversation, forking it and switching engines. An engine session cannot
//! follow a fork or a switch, so each leaves a budgeted handoff for the next run; a
//! rewind keeps the session when the engine can roll it back natively.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use orbyn_contracts::{
    CommitInfo, FileDiff, ForkThread, RestorePoint, RestoreWorkspace, RewindMode, RewindResult,
    RewindThread, SwitchEngine, Thread,
};
use orbyn_contracts::{ModelSelection, RunChanges};
use serde_json::{Value, json};

use crate::agents::driver::EngineDriver;
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::source_control::git;
use crate::store::AuditRecord;
use crate::util::new_id;

/// The ref one folder's checkpoint is kept under. Two repositories of one thread can share a Git common directory:
/// two linked worktrees of one repository are enrolled as two repositories, and one ref would be written twice, so
/// the folder's repository is part of the name and no recorded checkpoint is left unreachable
/// (docs/architecture.md).
pub fn checkpoint_ref(thread_id: &str, run_id: &str, repo_id: Option<&str>, point: &str) -> String {
    let folder = repo_id.map_or_else(
        || "checkout".to_owned(),
        |repo_id| format!("repo-{repo_id}"),
    );
    format!("refs/orbyn/checkpoints/{thread_id}/{run_id}/{folder}/{point}")
}

/// The native rollback a rewind applied, and what it needs to drop the point again if a
/// later step fails before the rewrite is committed.
struct NativeRevert {
    driver: Arc<dyn EngineDriver>,
    session: String,
}

/// A refusal with the status and code a client already knows, and plain words for a person.
fn refused(status: u16, code: &'static str, message: &str) -> CoreError {
    CoreError::Refused {
        status,
        code,
        message: message.to_owned(),
        detail: Value::Null,
    }
}

fn rewind_busy() -> CoreError {
    refused(
        409,
        "thread_busy",
        "This thread has an active run. Rewind it once the run finishes.",
    )
}

fn restore_failed_before_changes() -> CoreError {
    CoreError::Git("Your files were not changed. The thread was not rewound.".into())
}

const MAX_CHANGED_PATHS: usize = 200;

/// One folder a restore makes match: the ends it has to match, and the safety snapshot of what stands there now, so
/// every change it overwrites can be brought back.
struct Restoring {
    repo_id: Option<String>,
    worktree: PathBuf,
    target: String,
    safety: String,
}

/// What a restore's audit row says it did. A single folder keeps the shape it always had; a multi-repository thread
/// names each repository, because one row now moves several worktrees.
fn restore_detail(restores: &[Restoring]) -> Value {
    if let [one] = restores
        && one.repo_id.is_none()
    {
        return json!({ "target": one.target, "safety": one.safety });
    }
    json!({
        "repositories": restores
            .iter()
            .map(|one| json!({
                "repo_id": one.repo_id,
                "target": one.target,
                "safety": one.safety,
            }))
            .collect::<Vec<_>>(),
    })
}

/// Paths a raw engine item says were written: Codex's `fileChange` and Claude's own
/// file tools.
fn reported_paths(kind: &str, raw: &Value) -> Vec<String> {
    let text = |value: Option<&Value>| value.and_then(Value::as_str).map(str::to_owned);
    match kind {
        "fileChange" => raw
            .get("changes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|change| text(change.get("path")))
            .collect(),
        "tool_use" => claude_write_paths(raw),
        _ => Vec::new(),
    }
}

/// The file a Claude tool block says it writes.
pub(crate) fn claude_write_paths(block: &Value) -> Vec<String> {
    let pointer = match block.get("name").and_then(Value::as_str) {
        Some("Write" | "Edit" | "MultiEdit") => "/input/file_path",
        Some("NotebookEdit") => "/input/notebook_path",
        _ => return Vec::new(),
    };
    block
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .into_iter()
        .collect()
}

impl Core {
    /// The thread a checkpoint read diffs the checkout of: the repository asked for, else the primary, with
    /// its worktree as the folder. A multi-repository task folder is never handed to Git.
    fn trusted_thread(&self, id: &str) -> Result<Thread> {
        let thread = self.scoped_thread(id)?;
        if !self.store.project(&thread.project_id)?.trusted {
            return Err(CoreError::FolderUntrusted);
        }
        Ok(thread)
    }

    /// The thread a write to its own files acts in. A multi-repository thread's engine works in the task folder,
    /// so a file restore stays relative to it; only the reads above resolve one repository.
    pub(crate) fn idle_thread(&self, id: &str) -> Result<Thread> {
        let thread = self.store.thread(id)?;
        if !self.store.project(&thread.project_id)?.trusted {
            return Err(CoreError::FolderUntrusted);
        }
        if thread.active_run.is_some() {
            return Err(CoreError::ThreadBusy);
        }
        Ok(thread)
    }

    fn scratch_index(&self) -> Result<std::path::PathBuf> {
        let dir = self.config.data_dir.join("runs");
        std::fs::create_dir_all(&dir)?;
        Ok(dir.join(format!("index-{}", new_id())))
    }

    /// The folders a restore moves, each with its repository: every trusted repository's worktree of a
    /// multi-repository thread, or the engine's own folder for an ordinary one. A task folder is never one, so a
    /// restore used to fail there; an ordinary thread's restore stays in its own folder as it always did.
    fn restore_roots(&self, thread: &Thread) -> Result<Vec<(Option<String>, PathBuf)>> {
        let worktrees = crate::repos::trusted_worktrees(&self.store, thread)?;
        Ok(if worktrees.is_empty() {
            vec![(None, PathBuf::from(&thread.cwd))]
        } else {
            worktrees
                .into_iter()
                .map(|(repo_id, worktree)| (Some(repo_id), worktree))
                .collect()
        })
    }

    /// Refuses a restore while another live thread works in a folder it would move, since that thread's work would
    /// be overwritten without its knowledge. Every restore root is checked, and so is the thread's own folder: a
    /// multi-repository thread's folder is a task folder another thread can be working in, and only the roots are
    /// ever written (docs/multi-repo.md).
    fn refuse_shared_roots(
        &self,
        thread: &Thread,
        roots: &[(Option<String>, PathBuf)],
    ) -> Result<()> {
        let own = PathBuf::from(&thread.cwd);
        for path in std::iter::once(&own).chain(roots.iter().map(|(_, worktree)| worktree)) {
            let shared = self.store.threads_sharing_path(path, &thread.id)?;
            if !shared.is_empty() {
                return Err(CoreError::SharedDirectory(shared.len()));
            }
        }
        Ok(())
    }

    /// The repository a checkpoint read of this thread is in: the one its stored `cwd` names, because a read
    /// resolves the thread to a worktree and a task folder is not a checkout. `None` for an ordinary thread, whose
    /// checkpoint is its own folder (docs/multi-repo.md).
    fn checkpoint_repo(&self, thread: &Thread) -> Result<Option<String>> {
        let repos = self.store.thread_repos(&thread.id)?;
        let cwd = Path::new(&thread.cwd);
        Ok(repos
            .iter()
            .find(|repo| Path::new(&repo.worktree_path) == cwd)
            .or_else(|| repos.first())
            .map(|repo| repo.repo_id.clone()))
    }

    fn thread_checkpoint(
        &self,
        thread_id: &str,
        run_id: &str,
        repo: Option<&str>,
    ) -> Result<orbyn_contracts::Checkpoint> {
        let found = match repo {
            // A repository's ends are commits in its own Git objects, so its row is the one to read.
            Some(repo_id) => self
                .store
                .repository_checkpoint(thread_id, run_id, repo_id)?
                .map(|ends| orbyn_contracts::Checkpoint {
                    run_id: run_id.to_owned(),
                    start: ends.start,
                    end: ends.end,
                    created_at: ends.created_at,
                }),
            None => self
                .store
                .checkpoints(thread_id)?
                .into_iter()
                .find(|checkpoint| checkpoint.run_id == run_id),
        };
        found.ok_or_else(|| CoreError::NotFound("checkpoint".into()))
    }

    pub async fn turn_diff(&self, thread_id: &str, run_id: &str) -> Result<Vec<FileDiff>> {
        let thread = self.trusted_thread(thread_id)?;
        let repo = self.checkpoint_repo(&thread)?;
        let checkpoint = self.thread_checkpoint(thread_id, run_id, repo.as_deref())?;
        let end = checkpoint
            .end
            .ok_or_else(|| CoreError::Invalid("This turn is still running.".into()))?;
        git::diff_between(Path::new(&thread.cwd), &checkpoint.start, &end).await
    }

    /// Everything the thread has changed, up to the files as they are now.
    pub async fn full_thread_diff(&self, thread_id: &str) -> Result<Vec<FileDiff>> {
        let thread = self.trusted_thread(thread_id)?;
        let repo = self.checkpoint_repo(&thread)?;
        let first = match repo.as_deref() {
            Some(repo_id) => self
                .store
                .first_repository_checkpoint(thread_id, repo_id)?
                .map(|ends| ends.start),
            None => self
                .store
                .checkpoints(thread_id)?
                .into_iter()
                .next()
                .map(|checkpoint| checkpoint.start),
        };
        let Some(start) = first else {
            return Ok(Vec::new());
        };
        let cwd = Path::new(&thread.cwd);
        let now = git::snapshot(cwd, None, &self.scratch_index()?).await?;
        git::diff_between(cwd, &start, &now).await
    }

    pub async fn branch_diff(&self, thread_id: &str, base: &str) -> Result<Vec<FileDiff>> {
        let thread = self.trusted_thread(thread_id)?;
        git::branch_diff(Path::new(&thread.cwd), base).await
    }

    pub async fn commit_diff(&self, thread_id: &str, sha: &str) -> Result<Vec<FileDiff>> {
        let thread = self.trusted_thread(thread_id)?;
        git::commit_diff(Path::new(&thread.cwd), sha).await
    }

    pub async fn thread_commits(&self, thread_id: &str) -> Result<Vec<CommitInfo>> {
        let thread = self.trusted_thread(thread_id)?;
        git::recent_commits(Path::new(&thread.cwd), 100).await
    }

    /// Whether a folder still holds exactly what its safety snapshot recorded, taken again just before it is written.
    async fn folder_matches_its_safety(&self, one: &Restoring) -> Result<bool> {
        let index = self.scratch_index()?;
        let now = git::snapshot(&one.worktree, None, &index).await?;
        git::same_tree(&one.worktree, &one.safety, &now).await
    }

    /// Writes each folder's restore, one folder at a time, and refuses one whose files moved since its safety
    /// snapshot: that change is in no snapshot, so writing over it would lose it. The position of the folder that
    /// could not be written comes with the error, so a caller can record which folders did move.
    async fn apply_restores(
        &self,
        restores: &[Restoring],
    ) -> std::result::Result<(), (usize, CoreError)> {
        for (position, one) in restores.iter().enumerate() {
            let unchanged = self
                .folder_matches_its_safety(one)
                .await
                .map_err(|error| (position, error))?;
            if !unchanged {
                return Err((
                    position,
                    CoreError::Git(
                        "Your files changed while the restore was preparing, so this folder was left as it is."
                            .into(),
                    ),
                ));
            }
            git::restore_worktree(&one.worktree, &one.safety, &one.target)
                .await
                .map_err(|error| (position, error))?;
        }
        Ok(())
    }

    /// Words for the folder a restore could not move, without its path: a repository's name, or the thread's own
    /// folder for an ordinary thread.
    fn restore_root_name(&self, thread_id: &str, repo_id: Option<&str>) -> String {
        let Some(repo_id) = repo_id else {
            return "the thread's own folder".to_owned();
        };
        let named = self.store.thread_repos(thread_id).ok().and_then(|repos| {
            repos
                .into_iter()
                .find(|repo| repo.repo_id == repo_id)
                .map(|repo| repo.name)
        });
        match named {
            Some(name) => format!("the repository \"{name}\""),
            None => "a repository".to_owned(),
        }
    }

    /// The error a restore that stopped part-way returns: it names the folder that could not be written and the ones
    /// that already moved, so a client showing only the error cannot read it as "nothing changed" (docs/multi-repo.md).
    /// A failure before any folder moved keeps its own words, so the two cases stay apart.
    fn restore_stopped(
        &self,
        thread_id: &str,
        moved: &[Restoring],
        failed: &Restoring,
        error: &CoreError,
        rewound: bool,
    ) -> CoreError {
        let mut sentence = if moved.is_empty() {
            "Your files were not changed.".to_owned()
        } else {
            "Your files may have changed.".to_owned()
        };
        if rewound {
            sentence.push_str(" The thread was not rewound.");
        }
        sentence.push_str(&format!(
            " {} could not be restored",
            self.restore_root_name(thread_id, failed.repo_id.as_deref())
        ));
        if !moved.is_empty() {
            let names: Vec<String> = moved
                .iter()
                .map(|one| self.restore_root_name(thread_id, one.repo_id.as_deref()))
                .collect();
            sentence.push_str(&format!(", and {} already moved back", names.join(", ")));
        }
        sentence.push_str(&format!(" ({error})."));
        CoreError::Git(sentence)
    }

    /// Records a restore that stopped part-way. The success row keeps its shape; this one lists the folders that did
    /// move and names the one that could not, so a partial restore is never read as a whole one (docs/multi-repo.md).
    fn audit_partial_restore(
        &self,
        actor: &str,
        thread_id: &str,
        moved: &[Restoring],
        failed: &Restoring,
    ) -> Result<()> {
        let mut detail = restore_detail(moved);
        if let Some(fields) = detail.as_object_mut() {
            fields.insert(
                "failed".to_owned(),
                json!({
                    "repo_id": failed.repo_id,
                    "target": failed.target,
                    "safety": failed.safety,
                }),
            );
        }
        self.audit_thread(actor, "workspace.restore.failed", thread_id, detail)
    }

    /// Puts the files back. Refused while another live thread shares a folder it would move,
    /// since its work would be overwritten without its knowledge. The files as
    /// they stood are kept in a checkpoint ref first, so this can be undone. A folder whose
    /// files changed after that snapshot is left as it is rather than overwritten, and one
    /// restore runs at a time so a safety snapshot never records half of another's work
    /// (docs/multi-repo.md).
    pub async fn restore_workspace(
        &self,
        thread_id: &str,
        request: &RestoreWorkspace,
        actor: &str,
    ) -> Result<()> {
        // A run admitted while the restore waited for its turn could change a file after that folder's safety
        // snapshot, so the thread is held against runs for the whole restore. The hold is taken under the admission
        // lock, as a rewind's is, so no run starts between the idle check and the snapshots.
        let _held_against_runs = self.runs.hold_for_rewind(thread_id)?;
        let thread = self.idle_thread(thread_id)?;
        // A second restore could otherwise snapshot the folders this one has already moved and not the rest,
        // recording half a restore as the point to come back to.
        let _one_restore_at_a_time = self.workspace_restores.lock().await;
        let roots = self.restore_roots(&thread)?;
        self.refuse_shared_roots(&thread, &roots)?;
        let mut restores = Vec::new();
        for (repo_id, worktree) in roots {
            let checkpoint =
                self.thread_checkpoint(thread_id, &request.run_id, repo_id.as_deref())?;
            let target = match request.point {
                RestorePoint::Start => checkpoint.start,
                RestorePoint::End => checkpoint
                    .end
                    .ok_or_else(|| CoreError::Invalid("This turn never finished.".into()))?,
            };
            let safety = checkpoint_ref(thread_id, &new_id(), repo_id.as_deref(), "before-restore");
            let current = git::snapshot(&worktree, Some(&safety), &self.scratch_index()?).await?;
            restores.push(Restoring {
                repo_id,
                worktree,
                target,
                safety: current,
            });
        }
        self.audit_thread(
            actor,
            "workspace.restore",
            thread_id,
            restore_detail(&restores),
        )?;
        match self.apply_restores(&restores).await {
            Ok(()) => Ok(()),
            Err((position, error)) => {
                tracing::warn!(%error, thread = %thread_id, "restore could not move a folder back");
                let (moved, failed) = restores.split_at(position);
                let failed = &failed[0];
                if let Err(audit) = self.audit_partial_restore(actor, thread_id, moved, failed) {
                    tracing::warn!(%audit, thread = %thread_id, "could not record a partial restore");
                }
                Err(self.restore_stopped(thread_id, moved, failed, &error, false))
            }
        }
    }

    fn audit_record(
        &self,
        actor: &str,
        action: &str,
        thread_id: &str,
        detail: serde_json::Value,
    ) -> AuditRecord {
        AuditRecord {
            actor: actor.to_owned(),
            action: action.to_owned(),
            target: thread_id.to_owned(),
            decision: None,
            source: "user".into(),
            thread_id: Some(thread_id.to_owned()),
            run_id: None,
            detail,
        }
    }

    fn audit_thread(
        &self,
        actor: &str,
        action: &str,
        thread_id: &str,
        detail: serde_json::Value,
    ) -> Result<()> {
        self.store
            .append_audit(&self.audit_record(actor, action, thread_id, detail))?;
        Ok(())
    }

    /// The thread a rewind may move, with the words the rewind route shows a person.
    fn rewind_idle_thread(&self, id: &str) -> Result<Thread> {
        let thread = self.store.thread(id).map_err(|error| match error {
            CoreError::NotFound(_) => refused(404, "not_found", "This thread was not found."),
            other => other,
        })?;
        let project = self
            .store
            .project(&thread.project_id)
            .map_err(|error| match error {
                CoreError::NotFound(_) => refused(
                    404,
                    "not_found",
                    "The project for this thread was not found.",
                ),
                other => other,
            })?;
        if !project.trusted {
            return Err(refused(
                403,
                "folder_untrusted",
                "Trust this folder before rewinding its thread.",
            ));
        }
        if thread.active_run.is_some() {
            return Err(rewind_busy());
        }
        Ok(thread)
    }

    /// Restores the files a rewind asked for, before the thread is marked rewound. The
    /// success row is written only after the files are back; a restore that stops part-way
    /// writes the folders it did move and the one it could not, and names that one in the
    /// error a person sees (docs/multi-repo.md).
    async fn restore_for_rewind(&self, thread: &Thread, run_id: &str, actor: &str) -> Result<()> {
        // One restore at a time: a safety snapshot taken while another had moved only some folders would record
        // half a restore as the point to come back to.
        let _one_restore_at_a_time = self.workspace_restores.lock().await;
        let roots = self.restore_roots(thread)?;
        self.refuse_shared_roots(thread, &roots)?;
        let mut restores = Vec::new();
        for (repo_id, worktree) in roots {
            let checkpoint = self
                .thread_checkpoint(&thread.id, run_id, repo_id.as_deref())
                .map_err(|error| match error {
                    CoreError::NotFound(_) => refused(
                        404,
                        "not_found",
                        "The checkpoint for this run was not found.",
                    ),
                    other => other,
                })?;
            let target = checkpoint.start;
            let safety =
                checkpoint_ref(&thread.id, &new_id(), repo_id.as_deref(), "before-restore");
            let index = self.scratch_index().map_err(|error| {
                tracing::warn!(%error, thread = %thread.id, "rewind could not prepare a checkpoint");
                restore_failed_before_changes()
            })?;
            let current = git::snapshot(&worktree, Some(&safety), &index)
                .await
                .map_err(|error| {
                    tracing::warn!(%error, thread = %thread.id, "rewind could not snapshot the workspace");
                    restore_failed_before_changes()
                })?;
            restores.push(Restoring {
                repo_id,
                worktree,
                target,
                safety: current,
            });
        }
        match self.apply_restores(&restores).await {
            Ok(()) => self.audit_thread(
                actor,
                "workspace.restore",
                &thread.id,
                restore_detail(&restores),
            ),
            Err((position, error)) => {
                tracing::warn!(%error, thread = %thread.id, "rewind could not restore the workspace");
                let (moved, failed) = restores.split_at(position);
                let failed = &failed[0];
                if let Err(audit) = self.audit_partial_restore(actor, &thread.id, moved, failed) {
                    tracing::warn!(%audit, thread = %thread.id, "could not record a partial rewind restore");
                }
                Err(self.restore_stopped(&thread.id, moved, failed, &error, true))
            }
        }
    }

    /// Continues the conversation from before a run. The log stays whole, since
    /// it is append-only; clients hide what follows the logged rewind. The engine step
    /// and the files are worked out before the store is touched, and the store rewrite
    /// is one transaction, so a failure leaves the thread where it was.
    pub async fn rewind_thread(
        &self,
        thread_id: &str,
        request: &RewindThread,
        actor: &str,
    ) -> Result<RewindResult> {
        let thread = self.rewind_idle_thread(thread_id)?;
        let _hold = self
            .runs
            .hold_for_rewind(thread_id)
            .map_err(|error| match error {
                CoreError::ThreadBusy => rewind_busy(),
                other => other,
            })?;
        let through = self
            .store
            .seq_before_run(thread_id, &request.run_id)
            .map_err(|error| match error {
                CoreError::NotFound(_) => {
                    refused(404, "not_found", "This run was not found in this thread.")
                }
                other => other,
            })?;
        // The engine step first: a native rollback notes its point (or rolls the session
        // back), and the handoff text is read; neither writes to the store.
        let native = self.revert_natively(thread_id, &request.run_id).await;
        let handoff = if native.is_none() {
            Some(self.store.handoff_text(thread_id, through)?)
        } else {
            None
        };
        if request.restore_files
            && let Err(error) = self
                .restore_for_rewind(&thread, &request.run_id, actor)
                .await
        {
            if let Some(native) = &native {
                native
                    .driver
                    .forget_revert(Path::new(&thread.cwd), &native.session)
                    .await;
            }
            return Err(error);
        }
        let native = native.is_some();
        let audit = self.audit_record(
            actor,
            "thread.rewind",
            thread_id,
            if native {
                json!({ "through": through, "native": true })
            } else {
                json!({ "through": through })
            },
        );
        self.store
            .record_rewind(thread_id, through, handoff.as_deref(), &audit, native)?;
        Ok(RewindResult {
            native,
            mode: if native {
                RewindMode::Native
            } else {
                RewindMode::Handoff
            },
        })
    }

    /// Asks the engine to drop `run_id`'s turn and every later one from the session
    /// the thread still speaks in, which then carries on. None when the engine has
    /// no native rollback, the run's turn or session is not the thread's, or the
    /// engine refused; the rewind then leaves a handoff as before.
    async fn revert_natively(&self, thread_id: &str, run_id: &str) -> Option<NativeRevert> {
        let Ok(Some((session, turn))) = self.store.native_revert_point(thread_id, run_id) else {
            return None;
        };
        let Ok(thread) = self.store.thread(thread_id) else {
            return None;
        };
        let Ok(driver) = self.engines.driver(&thread.engine) else {
            return None;
        };
        match driver
            .revert_session(Path::new(&thread.cwd), &session, &turn)
            .await
        {
            Ok(()) => Some(NativeRevert { driver, session }),
            Err(error) => {
                tracing::info!(%error, "native rollback refused; the rewind leaves a handoff");
                None
            }
        }
    }

    pub fn fork_thread(&self, id: &str, request: &ForkThread, actor: &str) -> Result<Thread> {
        let parent = self.store.thread(id)?;
        if parent.active_run.is_some() && request.run_id.is_none() && !request.side {
            return Err(CoreError::ThreadBusy);
        }
        let through = match &request.run_id {
            Some(run) if request.before => self.store.seq_before_run(id, run)?,
            Some(run) => self.store.last_seq_of_run(id, run)?,
            None => self.store.latest_seq(id)?,
        };
        let title = request.title.clone().unwrap_or_else(|| {
            if request.side {
                SIDE_CHAT_TITLE.to_owned()
            } else {
                format!("Fork of {}", parent.title)
            }
        });
        let relation = if request.side { "side" } else { "fork" };
        self.audit_thread(
            actor,
            "thread.fork",
            id,
            json!({ "through": through, "relation": relation }),
        )?;
        self.store.fork_thread(&parent, through, &title, relation)
    }

    /// The next run starts a fresh engine process and session, so it reads skills,
    /// plugins and MCP config again, and carries the conversation as a handoff.
    pub fn restart_session(&self, id: &str, actor: &str) -> Result<()> {
        if self.store.thread(id)?.active_run.is_some() {
            return Err(CoreError::ThreadBusy);
        }
        self.audit_thread(actor, "thread.restart_session", id, json!({}))?;
        let handoff = self.store.recovery_handoff(id)?;
        self.store.set_handoff(id, &handoff)?;
        self.store
            .note_thread_event(id, "engine.session_restarted", json!({}))
    }

    /// Gives a side chat's own messages to its parent's next run. It joins whatever
    /// the next run already carries, and the parent keeps its engine session.
    pub fn bring_back(&self, side_id: &str, actor: &str) -> Result<()> {
        let side = self.store.thread(side_id)?;
        let lifecycle = side.lifecycle.as_ref();
        let parent = lifecycle
            .filter(|lifecycle| lifecycle.relation.as_deref() == Some("side"))
            .and_then(|lifecycle| lifecycle.parent_thread_id.clone())
            .ok_or_else(|| CoreError::Invalid("Only a side chat can be brought back.".into()))?;
        if self.store.thread(&parent)?.active_run.is_some() {
            return Err(CoreError::ThreadBusy);
        }
        let text = self.store.side_transcript(side_id)?;
        if text.trim().is_empty() {
            return Err(CoreError::Invalid(
                "This side chat has no messages yet.".into(),
            ));
        }
        self.audit_thread(
            actor,
            "thread.bring_back",
            &parent,
            json!({ "side": side_id }),
        )?;
        self.store
            .add_to_handoff(&parent, &format!("From a side chat:\n{text}"))
    }

    /// The files this run's engine reported writing, from its stored items.
    pub fn run_changes(&self, id: &str, run_id: &str) -> Result<RunChanges> {
        let run = self.store.run(run_id)?;
        if run.thread_id != id {
            return Err(CoreError::NotFound("run".into()));
        }
        let items = self.store.run_items(id, run_id)?;
        let mut paths: Vec<String> = Vec::new();
        for item in &items {
            for path in reported_paths(&item.kind, &item.raw) {
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
        }
        let files = self.store.changed_files(run_id)?;
        // A declared file reached the list through the handshake, not an item.
        let checkout = std::path::Path::new(&self.store.thread(id)?.cwd).to_path_buf();
        for file in &files {
            let absolute = checkout.join(&file.path).to_string_lossy().into_owned();
            if !paths.contains(&file.path) && !paths.contains(&absolute) {
                paths.push(file.path.clone());
            }
        }
        let truncated = paths.len() > MAX_CHANGED_PATHS;
        paths.truncate(MAX_CHANGED_PATHS);
        Ok(RunChanges {
            run_id: run_id.to_owned(),
            paths,
            truncated,
            files,
        })
    }

    /// Moves a thread to another engine with a budgeted summary of the conversation.
    pub fn switch_engine(&self, id: &str, request: &SwitchEngine, actor: &str) -> Result<Thread> {
        let thread = self.store.thread(id)?;
        if thread.active_run.is_some() {
            return Err(CoreError::ThreadBusy);
        }
        self.engines.driver(&request.engine)?;
        let through = self.store.latest_seq(id)?;
        let handoff = self.store.handoff_text(id, through)?;
        self.audit_thread(
            actor,
            "thread.switch_engine",
            id,
            json!({
                "from": thread.engine,
                "to": request.engine,
            }),
        )?;
        // What was asked for, then what was last chosen on that engine, then its
        // defaults. Admission checks the result, as it does any selection.
        let last = if request.engine == thread.engine {
            crate::agents::selection::of_thread(&thread)
        } else {
            self.store.remembered_selection(&request.engine)?
        };
        let chosen = crate::agents::selection::merged(
            &last,
            &ModelSelection {
                model: request.model.clone(),
                effort: request.effort.clone(),
                options: request.options.clone(),
            },
        );
        self.store
            .switch_thread_engine(id, &request.engine, &chosen, &handoff)?;
        if request.engine != thread.engine {
            self.store
                .note_thread_event(id, "engine.switched", json!({ "to": request.engine }))?;
        }
        self.store.thread(id)
    }
}

const SIDE_CHAT_TITLE: &str = "Side chat";

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::Duration;

    use async_trait::async_trait;
    use orbyn_contracts::{AgUiEvent, EngineCapabilities, EngineTier, PermissionMode, RunState};
    use tokio::sync::mpsc;

    use super::*;
    use crate::agents::claude_history::project_folder_name;
    use crate::agents::driver::claude::ClaudeDriver;
    use crate::agents::driver::host::{EnginePath, HostDriver, HostSpec, Pool, Timings};
    use crate::agents::driver::{EngineDescriptor, EngineEvent, EngineProbe, RunHandle, RunSpec};
    use crate::store::thread_repos::ThreadRepoRecord;
    use crate::store::{Admission, NewProject, NewThread, Store};
    use crate::testing::{TempDir, run_git};

    const SESSION: &str = "0f3c2b1a-1111-4222-8333-444455556666";
    const PROMPT: &str = "a1a1a1a1-1111-4222-8333-444455556666";
    const HEAD: &str = "b2b2b2b2-1111-4222-8333-444455556666";

    #[test]
    fn only_what_an_engine_reports_writing_counts() {
        let codex = json!({ "changes": [{ "path": "a.rs" }, { "path": "b.rs" }] });
        assert_eq!(reported_paths("fileChange", &codex), ["a.rs", "b.rs"]);
        let edit = json!({ "name": "Edit", "input": { "file_path": "/w/a.rs" } });
        assert_eq!(reported_paths("tool_use", &edit), ["/w/a.rs"]);
        let read = json!({ "name": "Read", "input": { "file_path": "/w/a.rs" } });
        assert!(reported_paths("tool_use", &read).is_empty());
        let bash = json!({ "name": "Bash", "input": { "command": "echo > x" } });
        assert!(reported_paths("tool_use", &bash).is_empty());
        assert!(reported_paths("commandExecution", &json!({})).is_empty());
    }

    fn open_core(dir: &TempDir) -> Arc<Core> {
        let mut config =
            crate::CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
        config.engines.lookup_latest = false;
        Arc::new(Core::open(config).unwrap())
    }

    fn seed(repo: &Path) {
        std::fs::create_dir_all(repo).unwrap();
        run_git(repo, &["init", "-q", "-b", "main"]);
        run_git(repo, &["config", "user.email", "t@example.com"]);
        run_git(repo, &["config", "user.name", "T"]);
        std::fs::write(repo.join("base.txt"), "base\n").unwrap();
        run_git(repo, &["add", "."]);
        run_git(repo, &["commit", "-q", "-m", "first"]);
    }

    fn head(repo: &Path) -> String {
        let output = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(repo)
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    fn project_and_thread(core: &Core, repo: &Path, engine: &str) -> String {
        let path = repo.to_string_lossy().into_owned();
        let project = core
            .store
            .add_project(NewProject {
                path: &path,
                name: "repo",
                is_git: true,
                managed: false,
                scratch: false,
            })
            .unwrap();
        core.store.trust_project(&project.id, "tester").unwrap();
        let thread = core
            .store
            .create_thread(NewThread {
                project_id: project.id,
                title: "t".into(),
                engine: engine.into(),
                model: None,
                cwd: path,
                worktree: None,
                skill_profile: None,
                parent_thread_id: None,
                relation: None,
                scratch: false,
            })
            .unwrap();
        thread.id
    }

    fn ran(store: &Store, thread: &str, prompt: &str, session: &str, turn: Option<&str>) -> String {
        let run = store
            .admit_run(Admission {
                thread_id: thread,
                prompt,
                actor: "tester",
            })
            .unwrap();
        store.mark_starting(&run.id).unwrap();
        store.mark_running(&run.id).unwrap();
        store.set_engine_session(&run.id, session).unwrap();
        if let Some(turn) = turn {
            store.set_engine_turn(&run.id, turn).unwrap();
        }
        store
            .finish_run(&run.id, RunState::Completed, None)
            .unwrap();
        run.id
    }

    struct NativeFixture {
        _dir: TempDir,
        core: Arc<Core>,
        pool: Arc<Pool>,
        repo: PathBuf,
        thread: String,
        second: String,
    }

    /// A Claude thread whose second run can be rewound natively through a session file.
    fn native_fixture() -> NativeFixture {
        let dir = TempDir::new();
        let repo = dir.path().join("repo");
        seed(&repo);
        let core = open_core(&dir);
        let pool = Pool::new(
            "claude",
            HostSpec {
                program: "/nonexistent/orbyn-engine-host".into(),
                args: Vec::new(),
                env: Vec::new(),
                instance: "rewind-test".into(),
            },
            Timings::default(),
            None,
        );
        let projects = dir.path().join("projects");
        let folder = projects.join(project_folder_name(&repo.to_string_lossy()));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join(format!("{SESSION}.jsonl")),
            format!(
                "{}\n{}\n",
                json!({"type":"assistant","uuid":HEAD,"sessionId":SESSION,"message":{}}),
                json!({"type":"user","uuid":PROMPT,"sessionId":SESSION,"message":{}}),
            ),
        )
        .unwrap();
        let driver = HostDriver::new(
            Arc::new(ClaudeDriver::new("claude".into(), Duration::from_secs(1))),
            pool.clone(),
            "claude".into(),
        )
        .with_history_root(projects)
        .with_path(Arc::new(|| Ok(EnginePath::Sdk)));
        core.engines.register(Arc::new(driver));
        let thread = project_and_thread(&core, &repo, "claude");
        let first = ran(&core.store, &thread, "first", SESSION, None);
        let second = ran(&core.store, &thread, "second", SESSION, Some(PROMPT));
        assert_eq!(
            core.store.native_revert_point(&thread, &second).unwrap(),
            Some((SESSION.to_owned(), PROMPT.to_owned())),
            "the second run has an earlier run in the same session to keep"
        );
        let start = head(&repo);
        core.store.add_checkpoint(&thread, &second, &start).unwrap();
        core.store.finish_checkpoint(&second, &start).unwrap();
        let _ = first;
        NativeFixture {
            _dir: dir,
            core,
            pool,
            repo,
            thread,
            second,
        }
    }

    fn has_rewound_event(core: &Core, thread: &str) -> bool {
        core.store
            .events_after(thread, 0, 10_000)
            .unwrap()
            .iter()
            .any(|event| {
                matches!(
                    &event.event,
                    AgUiEvent::Custom { name, .. } if name == "thread.rewound"
                )
            })
    }

    fn has_rewind_audit(core: &Core, thread: &str) -> bool {
        core.store
            .list_audit(Some(thread), 1000)
            .unwrap()
            .iter()
            .any(|event| event.action == "thread.rewind")
    }

    #[tokio::test]
    async fn a_failed_file_restore_leaves_the_thread_unrewound_and_its_point_forgotten() {
        let f = native_fixture();
        std::fs::write(f.repo.join("after.txt"), "later").unwrap();
        // The safety snapshot fails before a file is touched: the folder is no longer a
        // checkout, so the checkpoint's start commit cannot be read.
        std::fs::remove_dir_all(f.repo.join(".git")).unwrap();

        let error = f
            .core
            .rewind_thread(
                &f.thread,
                &RewindThread {
                    run_id: f.second.clone(),
                    restore_files: true,
                },
                "tester",
            )
            .await
            .expect_err("the restore cannot succeed without Git");
        assert_eq!(
            error.to_string(),
            "Your files were not changed. The thread was not rewound."
        );
        assert_eq!(
            std::fs::read_to_string(f.repo.join("after.txt")).unwrap(),
            "later",
            "files stay as they were"
        );
        assert!(
            f.core.store.pending_handoff(&f.thread).unwrap().is_none(),
            "no handoff was set"
        );
        assert!(!has_rewound_event(&f.core, &f.thread));
        assert!(!has_rewind_audit(&f.core, &f.thread));
        assert_eq!(
            f.pool.take_resume_at("claude", SESSION),
            None,
            "the point noted by the native step was forgotten"
        );
    }

    /// A driver whose engine step fails and corrupts the event log the handoff reads, so
    /// the rewind fails before it can touch the files.
    struct BreakHandoff {
        store: Arc<Store>,
        thread: String,
    }

    #[async_trait]
    impl EngineDriver for BreakHandoff {
        fn descriptor(&self) -> EngineDescriptor {
            EngineDescriptor {
                id: "fake".into(),
                name: "Fake".into(),
                tier: EngineTier::Terminal,
                capabilities: EngineCapabilities {
                    streaming: false,
                    approvals: false,
                    resume: false,
                    mcp: false,
                    sandbox: false,
                    managed: true,
                    skill_delivery: Vec::new(),
                    goal_control: false,
                    decline_feedback: false,
                    permission_modes: PermissionMode::ALL.to_vec(),
                },
            }
        }

        async fn probe(&self) -> EngineProbe {
            EngineProbe::default()
        }

        async fn start_run(
            &self,
            _spec: RunSpec,
            _events: mpsc::Sender<EngineEvent>,
        ) -> Result<RunHandle> {
            Err(CoreError::Unsupported("no run".into()))
        }

        async fn revert_session(&self, _cwd: &Path, _session: &str, _turn: &str) -> Result<()> {
            self.store
                .reader()
                .execute(
                    "INSERT INTO thread_events (thread_id, seq, run_id, ts, event)
                     SELECT id, next_seq, NULL, '2026-01-01T00:00:00Z', 'not json'
                     FROM threads WHERE id = ?1",
                    [&self.thread],
                )
                .unwrap();
            Err(CoreError::Unsupported("the engine refused".into()))
        }
    }

    #[tokio::test]
    async fn a_failed_handoff_step_does_not_restore_files() {
        let dir = TempDir::new();
        let repo = dir.path().join("repo");
        seed(&repo);
        let core = open_core(&dir);
        let thread = project_and_thread(&core, &repo, "fake");
        core.engines.register(Arc::new(BreakHandoff {
            store: core.store.clone(),
            thread: thread.clone(),
        }));
        ran(&core.store, &thread, "first", SESSION, None);
        let second = ran(&core.store, &thread, "second", SESSION, Some(PROMPT));
        let start = head(&repo);
        core.store.add_checkpoint(&thread, &second, &start).unwrap();
        core.store.finish_checkpoint(&second, &start).unwrap();
        std::fs::write(repo.join("after.txt"), "later").unwrap();

        core.rewind_thread(
            &thread,
            &RewindThread {
                run_id: second.clone(),
                restore_files: true,
            },
            "tester",
        )
        .await
        .expect_err("the handoff text cannot be read");
        assert_eq!(
            std::fs::read_to_string(repo.join("after.txt")).unwrap(),
            "later",
            "the files were not restored on a failed engine step"
        );
        assert!(core.store.pending_handoff(&thread).unwrap().is_none());
        assert!(!has_rewind_audit(&core, &thread));
    }

    /// A thread over two repositories, each restored in its own worktree, with a task folder of its own.
    struct TwoRepositories {
        _dir: TempDir,
        core: Arc<Core>,
        thread: Thread,
        primary: String,
        other: String,
        primary_worktree: PathBuf,
        other_worktree: PathBuf,
    }

    async fn two_repositories() -> TwoRepositories {
        let dir = TempDir::new();
        let primary_worktree = dir.path().join("first");
        seed(&primary_worktree);
        let other_worktree = dir.path().join("second");
        seed(&other_worktree);
        let task = dir.path().join("task");
        std::fs::create_dir_all(&task).unwrap();
        let core = open_core(&dir);
        let path = primary_worktree.to_string_lossy().into_owned();
        let project = core
            .store
            .add_project(NewProject {
                path: &path,
                name: "first",
                is_git: true,
                managed: false,
                scratch: false,
            })
            .unwrap();
        core.store.trust_project(&project.id, "tester").unwrap();
        let thread = core
            .store
            .create_thread(NewThread {
                project_id: project.id.clone(),
                title: "t".into(),
                engine: "codex".into(),
                model: None,
                cwd: task.to_string_lossy().into_owned(),
                worktree: None,
                skill_profile: None,
                parent_thread_id: None,
                relation: None,
                scratch: false,
            })
            .unwrap();
        let other = "repo-second".to_owned();
        for (position, (repo_id, name, worktree)) in [
            (project.id.clone(), "first", &primary_worktree),
            (other.clone(), "second", &other_worktree),
        ]
        .into_iter()
        .enumerate()
        {
            core.store
                .thread_repo_insert(
                    &ThreadRepoRecord {
                        thread_id: thread.id.clone(),
                        repo_id,
                        position: position as u32,
                        name: name.to_owned(),
                        branch: "orbyn/thread".into(),
                        // The fixture writes the row itself, with no base: a thread stored before the column.
                        base_branch: None,
                        worktree_path: worktree.to_string_lossy().into_owned(),
                    },
                    &crate::util::now(),
                )
                .unwrap();
        }
        TwoRepositories {
            _dir: dir,
            core,
            thread,
            primary: project.id,
            other,
            primary_worktree,
            other_worktree,
        }
    }

    /// A safety snapshot is taken before the restore writes, and a file can be saved in between. That edit is in no
    /// snapshot, so the folder is left as it is rather than overwritten.
    #[tokio::test]
    async fn a_restore_leaves_alone_a_folder_whose_files_moved_after_its_safety_snapshot() {
        let dir = TempDir::new();
        let core = open_core(&dir);
        let repo = dir.path().join("repo");
        seed(&repo);
        let target = git::snapshot(&repo, None, &core.scratch_index().unwrap())
            .await
            .unwrap();
        std::fs::write(repo.join("base.txt"), "edited before the restore\n").unwrap();
        let safety = git::snapshot(&repo, None, &core.scratch_index().unwrap())
            .await
            .unwrap();
        std::fs::write(repo.join("base.txt"), "saved while the restore prepared\n").unwrap();

        let restores = vec![Restoring {
            repo_id: None,
            worktree: repo.clone(),
            target,
            safety,
        }];
        core.apply_restores(&restores)
            .await
            .expect_err("the folder no longer holds what its safety snapshot recorded");
        assert_eq!(
            std::fs::read_to_string(repo.join("base.txt")).unwrap(),
            "saved while the restore prepared\n",
            "the edit that no snapshot records is still there"
        );
    }

    /// A folder that still holds what its safety snapshot recorded is moved back, so the refusal above is not a
    /// blanket one.
    #[tokio::test]
    async fn a_restore_moves_a_folder_that_still_matches_its_safety_snapshot() {
        let dir = TempDir::new();
        let core = open_core(&dir);
        let repo = dir.path().join("repo");
        seed(&repo);
        let target = git::snapshot(&repo, None, &core.scratch_index().unwrap())
            .await
            .unwrap();
        std::fs::write(repo.join("base.txt"), "edited before the restore\n").unwrap();
        let safety = git::snapshot(&repo, None, &core.scratch_index().unwrap())
            .await
            .unwrap();

        let restores = vec![Restoring {
            repo_id: None,
            worktree: repo.clone(),
            target,
            safety,
        }];
        core.apply_restores(&restores).await.unwrap();
        assert_eq!(
            std::fs::read_to_string(repo.join("base.txt")).unwrap(),
            "base\n",
            "the folder holds the checkpoint's files again"
        );
    }

    /// A restore that stopped part-way names the folder it could not write and the ones that already moved, so the
    /// error alone cannot be read as "nothing changed". A failure before anything moved says so instead.
    #[tokio::test]
    async fn a_failed_restore_names_the_repository_and_what_already_moved() {
        let f = two_repositories().await;
        let missing = "0123456789012345678901234567890123456789";
        let moved = [Restoring {
            repo_id: Some(f.primary.clone()),
            worktree: f.primary_worktree.clone(),
            target: missing.to_owned(),
            safety: missing.to_owned(),
        }];
        let failed = Restoring {
            repo_id: Some(f.other.clone()),
            worktree: f.other_worktree.clone(),
            target: missing.to_owned(),
            safety: missing.to_owned(),
        };
        let error = CoreError::Git("fatal: bad object".into());

        let partial = f
            .core
            .restore_stopped(&f.thread.id, &moved, &failed, &error, false)
            .to_string();
        assert!(
            partial.contains("Your files may have changed"),
            "a moved folder is not reported as untouched: {partial}"
        );
        assert!(
            partial.contains("the repository \"second\" could not be restored"),
            "the folder that failed is named: {partial}"
        );
        assert!(
            partial.contains("the repository \"first\" already moved back"),
            "the folders that moved are named: {partial}"
        );
        assert!(
            partial.contains("fatal: bad object"),
            "Git's own error is kept: {partial}"
        );

        let nothing = f
            .core
            .restore_stopped(&f.thread.id, &[], &failed, &error, false)
            .to_string();
        assert!(
            nothing.contains("Your files were not changed"),
            "a restore that moved nothing says so: {nothing}"
        );
        assert!(
            !nothing.contains("already moved back"),
            "no folder is named as moved when none was: {nothing}"
        );
    }

    /// A rewind that restores one repository and then fails on another has already moved the first, so that move is
    /// on the record together with the repository that could not be restored.
    #[tokio::test]
    async fn a_rewind_that_restores_one_repository_and_fails_another_records_both() {
        let f = two_repositories().await;
        let run = "run-partial";
        let start = head(&f.primary_worktree);
        f.core
            .store
            .add_repository_checkpoint(&f.thread.id, run, &f.primary, &start)
            .unwrap();
        // A commit Git does not have: its restore fails after the first repository has moved.
        let missing = "0123456789012345678901234567890123456789";
        f.core
            .store
            .add_repository_checkpoint(&f.thread.id, run, &f.other, missing)
            .unwrap();
        std::fs::write(f.primary_worktree.join("written.txt"), "written\n").unwrap();
        std::fs::write(f.other_worktree.join("kept.txt"), "kept\n").unwrap();

        let error = f
            .core
            .restore_for_rewind(&f.thread, run, "tester")
            .await
            .expect_err("the second repository has no commit to restore");

        assert!(
            !f.primary_worktree.join("written.txt").exists(),
            "the first repository was restored before the second failed"
        );
        let audit = f
            .core
            .store
            .list_audit(Some(f.thread.id.as_str()), 10)
            .unwrap();
        let row = audit
            .iter()
            .find(|row| row.action == "workspace.restore.failed")
            .expect("the partial restore is on the record");
        assert_eq!(
            row.detail["repositories"][0]["repo_id"],
            json!(f.primary),
            "the repository that moved is named"
        );
        assert_eq!(
            row.detail["failed"]["repo_id"],
            json!(f.other),
            "the repository that could not move is named"
        );
        assert!(
            row.detail["failed"]["safety"].is_string(),
            "the safety snapshot of the folder that did not move is kept"
        );
        assert!(
            !audit.iter().any(|row| row.action == "workspace.restore"),
            "a partial restore is never written as a whole one"
        );
        let message = error.to_string();
        assert!(
            message.contains("the repository \"second\""),
            "the person is told which repository failed: {message}"
        );
        assert!(
            message.contains("fatal:") || message.contains("unknown revision"),
            "the underlying error is kept: {message}"
        );
        assert_eq!(
            std::fs::read_to_string(f.other_worktree.join("kept.txt")).unwrap(),
            "kept\n",
            "the repository that could not be restored was left as it was"
        );
    }
}
