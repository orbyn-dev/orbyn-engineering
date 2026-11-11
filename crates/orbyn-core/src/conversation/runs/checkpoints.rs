use super::*;

impl RunManager {
    /// A failed snapshot costs that run its checkpoint, never the run itself.
    pub(super) async fn checkpoint_start(&self, scope: &RunScope, checkout: &Path, run_dir: &Path) {
        let reference = checkpoint_ref(&scope.thread_id, &scope.run_id, "start");
        let index = run_dir.join("checkpoint-index");
        match snapshot(checkout, Some(&reference), &index).await {
            Ok(sha) => {
                if let Err(error) = self
                    .store
                    .add_checkpoint(&scope.thread_id, &scope.run_id, &sha)
                {
                    tracing::warn!(%error, "could not record a checkpoint");
                }
            }
            Err(error) => tracing::warn!(%error, "could not take a checkpoint"),
        }
    }

    pub(super) async fn checkpoint_end(&self, scope: &RunScope, thread: &Thread, run_dir: &Path) {
        if !matches!(self.store.checkpoint(&scope.run_id), Ok(Some(_))) {
            return;
        }
        let reference = checkpoint_ref(&scope.thread_id, &scope.run_id, "end");
        let index = run_dir.join("checkpoint-index");
        match snapshot(Path::new(&thread.cwd), Some(&reference), &index).await {
            Ok(sha) => {
                if let Err(error) = self.store.finish_checkpoint(&scope.run_id, &sha) {
                    tracing::warn!(%error, "could not record a checkpoint");
                }
            }
            Err(error) => tracing::warn!(%error, "could not take a checkpoint"),
        }
    }

    /// Writes the terminal state and hands expired approvals back to their waiters.
    pub(super) fn close_run(
        &self,
        run_id: &str,
        state: RunState,
        error: Option<&str>,
        sign_in_note: Option<&Value>,
    ) -> Option<Run> {
        // An agent may have committed, switched branch or added a worktree during the run.
        self.workspace.invalidate_status();
        match self
            .store
            .finish_run_with_sign_in(run_id, state, error, sign_in_note)
        {
            Ok((run, expired)) => {
                self.gate.settle(&expired);
                // A run may have pushed to a branch that has a pull request.
                if let Ok(thread) = self.store.thread(&run.thread_id) {
                    self.scm_reads.project_all_changed(&thread.project_id);
                }
                Some(run)
            }
            Err(CoreError::RunClosed) => None,
            Err(failure) => {
                tracing::error!(%failure, %run_id, "could not close a run");
                None
            }
        }
    }

    pub(super) async fn drive(
        &self,
        thread: &Thread,
        scope: &RunScope,
        message: (String, Vec<PathBuf>),
        run_dir: &Path,
        token: &mut Option<String>,
        stops: &Stops,
    ) -> EngineOutcome {
        let first = self
            .attempt(thread, scope, message.clone(), run_dir, token, stops, false)
            .await;
        if first != EngineOutcome::SessionLost {
            return first;
        }
        // The engine's session is gone, so a fresh one gets the budgeted history
        // from the log instead, once.
        match self.store.run_owns_engine(&scope.run_id) {
            Ok(true) => {}
            Ok(false) => return EngineOutcome::SessionLost,
            Err(error) => return EngineOutcome::Failed(error.public_message()),
        }
        if let Err(error) = self.recover_session(thread, &scope.run_id) {
            return EngineOutcome::Failed(error.public_message());
        }
        self.attempt(thread, scope, message, run_dir, token, stops, true)
            .await
    }

    fn recover_session(&self, thread: &Thread, run: &str) -> Result<()> {
        let handoff = self.store.recovery_handoff(&thread.id)?;
        self.store.set_run_handoff(run, &handoff)?;
        self.store
            .note_thread_event(&thread.id, "engine.session_lost", json!({}))
    }
}
