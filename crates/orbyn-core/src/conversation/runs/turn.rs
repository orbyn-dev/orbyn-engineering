use super::*;

impl RunManager {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn attempt(
        &self,
        thread: &Thread,
        scope: &RunScope,
        message: (String, Vec<PathBuf>),
        run_dir: &Path,
        token: &mut Option<String>,
        stops: &Stops,
        retry: bool,
    ) -> EngineOutcome {
        match self
            .start(thread, scope, message, run_dir, token, &stops.cancel, retry)
            .await
        {
            Ok(Some((handle, events))) => self.pump(thread, scope, handle, events, stops).await,
            Ok(None) => EngineOutcome::Cancelled,
            Err(CoreError::RunClosed | CoreError::StaleOwner) => EngineOutcome::Cancelled,
            Err(CoreError::SessionLost) => EngineOutcome::SessionLost,
            Err(error) => EngineOutcome::Failed(error.public_message()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn start(
        &self,
        thread: &Thread,
        scope: &RunScope,
        (prompt, files): (String, Vec<PathBuf>),
        run_dir: &Path,
        token: &mut Option<String>,
        cancel: &CancellationToken,
        retry: bool,
    ) -> Result<Option<(RunHandle, mpsc::Receiver<EngineEvent>)>> {
        if !retry {
            self.store.mark_starting(&scope.run_id)?;
        }
        let driver = self.engines.driver(&thread.engine)?;
        let descriptor = driver.descriptor();
        let root = self.workspace.root()?;
        let library_thread = Path::new(&thread.cwd) == root;
        let checkout = if library_thread {
            root.clone()
        } else {
            check_target(&root, Path::new(&thread.cwd)).await?
        };
        let route = self.skills.route(
            &descriptor.capabilities.skill_delivery,
            thread.skill_profile.as_deref(),
        )?;
        std::fs::create_dir_all(run_dir)?;
        if !retry {
            self.checkpoint_start(scope, &checkout, run_dir).await;
        }
        if let Some(token) = token.take() {
            self.mcp.revoke(&token);
        }
        // Only the turn right after the one that started it may take a warm process.
        let previous = self.store.previous_run(&thread.id, &scope.run_id)?;
        let mut warm = self.engines.take_warm(&thread.id).filter(|entry| {
            entry.engine == thread.engine && previous.as_deref() == Some(entry.after_run.as_str())
        });
        let mcp_url = if descriptor.capabilities.mcp {
            let binding = McpBinding {
                scope: scope.clone(),
                permission_prompt: driver.family() == "claude",
                skills: route.selection.clone(),
            };
            // A warm process already holds its token, which now answers for this run.
            let reused = warm
                .as_ref()
                .filter(|entry| self.mcp.rebind(&entry.token, binding.clone()))
                .map(|entry| (entry.token.clone(), entry.url.clone()));
            let (issued, url) = match reused {
                Some(pair) => pair,
                None => {
                    warm = None;
                    let issued = self.mcp.register(binding)?;
                    let url = format!("{}/mcp/{issued}", self.base_url);
                    (issued, url)
                }
            };
            *token = Some(issued);
            Some(url)
        } else {
            warm = None;
            None
        };
        // From here the run's token is its own, whether or not the process is adopted.
        let warm = warm.map(crate::agents::engines::warm::WarmEntry::adopt);
        if cancel.is_cancelled() {
            return Ok(None);
        }
        let (revision, dirty) = self.workspace.identity().await?;
        let handoff = self.store.run_handoff(&scope.run_id)?;
        let resume = self.store.run_engine_session(&scope.run_id)?;
        let native = descriptor.tier == EngineTier::Native;
        let (mut start, mut at_checkout) =
            engine_start(driver.as_ref(), native, &root, &checkout, resume.as_deref());
        // The write scope was fixed at admission from the start folder chosen then. A run
        // starts where that scope says its relative paths land, so the two cannot
        // disagree if the session record changed in between.
        if driver.start_dir(&root, &checkout, None).is_some()
            && let Some(engine_cwd) = scope.write_scope.as_ref().map(|s| s.engine_cwd.clone())
        {
            at_checkout = engine_cwd == checkout;
            start = engine_cwd;
        }
        let confine =
            self.confinement_for(thread, driver.as_ref(), &root, &checkout, &start, run_dir)?;
        // A process started ahead of this turn was not started under the restriction.
        let warm = if confine.is_some() { None } else { warm };
        let mut context = run_context(&RunContext {
            library: &root,
            revision: &revision,
            dirty,
            target_root: &target_root(&checkout).await,
            checkout: &checkout,
            branch: branch_name(&checkout).await.as_deref(),
            checkout_revision: checkout_revision(&checkout).await.as_deref(),
            runs_at_library: !at_checkout,
            library_thread,
            route: &route,
            skill_tools: descriptor.capabilities.mcp,
            plan: scope.plan,
            files: &files,
            handoff: handoff.as_deref(),
        });
        let kind = RunKind::parse(&prompt);
        // An app's note rides with the next ordinary turn only, and the timeline
        // records exactly what was sent before the engine sees it.
        if kind == RunKind::Turn
            && let Some(note) = self.take_app_note(&thread.id)
        {
            context.push_str(&note.context_block());
            let sent = AgUiEvent::Custom {
                name: "mcp_app.context".into(),
                value: serde_json::json!({ "server": note.server, "text": note.text }),
            };
            self.notes_sent
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(scope.run_id.clone(), sent);
        }
        let chosen = self
            .selection_for_run(thread, driver.as_ref(), &scope.run_id)
            .await?;
        match &kind {
            RunKind::Goal(GoalAction::Set(objective)) => self.store.set_goal(
                &thread.id,
                Some(&ThreadGoal {
                    objective: objective.clone(),
                    status: "active".into(),
                    tokens_used: None,
                    token_budget: None,
                    seconds_used: None,
                }),
            )?,
            RunKind::Goal(GoalAction::Clear) => self.store.set_goal(&thread.id, None)?,
            _ => {}
        }
        let spec = RunSpec {
            run_id: scope.run_id.clone(),
            thread_id: thread.id.clone(),
            cwd: start,
            library: root,
            checkout,
            prompt: format!("{context}\n\n{prompt}"),
            context,
            kind,
            mode: scope.mode,
            plan: scope.plan,
            user_prompt: prompt,
            model: chosen.model.clone(),
            effort: chosen.effort.clone(),
            options: chosen.options.clone(),
            resume,
            mcp_url,
            run_dir: run_dir.to_path_buf(),
            confine,
        };
        let (sender, receiver) = mpsc::channel(EVENT_BUFFER);
        let mut warm = warm;
        let key = warm.as_ref().and(driver.warm_key(&spec));
        let matches = warm.as_mut().is_some_and(|process| {
            key.as_deref() == Some(process.key.as_str()) && process.is_alive()
        });
        let handle = match warm {
            Some(process) if matches => {
                self.store.note_thread_event(
                    &thread.id,
                    "engine.warm",
                    json!({ "adopted": true }),
                )?;
                driver.adopt_run(spec, sender, process).await?
            }
            unused => {
                drop(unused);
                driver.start_run(spec, sender).await?
            }
        };
        self.store
            .note_thread_event(&thread.id, "engine.selection", json!(chosen))?;
        if let Some(active) = self.lock().get_mut(&scope.run_id) {
            active.input.clone_from(&handle.input);
            active.control.clone_from(&handle.control);
        }
        if let Err(error) = self.store.mark_running(&scope.run_id) {
            handle.cancel.cancel();
            handle.task.abort();
            return Err(error);
        }
        Ok(Some((handle, receiver)))
    }

    /// The selection this run goes out with, checked against the engine's current
    /// list: admission may have had none to hand, and a queued message can outlive
    /// a model. Nothing is asked of the engine when there is nothing to check.
    async fn selection_for_run(
        &self,
        thread: &Thread,
        driver: &dyn EngineDriver,
        run: &str,
    ) -> Result<ModelSelection> {
        let chosen = selection::of_thread(thread);
        let keeps = driver.keeps_turn_overrides();
        if !keeps && selection::is_empty(&chosen) {
            return Ok(chosen);
        }
        let catalog = self.engines.models(&thread.engine, false).await?;
        let chosen = if self.store.is_preparation_retry(run)? {
            crate::project::project_setup::check_captured_selection(
                &self.store,
                &thread.engine,
                &chosen,
                &catalog,
            )?;
            chosen
        } else {
            self.fall_back_if_gone(thread, chosen, &catalog)?
        };
        selection::check(&catalog, &chosen)?;
        Ok(if keeps {
            selection::with_defaults(&catalog, &chosen)
        } else {
            chosen
        })
    }

    /// Runs on `fallback_model` when the engine's own list no longer has the thread's
    /// model. A custom model is on no list, and a stand-in list proves nothing, so
    /// neither counts. The thread keeps its choice for when the model comes back, and
    /// effort and options belonged to the model that is gone.
    fn fall_back_if_gone(
        &self,
        thread: &Thread,
        chosen: ModelSelection,
        catalog: &orbyn_contracts::EngineModels,
    ) -> Result<ModelSelection> {
        let Some(model) = chosen.model.clone() else {
            return Ok(chosen);
        };
        let fallback = crate::settings::preferences::stored_preference(
            &self.store,
            "fallback_model",
            Some(&thread.project_id),
        )?;
        let fallback = fallback.as_str().unwrap_or_default();
        let listed = |id: &str| catalog.models.iter().any(|known| known.id == id);
        let trusted = !catalog.fallback && catalog.error.is_none();
        if fallback.is_empty() || !trusted || listed(&model) || !listed(fallback) {
            return Ok(chosen);
        }
        let customs =
            crate::settings::preferences::stored_preference(&self.store, "custom_models", None)?;
        let custom = customs.as_array().is_some_and(|list| {
            list.iter().any(|item| {
                item["engine"] == thread.engine.as_str() && item["id"] == model.as_str()
            })
        });
        if custom {
            return Ok(chosen);
        }
        self.store.note_thread_event(
            &thread.id,
            "engine.warning",
            json!({ "message": format!("{model} is not available, so this turn runs on {fallback}.") }),
        )?;
        Ok(ModelSelection {
            model: Some(fallback.to_owned()),
            effort: None,
            options: std::collections::BTreeMap::new(),
        })
    }

    /// Commits engine events in order until the engine finishes or the run is
    /// cancelled, then brings the engine down before reporting the outcome.
    async fn pump(
        &self,
        thread: &Thread,
        scope: &RunScope,
        handle: RunHandle,
        mut events: mpsc::Receiver<EngineEvent>,
        stops: &Stops,
    ) -> EngineOutcome {
        let cancel = &stops.cancel;
        let mut cancelling = false;
        let whole = crate::settings::preferences::stored_preference(
            &self.store,
            "response_streaming_mode",
            Some(&thread.project_id),
        )
        .is_ok_and(|mode| mode == "turn");
        let mut shaper = EventShaper::new(whole);
        // A note taken for this turn is on the timeline before anything the engine says.
        let sent = self
            .notes_sent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&scope.run_id);
        if let Some(sent) = sent
            && let Some(outcome) = self.write_events(scope, vec![sent])
        {
            tracing::debug!(?outcome, "could not record an app note");
        }
        let stop_deadline = tokio::time::sleep(Duration::MAX / 4);
        tokio::pin!(stop_deadline);
        // Only a delegated run, which nobody is watching, is stopped for going quiet.
        let delegated = thread
            .lifecycle
            .as_ref()
            .is_some_and(|l| l.relation.as_deref() == Some("subagent"));
        let silence = tokio::time::sleep(SILENCE_LIMIT);
        tokio::pin!(silence);
        let outcome = loop {
            let text_due = shaper.due();
            tokio::select! {
                biased;
                () = stops.force.cancelled() => {
                    cancelling = true;
                    break EngineOutcome::Cancelled;
                }
                () = cancel.cancelled(), if !cancelling => {
                    cancelling = true;
                    stop_deadline
                        .as_mut()
                        .reset(tokio::time::Instant::now() + STOP_TIMEOUT);
                    // A blocked engine must hear its denial before it can stop.
                    if let Err(error) = self.gate.expire_run(&thread.id, &scope.run_id, "system:cancel") {
                        tracing::warn!(%error, "could not expire approvals on cancel");
                    }
                    if let Err(error) = self.questions.expire_run(&thread.id, &scope.run_id, "system:cancel") {
                        tracing::warn!(%error, "could not expire questions on cancel");
                    }
                    handle.cancel.cancel();
                }
                () = &mut stop_deadline, if cancelling => break EngineOutcome::Cancelled,
                () = &mut silence, if delegated && !cancelling => {
                    // Waiting on a person is not silence.
                    // A turn held open for a quiet workflow is waiting, not stuck.
                    let waiting = self.store.run(&scope.run_id).is_ok_and(|run| {
                        run.state == RunState::AwaitingApproval
                    }) || self.workflow_running(&thread.id, &scope.run_id);
                    silence.as_mut().reset(tokio::time::Instant::now() + SILENCE_LIMIT);
                    if !waiting {
                        break EngineOutcome::Failed(
                            "This delegated run stopped responding, so it was ended.".into(),
                        );
                    }
                }
                () = sleep_until_due(text_due), if text_due.is_some() => {
                    if let Some(outcome) = self.write_events(scope, shaper.flush()) {
                        break outcome;
                    }
                }
                event = events.recv() => {
                    silence.as_mut().reset(tokio::time::Instant::now() + SILENCE_LIMIT);
                    let Some(event) = event else {
                        break if cancelling {
                            EngineOutcome::Cancelled
                        } else {
                            EngineOutcome::Failed("The engine stopped without finishing.".into())
                        };
                    };
                    // Anything else keeps its place after the text before it.
                    if !matches!(event, EngineEvent::Agui(_))
                        && let Some(outcome) = self.write_events(scope, shaper.flush())
                    {
                        break outcome;
                    }
                    match event {
                        EngineEvent::Agui(event) => {
                            if let Some(outcome) = self.write_events(scope, shaper.push(event)) {
                                break outcome;
                            }
                        }
                        EngineEvent::SteerFailed(prompt) => {
                            self.requeue_steer(scope, &prompt);
                        }
                        EngineEvent::Limited { resets_at } => {
                            self.limit_signals().insert(scope.run_id.clone(), resets_at);
                        }
                        EngineEvent::Session(session) => {
                            if let Err(error) = self.store.set_engine_session(&scope.run_id, &session) {
                                tracing::warn!(%error, "could not store the engine session");
                            }
                        }
                        EngineEvent::Turn(turn) => {
                            if let Err(error) = self.store.set_engine_turn(&scope.run_id, &turn) {
                                tracing::debug!(%error, "could not keep the engine turn");
                            }
                        }
                        EngineEvent::Item(item) => {
                            if let Err(error) = self.store.record_item(&scope.run_id, &item) {
                                tracing::debug!(%error, "could not keep an engine item");
                            }
                            // A guarded Crew command the engine reports finished gets its exit status in the ledger.
                            if item.exit_code.is_some()
                                && let Some(core) = self.crew.get().and_then(std::sync::Weak::upgrade)
                            {
                                core.guard_exit_seen(&scope.thread_id, &item);
                            }
                        }
                        EngineEvent::Files(paths) => {
                            crate::conversation::file_changes::finished(&self.store, scope, &paths).await;
                        }
                        EngineEvent::Usage(usage) => {
                            if let Err(error) = self.store.record_usage(&scope.run_id, &usage) {
                                tracing::debug!(%error, "could not keep usage");
                            }
                        }
                        EngineEvent::Commands(commands) => {
                            if let Err(error) = self.store.set_engine_commands(&thread.engine, &commands) {
                                tracing::debug!(%error, "could not keep engine commands");
                            }
                        }
                        EngineEvent::Goal(goal) => {
                            if let Err(error) = self.store.set_goal(&thread.id, goal.as_ref()) {
                                tracing::debug!(%error, "could not keep the goal");
                            }
                        }
                        EngineEvent::Question(ask) => {
                            let desk = self.questions.clone();
                            let run_id = scope.run_id.clone();
                            tokio::spawn(async move {
                                let _ = ask.reply.send(desk.ask(&run_id, ask.questions).await);
                            });
                        }
                        EngineEvent::Permission(request) => {
                            self.answer(scope, request);
                        }
                        EngineEvent::McpApp(app) => {
                            if let Some(marker) = self.store_app(&thread.id, &app)
                                && let Some(outcome) = self.write_events(scope, shaper.push(marker))
                            {
                                break outcome;
                            }
                        }
                        EngineEvent::Finished(outcome) => break outcome,
                    }
                }
            }
        };
        let _ = self.write_events(scope, shaper.flush());
        // Blocked engines hear a denial even when the engine ended on its own.
        if let Err(error) = self
            .gate
            .expire_run(&thread.id, &scope.run_id, "system:run_end")
        {
            tracing::warn!(%error, "could not expire approvals at run end");
        }
        if let Err(error) = self
            .questions
            .expire_run(&thread.id, &scope.run_id, "system:run_end")
        {
            tracing::warn!(%error, "could not expire questions at run end");
        }
        drop(events);
        handle.cancel.cancel();
        let mut task = handle.task;
        if stops.force.is_cancelled() {
            // Dropping the task's process handle kills the tree.
            task.abort();
            let _ = tokio::time::timeout(FORCE_GRACE, &mut task).await;
        } else if tokio::time::timeout(STOP_TIMEOUT, &mut task).await.is_err() {
            task.abort();
            let _ = task.await;
        }
        if cancelling {
            EngineOutcome::Cancelled
        } else {
            outcome
        }
    }

    /// Writes events in order. Returns how the run ends if it can no longer write.
    fn write_events(&self, scope: &RunScope, events: Vec<AgUiEvent>) -> Option<EngineOutcome> {
        for event in events {
            match self.store.append_run_event(&scope.run_id, &event) {
                Ok(_) => {}
                Err(CoreError::RunClosed | CoreError::StaleOwner) => {
                    return Some(EngineOutcome::Cancelled);
                }
                Err(error) => return Some(EngineOutcome::Failed(error.public_message())),
            }
        }
        None
    }

    /// Runs the gate on its own task so a long wait does not stall the event log.
    fn answer(&self, scope: &RunScope, request: PermissionRequest) {
        let gate = self.gate.clone();
        let scope = scope.clone();
        tokio::spawn(async move {
            let PermissionRequest {
                kind,
                target,
                summary,
                detail,
                paths,
                reply,
            } = request;
            let ask = ActionAsk {
                kind,
                target,
                summary,
                detail,
                paths,
            };
            let verdict = gate.request(&scope, ask).await;
            let _ = reply.send(verdict);
        });
    }
}
