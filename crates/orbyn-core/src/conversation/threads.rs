//! Thread operations that reach beyond the store: closing terminals on settle,
//! the requested title call and the periodic lifecycle tick.

use orbyn_contracts::{Thread, UpdateThread};

use crate::agents::titles::{TitleCall, generate_title};
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::store::{LifecycleRules, TickReport};
use crate::util::unix_now;

const TITLE_CONTEXT_CHARS: usize = 8_000;
/// Linked pull requests are polled every second lifecycle tick.
const PULL_WATCH_TICKS: u32 = 2;

/// Automatic pull runs every 30 minutes.
const AUTO_PULL_TICKS: u32 = 30;

const LIMIT_CONTINUATION: &str = "Continue where you left off.";
/// Automatic continuations after a restart or a limit reset before a thread waits for you.
const MAX_RESUME_TRIES: i64 = 3;

impl Core {
    pub fn update_thread(&self, id: &str, patch: &UpdateThread, actor: &str) -> Result<Thread> {
        let thread = self.store.update_thread(id, patch, actor)?;
        if patch.settled == Some(true) || patch.archived == Some(true) {
            self.terminals.close(id);
            self.previews.close_thread(id);
        }
        if patch.settled == Some(true) {
            self.run_hook_scripts(id, |script| script.run_on_settle);
        }
        Ok(thread)
    }

    pub fn delete_thread(&self, id: &str, actor: &str) -> Result<()> {
        let files = self.store.family_attachment_paths(id)?;
        for deleted in self.store.delete_thread(id, actor)? {
            self.setups.forget(&deleted);
            self.terminals.close(&deleted);
            self.previews.close_thread(&deleted);
            self.remove_renders(&deleted);
        }
        for file in files {
            let _ = std::fs::remove_file(file);
        }
        Ok(())
    }

    /// Asks the thread's engine for a title again, on request.
    pub async fn regenerate_title(&self, id: &str, actor: &str) -> Result<Thread> {
        let thread = self.store.thread(id)?;
        let conversation = self.store.transcript(id, TITLE_CONTEXT_CHARS)?;
        if conversation.trim().is_empty() {
            return Err(CoreError::Invalid(
                "This thread has no conversation to title yet.".into(),
            ));
        }
        let driver = self.engines.driver(&thread.engine)?;
        generate_title(
            &self.store,
            &self.engines,
            driver.as_ref(),
            TitleCall {
                thread: &thread,
                conversation: &conversation,
                actor,
                source: "user",
            },
        )
        .await?;
        self.store.thread(id)
    }

    /// One pass of the periodic rules. `now` is passed in so tests can move time.
    pub fn lifecycle_tick(&self, now: i64) -> Result<TickReport> {
        let settings = self.store.settings()?.auto_settle;
        let report = self.store.lifecycle_tick_with_projects(
            LifecycleRules {
                now,
                idle_seconds: settings
                    .after_days
                    .map(|days| i64::from(days) * 24 * 60 * 60),
                keep_pull_requests: settings.keep_pull_requests,
            },
            &self.project_lifecycle_settings()?,
        )?;
        for id in &report.settled {
            self.terminals.close(id);
            self.previews.close_thread(id);
            self.run_hook_scripts(id, |script| script.run_on_settle);
        }
        Ok(report)
    }

    /// Starts the continuations that are due, overdue ones after a restart included.
    /// One that can't start yet stays scheduled for the next pass. Automatic
    /// continuations are bounded: once the tries are spent, the schedule is dropped,
    /// the limit stays on the thread, and it waits for you.
    pub fn resume_limited(&self) {
        for id in self.store.due_resumes(unix_now()).unwrap_or_default() {
            match self.store.take_resume_try(&id, MAX_RESUME_TRIES) {
                Ok(true) => {}
                Ok(false) => {
                    self.stop_limit_retry(&id);
                    continue;
                }
                Err(error) => {
                    tracing::warn!(%error, "could not count a limit continuation");
                    continue;
                }
            }
            if let Err(error) = self.runs.admit(&id, LIMIT_CONTINUATION, "system:limit") {
                tracing::debug!(%error, "a limit continuation did not start");
                // It never ran, so it does not count: it stays scheduled for the next pass.
                if let Err(error) = self.store.refund_resume_try(&id) {
                    tracing::warn!(%error, "could not give back a limit continuation");
                }
            }
        }
    }

