use super::*;

impl RunManager {
    /// The engine refused a message it was handed mid-turn, so it goes in the queue.
    /// Replaces this thread's app note, or clears it with `None`.
    pub fn set_app_note(&self, thread_id: &str, note: Option<crate::tools::mcp_apps::AppNote>) {
        let mut notes = self
            .app_notes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match note {
            Some(note) => notes.insert(thread_id.to_owned(), note),
            None => notes.remove(thread_id),
        };
    }

    pub(super) fn take_app_note(&self, thread_id: &str) -> Option<crate::tools::mcp_apps::AppNote> {
        self.app_notes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(thread_id)
    }

    /// Stores an MCP App's page as one of this thread's renders and answers the
    /// timeline marker that ties it to its tool row. A page the render store refuses,
    /// such as one over its size limit, leaves the row an ordinary tool call.
    pub(super) fn store_app(
        &self,
        thread_id: &str,
        app: &crate::tools::mcp_apps::McpAppSnapshot,
    ) -> Option<AgUiEvent> {
        match crate::conversation::renders::publish_app_page(
            &self.renders_dir,
            thread_id,
            &app.html,
            &serde_json::json!({ "server": app.server, "tool": app.tool, "resourceUri": app.resource_uri }),
        ) {
            Ok(render) => Some(AgUiEvent::Custom {
                name: "mcp_app".into(),
                value: crate::tools::mcp_apps::timeline_marker(
                    app,
                    &render.id,
                    &render.frame_token,
                ),
            }),
            Err(error) => {
                tracing::info!(%error, "could not store an MCP App page");
                None
            }
        }
    }

    pub(super) fn requeue_steer(&self, scope: &RunScope, prompt: &str) {
        let queued = self
            .store
            .requeue_failed_steer(&scope.run_id, &scope.thread_id, prompt);
        if let Err(error) = queued {
            tracing::warn!(%error, "could not requeue a steered message");
        }
    }

    /// A delegated thread with nothing left to do tells its parent, through the
    /// parent's queue, so the report waits its turn and wakes the parent.
    pub(super) fn report_to_parent(self: &Arc<Self>, thread_id: &str, run_id: &str) {
        let stopped = &self.stopped_with_parent;
        if stopped.lock().is_ok_and(|mut held| held.remove(run_id)) {
            return;
        }
        let report = || -> Result<()> {
            // A Crew worker answers through `crew_report`; the core tells its lead.
            if self.store.crew_task_for_worker(thread_id)?.is_some() {
                return Ok(());
            }
            let thread = self.store.thread(thread_id)?;
            let lifecycle = thread.lifecycle.as_ref();
            let parent = lifecycle
                .filter(|l| l.relation.as_deref() == Some("subagent"))
                .and_then(|l| l.parent_thread_id.clone());
            let Some(parent) = parent else { return Ok(()) };
            if thread.active_run.is_some() || !self.store.list_queue(thread_id)?.is_empty() {
                return Ok(());
            }
            let state = self.store.run(run_id)?.state;
            let state = serde_json::to_value(state)?;
            // The newest text, within the handoff budget: the ending is the result.
            let transcript = self.store.recovery_handoff(thread_id)?;
            let prompt = crate::conversation::automations::child_report(
                &thread.title,
                state.as_str().unwrap_or("ended"),
                &transcript,
            );
            self.enqueue(&parent, &prompt, &[]).map(|_| ())
        };
        if let Err(error) = report() {
            tracing::warn!(%error, "a delegated thread could not report back");
        }
    }

    fn note_stopped_with_parent(&self, run_id: &str) {
        if let Ok(mut held) = self.stopped_with_parent.lock() {
            held.insert(run_id.to_owned());
        }
    }

