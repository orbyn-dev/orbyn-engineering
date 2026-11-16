use super::*;

impl RunManager {
    pub(super) async fn execute(
        self: Arc<Self>,
        thread: Thread,
        run_id: String,
        prompt: String,
        files: Vec<PathBuf>,
        write_scope: WriteScope,
        stops: Stops,
    ) {
        let lifecycle = thread.lifecycle.clone();
        let driver = self.engines.driver(&thread.engine).ok();
        let saved = lifecycle
            .as_ref()
            .map_or(PermissionMode::Supervised, |l| l.permission_mode);
        let scope = RunScope {
            run_id: run_id.clone(),
            thread_id: thread.id.clone(),
            engine: thread.engine.clone(),
            family: driver
                .as_ref()
                .map_or_else(|| thread.engine.clone(), |driver| driver.family()),
            write_scope: Some(write_scope),
            // A mode this engine does not offer runs Supervised, never looser.
            mode: driver
                .as_ref()
                .map_or(PermissionMode::Supervised, |driver| {
                    crate::agents::permission_modes::effective(
                        &driver.descriptor().capabilities.permission_modes,
                        saved,
                    )
                }),
            plan: lifecycle.is_some_and(|l| l.plan_mode),
        };
        let run_dir = self.runs_dir.join(&run_id);
        let mut token = None;
        let outcome = {
            let drive = async {
                match self
                    .setups
                    .before_prepared_turn(&thread.id, &run_id, &stops.cancel)
                    .await
                {
                    Ok(true) => {}
                    Ok(false) => return EngineOutcome::Cancelled,
                    Err(error) => return EngineOutcome::Failed(error.public_message()),
                }
                self.title_new_thread(&thread, &prompt);
                self.drive(
                    &thread,
                    &scope,
                    (prompt, files),
                    &run_dir,
                    &mut token,
                    &stops,
                )
                .await
            };
            // The stream below answers a forced stop itself. This is for a stage
            // around it that hangs, such as a snapshot on a stuck disk.
            let forced = async {
                stops.force.cancelled().await;
                tokio::time::sleep(FORCE_GRACE).await;
            };
            tokio::select! {
                outcome = drive => outcome,
                () = forced => EngineOutcome::Cancelled,
            }
        };
        if let Some(token) = token {
            self.mcp.revoke(&token);
        }
        let end = self.checkpoint_end(&scope, &thread, &run_dir);
        tokio::select! {
            () = end => {}
            () = async {
                stops.force.cancelled().await;
                tokio::time::sleep(FORCE_GRACE).await;
            } => {}
        }
        if let Err(error) = std::fs::remove_dir_all(&run_dir)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(%error, "could not remove a run directory");
        }
        let mut limit = None;
        let mut sign_in_note = None;
        let (state, message) = match outcome {
            EngineOutcome::Completed => (RunState::Completed, None),
            EngineOutcome::Cancelled => (RunState::Cancelled, None),
            EngineOutcome::Failed(message) => {
                let sign_in = self.engines.driver(&scope.engine).ok();
                let sign_in = sign_in.as_ref().and_then(|driver| driver.sign_in_command());
                let sign_in = sign_in.as_deref();
                if diagnostics::is_sign_in_failure(&message) {
                    sign_in_note = Some(json!({ "command": sign_in }));
                }
                if let Some(resets_at) = self.limit_of(&scope.run_id, &message) {
                    limit = Some((message.clone(), resets_at));
                }
                let message = diagnostics::failure_message(&message, sign_in);
                (RunState::Failed, Some(message))
            }
            EngineOutcome::SessionLost => (
                RunState::Failed,
                Some("The engine lost this conversation.".to_owned()),
            ),
        };
        self.limit_signals().remove(&run_id);
        let closed = self.close_run(&run_id, state, message.as_deref(), sign_in_note.as_ref());
        let completed = closed
            .as_ref()
            .is_some_and(|run| run.state == RunState::Completed);
        if closed.is_some_and(|run| run.state == RunState::Failed)
            && let Some((message, resets_at)) = limit
        {
            self.record_limit(&scope.thread_id, message, resets_at);
        }
        if completed {
            tokio::spawn(self.warm_next(scope.thread_id.clone(), run_id));
        }
    }

    /// Starts the process `thread`'s next turn would launch, so that turn need not
    /// wait for it. It is adopted only if every launch input still matches, and only
    /// by the run right after `after_run`; otherwise it is ended unused.
    async fn warm_next(self: Arc<Self>, thread_id: String, after_run: String) {
        if let Err(error) = self.try_warm_next(&thread_id, after_run).await {
            tracing::debug!(%error, "could not start the next turn's process early");
        }
    }

    async fn try_warm_next(self: &Arc<Self>, thread_id: &str, after_run: String) -> Result<()> {
        // Off unless asked for: Claude runs SessionStart hooks and starts MCP servers
        // as soon as it launches, so a process never adopted would run them for nothing.
        let wanted =
            crate::settings::preferences::stored_preference(&self.store, "warm_next_turn", None)?;
        if wanted != json!(true) {
            return Ok(());
        }
        // A crew worker's process is started under a restriction, which an early start lacks.
        if self.store.crew_task_for_worker(thread_id)?.is_some() {
            return Ok(());
        }
        let thread = self.store.thread(thread_id)?;
        let driver = self.engines.driver(&thread.engine)?;
        let lifecycle = thread.lifecycle.as_ref();
        if !driver.can_warm()
            || thread.active_run.is_some()
            || lifecycle.is_some_and(|l| l.archived_at.is_some())
        {
            return Ok(());
        }
        let descriptor = driver.descriptor();
        if !descriptor.capabilities.mcp {
            return Ok(());
        }
        let root = self.workspace.root()?;
        let checkout = if Path::new(&thread.cwd) == root {
            root.clone()
        } else {
            check_target(&root, Path::new(&thread.cwd)).await?
        };
        let warm_start = engine_start(
            driver.as_ref(),
            descriptor.tier == EngineTier::Native,
            &root,
            &checkout,
            self.store.engine_session(&thread.id)?.as_deref(),
        );
        let route = self.skills.route(
            &descriptor.capabilities.skill_delivery,
            thread.skill_profile.as_deref(),
        )?;
        let saved = lifecycle.map_or(PermissionMode::Supervised, |l| l.permission_mode);
        let mode = crate::agents::permission_modes::effective(
            &descriptor.capabilities.permission_modes,
            saved,
        );
        let plan = lifecycle.is_some_and(|l| l.plan_mode);
        let scope = RunScope {
            run_id: format!("warm-{}", new_id()),
            thread_id: thread.id.clone(),
            engine: thread.engine.clone(),
            family: driver.family(),
            // The same limit the thread's runs get, while this token is live unadopted.
            write_scope: Some(write_scope(&root, Path::new(&thread.cwd), warm_start.1)),
            mode,
            plan,
        };
        let token = self.mcp.register(McpBinding {
            scope,
            permission_prompt: driver.family() == "claude",
            skills: route.selection.clone(),
        })?;
        let url = format!("{}/mcp/{token}", self.base_url);
        let chosen = selection::of_thread(&thread);
        let spec = RunSpec {
            run_id: String::new(),
            thread_id: thread.id.clone(),
            cwd: warm_start.0,
            library: root,
            checkout,
            prompt: String::new(),
            user_prompt: String::new(),
            context: String::new(),
            kind: RunKind::Turn,
            mode,
            plan,
            model: chosen.model,
            effort: chosen.effort,
            options: chosen.options,
            resume: self.store.engine_session(&thread.id)?,
            mcp_url: Some(url.clone()),
            // Under the runs folder, so a confined worker cannot read the token file the
            // process writes there, and a start-up sweep removes it if the core dies.
            run_dir: self
                .runs_dir
                .join(format!("warm-{}", crate::util::new_id())),
            confine: None,
        };
        // No update may begin while this process starts and is parked: it would hold
        // the binary being replaced, which Windows refuses.
        let Some(_starting) = self.engines.begin_warm() else {
            self.mcp.revoke(&token);
            return Ok(());
        };
        // Starting fires the checkout's SessionStart hooks and `.mcp.json`, so trust is
        // read again here, after the awaits above, rather than relied on from the top.
        if !self.store.project(&thread.project_id)?.trusted {
            self.mcp.revoke(&token);
            return Ok(());
        }
        let started = driver.start_warm(spec).await;
        let process = match started {
            Ok(Some(process)) => process,
            other => {
                self.mcp.revoke(&token);
                return other.map(|_| ());
            }
        };
        let mcp = self.mcp.clone();
        let revoked = token.clone();
        let entry = crate::agents::engines::warm::WarmEntry::new(
            thread.engine.clone(),
            after_run,
            (token, url),
            process,
            Box::new(move || mcp.revoke(&revoked)),
        );
        let id = entry.id.clone();
        // A run may have been admitted meanwhile; it would not have seen this one.
        if self.store.thread(thread_id)?.active_run.is_some() {
            return Ok(());
        }
        if self.engines.park_warm(thread_id, entry) {
            let engines = self.engines.clone();
            let thread_id = thread_id.to_owned();
            tokio::spawn(async move {
                tokio::time::sleep(WARM_IDLE).await;
                engines.expire_warm(&thread_id, &id);
            });
        }
        Ok(())
    }

    /// A limit the engine signalled, or one its failure text names.
    fn limit_of(&self, run_id: &str, message: &str) -> Option<Option<i64>> {
        let signalled = self.limit_signals().remove(run_id);
        signalled.or_else(|| diagnostics::is_limit_failure(message).then_some(None))
    }

    pub(super) fn limit_signals(&self) -> std::sync::MutexGuard<'_, HashMap<String, Option<i64>>> {
        self.limit_signals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn record_limit(&self, thread_id: &str, message: String, resets_at: Option<i64>) {
        let enabled = |key: &str| {
            crate::settings::preferences::stored_preference(&self.store, key, None)
                .is_ok_and(|on| on == json!(true))
        };
        let stop = LimitStop {
            message,
            resets_at: resets_at.and_then(crate::conversation::automations::rfc3339),
            resume: enabled("auto_resume_limited_threads"),
            snooze: enabled("snooze_limited_threads"),
        };
        if let Err(error) = self.store.stop_on_limit(thread_id, &stop) {
            tracing::warn!(%error, "could not record a limit stop");
        }
    }
}
