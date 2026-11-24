//! Diffs over checkpoints, and moving a thread back: restoring files, rewinding
//! the conversation, forking it and switching engines. An engine session cannot
//! follow a fork or a switch, so each leaves a budgeted handoff for the next run; a
//! rewind keeps the session when the engine can roll it back natively.

use std::path::Path;
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

pub fn checkpoint_ref(thread_id: &str, run_id: &str, point: &str) -> String {
    format!("refs/orbyn/checkpoints/{thread_id}/{run_id}/{point}")
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
    fn trusted_thread(&self, id: &str) -> Result<Thread> {
        let thread = self.store.thread(id)?;
        if !self.store.project(&thread.project_id)?.trusted {
            return Err(CoreError::FolderUntrusted);
        }
        Ok(thread)
    }

    pub(crate) fn idle_thread(&self, id: &str) -> Result<Thread> {
        let thread = self.trusted_thread(id)?;
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

    fn thread_checkpoint(
        &self,
        thread_id: &str,
        run_id: &str,
    ) -> Result<orbyn_contracts::Checkpoint> {
        self.store
            .checkpoints(thread_id)?
            .into_iter()
            .find(|checkpoint| checkpoint.run_id == run_id)
            .ok_or_else(|| CoreError::NotFound("checkpoint".into()))
    }

    pub async fn turn_diff(&self, thread_id: &str, run_id: &str) -> Result<Vec<FileDiff>> {
        let thread = self.trusted_thread(thread_id)?;
        let checkpoint = self.thread_checkpoint(thread_id, run_id)?;
        let end = checkpoint
            .end
            .ok_or_else(|| CoreError::Invalid("This turn is still running.".into()))?;
        git::diff_between(Path::new(&thread.cwd), &checkpoint.start, &end).await
    }

    /// Everything the thread has changed, up to the files as they are now.
    pub async fn full_thread_diff(&self, thread_id: &str) -> Result<Vec<FileDiff>> {
        let thread = self.trusted_thread(thread_id)?;
        let Some(first) = self.store.checkpoints(thread_id)?.into_iter().next() else {
            return Ok(Vec::new());
        };
        let cwd = Path::new(&thread.cwd);
        let now = git::snapshot(cwd, None, &self.scratch_index()?).await?;
        git::diff_between(cwd, &first.start, &now).await
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

    /// Puts the files back. Refused while another live thread shares the folder,
    /// since its work would be overwritten without its knowledge. The files as
    /// they stood are kept in a checkpoint ref first, so this can be undone.
    pub async fn restore_workspace(
        &self,
        thread_id: &str,
        request: &RestoreWorkspace,
        actor: &str,
    ) -> Result<()> {
        let thread = self.idle_thread(thread_id)?;
        let shared = self.store.threads_sharing_checkout(&thread)?;
        if !shared.is_empty() {
            return Err(CoreError::SharedDirectory(shared.len()));
        }
        let checkpoint = self.thread_checkpoint(thread_id, &request.run_id)?;
        let target = match request.point {
            RestorePoint::Start => checkpoint.start,
            RestorePoint::End => checkpoint
                .end
                .ok_or_else(|| CoreError::Invalid("This turn never finished.".into()))?,
        };
        let cwd = Path::new(&thread.cwd);
        let safety = checkpoint_ref(thread_id, &new_id(), "before-restore");
        let current = git::snapshot(cwd, Some(&safety), &self.scratch_index()?).await?;
        self.audit_thread(
            actor,
            "workspace.restore",
            thread_id,
            json!({
                "target": target,
                "safety": current,
            }),
        )?;
        git::restore_worktree(cwd, &current, &target).await
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
            CoreError::NotFound(_) => refused(
                404,
                "not_found",
                "This thread was not found.",
            ),
            other => other,
        })?;
        let project = self.store.project(&thread.project_id).map_err(|error| match error {
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
    /// audit row is written only after the files are back, so a failed restore leaves no
    /// record of one.
    async fn restore_for_rewind(
        &self,
        thread: &Thread,
        run_id: &str,
        actor: &str,
    ) -> Result<()> {
        let shared = self.store.threads_sharing_checkout(thread)?;
        if !shared.is_empty() {
            return Err(CoreError::SharedDirectory(shared.len()));
        }
        let checkpoint = self
            .thread_checkpoint(&thread.id, run_id)
            .map_err(|error| match error {
                CoreError::NotFound(_) => refused(
                    404,
                    "not_found",
                    "The checkpoint for this run was not found.",
                ),
                other => other,
            })?;
        let target = checkpoint.start;
        let cwd = Path::new(&thread.cwd);
        let safety = checkpoint_ref(&thread.id, &new_id(), "before-restore");
        let index = self.scratch_index().map_err(|error| {
            tracing::warn!(%error, thread = %thread.id, "rewind could not prepare a checkpoint");
            restore_failed_before_changes()
        })?;
        let current = git::snapshot(cwd, Some(&safety), &index)
            .await
            .map_err(|error| {
                tracing::warn!(%error, thread = %thread.id, "rewind could not snapshot the workspace");
                restore_failed_before_changes()
            })?;
        git::restore_worktree(cwd, &current, &target)
            .await
            .map_err(|error| {
                tracing::warn!(%error, thread = %thread.id, "rewind could not restore the workspace");
                CoreError::Git(
                    "Your files may have changed. The thread was not rewound.".into(),
                )
            })?;
        self.audit_thread(
            actor,
            "workspace.restore",
            &thread.id,
            json!({
                "target": target,
                "safety": current,
            }),
        )
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
        let _hold = self.runs.hold_for_rewind(thread_id).map_err(|error| match error {
            CoreError::ThreadBusy => rewind_busy(),
            other => other,
        })?;
        let through = self
            .store
            .seq_before_run(thread_id, &request.run_id)
            .map_err(|error| match error {
                CoreError::NotFound(_) => refused(
                    404,
                    "not_found",
                    "This run was not found in this thread.",
                ),
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
    use orbyn_contracts::{
        AgUiEvent, EngineCapabilities, EngineTier, PermissionMode, RunState,
    };
    use tokio::sync::mpsc;

    use super::*;
    use crate::agents::claude_history::project_folder_name;
    use crate::agents::driver::claude::ClaudeDriver;
    use crate::agents::driver::host::{EnginePath, HostDriver, HostSpec, Pool, Timings};
    use crate::agents::driver::{
        EngineDescriptor, EngineProbe, EngineEvent, RunHandle, RunSpec,
    };
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
        let mut config = crate::CoreConfig::new(
            dir.path().join("data"),
            "http://127.0.0.1:1".into(),
        );
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
            Arc::new(ClaudeDriver::new(
                "claude".into(),
                Duration::from_secs(1),
            )),
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
}