    /// Requests cancellation, with any delegated threads running under this one.
    /// The run reaches `cancelled` once its engine is down.
    pub fn cancel(&self, run_id: &str) -> Result<()> {
        let run = self.store.run(run_id)?;
        if run.state.is_terminal() {
            return Err(CoreError::RunClosed);
        }
        self.store.request_stop(run_id)?;
        let cancellation = self
            .lock()
            .get(run_id)
            .map(|active| active.stops.cancel.clone());
        if let Some(cancel) = &cancellation {
            cancel.cancel();
        }
        // Once accepted, parent cancellation cannot depend on child lookup or
        // cleanup succeeding. Attempt setup cancellation before that traversal too.
        let setup = self.setups.cancel_run(&run.thread_id, run_id);
        for child in self.store.child_threads(&run.thread_id)? {
            if let Some(active) = self.store.thread(&child)?.active_run {
                self.note_stopped_with_parent(&active.id);
                let _ = self.cancel(&active.id);
            }
        }
        setup?;
        if cancellation.is_some() || self.store.run(run_id)?.state == RunState::Cancelled {
            Ok(())
        } else {
            Err(CoreError::StaleOwner)
        }
    }

    /// Ends a thread's run without waiting for its engine, whose process tree is
    /// killed, with those of its delegated threads. If a stage before or after the
    /// engine is what hangs, the run is closed anyway, and fencing keeps anything it
    /// does later out of the log. Nothing running is not a failure, so a second click
    /// changes nothing.
    pub async fn force_stop(&self, thread_id: &str, actor: &str) -> Result<ForcedStop> {
        let thread = self.store.thread(thread_id)?;
        let Some(active) = thread.active_run else {
            return Ok(ForcedStop {
                run_id: None,
                state: thread.last_run_state,
            });
        };
        self.store.append_audit(&AuditRecord {
            actor: actor.to_owned(),
            action: "run.force_stop".into(),
            target: active.id.clone(),
            decision: None,
            source: "user".into(),
            thread_id: Some(thread_id.to_owned()),
            run_id: Some(active.id.clone()),
            detail: Value::Null,
        })?;
        match self.store.request_stop(&active.id) {
            Ok(()) => {}
            Err(CoreError::RunClosed) => {
                return Ok(ForcedStop {
                    run_id: Some(active.id.clone()),
                    state: Some(self.store.run(&active.id)?.state),
                });
            }
            Err(error) => return Err(error),
        }
        if let Some(run) = self.lock().get(&active.id) {
            run.stops.force.cancel();
            run.stops.cancel.cancel();
        }
        let setup = self.setups.cancel_run(thread_id, &active.id);
        for child in self.store.child_threads(thread_id)? {
            if let Some(running) = self.store.thread(&child)?.active_run {
                self.note_stopped_with_parent(&running.id);
                Box::pin(self.force_stop(&child, actor)).await?;
            }
        }
        setup?;
        let deadline = tokio::time::Instant::now() + FORCE_WAIT;
        while tokio::time::Instant::now() < deadline {
            if self.store.run(&active.id)?.state.is_terminal() {
                break;
            }
            tokio::time::sleep(FORCE_POLL).await;
        }
        if !self.store.run(&active.id)?.state.is_terminal() {
            tracing::warn!(run = %active.id, "closing a run that did not stop");
            let _ = self.close_run(&active.id, RunState::Cancelled, None, None);
            self.mcp.revoke_run(&active.id);
            if let Err(error) = self
                .questions
                .expire_run(thread_id, &active.id, "system:cancel")
            {
                tracing::warn!(%error, "could not expire questions on a forced stop");
            }
        }
        if let Err(error) = self.store.note_thread_event(
            thread_id,
            "run.force_stopped",
            json!({ "runId": active.id }),
        ) {
            tracing::debug!(%error, "could not note a forced stop");
        }
        Ok(ForcedStop {
            state: Some(self.store.run(&active.id)?.state),
            run_id: Some(active.id),
        })
    }

    /// Stops every run and waits for their cleanup.
    pub async fn shutdown(&self) {
        self.engines.end_warm();
        let tasks: Vec<JoinHandle<()>> = {
            let mut active = self.lock();
            active
                .values_mut()
                .filter_map(|run| {
                    run.stops.cancel.cancel();
                    run.task.take()
                })
                .collect()
        };
        for task in tasks {
            let _ = tokio::time::timeout(STOP_TIMEOUT * 2, task).await;
        }
    }

    pub fn active_count(&self) -> usize {
        self.lock().len()
    }

    pub(super) fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, ActiveRun>> {
        self.active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
