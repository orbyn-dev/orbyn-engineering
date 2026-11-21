//! Adverse orderings around one run, against a scripted in-process driver.
//! Wire-format behavior is covered against the fake engine in `orbynd/tests`.

mod follow_ups;
#[cfg(unix)]
mod update;

use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use orbyn_contracts::{
    ActionKind, AgUiEvent, ApprovalState, CreateThread, EngineCapabilities, EngineTier, RunState,
    SkillDelivery,
};
use serde_json::{Value, json};

use super::*;
use crate::Core;
use crate::agents::driver::{
    EngineDescriptor, EngineDriver, EngineProbe, SteerMessage, UpdatePlan, ask,
};
use crate::app::CoreConfig;
use crate::governance::approvals::Verdict;
use crate::testing::TempDir;

type Script = Arc<
    dyn Fn(RunSpec, mpsc::Sender<EngineEvent>, CancellationToken) -> JoinHandle<()> + Send + Sync,
>;

struct ScriptDriver {
    script: Script,
    capabilities: EngineCapabilities,
    /// Starts a stand-in process ahead of the next turn, as Claude does.
    warms: bool,
    update_script: String,
    probes: std::sync::atomic::AtomicU64,
    start_failure: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
    /// What a running turn's steer input received, when a test asks for one.
    steer_log: Option<Arc<std::sync::Mutex<Vec<SteerMessage>>>>,
}

