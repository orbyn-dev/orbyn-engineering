use super::*;

impl RunManager {
    /// Admits a run and starts it. The `202` reply and the first events can
    /// reach a client in either order; both are consistent with the log.
    pub fn admit(self: &Arc<Self>, thread_id: &str, prompt: &str, actor: &str) -> Result<Run> {
        self.admit_with(thread_id, prompt, &[], actor)
    }

    pub fn admit_with(
        self: &Arc<Self>,
        thread_id: &str,
        prompt: &str,
        attachments: &[String],
        actor: &str,
    ) -> Result<Run> {
        check_prompt(prompt)?;
        let thread = self.store.thread(thread_id)?;
        self.check_goal_action(&thread, prompt)?;
        self.attachment_files(thread_id, attachments)?;
        self.engines
            .check_selection(&thread.engine, &selection::of_thread(&thread))?;
        let root = self.workspace.root()?;
        let run = self.workspace.admit_with(|| {
            self.check_console(thread_id)?;
            self.engines.admit_with(&thread.engine, || {
                self.store.admit_prepared_run(
                    Admission {
                        thread_id,
                        prompt,
                        actor,
                    },
                    attachments,
                )
            })
        })?;
        self.launch(thread, run.clone(), prompt.to_owned(), attachments, &root)?;
        Ok(run)
    }

    /// Retry only the immutable input and project action captured for this run.
    pub(crate) fn retry_preparation(
        self: &Arc<Self>,
        thread_id: &str,
        failed_run: &str,
        actor: &str,
        capture: &crate::project::project_setup::PreparationCapture,
        spawn: impl FnOnce(&Thread, &orbyn_contracts::ProjectScript) -> Result<()>,
    ) -> Result<Run> {
        let mut thread = self.store.thread(thread_id)?;
        thread.engine.clone_from(&capture.engine);
        thread.model.clone_from(&capture.selection.model);
        thread.effort.clone_from(&capture.selection.effort);
        thread.options.clone_from(&capture.selection.options);
        self.check_goal_action(&thread, &capture.prompt)?;
        self.engines
            .check_selection(&thread.engine, &selection::of_thread(&thread))?;
        let root = self.workspace.root()?;
        let retry = self.workspace.admit_with(|| {
            self.check_console(thread_id)?;
            self.engines.admit_with(&capture.engine, || {
                self.engines.check_enabled(&capture.engine)?;
                self.engines
                    .driver(&capture.engine)?
                    .check_launch_available()?;
                self.engines
                    .check_selection(&capture.engine, &capture.selection)?;
                self.store
                    .retry_preparation(thread_id, failed_run, actor, capture)
            })
        })?;
        let run = retry.run;
        let thread = retry.thread;
        // Register cancellation before any project process can be created. The
        // setup manager's launch/cancel lock fences a stop that wins this race.
        if let Err(error) = self.launch(
            thread.clone(),
            run.clone(),
            retry.prompt,
            &retry.attachments,
            &root,
        ) {
            self.setups.fail_preparation(thread_id, &run.id);
            let _ = self.close_run(
                &run.id,
                RunState::Failed,
                Some(&error.public_message()),
                None,
            );
            return Err(error);
        }
        if let Err(error) = spawn(&thread, &retry.script) {
            self.setups.fail_preparation(thread_id, &run.id);
            tracing::warn!(%error, "could not retry project preparation");
        }
        Ok(run)
    }

    /// Pausing and resuming a goal is the engine's own feature, so an engine
    /// without it refuses here rather than reading the words as a prompt.
    fn check_goal_action(&self, thread: &Thread, prompt: &str) -> Result<()> {
        self.engines.check_enabled(&thread.engine)?;
        let driver = self.engines.driver(&thread.engine)?;
        let kind = RunKind::parse(prompt);
        // These reach the engine bare, so earlier conversation has no way along.
        if kind != RunKind::Turn && self.store.pending_handoff(&thread.id)?.is_some() {
            return Err(CoreError::Unsupported(
                "Send a message first: this thread's earlier conversation has not reached \
                 the engine yet."
                    .into(),
            ));
        }
        let held = matches!(kind, RunKind::Goal(GoalAction::Pause | GoalAction::Resume));
        if held && !driver.descriptor().capabilities.goal_control {
            return Err(CoreError::Unsupported(
                "This engine cannot pause or resume a goal.".into(),
            ));
        }
        Ok(())
    }