    /// Leaves a limit that ran out of automatic continuations waiting for the person.
    fn stop_limit_retry(&self, id: &str) {
        if let Err(error) = self.store.stop_limit_retry(id, MAX_RESUME_TRIES) {
            tracing::warn!(%error, "could not record an exhausted limit continuation");
        }
    }

    /// Picks up threads a restart interrupted, unless the setting is off. A thread that
    /// keeps dying stops after a few tries, and your own message resets the count.
    pub fn continue_interrupted(&self) {
        for id in &self.recovery.interrupted_threads {
            let enabled = self.store.thread(id).ok().is_some_and(|thread| {
                self.preference("continue_threads_after_update", Some(&thread.project_id))
                    .is_ok_and(|on| on == serde_json::json!(true))
            });
            if !enabled {
                continue;
            }
            if !self
                .store
                .take_resume_try(id, MAX_RESUME_TRIES)
                .unwrap_or(false)
            {
                continue;
            }
            if let Err(error) = self.runs.admit(id, LIMIT_CONTINUATION, "system:restart") {
                tracing::debug!(%error, "a thread did not continue after the restart");
            }
        }
    }

    /// Runs the periodic rules until `stop` fires.
    pub async fn run_lifecycle(&self, stop: tokio_util::sync::CancellationToken) {
        self.continue_interrupted();
        let mut ticks = tokio::time::interval(std::time::Duration::from_secs(60));
        let mut watched: u32 = 0;
        loop {
            tokio::select! {
                () = stop.cancelled() => return,
                _ = ticks.tick() => {
                    if let Err(error) = self.lifecycle_tick(unix_now()) {
                        tracing::warn!(%error, "lifecycle tick failed");
                    }
                    watched += 1;
                    let polling = self.preference("poll_pull_requests", None);
                    if watched.is_multiple_of(PULL_WATCH_TICKS)
                        && polling.is_ok_and(|on| on != serde_json::json!(false))
                    {
                        self.watch_pulls().await;
                    }
                    self.resume_limited();
                    self.run_due_tasks().await;
                    if let Ok(mode) = self.preference("app_update_mode", None) {
                        self.updates.auto_install(mode.as_str().unwrap_or_default()).await;
                    }
                    if watched.is_multiple_of(AUTO_PULL_TICKS) {
                        self.auto_pull().await;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! A limit is continued automatically, once per reset, and never for longer than
    //! the tries allow: a run that fails on something else never retries itself.

    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::time::Duration;

    use async_trait::async_trait;
    use orbyn_contracts::{
        AgUiEvent, CreateThread, EngineCapabilities, EngineTier, RunState, ThreadLimit,
        UpdateThread, WorkspaceSetup,
    };
    use serde_json::json;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use super::MAX_RESUME_TRIES;
    use crate::Core;
    use crate::agents::driver::{
        EngineDescriptor, EngineDriver, EngineEvent, EngineOutcome, EngineProbe, RunHandle, RunSpec,
    };
    use crate::app::CoreConfig;
    use crate::conversation::automations::rfc3339;
    use crate::store::LimitStop;
    use crate::testing::TempDir;
    use crate::util::unix_now;

    /// One scripted end per started run, so a test can say what a continuation sees.
    type Script = Arc<dyn Fn() -> Vec<EngineEvent> + Send + Sync>;

    struct ScriptDriver {
        script: Script,
    }

    fn capabilities() -> EngineCapabilities {
        EngineCapabilities {
            streaming: true,
            approvals: true,
            resume: true,
            mcp: false,
            sandbox: false,
            managed: true,
            skill_delivery: Vec::new(),
            goal_control: false,
            decline_feedback: false,
            permission_modes: orbyn_contracts::PermissionMode::ALL.to_vec(),
        }
    }

    #[async_trait]
    impl EngineDriver for ScriptDriver {
        fn descriptor(&self) -> EngineDescriptor {
            EngineDescriptor {
                id: "script".into(),
                name: "Script".into(),
                tier: EngineTier::Terminal,
                capabilities: capabilities(),
            }
        }

        async fn probe(&self) -> EngineProbe {
            EngineProbe {
                path: Some("/bin/echo".into()),
                version: Some("1.0.0".into()),
                ..EngineProbe::default()
            }
        }

        async fn start_run(
            &self,
            _spec: RunSpec,
            events: mpsc::Sender<EngineEvent>,
        ) -> crate::error::Result<RunHandle> {
            let cancel = CancellationToken::new();
            let script = self.script.clone();
            let task = tokio::spawn(async move {
                for event in script() {
                    if events.send(event).await.is_err() {
                        return;
                    }
                }
            });
            Ok(RunHandle {
                cancel,
                task,
                input: None,
                control: None,
            })
        }
    }

    struct Fixture {
        _dir: TempDir,
        core: Core,
        thread: String,
    }

    async fn fixture(script: Script) -> Fixture {
        let dir = TempDir::new();
        let root = crate::testing::library(&dir.path().join("library"));
        let mut config = CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
        config.engines.lookup_latest = false;
        config.engines.update_timeout = Duration::from_secs(30);
        let core = Core::open(config).unwrap();
        core.engines.register(Arc::new(ScriptDriver { script }));
        core.workspace
            .setup(WorkspaceSetup {
                url: None,
                adopt_path: Some(root.to_string_lossy().into_owned()),
            })
            .await
            .unwrap();
        let project = core.new_project("project", false, None).await.unwrap();
        core.store.trust_project(&project.id, "t").unwrap();
        let thread = core
            .create_thread(CreateThread {
                project_id: Some(project.id),
                engine: "script".into(),
                model: None,
                title: None,
                worktree: None,
                skill_profile: None,
                base_branch: None,
                effort: None,
                options: BTreeMap::new(),
            })
            .await
            .unwrap();
        Fixture {
            _dir: dir,
            core,
            thread: thread.id,
        }
    }

    impl Fixture {
        /// The setting the settings screen offers, turned on for the test.
        fn auto_resume(&self) {
            self.core
                .set_preferences(
                    None,
                    &BTreeMap::from([("auto_resume_limited_threads".to_owned(), json!(true))]),
                )
                .unwrap();
        }

        fn limit(&self) -> Option<ThreadLimit> {
            self.core
                .store
                .thread(&self.thread)
                .unwrap()
                .lifecycle
                .unwrap()
                .limit
        }
    }

    /// A failure the engine signalled a reset for: the reset is what schedules the retry.
    fn limit_script(reset: impl Fn() -> i64 + Send + Sync + 'static) -> Script {
        Arc::new(move || {
            vec![
                EngineEvent::Limited {
                    resets_at: Some(reset()),
                },
                EngineEvent::Finished(EngineOutcome::Failed(
                    "You have hit your usage limit.".into(),
                )),
            ]
        })
    }

    fn failure_script(message: &'static str) -> Script {
        Arc::new(move || vec![EngineEvent::Finished(EngineOutcome::Failed(message.into()))])
    }

    async fn until_terminal(core: &Core, run: &str) -> RunState {
        for _ in 0..4000 {
            let state = core.store.run(run).unwrap().state;
            if state.is_terminal() {
                return state;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        panic!("run never ended");
    }

    /// Waits for the run's own task to close, which is after it records its limit.
    async fn until_idle(core: &Core) {
        for _ in 0..4000 {
            if core.runs.active_count() == 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        panic!("a run was left active");
    }

    fn ended(core: &Core, thread: &str) -> Vec<AgUiEvent> {
        core.store
            .events_after(thread, 0, 200)
            .unwrap()
            .into_iter()
            .map(|event| event.event)
            .collect()
    }

    #[tokio::test]
    async fn a_usage_limit_failure_schedules_its_continuation_at_the_reset() {
        let reset = unix_now() + 600;
        let f = fixture(limit_script(move || reset)).await;
        f.auto_resume();
        let run = f.core.runs.admit(&f.thread, "go", "tester").unwrap();
        assert_eq!(until_terminal(&f.core, &run.id).await, RunState::Failed);
        until_idle(&f.core).await;
        let limit = f.limit().expect("the thread keeps the limit");
        assert!(limit.resets_at.is_some(), "the engine's reset is kept");
        assert!(
            limit.resume_at.is_some(),
            "the reset schedules a continuation"
        );
        assert_eq!(
            f.core.store.run_count(&f.thread).unwrap(),
            1,
            "the failure starts nothing by itself"
        );
        assert!(
            f.core.store.due_resumes(reset - 1).unwrap().is_empty(),
            "nothing starts before the reset"
        );
        assert_eq!(
            f.core.store.due_resumes(reset).unwrap(),
            vec![f.thread.clone()]
        );
    }

    #[tokio::test]
    async fn a_failure_that_is_not_a_limit_schedules_nothing() {
        let f = fixture(failure_script("The tool call failed.")).await;
        f.auto_resume();
        let run = f.core.runs.admit(&f.thread, "go", "tester").unwrap();
        assert_eq!(until_terminal(&f.core, &run.id).await, RunState::Failed);
        until_idle(&f.core).await;
        assert!(f.limit().is_none(), "there is no limit to continue");
        assert!(f.core.store.due_resumes(i64::MAX).unwrap().is_empty());
        assert_eq!(f.core.store.run_count(&f.thread).unwrap(), 1);
    }

    #[tokio::test]
    async fn automatic_continuations_run_out_and_leave_the_thread_waiting_for_you() {
        let f = fixture(limit_script(|| unix_now() + 600)).await;
        f.auto_resume();
        let first = f.core.runs.admit(&f.thread, "go", "tester").unwrap();
        assert_eq!(until_terminal(&f.core, &first.id).await, RunState::Failed);
        until_idle(&f.core).await;
        assert!(
            f.limit().is_some_and(|limit| limit.resume_at.is_some()),
            "the limit schedules its continuation"
        );

        // The same limit, with a reset already behind us, so the next pass finds the
        // continuation due without waiting for a window to pass.
        f.core
            .store
            .stop_on_limit(
                &f.thread,
                &LimitStop {
                    message: "You have hit your usage limit.".into(),
                    resets_at: rfc3339(unix_now() - 60),
                    resume: true,
                    snooze: false,
                },
            )
            .unwrap();
        f.core
            .update_thread(
                &f.thread,
                &UpdateThread {
                    resume_at_reset: Some(true),
                    ..Default::default()
                },
                "tester",
            )
            .unwrap();
        assert_eq!(
            f.core.store.due_resumes(unix_now()).unwrap(),
            vec![f.thread.clone()]
        );

        // The three automatic continuations the budget allows.
        for _ in 0..MAX_RESUME_TRIES {
            assert!(
                f.core
                    .store
                    .take_resume_try(&f.thread, MAX_RESUME_TRIES)
                    .unwrap(),
                "the budget allows three continuations"
            );
        }

        // The next pass drops the schedule instead of starting a run.
        f.core.resume_limited();
        assert_eq!(
            f.core.store.run_count(&f.thread).unwrap(),
            1,
            "nothing starts once the tries are spent"
        );
        assert_eq!(f.core.store.run(&first.id).unwrap().state, RunState::Failed);
        let limit = f.limit().expect("the limit stays on the thread");
        assert!(
            limit.resume_at.is_none(),
            "nothing stays scheduled, so the thread waits for you"
        );
        assert!(f.core.store.due_resumes(i64::MAX).unwrap().is_empty());
        assert!(
            !f.core
                .store
                .take_resume_try(&f.thread, MAX_RESUME_TRIES)
                .unwrap(),
            "the tries are spent"
        );
        assert!(
            ended(&f.core, &f.thread)
                .iter()
                .any(|event| matches!(event, AgUiEvent::Custom { name, .. } if name == "limit.retry_exhausted")),
            "the timeline says the automatic continuation stopped"
        );

        // Choosing the continuation on the thread gives the budget back.
        f.core
            .update_thread(
                &f.thread,
                &UpdateThread {
                    resume_at_reset: Some(true),
                    ..Default::default()
                },
                "tester",
            )
            .unwrap();
        assert!(
            f.limit().is_some_and(|limit| limit.resume_at.is_some()),
            "a person's own choice schedules it again"
        );
        assert!(
            f.core
                .store
                .take_resume_try(&f.thread, MAX_RESUME_TRIES)
                .unwrap(),
            "with a fresh budget"
        );
    }
}
