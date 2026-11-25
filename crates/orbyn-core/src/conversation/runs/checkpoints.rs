use super::*;

impl RunManager {
    /// The folders a run snapshots, and the repository each one belongs to: every trusted repository's worktree of a
    /// multi-repository thread, or the engine's own checkout for an ordinary one. A task folder is never one, because
    /// it is not a repository: a run in it used to keep no checkpoint at all (docs/multi-repo.md).
    fn checkpoint_roots(
        &self,
        thread: &Thread,
        checkout: &Path,
    ) -> Result<Vec<(Option<String>, PathBuf)>> {
        let worktrees = crate::repos::trusted_worktrees(&self.store, thread)?;
        Ok(if worktrees.is_empty() {
            vec![(None, checkout.to_path_buf())]
        } else {
            worktrees
                .into_iter()
                .map(|(repo_id, worktree)| (Some(repo_id), worktree))
                .collect()
        })
    }

    /// A failed snapshot costs that run its checkpoint, never the run itself.
    pub(super) async fn checkpoint_start(
        &self,
        thread: &Thread,
        scope: &RunScope,
        checkout: &Path,
        run_dir: &Path,
    ) {
        let reference = checkpoint_ref(&scope.thread_id, &scope.run_id, "start");
        let index = run_dir.join("checkpoint-index");
        let roots = match self.checkpoint_roots(thread, checkout) {
            Ok(roots) => roots,
            Err(error) => {
                tracing::warn!(%error, "could not list the folders to snapshot");
                return;
            }
        };
        for (position, (repo_id, root)) in roots.into_iter().enumerate() {
            let sha = match snapshot(&root, Some(&reference), &index).await {
                Ok(sha) => sha,
                Err(error) => {
                    tracing::warn!(%error, "could not take a checkpoint");
                    continue;
                }
            };
            let recorded = match &repo_id {
                Some(repo_id) => self
                    .store
                    .add_repository_checkpoint(&scope.thread_id, &scope.run_id, repo_id, &sha)
                    // One run is one row for the turn picker, so the first repository's ends are the run's too.
                    .and_then(|()| match position {
                        0 => self
                            .store
                            .add_checkpoint(&scope.thread_id, &scope.run_id, &sha),
                        _ => Ok(()),
                    }),
                None => self
                    .store
                    .add_checkpoint(&scope.thread_id, &scope.run_id, &sha),
            };
            if let Err(error) = recorded {
                tracing::warn!(%error, "could not record a checkpoint");
            }
        }
    }

    pub(super) async fn checkpoint_end(&self, scope: &RunScope, thread: &Thread, run_dir: &Path) {
        if !matches!(self.store.run_has_checkpoint(&scope.run_id), Ok(true)) {
            return;
        }
        let reference = checkpoint_ref(&scope.thread_id, &scope.run_id, "end");
        let index = run_dir.join("checkpoint-index");
        let roots = match self.checkpoint_roots(thread, Path::new(&thread.cwd)) {
            Ok(roots) => roots,
            Err(error) => {
                tracing::warn!(%error, "could not list the folders to snapshot");
                return;
            }
        };
        for (position, (repo_id, root)) in roots.into_iter().enumerate() {
            let sha = match snapshot(&root, Some(&reference), &index).await {
                Ok(sha) => sha,
                Err(error) => {
                    tracing::warn!(%error, "could not take a checkpoint");
                    continue;
                }
            };
            let recorded = match &repo_id {
                Some(repo_id) => self
                    .store
                    .finish_repository_checkpoint(&scope.run_id, repo_id, &sha)
                    .and_then(|()| match position {
                        0 => self.store.finish_checkpoint(&scope.run_id, &sha),
                        _ => Ok(()),
                    }),
                None => self.store.finish_checkpoint(&scope.run_id, &sha),
            };
            if let Err(error) = recorded {
                tracing::warn!(%error, "could not record a checkpoint");
            }
        }
    }

    /// Writes the terminal state and hands expired approvals back to their waiters.
    pub(super) fn close_run(
        &self,
        run_id: &str,
        state: RunState,
        error: Option<&str>,
        failure_note: Option<&Value>,
    ) -> Option<Run> {
        // An agent may have committed, switched branch or added a worktree during the run.
        self.workspace.invalidate_status();
        match self
            .store
            .finish_run_with_sign_in(run_id, state, error, failure_note)
        {
            Ok((run, expired)) => {
                self.gate.settle(&expired);
                // A run may have pushed to a branch that has a pull request.
                if let Ok(thread) = self.store.thread(&run.thread_id) {
                    self.scm_reads.project_all_changed(&thread.project_id);
                    // And the run's files: change letters and the dirty summary moved.
                    self.explorer.changed(&thread.project_id);
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