    /// The files a message attaches, with the records the transcript shows.
    fn attachment_files(
        &self,
        thread_id: &str,
        ids: &[String],
    ) -> Result<(Vec<PathBuf>, Vec<Attachment>)> {
        let stored = self.store.list_attachments(thread_id)?;
        let mut files = Vec::with_capacity(ids.len());
        let mut sent = Vec::with_capacity(ids.len());
        for id in ids {
            let (attachment, path) = stored
                .iter()
                .find(|(attachment, _)| attachment.id == *id)
                .ok_or_else(|| CoreError::Invalid("That attachment is not ready.".into()))?;
            files.push(PathBuf::from(path));
            sent.push(attachment.clone());
        }
        Ok((files, sent))
    }

    fn launch(
        self: &Arc<Self>,
        thread: Thread,
        run: Run,
        prompt: String,
        attachments: &[String],
        root: &Path,
    ) -> Result<()> {
        let (files, sent) = self.attachment_files(&thread.id, attachments)?;
        // Written before the engine starts, so a transcript can show what the
        // message carried without asking the engine or the composer.
        if !sent.is_empty() {
            self.store.append_run_event(
                &run.id,
                &AgUiEvent::Custom {
                    name: "user.attachments".into(),
                    value: json!({ "attachments": sent }),
                },
            )?;
        }
        let driver = self.engines.driver(&thread.engine)?;
        let (_, from_checkout) = engine_start(
            driver.as_ref(),
            driver.descriptor().tier == EngineTier::Native,
            root,
            Path::new(&thread.cwd),
            self.store.engine_session(&thread.id)?.as_deref(),
        );
        let scope = write_scope(root, Path::new(&thread.cwd), from_checkout);
        let preparation_retry = self.store.is_preparation_retry(&run.id)?;
        let stops = Stops::default();
        // Registered before the task starts, so a cancel cannot miss it.
        self.lock().insert(
            run.id.clone(),
            ActiveRun {
                stops: stops.clone(),
                task: None,
                input: None,
                control: None,
            },
        );
        if !preparation_retry {
            self.remember_selection(&thread);
        }
        let manager = self.clone();
        let thread_id = thread.id.clone();
        let task = tokio::spawn({
            let run_id = run.id.clone();
            async move {
                let supervised = {
                    let manager = manager.clone();
                    let run_id = run_id.clone();
                    tokio::spawn(async move {
                        manager
                            .execute(thread, run_id, prompt, files, scope, stops)
                            .await;
                    })
                };
                // A panic in the run must not strand it as active.
                if supervised.await.is_err() {
                    let _ = manager.close_run(
                        &run_id,
                        RunState::Failed,
                        Some("The run stopped unexpectedly."),
                        None,
                    );
                }
                manager.lock().remove(&run_id);
                manager.drain(&thread_id);
                manager.report_to_parent(&thread_id, &run_id);
                manager.crew_run_ended(&thread_id, &run_id);
            }
        });
        if let Some(active) = self.lock().get_mut(&run.id) {
            active.task = Some(task);
        }
        Ok(())
    }

    /// A thread's first message names it, in the background, so the run does not
    /// wait for it and a failure leaves the default title and nothing else.
    pub(super) fn title_new_thread(self: &Arc<Self>, thread: &Thread, prompt: &str) {
        let delegated = thread
            .lifecycle
            .as_ref()
            .is_some_and(|l| l.relation.as_deref() == Some("subagent"));
        let wanted = thread.title == titles::NEW_THREAD_TITLE
            && !delegated
            && RunKind::parse(prompt) == RunKind::Turn
            && self
                .store
                .run_count(&thread.id)
                .is_ok_and(|count| count == 1)
            && titles::auto_titles_on(&self.store, &thread.project_id);
        if !wanted {
            return;
        }
        let Ok(driver) = self.engines.driver(&thread.engine) else {
            return;
        };
        let (store, engines) = (self.store.clone(), self.engines.clone());
        let (thread, message) = (thread.clone(), titles::first_message(prompt));
        tokio::spawn(async move {
            let call = TitleCall {
                thread: &thread,
                conversation: &message,
                actor: "system:title",
                source: "system",
            };
            if let Err(error) =
                titles::generate_title(&store, &engines, driver.as_ref(), call).await
            {
                tracing::debug!(%error, "could not title a new thread");
            }
        });
    }