fn capabilities() -> EngineCapabilities {
    EngineCapabilities {
        streaming: true,
        approvals: true,
        resume: true,
        mcp: true,
        sandbox: false,
        managed: true,
        skill_delivery: vec![SkillDelivery::McpSkillServer],
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
            capabilities: self.capabilities.clone(),
        }
    }

    async fn probe(&self) -> EngineProbe {
        EngineProbe {
            path: Some("/bin/echo".into()),
            version: Some(if self.update_script.contains("no change") {
                "1.0.0".into()
            } else {
                format!(
                    "1.0.{}",
                    self.probes
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                )
            }),
            ..EngineProbe::default()
        }
    }

    async fn update_plan(&self, _: &EngineProbe) -> Option<UpdatePlan> {
        Some(UpdatePlan {
            installer: orbyn_contracts::InstallerKind::SelfManaged,
            program: "/bin/sh".into(),
            args: vec!["-c".into(), self.update_script.clone()],
            lock_key: "script".into(),
        })
    }

    fn can_warm(&self) -> bool {
        self.warms
    }

    fn warm_key(&self, _spec: &RunSpec) -> Option<String> {
        self.warms.then(|| "script".to_owned())
    }

    async fn start_warm(
        &self,
        _spec: RunSpec,
    ) -> Result<Option<crate::agents::driver::WarmProcess>> {
        #[cfg(unix)]
        return crate::agents::driver::stand_in_warm("script").map(Some);
        #[cfg(not(unix))]
        Ok(None)
    }

    async fn start_run(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<EngineEvent>,
    ) -> Result<RunHandle> {
        if let Some((entered, release)) = &self.start_failure {
            entered.notify_one();
            release.notified().await;
            return Err(CoreError::Invalid(
                "startup failed after the stop was accepted: authentication required; rate limit reached".into(),
            ));
        }
        let cancel = CancellationToken::new();
        let task = (self.script)(spec, events, cancel.clone());
        let input = self.steer_log.clone().map(|log| {
            let (sender, mut receiver) = mpsc::unbounded_channel::<SteerMessage>();
            tokio::spawn(async move {
                while let Some(steer) = receiver.recv().await {
                    log.lock().unwrap().push(steer);
                }
            });
            sender
        });
        Ok(RunHandle {
            cancel,
            task,
            input,
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
    fixture_with(script, "echo updated", Duration::from_secs(30)).await
}

async fn fixture_with(script: Script, update: &str, update_timeout: Duration) -> Fixture {
    fixture_with_start_failure(script, update, update_timeout, None).await
}

async fn fixture_with_start_failure(
    script: Script,
    update: &str,
    update_timeout: Duration,
    start_failure: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
) -> Fixture {
    fixture_full(
        script,
        update,
        update_timeout,
        start_failure,
        capabilities(),
        false,
    )
    .await
}

async fn fixture_full(
    script: Script,
    update: &str,
    update_timeout: Duration,
    start_failure: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
    capabilities: EngineCapabilities,
    warms: bool,
) -> Fixture {
    fixture_steering(
        script,
        update,
        update_timeout,
        start_failure,
        capabilities,
        warms,
        None,
    )
    .await
}

async fn fixture_steering(
    script: Script,
    update: &str,
    update_timeout: Duration,
    start_failure: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
    capabilities: EngineCapabilities,
    warms: bool,
    steer_log: Option<Arc<std::sync::Mutex<Vec<SteerMessage>>>>,
) -> Fixture {
    let dir = TempDir::new();
    let root = crate::testing::library(&dir.path().join("library"));
    let mut config = CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
    config.engines.lookup_latest = false;
    config.engines.update_timeout = update_timeout;
    let core = Core::open(config).unwrap();
    core.engines.register(Arc::new(ScriptDriver {
        script,
        capabilities,
        warms,
        update_script: update.to_owned(),
        probes: Default::default(),
        start_failure,
        steer_log,
    }));
    core.workspace
        .setup(orbyn_contracts::WorkspaceSetup {
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
            options: std::collections::BTreeMap::new(),
        })
        .await
        .unwrap();
    Fixture {
        _dir: dir,
        core,
        thread: thread.id,
    }
}

async fn until_state(core: &Core, run: &str, wanted: RunState) {
    for _ in 0..2000 {
        if core.store.run(run).unwrap().state == wanted {
            return;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    panic!(
        "run stayed {:?}, wanted {wanted:?}",
        core.store.run(run).unwrap().state
    );
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

async fn until_pending(core: &Core, thread: &str) -> orbyn_contracts::Approval {
    for _ in 0..2000 {
        if let Some(found) = core.store.pending_for_thread(thread).unwrap().pop() {
            return found;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    panic!("no approval appeared");
}

fn text(delta: &str) -> EngineEvent {
    AgUiEvent::TextMessageContent {
        message_id: "m".into(),
        delta: delta.into(),
    }
    .into()
}

async fn request(events: &mpsc::Sender<EngineEvent>) -> Verdict {
    ask(
        events,
        ActionKind::Command,
        "ls".into(),
        "Run ls".into(),
        Value::Null,
    )
    .await
}

fn finished() -> Script {
    Arc::new(|_, events, _| {
        tokio::spawn(async move {
            let _ = events.send(text("hello")).await;
            let _ = events
                .send(EngineEvent::Finished(EngineOutcome::Completed))
                .await;
        })
    })
}

#[tokio::test]
async fn a_saved_mode_the_engine_lacks_runs_supervised() {
    use orbyn_contracts::PermissionMode::{Auto, FullAccess, Supervised};
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let record = seen.clone();
    let script: Script = Arc::new(move |spec: RunSpec, events, _| {
        record.lock().unwrap().push(spec.mode);
        tokio::spawn(async move {
            let _ = events.send(text("done")).await;
            let _ = events
                .send(EngineEvent::Finished(EngineOutcome::Completed))
                .await;
        })
    });
    let mut caps = capabilities();
    caps.permission_modes = crate::agents::permission_modes::offered("pi");
    let f = fixture_full(
        script,
        "echo updated",
        Duration::from_secs(30),
        None,
        caps,
        false,
    )
    .await;
    for (saved, ran) in [(Auto, Supervised), (FullAccess, FullAccess)] {
        let patch = orbyn_contracts::UpdateThread {
            permission_mode: Some(saved),
            ..Default::default()
        };
        f.core.store.update_thread(&f.thread, &patch, "t").unwrap();
        let run = f.core.runs.admit(&f.thread, "go", "t").unwrap();
        assert_eq!(until_terminal(&f.core, &run.id).await, RunState::Completed);
        assert_eq!(seen.lock().unwrap().last(), Some(&ran));
        // The thread keeps what was chosen; only the run is narrowed.
        let thread = f.core.store.thread(&f.thread).unwrap();
        assert_eq!(thread.lifecycle.unwrap().permission_mode, saved);
    }
}

#[tokio::test]
async fn completed_run_logs_in_order_and_leaves_no_residue() {
    let f = fixture(finished()).await;
    let run = f.core.runs.admit(&f.thread, "go", "t").unwrap();
    assert_eq!(until_terminal(&f.core, &run.id).await, RunState::Completed);
    let events = f.core.store.events_after(&f.thread, 0, 100).unwrap();
    assert!(matches!(events[0].event, AgUiEvent::RunStarted { .. }));
    assert!(matches!(
        events.last().unwrap().event,
        AgUiEvent::RunFinished { .. }
    ));
    for _ in 0..500 {
        if f.core.runs.active_count() == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    assert_eq!(f.core.runs.active_count(), 0);
    assert!(!f.core.config.data_dir.join("runs").join(&run.id).exists());
}

#[tokio::test]
async fn a_run_records_the_attachments_its_message_carried() {
    let f = fixture(finished()).await;
    let attachment = f
        .core
        .add_attachment(&f.thread, "notes.txt", "text/plain", b"hello")
        .unwrap();
    let run = f
        .core
        .runs
        .admit_with(&f.thread, "go", std::slice::from_ref(&attachment.id), "t")
        .unwrap();
    assert_eq!(until_terminal(&f.core, &run.id).await, RunState::Completed);
    let events = f.core.store.events_after(&f.thread, 0, 100).unwrap();
    let event = events
        .iter()
        .find(
            |event| matches!(&event.event, AgUiEvent::Custom { name, .. } if name == "user.attachments"),
        )
        .expect("a user.attachments event");
    assert_eq!(event.run_id.as_deref(), Some(run.id.as_str()));
    let AgUiEvent::Custom { value, .. } = &event.event else {
        unreachable!()
    };
    let sent = value
        .get("attachments")
        .and_then(Value::as_array)
        .expect("an attachments array");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["id"], json!(attachment.id));
    assert_eq!(sent[0]["name"], json!("notes.txt"));
    assert_eq!(sent[0]["mime"], json!("text/plain"));
    assert_eq!(sent[0]["size"], json!(5));
}

#[tokio::test]
async fn a_run_without_attachments_records_none() {
    let f = fixture(finished()).await;
    let run = f.core.runs.admit(&f.thread, "go", "t").unwrap();
    assert_eq!(until_terminal(&f.core, &run.id).await, RunState::Completed);
    let events = f.core.store.events_after(&f.thread, 0, 100).unwrap();
    assert!(!events.iter().any(|event| matches!(
        &event.event,
        AgUiEvent::Custom { name, .. } if name == "user.attachments"
    )));
}

#[tokio::test]
async fn admission_is_durable_before_the_engine_starts() {
    let seen: Arc<Mutex<Option<RunState>>> = Arc::default();
    let store_slot: Arc<Mutex<Option<Arc<Store>>>> = Arc::default();
    let script: Script = {
        let seen = seen.clone();
        let store_slot = store_slot.clone();
        Arc::new(move |spec, events, _| {
            let store = store_slot.lock().unwrap().clone().unwrap();
            *seen.lock().unwrap() = Some(store.run(&spec.run_id).unwrap().state);
            tokio::spawn(async move {
                let _ = events
                    .send(EngineEvent::Finished(EngineOutcome::Completed))
                    .await;
            })
        })
    };
    let f = fixture(script).await;
    *store_slot.lock().unwrap() = Some(f.core.store.clone());
    let run = f.core.runs.admit(&f.thread, "go", "t").unwrap();
    until_terminal(&f.core, &run.id).await;
    assert_eq!(*seen.lock().unwrap(), Some(RunState::Starting));
}

#[tokio::test]
async fn second_admission_is_thread_busy() {
    let script: Script = Arc::new(|_, events, cancel| {
        tokio::spawn(async move {
            cancel.cancelled().await;
            let _ = events
                .send(EngineEvent::Finished(EngineOutcome::Cancelled))
                .await;
        })
    });
    let f = fixture(script).await;
    let run = f.core.runs.admit(&f.thread, "go", "t").unwrap();
    until_state(&f.core, &run.id, RunState::Running).await;
    assert!(matches!(
        f.core.runs.admit(&f.thread, "again", "t"),
        Err(CoreError::ThreadBusy)
    ));
    f.core.runs.cancel(&run.id).unwrap();
    assert_eq!(until_terminal(&f.core, &run.id).await, RunState::Cancelled);
}

#[tokio::test]
async fn stopping_an_engine_family_cancels_only_its_running_runs_and_counts_them() {
    let script: Script = Arc::new(|_, events, cancel| {
        tokio::spawn(async move {
            cancel.cancelled().await;
            let _ = events
                .send(EngineEvent::Finished(EngineOutcome::Cancelled))
                .await;
        })
    });
    let f = fixture(script).await;
    let run = f.core.runs.admit(&f.thread, "go", "t").unwrap();
    until_state(&f.core, &run.id, RunState::Running).await;
    assert_eq!(f.core.stop_engine_runs("cursor"), 0, "another family");
    assert_eq!(f.core.store.run(&run.id).unwrap().state, RunState::Running);
    assert_eq!(f.core.stop_engine_runs("script"), 1);
    assert_eq!(until_terminal(&f.core, &run.id).await, RunState::Cancelled);
    assert_eq!(f.core.stop_engine_runs("script"), 0, "nothing left to stop");
}

#[tokio::test]
async fn accepted_cancel_before_driver_start_finishes_wins_over_its_failure() {
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let f = fixture_with_start_failure(
        finished(),
        "echo updated",
        Duration::from_secs(30),
        Some((entered.clone(), release.clone())),
    )
    .await;
    f.core
        .set_preferences(
            None,
            &std::collections::BTreeMap::from([
                ("auto_resume_limited_threads".into(), json!(true)),
                ("snooze_limited_threads".into(), json!(true)),
            ]),
        )
        .unwrap();
    let run = f.core.runs.admit(&f.thread, "go", "tester").unwrap();
    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .unwrap();
    assert_eq!(f.core.store.run(&run.id).unwrap().state, RunState::Starting);
    f.core.runs.cancel(&run.id).unwrap();
    release.notify_one();
    assert_eq!(
        until_terminal(&f.core, &run.id).await,
        RunState::Cancelled,
        "accepted stop lost to startup failure: {:?}",
        f.core.store.run(&run.id).unwrap()
    );
    assert!(f.core.store.run(&run.id).unwrap().error.is_none());
    for _ in 0..500 {
        if f.core.runs.active_count() == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    assert_eq!(f.core.runs.active_count(), 0);
    assert!(
        f.core
            .store
            .thread(&f.thread)
            .unwrap()
            .lifecycle
            .unwrap()
            .limit
            .is_none()
    );
    assert!(f.core.store.due_resumes(i64::MAX).unwrap().is_empty());
    let events = f.core.store.events_after(&f.thread, 0, 100).unwrap();
    assert!(!events.iter().any(|event| matches!(&event.event, AgUiEvent::Custom { name, .. } if name == "engine.sign_in_required")));
}

#[tokio::test]
async fn late_rejected_steer_after_cancel_remains_held_without_starting_work() {
    let f = fixture(finished()).await;
    let run = f.core.runs.admit(&f.thread, "original", "tester").unwrap();
    f.core.store.request_stop(&run.id).unwrap();
    f.core
        .store
        .finish_run(&run.id, RunState::Completed, None)
        .unwrap();
    let scope = RunScope {
        run_id: run.id.clone(),
        thread_id: f.thread.clone(),
        engine: "script".into(),
        family: "script".into(),
        write_scope: None,
        mode: PermissionMode::Supervised,
        plan: false,
    };
    f.core.runs.requeue_steer(
        &scope,
        &SteerMessage {
            message_id: "late-steer".into(),
            prompt: "keep rejected prompt".into(),
        },
    );
    let queued = f.core.store.list_queue(&f.thread).unwrap();
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].prompt, "keep rejected prompt");
    assert!(
        queued[0].held,
        "a rejected steer after cancelled commit must not start a new run"
    );
    for _ in 0..500 {
        if f.core.runs.active_count() == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    assert_eq!(f.core.runs.active_count(), 0);
    assert!(f.core.store.thread(&f.thread).unwrap().active_run.is_none());
    assert_eq!(f.core.store.list_queue(&f.thread).unwrap().len(), 1);
}

#[tokio::test]
async fn cancel_during_approval_denies_the_engine_and_expires_it() {
    let heard: Arc<Mutex<Option<Verdict>>> = Arc::default();
    let script: Script = {
        let heard = heard.clone();
        Arc::new(move |_, events, cancel| {
            let heard = heard.clone();
            tokio::spawn(async move {
                let verdict = request(&events).await;
                *heard.lock().unwrap() = Some(verdict);
                cancel.cancelled().await;
                let _ = events
                    .send(EngineEvent::Finished(EngineOutcome::Cancelled))
                    .await;
            })
        })
    };
    let f = fixture(script).await;
    let run = f.core.runs.admit(&f.thread, "go", "t").unwrap();
    let approval = until_pending(&f.core, &f.thread).await;
    assert_eq!(
        f.core.store.run(&run.id).unwrap().state,
        RunState::AwaitingApproval
    );
    f.core.runs.cancel(&run.id).unwrap();
    assert_eq!(until_terminal(&f.core, &run.id).await, RunState::Cancelled);
    assert!(!heard.lock().unwrap().clone().unwrap().is_allowed());
    assert_eq!(
        f.core.store.approval(&approval.id).unwrap().state,
        ApprovalState::Expired
    );
}

#[tokio::test]
async fn engine_exit_during_approval_fails_the_run_and_expires() {
    let script: Script = Arc::new(|_, events, _| {
        tokio::spawn(async move {
            let (reply, _verdict) = tokio::sync::oneshot::channel();
            let _ = events
                .send(EngineEvent::Permission(PermissionRequest {
                    kind: ActionKind::Command,
                    target: "ls".into(),
                    summary: "ls".into(),
                    detail: Value::Null,
                    paths: Vec::new(),
                    reply,
                }))
                .await;
            tokio::time::sleep(Duration::from_millis(50)).await;
            // Dropping `events` is the engine disappearing without a word.
        })
    });
    let f = fixture(script).await;
    let run = f.core.runs.admit(&f.thread, "go", "t").unwrap();
    let approval = until_pending(&f.core, &f.thread).await;
    assert_eq!(until_terminal(&f.core, &run.id).await, RunState::Failed);
    assert_eq!(
        f.core.store.approval(&approval.id).unwrap().state,
        ApprovalState::Expired
    );
}

#[tokio::test]
async fn decision_racing_run_end_cannot_land_on_a_finished_run() {
    let script: Script = Arc::new(|_, events, _| {
        tokio::spawn(async move {
            let asker = events.clone();
            tokio::spawn(async move { request(&asker).await });
            tokio::time::sleep(Duration::from_millis(20)).await;
            let _ = events
                .send(EngineEvent::Finished(EngineOutcome::Completed))
                .await;
        })
    });
    let f = fixture(script).await;
    let run = f.core.runs.admit(&f.thread, "go", "t").unwrap();
    let approval = until_pending(&f.core, &f.thread).await;
    until_terminal(&f.core, &run.id).await;
    assert!(matches!(
        f.core.gate.decide(&approval.id, true, true, "late"),
        Err(CoreError::AlreadyDecided(_))
    ));
    assert!(f.core.store.list_rules().unwrap().is_empty());
}

#[tokio::test]
async fn late_events_after_finish_change_nothing() {
    let sent_late = Arc::new(AtomicBool::new(false));
    let script: Script = {
        let sent_late = sent_late.clone();
        Arc::new(move |_, events, cancel| {
            let sent_late = sent_late.clone();
            tokio::spawn(async move {
                let _ = events.send(text("done")).await;
                let _ = events
                    .send(EngineEvent::Finished(EngineOutcome::Completed))
                    .await;
                cancel.cancelled().await;
                // The receiver is gone or the run is closed; either way nothing lands.
                let _ = events.send(text("too late")).await;
                sent_late.store(true, Ordering::SeqCst);
            })
        })
    };
    let f = fixture(script).await;
    let run = f.core.runs.admit(&f.thread, "go", "t").unwrap();
    assert_eq!(until_terminal(&f.core, &run.id).await, RunState::Completed);
    assert!(sent_late.load(Ordering::SeqCst));
    let events = f.core.store.events_after(&f.thread, 0, 100).unwrap();
    assert!(matches!(
        events.last().unwrap().event,
        AgUiEvent::RunFinished { .. }
    ));
    assert_eq!(
        f.core.store.run(&run.id).unwrap().state,
        RunState::Completed
    );
}

#[tokio::test]
async fn driver_that_ignores_cancel_is_still_torn_down() {
    let script: Script = Arc::new(|_, events, _| {
        tokio::spawn(async move {
            let _held_open = events;
            std::future::pending::<()>().await;
        })
    });
    let f = fixture(script).await;
    let run = f.core.runs.admit(&f.thread, "go", "t").unwrap();
    until_state(&f.core, &run.id, RunState::Running).await;
    tokio::time::pause();
    f.core.runs.cancel(&run.id).unwrap();
    tokio::time::sleep(STOP_TIMEOUT * 3).await;
    tokio::time::resume();
    assert_eq!(until_terminal(&f.core, &run.id).await, RunState::Cancelled);
}

#[tokio::test]
async fn a_forced_stop_does_not_wait_for_an_engine_that_ignores_it() {
    let script: Script = Arc::new(|_, events, _| {
        tokio::spawn(async move {
            let _held_open = events;
            std::future::pending::<()>().await;
        })
    });
    let f = fixture(script).await;
    let run = f.core.runs.admit(&f.thread, "go", "t").unwrap();
    until_state(&f.core, &run.id, RunState::Running).await;
    let started = std::time::Instant::now();
    let stopped = f.core.runs.force_stop(&f.thread, "t").await.unwrap();
    assert!(started.elapsed() < STOP_TIMEOUT, "{:?}", started.elapsed());
    assert_eq!(stopped.run_id.as_deref(), Some(run.id.as_str()));
    assert_eq!(stopped.state, Some(RunState::Cancelled));
    let again = f.core.runs.force_stop(&f.thread, "t").await.unwrap();
    assert_eq!(again.run_id, None, "a second click changes nothing");
}

#[tokio::test]
async fn a_forced_stop_closes_a_run_nothing_owns() {
    let script: Script = Arc::new(|_, events, _| {
        tokio::spawn(async move {
            let _held_open = events;
            std::future::pending::<()>().await;
        })
    });
    let f = fixture(script).await;
    let run = f.core.runs.admit(&f.thread, "go", "t").unwrap();
    until_state(&f.core, &run.id, RunState::Running).await;
    let owner = f.core.runs.lock().remove(&run.id).unwrap();
    let stopped = f.core.runs.force_stop(&f.thread, "t").await.unwrap();
    assert_eq!(stopped.state, Some(RunState::Cancelled));
    owner.stops.force.cancel();
}

fn silent_engine() -> Script {
    Arc::new(|_, events, _| {
        tokio::spawn(async move {
            let _held_open = events;
            std::future::pending::<()>().await;
        })
    })
}

#[tokio::test]
async fn a_delegated_run_that_goes_silent_is_ended_and_says_why() {
    let f = fixture(silent_engine()).await;
    let task = orbyn_contracts::DelegateTask {
        prompt: "work".into(),
        title: None,
        engine: None,
        model: None,
        effort: None,
        options: std::collections::BTreeMap::new(),
    };
    let child = f.core.delegate(&f.thread, &task, "t").unwrap();
    let run = f.core.store.thread(&child.id).unwrap().active_run.unwrap();
    until_state(&f.core, &run.id, RunState::Running).await;
    tokio::time::pause();
    tokio::time::sleep(SILENCE_LIMIT * 2).await;
    tokio::time::resume();
    assert_eq!(until_terminal(&f.core, &run.id).await, RunState::Failed);
    let error = f.core.store.run(&run.id).unwrap().error.unwrap();
    assert!(error.contains("stopped responding"), "{error}");
}

#[tokio::test]
async fn a_run_someone_watches_is_never_ended_for_silence() {
    let f = fixture(silent_engine()).await;
    let run = f.core.runs.admit(&f.thread, "go", "t").unwrap();
    until_state(&f.core, &run.id, RunState::Running).await;
    tokio::time::pause();
    tokio::time::sleep(SILENCE_LIMIT * 2).await;
    tokio::time::resume();
    tokio::task::yield_now().await;
    let state = f.core.store.run(&run.id).unwrap().state;
    assert_eq!(state, RunState::Running);
    f.core.runs.force_stop(&f.thread, "t").await.unwrap();
}

#[tokio::test]
async fn the_gate_keeps_a_files_old_bytes_before_a_write_it_allows() {
    let script: Script = Arc::new(|spec, events, _| {
        tokio::spawn(async move {
            let path = spec.checkout.join("a.txt").to_string_lossy().into_owned();
            let verdict = crate::agents::driver::ask_paths(
                &events,
                ActionKind::FileChange,
                path.clone(),
                "Edit".into(),
                Value::Null,
                Vec::new(),
            )
            .await;
            if verdict.is_allowed() {
                std::fs::write(&path, "new").unwrap();
                let _ = events.send(EngineEvent::Files(vec![path])).await;
            }
            let _ = events
                .send(EngineEvent::Finished(EngineOutcome::Completed))
                .await;
        })
    });
    let f = fixture(script).await;
    let cwd = f.core.store.thread(&f.thread).unwrap().cwd;
    std::fs::write(Path::new(&cwd).join("a.txt"), "old").unwrap();
    let run = f.core.runs.admit(&f.thread, "go", "t").unwrap();
    let approval = until_pending(&f.core, &f.thread).await;
    f.core.gate.decide(&approval.id, true, false, "t").unwrap();
    assert_eq!(until_terminal(&f.core, &run.id).await, RunState::Completed);
    let kept = f.core.changed_file(&f.thread, &run.id, "a.txt").unwrap();
    assert_eq!(kept.before.as_deref(), Some("old"));
    assert_eq!(kept.after.as_deref(), Some("new"));
    assert_eq!(kept.status, orbyn_contracts::ChangeStatus::Modified);
}

#[tokio::test]
async fn panicking_engine_task_fails_the_run() {
    let script: Script = Arc::new(|_, _events, _| {
        tokio::spawn(async move {
            panic!("driver bug");
        })
    });
    let f = fixture(script).await;
    let run = f.core.runs.admit(&f.thread, "go", "t").unwrap();
    assert_eq!(until_terminal(&f.core, &run.id).await, RunState::Failed);
}

#[tokio::test]
async fn mcp_token_lives_exactly_as_long_as_the_run() {
    let url: Arc<Mutex<Option<String>>> = Arc::default();
    let script: Script = {
        let url = url.clone();
        Arc::new(move |spec, events, cancel| {
            *url.lock().unwrap() = spec.mcp_url.clone();
            tokio::spawn(async move {
                cancel.cancelled().await;
                let _ = events
                    .send(EngineEvent::Finished(EngineOutcome::Cancelled))
                    .await;
            })
        })
    };
    let f = fixture(script).await;
    let run = f.core.runs.admit(&f.thread, "go", "t").unwrap();
    until_state(&f.core, &run.id, RunState::Running).await;
    let url = url.lock().unwrap().clone().unwrap();
    let token = url.rsplit('/').next().unwrap().to_owned();
    assert!(f.core.mcp.is_valid(&token));
    f.core.runs.cancel(&run.id).unwrap();
    until_terminal(&f.core, &run.id).await;
    assert!(!f.core.mcp.is_valid(&token));
    let reply = f
        .core
        .mcp
        .handle(
            &token,
            &json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }),
        )
        .await;
    assert!(reply.is_err());
}

#[tokio::test]
async fn run_context_names_the_checkout_and_the_prompt_stays_clean_in_the_log() {
    let seen: Arc<Mutex<Option<RunSpec>>> = Arc::default();
    let script: Script = {
        let seen = seen.clone();
        Arc::new(move |spec, events, _| {
            *seen.lock().unwrap() = Some(spec);
            tokio::spawn(async move {
                let _ = events
                    .send(EngineEvent::Finished(EngineOutcome::Completed))
                    .await;
            })
        })
    };
    let f = fixture(script).await;
    let run = f.core.runs.admit(&f.thread, "do the thing", "t").unwrap();
    until_terminal(&f.core, &run.id).await;
    let spec = seen.lock().unwrap().clone().unwrap();
    let root = f.core.workspace.root().unwrap();
    assert_eq!(spec.checkout, root.join("repo/project"));
    assert_eq!(
        spec.cwd, spec.checkout,
        "a non-native engine works in the checkout"
    );
    assert!(spec.prompt.ends_with("do the thing"));
    assert_eq!(spec.user_prompt, "do the thing");
    for needle in [
        root.to_str().unwrap(),
        spec.checkout.to_str().unwrap(),
        "open its files directly",
        "They specialize shared methods",
        "skill_search",
    ] {
        assert!(spec.prompt.contains(needle), "{needle}");
    }
    for limit in ["read-only", "not guaranteed", "Selected checkout"] {
        assert!(!spec.prompt.contains(limit), "{limit}");
    }
    assert_eq!(
        f.core.store.transcript(&f.thread, 1000).unwrap().trim(),
        "do the thing"
    );
}

#[tokio::test]
async fn a_moved_checkout_fails_the_run_instead_of_escaping() {
    let f = fixture(finished()).await;
    let thread = f.core.store.thread(&f.thread).unwrap();
    std::fs::remove_dir_all(&thread.cwd).unwrap();
    let run = f.core.runs.admit(&f.thread, "go", "t").unwrap();
    assert_eq!(until_terminal(&f.core, &run.id).await, RunState::Failed);
}

#[tokio::test]
async fn second_owner_of_a_data_dir_is_refused() {
    let f = fixture(finished()).await;
    let config = f.core.config.clone();
    assert!(matches!(Core::open(config), Err(CoreError::DataDirLocked)));
}

#[tokio::test]
async fn shutdown_ends_runs_as_cancelled() {
    let script: Script = Arc::new(|_, events, cancel| {
        tokio::spawn(async move {
            cancel.cancelled().await;
            let _ = events
                .send(EngineEvent::Finished(EngineOutcome::Cancelled))
                .await;
        })
    });
    let f = fixture(script).await;
    let run = f.core.runs.admit(&f.thread, "go", "t").unwrap();
    until_state(&f.core, &run.id, RunState::Running).await;
    f.core.shutdown().await;
    assert_eq!(
        f.core.store.run(&run.id).unwrap().state,
        RunState::Cancelled
    );
}

#[tokio::test]
async fn empty_prompt_is_rejected_before_admission() {
    let f = fixture(finished()).await;
    assert!(matches!(
        f.core.runs.admit(&f.thread, "  ", "t"),
        Err(CoreError::Invalid(_))
    ));
    assert_eq!(f.core.store.latest_seq(&f.thread).unwrap(), 0);
}

#[tokio::test]
async fn resume_passes_the_stored_session() {
    let resumes: Arc<Mutex<Vec<Option<String>>>> = Arc::default();
    let script: Script = {
        let resumes = resumes.clone();
        Arc::new(move |spec, events, _| {
            resumes.lock().unwrap().push(spec.resume.clone());
            tokio::spawn(async move {
                let _ = events.send(EngineEvent::Session("session-1".into())).await;
                let _ = events
                    .send(EngineEvent::Finished(EngineOutcome::Completed))
                    .await;
            })
        })
    };
    let f = fixture(script).await;
    for _ in 0..2 {
        let run = f.core.runs.admit(&f.thread, "go", "t").unwrap();
        until_terminal(&f.core, &run.id).await;
        for _ in 0..2500 {
            if f.core.runs.active_count() == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }
    assert_eq!(
        *resumes.lock().unwrap(),
        [None, Some("session-1".to_owned())]
    );
}

#[tokio::test]
async fn no_run_starts_while_its_thread_is_rewound() {
    let f = fixture(finished()).await;
    let hold = f.core.runs.hold_for_rewind(&f.thread).unwrap();
    assert!(matches!(
        f.core.runs.admit(&f.thread, "go", "t"),
        Err(CoreError::ThreadBusy)
    ));
    assert!(matches!(
        f.core.runs.hold_for_rewind(&f.thread),
        Err(CoreError::ThreadBusy)
    ));
    drop(hold);
    let run = f.core.runs.admit(&f.thread, "go", "t").unwrap();
    until_terminal(&f.core, &run.id).await;
}

fn a_secret() -> String {
    format!("ghp_{}", "a1B2c3D4e5".repeat(4))
}

fn recording(seen: &Arc<std::sync::Mutex<Vec<RunSpec>>>) -> Script {
    let seen = seen.clone();
    Arc::new(move |spec: RunSpec, events, _| {
        seen.lock().unwrap().push(spec);
        tokio::spawn(async move {
            let _ = events.send(text("done")).await;
            let _ = events
                .send(EngineEvent::Finished(EngineOutcome::Completed))
                .await;
        })
    })
}

fn timeline(f: &Fixture) -> String {
    serde_json::to_string(&f.core.store.events_after(&f.thread, 0, 200).unwrap()).unwrap()
}

#[tokio::test]
async fn a_secret_in_a_message_reaches_the_engine_masked_and_stays_in_the_thread() {
    let secret = a_secret();
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let f = fixture(recording(&seen)).await;
    let run = f
        .core
        .runs
        .admit(&f.thread, &format!("deploy with {secret} now"), "t")
        .unwrap();
    assert_eq!(until_terminal(&f.core, &run.id).await, RunState::Completed);
    let spec = seen.lock().unwrap().remove(0);
    for sent in [&spec.prompt, &spec.user_prompt] {
        assert!(
            !sent.contains(&secret),
            "the engine read the secret: {sent}"
        );
        assert!(
            sent.contains("deploy with [hidden: GitHub token] now"),
            "{sent}"
        );
    }
    assert!(!spec.context.contains(&secret));
    assert!(
        timeline(&f).contains(&secret),
        "the person's thread keeps what they typed"
    );
}

#[tokio::test]
async fn a_queued_follow_up_reaches_the_engine_masked_and_stays_queued_as_typed() {
    let secret = a_secret();
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let release = Arc::new(tokio::sync::Notify::new());
    let gate = release.clone();
    let record = seen.clone();
    let script: Script = Arc::new(move |spec: RunSpec, events, _| {
        record.lock().unwrap().push(spec);
        let gate = gate.clone();
        tokio::spawn(async move {
            gate.notified().await;
            let _ = events.send(text("done")).await;
            let _ = events
                .send(EngineEvent::Finished(EngineOutcome::Completed))
                .await;
        })
    });
    let f = fixture(script).await;
    let first = f.core.runs.admit(&f.thread, "go", "t").unwrap();
    until_state(&f.core, &first.id, RunState::Running).await;
    let queued = f
        .core
        .runs
        .enqueue(&f.thread, &format!("then use {secret}"), &[])
        .unwrap();
    assert_eq!(
        f.core.store.list_queue(&f.thread).unwrap()[0].prompt,
        format!("then use {secret}"),
        "the queue keeps the typed text ({})",
        queued.id
    );
    release.notify_one();
    assert_eq!(
        until_terminal(&f.core, &first.id).await,
        RunState::Completed
    );
    for _ in 0..2000 {
        if seen.lock().unwrap().len() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    release.notify_one();
    let specs = seen.lock().unwrap();
    assert_eq!(specs.len(), 2);
    assert!(!specs[1].prompt.contains(&secret));
    assert!(
        specs[1]
            .user_prompt
            .contains("then use [hidden: GitHub token]")
    );
}

#[tokio::test]
async fn a_steer_reaches_the_engine_masked_and_the_timeline_keeps_it_as_typed() {
    let secret = a_secret();
    let log = Arc::new(std::sync::Mutex::new(Vec::new()));
    let release = Arc::new(tokio::sync::Notify::new());
    let gate = release.clone();
    let script: Script = Arc::new(move |_, events, _| {
        let gate = gate.clone();
        tokio::spawn(async move {
            gate.notified().await;
            let _ = events.send(text("done")).await;
            let _ = events
                .send(EngineEvent::Finished(EngineOutcome::Completed))
                .await;
        })
    });
    let f = fixture_steering(
        script,
        "echo updated",
        Duration::from_secs(30),
        None,
        capabilities(),
        false,
        Some(log.clone()),
    )
    .await;
    let run = f.core.runs.admit(&f.thread, "go", "t").unwrap();
    until_state(&f.core, &run.id, RunState::Running).await;
    f.core
        .runs
        .steer(&f.thread, Some("s1"), &format!("also {secret}"))
        .unwrap();
    for _ in 0..2000 {
        if !log.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    let got = log.lock().unwrap().clone();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].prompt, "also [hidden: GitHub token]");
    assert!(
        timeline(&f).contains(&secret),
        "the timeline keeps the typed steer"
    );
    // A refused steer goes back to the queue as the person typed it.
    let scope = RunScope {
        run_id: run.id.clone(),
        thread_id: f.thread.clone(),
        engine: "script".into(),
        family: "script".into(),
        write_scope: None,
        mode: PermissionMode::Supervised,
        plan: false,
    };
    f.core.runs.requeue_steer(&scope, &got[0]);
    let queued = f.core.store.list_queue(&f.thread).unwrap();
    assert_eq!(queued[0].prompt, format!("also {secret}"));
    release.notify_one();
}

#[tokio::test]
async fn the_agents_switch_and_a_person_pattern_decide_what_the_engine_reads() {
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let f = fixture(recording(&seen)).await;
    f.core
        .change_redaction_surfaces(
            "t",
            &orbyn_contracts::RedactionSurfaceChange {
                terminal: None,
                agents: Some(false),
            },
        )
        .unwrap();
    let secret = a_secret();
    let run = f
        .core
        .runs
        .admit(&f.thread, &format!("use {secret}"), "t")
        .unwrap();
    until_terminal(&f.core, &run.id).await;
    assert!(
        seen.lock().unwrap()[0].user_prompt.contains(&secret),
        "with the switch off nothing is hidden"
    );
}

#[tokio::test]
async fn a_person_pattern_is_masked_for_the_engine_before_any_cut() {
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let f = fixture(recording(&seen)).await;
    f.core
        .save_redaction_pattern(
            "t",
            None,
            &orbyn_contracts::RedactionPatternInput {
                name: "Project code".into(),
                pattern: "corp_[a-z]{6}".into(),
                kind: orbyn_contracts::PatternKind::Regex,
                applies_to: orbyn_contracts::PatternScope::Anywhere,
                accept_broad: None,
            },
        )
        .unwrap();
    let run = f
        .core
        .runs
        .admit(&f.thread, "the code is corp_abcdef ok", "t")
        .unwrap();
    until_terminal(&f.core, &run.id).await;
    let sent = seen.lock().unwrap()[0].user_prompt.clone();
    assert_eq!(sent, "the code is [hidden: Project code] ok");
    assert!(timeline(&f).contains("corp_abcdef"));
}

/// What each attached file held at the path the engine was told, read while the run's folder
/// still exists (it is removed when the run ends).
type Read = Arc<std::sync::Mutex<Vec<(String, Vec<u8>)>>>;

fn reading(read: &Read) -> Script {
    let read = read.clone();
    Arc::new(move |spec: RunSpec, events, _| {
        for line in spec.context.lines().filter_map(|l| l.strip_prefix("- ")) {
            if let Ok(bytes) = std::fs::read(line) {
                read.lock().unwrap().push((line.to_owned(), bytes));
            }
        }
        tokio::spawn(async move {
            let _ = events.send(text("done")).await;
            let _ = events
                .send(EngineEvent::Finished(EngineOutcome::Completed))
                .await;
        })
    })
}

/// Attaches `bytes` as `name`, runs one turn and returns what the engine read, the stored file's
/// path and the timeline.
async fn attach_and_run(
    name: &str,
    bytes: &[u8],
    switch_off: bool,
) -> (Vec<(String, Vec<u8>)>, String) {
    let read: Read = Arc::default();
    let f = fixture(reading(&read)).await;
    if switch_off {
        f.core
            .change_redaction_surfaces(
                "t",
                &orbyn_contracts::RedactionSurfaceChange {
                    terminal: None,
                    agents: Some(false),
                },
            )
            .unwrap();
    }
    let attachment = f
        .core
        .add_attachment(&f.thread, name, "application/octet-stream", bytes)
        .unwrap();
    let stored = f.core.store.list_attachments(&f.thread).unwrap()[0]
        .1
        .clone();
    let run = f
        .core
        .runs
        .admit_with(&f.thread, "look", std::slice::from_ref(&attachment.id), "t")
        .unwrap();
    assert_eq!(until_terminal(&f.core, &run.id).await, RunState::Completed);
    // The person's stored attachment and the thread keep the original.
    assert_eq!(std::fs::read(&stored).unwrap(), bytes);
    assert!(timeline(&f).contains(&attachment.id));
    let got = read.lock().unwrap().clone();
    (got, stored)
}

#[tokio::test]
async fn an_attached_text_file_reaches_the_engine_masked_and_the_original_stays() {
    let secret = a_secret();
    let body = format!("REMOTE={secret}\nMODE=dev\n");
    let (got, stored) = attach_and_run(".env", body.as_bytes(), false).await;
    assert_eq!(got.len(), 1);
    let (path, bytes) = &got[0];
    assert_ne!(
        path, &stored,
        "the engine gets a copy, not the person's file"
    );
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(!text.contains(&secret), "{text}");
    assert_eq!(text, "REMOTE=[hidden: GitHub token]\nMODE=dev\n");
}

#[tokio::test]
async fn an_attached_text_file_with_nothing_to_hide_is_passed_by_its_own_path() {
    let (got, stored) = attach_and_run("notes.txt", b"hello\n", false).await;
    assert_eq!(got, vec![(stored, b"hello\n".to_vec())]);
}

#[tokio::test]
async fn a_binary_attachment_is_passed_unchanged() {
    let mut bytes = a_secret().into_bytes();
    bytes.extend_from_slice(&[0, 159, 146, 150]);
    let (got, stored) = attach_and_run("blob.bin", &bytes, false).await;
    assert_eq!(got, vec![(stored, bytes)]);
}

#[tokio::test]
async fn an_attached_file_over_the_cap_is_passed_unchanged() {
    let mut body = format!("TOKEN={}\n", a_secret()).into_bytes();
    body.resize(super::masked_files::CAP + 1, b'x');
    let (got, stored) = attach_and_run("big.txt", &body, false).await;
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, stored);
    assert_eq!(got[0].1, body);
}

#[tokio::test]
async fn with_the_agents_switch_off_an_attached_file_is_passed_unchanged() {
    let body = format!("TOKEN={}\n", a_secret());
    let (got, stored) = attach_and_run(".env", body.as_bytes(), true).await;
    assert_eq!(got, vec![(stored, body.into_bytes())]);
}

#[tokio::test]
async fn when_the_redaction_settings_cannot_be_read_nothing_reaches_the_engine() {
    let secret = a_secret();
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let read: Read = Arc::default();
    let both: Script = {
        let (recorder, reader) = (recording(&seen), reading(&read));
        Arc::new(move |spec, events, cancel| {
            reader(spec.clone(), events.clone(), cancel.clone());
            recorder(spec, events, cancel)
        })
    };
    let f = fixture(both).await;
    let body = format!("REMOTE={secret}\n");
    let attachment = f
        .core
        .add_attachment(
            &f.thread,
            ".env",
            "application/octet-stream",
            body.as_bytes(),
        )
        .unwrap();
    // The redaction row holds a value that is not text, so reading it fails while every other
    // setting still reads: admission passes and the failure is the settings read of the turn.
    rusqlite::Connection::open(f.core.config.data_dir.join("orbyn.sqlite3"))
        .unwrap()
        .execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('redaction', x'00ff')",
            [],
        )
        .unwrap();
    assert!(f.core.store.redaction_json().is_err());
    for files in [vec![attachment.id.clone()], Vec::new()] {
        let run = f
            .core
            .runs
            .admit_with(&f.thread, &format!("deploy with {secret}"), &files, "t")
            .unwrap();
        assert_eq!(until_terminal(&f.core, &run.id).await, RunState::Failed);
        assert!(f.core.store.run(&run.id).unwrap().error.is_some());
    }
    assert!(seen.lock().unwrap().is_empty(), "the engine was started");
    assert!(read.lock().unwrap().is_empty(), "the engine read a file");
}