    /// What a person chose carries to new threads on this engine. A delegated
    /// thread's choices are the parent's, not theirs.
    fn remember_selection(&self, thread: &Thread) {
        let relation = thread
            .lifecycle
            .as_ref()
            .and_then(|l| l.relation.as_deref());
        if relation == Some("subagent") {
            return;
        }
        let chosen = selection::of_thread(thread);
        if let Err(error) = self.store.remember_selection(&thread.engine, &chosen) {
            tracing::debug!(%error, "could not remember a selection");
        }
    }

    /// Called under the workspace admission lock, which opening a console also takes.
    fn check_console(&self, thread_id: &str) -> Result<()> {
        if crate::agents::console::console_alive(&self.terminals, thread_id) {
            return Err(CoreError::ConsoleOpen);
        }
        if self
            .rewinding
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(thread_id)
        {
            return Err(CoreError::ThreadBusy);
        }
        Ok(())
    }

    /// Holds a thread for a rewind. Taken under the admission lock, so a run
    /// cannot start between the idle check and the hold.
    pub(crate) fn hold_for_rewind(self: &Arc<Self>, thread_id: &str) -> Result<RewindHold> {
        self.workspace.admit_with(|| {
            if self.store.thread(thread_id)?.active_run.is_some() {
                return Err(CoreError::ThreadBusy);
            }
            let mut rewinding = self
                .rewinding
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !rewinding.insert(thread_id.to_owned()) {
                return Err(CoreError::ThreadBusy);
            }
            Ok(RewindHold {
                runs: self.clone(),
                thread_id: thread_id.to_owned(),
            })
        })
    }

    /// Starts the next queued message if the thread is free. A failure leaves
    /// the message queued, so nothing typed is lost.
    pub fn drain(self: &Arc<Self>, thread_id: &str) {
        if let Err(error) = self.try_drain(thread_id) {
            tracing::warn!(%error, "could not start the next queued message");
        }
    }

    fn try_drain(self: &Arc<Self>, thread_id: &str) -> Result<()> {
        let thread = self.store.thread(thread_id)?;
        let root = self.workspace.root()?;
        let next = self.workspace.admit_with(|| {
            self.check_console(thread_id)?;
            self.engines
                .admit_with(&thread.engine, || self.store.admit_next(thread_id, "user"))
        })?;
        if let Some((run, message)) = next {
            // The message's own selection reached the thread with its admission.
            let thread = self.store.thread(thread_id)?;
            self.launch(thread, run, message.prompt, &message.attachment_ids, &root)?;
        }
        Ok(())
    }

    /// Queues a message and starts it at once if the thread is idle.
    pub fn enqueue(
        self: &Arc<Self>,
        thread_id: &str,
        prompt: &str,
        attachments: &[String],
    ) -> Result<QueuedMessage> {
        self.enqueue_with(thread_id, prompt, attachments, &ModelSelection::default())
    }

    /// `enqueue`, with what the message sets on the thread when it starts. A value
    /// the model does not offer is refused here, so nothing waits in line to fail.
    pub fn enqueue_with(
        self: &Arc<Self>,
        thread_id: &str,
        prompt: &str,
        attachments: &[String],
        wanted: &ModelSelection,
    ) -> Result<QueuedMessage> {
        check_prompt(prompt)?;
        let thread = self.store.thread(thread_id)?;
        if thread
            .lifecycle
            .as_ref()
            .and_then(|l| l.archived_at.as_ref())
            .is_some()
        {
            return Err(CoreError::ThreadArchived);
        }
        self.check_goal_action(&thread, prompt)?;
        self.attachment_files(thread_id, attachments)?;
        let chosen = selection::merged(&selection::of_thread(&thread), wanted);
        self.engines.check_selection(&thread.engine, &chosen)?;
        let message = self.store.enqueue(thread_id, prompt, attachments, wanted)?;
        self.drain(thread_id);
        Ok(message)
    }

    /// Sends one queued message next. A running turn is stopped for it.
    pub fn send_now(self: &Arc<Self>, id: &str) -> Result<()> {
        let message = self.store.mark_urgent(id)?;
        match self.store.thread(&message.thread_id)?.active_run {
            Some(run) => self.cancel(&run.id),
            None => {
                self.drain(&message.thread_id);
                Ok(())
            }
        }
    }

    pub fn resume_queue(self: &Arc<Self>, thread_id: &str) -> Result<()> {
        self.store.thread(thread_id)?;
        self.store.release_queue(thread_id)?;
        self.drain(thread_id);
        Ok(())
    }

    /// Adds text to the running turn, for engines that accept it.
    pub fn steer(self: &Arc<Self>, thread_id: &str, prompt: &str) -> Result<SteerState> {
        check_prompt(prompt)?;
        let run = self
            .store
            .thread(thread_id)?
            .active_run
            .ok_or_else(|| CoreError::Invalid("Nothing is running to steer.".into()))?;
        if matches!(run.state, RunState::Admitted | RunState::Starting) {
            return Err(CoreError::Invalid("This run is still starting.".into()));
        }
        let input = self
            .lock()
            .get(&run.id)
            .and_then(|active| active.input.clone());
        let Some(input) = input else {
            self.enqueue(thread_id, prompt, &[])?;
            return Ok(SteerState::Queued);
        };
        let message_id = format!("{}:{}:prompt", run.id, new_id());
        for event in [
            AgUiEvent::TextMessageStart {
                message_id: message_id.clone(),
                role: "user".into(),
            },
            AgUiEvent::TextMessageContent {
                message_id: message_id.clone(),
                delta: prompt.to_owned(),
            },
            AgUiEvent::TextMessageEnd { message_id },
        ] {
            self.store.append_run_event(&run.id, &event)?;
        }
        input
            .send(prompt.to_owned())
            .map_err(|_| CoreError::RunClosed)?;
        Ok(SteerState::Delivered)
    }

    /// Asks the running engine to stop the workflow one of its tool calls launched.
    /// Acceptance is not the end: the engine reports that as the workflow's state.
    pub fn stop_workflow(&self, thread_id: &str, tool_call_id: &str, actor: &str) -> Result<()> {
        let workflow = self
            .store
            .item(
                thread_id,
                &crate::agents::driver::workflow_item_id(tool_call_id),
            )
            .map_err(|_| CoreError::NotFound("workflow".into()))?;
        if workflow.raw.get("state").and_then(Value::as_str) != Some("running") {
            return Err(CoreError::Invalid(
                "This workflow is no longer running.".into(),
            ));
        }
        let run = self.store.thread(thread_id)?.active_run;
        let Some(run) = run.filter(|run| workflow.run_id.as_deref() == Some(run.id.as_str()))
        else {
            return Err(CoreError::Invalid(
                "This workflow is no longer running.".into(),
            ));
        };
        let control = self
            .lock()
            .get(&run.id)
            .and_then(|active| active.control.clone())
            .ok_or_else(|| CoreError::Invalid("This engine cannot stop a workflow.".into()))?;
        self.store.append_audit(&AuditRecord {
            actor: actor.to_owned(),
            action: "workflow.stop".into(),
            target: tool_call_id.to_owned(),
            decision: None,
            source: "user".into(),
            thread_id: Some(thread_id.to_owned()),
            run_id: Some(run.id.clone()),
            detail: Value::Null,
        })?;
        let sent = control.send(RunControl::StopWorkflow {
            tool_call_id: tool_call_id.to_owned(),
        });
        if sent.is_err() {
            // The audit row came first, as every effect's does; this one records that
            // the stop never reached the engine.
            self.store.append_audit(&AuditRecord {
                actor: actor.to_owned(),
                action: "workflow.stop_failed".into(),
                target: tool_call_id.to_owned(),
                decision: None,
                source: "system".into(),
                thread_id: Some(thread_id.to_owned()),
                run_id: Some(run.id.clone()),
                detail: Value::Null,
            })?;
            return Err(CoreError::RunClosed);
        }
        Ok(())
    }

    /// A workflow this run launched still runs, as its driver last reported.
    pub(super) fn workflow_running(&self, thread_id: &str, run_id: &str) -> bool {
        self.store.run_items(thread_id, run_id).is_ok_and(|items| {
            items.iter().any(|item| {
                item.kind == "workflow"
                    && item.raw.get("state").and_then(Value::as_str) == Some("running")
            })
        })
    }
}
