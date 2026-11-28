//! A real `Core` with one fake engine that plays the worker. Nothing here starts a process
//! except `git` and the checks themselves: the engine is a closure that writes files in the
//! checkout and calls `crew_report` through the same MCP service a real engine would.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;

use async_trait::async_trait;
use orbyn_contracts::{
    AuthState, CreateThread, EngineTier, ModelInfo, PermissionMode, WorkspaceSetup,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::fake::{caps, model};
use crate::agents::driver::{
    ApprovalSpec, EngineDescriptor, EngineDriver, EngineEvent, EngineOutcome, EngineProbe,
    ModelCatalog, RunHandle, RunSpec, SteerMessage,
};
use crate::app::Core;
use crate::error::Result;
use crate::governance::approvals::RunScope;
use crate::testing::{TempDir, run_git};
use crate::tools::mcp::McpBinding;
use crate::{CoreConfig, store::CrewTask};

/// How long [`World::until`] waits before it panics. It is not one of the production budgets: a test's wait must
/// survive a shared machine, which the archived full-suite log showed tens of times slower than idle. Three
/// minutes is far past any loaded start and short enough that a deadlocked test still fails in the run. A test
/// that polls the store in its own loop borrows it, so no wait in this module is decided by the run's length.
pub(crate) const WAIT_LIMIT: Duration = Duration::from_secs(3 * 60);

/// A run and the restriction it was started under: `(thread, rules)`.
pub type Confined = (String, Option<crate::host::confine::Confinement>);

/// What a fake worker does in one turn.
pub enum Turn {
    /// Reports with these arguments after doing its work.
    Report(Value),
    /// Ends the turn without reporting.
    Silent,
    /// The engine fails.
    Crash,
    /// Never ends, so the task stays `running` and keeps its lease.
    Hang,
    /// Calls these tools in order, such as `crew_finding` a few times and then `crew_report`.
    Steps(Vec<(&'static str, Value)>),
    /// Reports, then keeps its turn open until told, as a worker that goes on working would.
    Linger(Value, Arc<tokio::sync::Notify>),
    /// Says something first, as a real engine does, then reports. A background crew's worker must be heard to answer; the other
    /// turns above produce no assistant output at all.
    Say(Value),
}

/// The worker's work: given the turn number its world hands it and the checkout, do it and say what
/// happens next. The number is the worker's own turn (from 0) in a world made by [`world_own_turns`],
/// and the world's start order in a world that still dispatches on it (see [`WorkerTurns`]).
pub type Script = Arc<dyn Fn(usize, &std::path::Path) -> Turn + Send + Sync>;

/// Each worker's own turns, counted from 0 and keyed by the worker's thread, so one worker's turns
/// cannot shift another worker's (#1750). The fake updates it for every worker run; a world made by
/// [`world_own_turns`] hands a worker this number instead of the shared start order.
pub type WorkerTurns = Arc<Mutex<BTreeMap<String, usize>>>;

/// Every worker run the fake started and the turn number its script was handed: `(thread, turn)`.
pub type HandedTurns = Arc<Mutex<Vec<(String, usize)>>>;

#[derive(Clone)]
pub enum ApprovalTurn {
    Reply(String),
    Hang,
    Permission,
    Session,
    OverBudget,
    Fail,
}

pub type ApprovalTurns = Arc<Mutex<BTreeMap<String, ApprovalTurn>>>;
pub type ApprovalSpecs = Arc<Mutex<Vec<(String, ApprovalSpec)>>>;

#[derive(Clone)]
pub struct Fake {
    approval_turns: ApprovalTurns,
    approval_specs: ApprovalSpecs,
    id: String,
    family: String,
    /// Takes input while a turn runs, as Claude and Codex do; false is an engine that does not.
    steerable: bool,
    /// Takes this many steers per run, then reports every later one as refused (SteerFailed);
    /// `None` takes every steer, as an engine that accepts input does.
    refuses_after: Option<usize>,
    /// Lists a lead-tier model as well, so a review role can be routed to it.
    tiered: bool,
    /// What was handed to a running turn's input: `(thread, text)`.
    steered: Arc<Mutex<Vec<(String, String)>>>,
    core: Arc<OnceLock<Weak<Core>>>,
    script: Script,
    /// The world's start order: which worker the world started, counted across every worker. Kept as its
    /// own item for the scripts that still dispatch on it; a new test should assert on a worker's own turns.
    schedule: Arc<AtomicUsize>,
    /// Each worker's own turn count, from 0, keyed by its thread.
    turns: WorkerTurns,
    /// Hand the script each worker's own turn instead of the world's start order.
    own_turns: bool,
    /// The number each worker run was handed, so a test can assert what a worker's script saw.
    handed: HandedTurns,
    /// Every prompt any thread was sent: `(thread, text)`.
    pub prompts: Arc<Mutex<Vec<(String, String)>>>,
    /// What `crew_report` answered each time.
    pub answers: Arc<Mutex<Vec<String>>>,
    /// The restriction each worker run was started under: `(thread, rules)`.
    confines: Arc<Mutex<Vec<Confined>>>,
    /// The crew task each run was started for, none for a run that is not a worker's: `(thread, task)`.
    crew_tasks: CrewTasks,
}

/// `(thread, task)` for each run the fake engine started: the task is none for a run that is not a worker's.
pub type CrewTasks = Arc<Mutex<Vec<(String, Option<String>)>>>;

impl Fake {
    /// The number this worker run's script is handed. The worker's own count always moves, so every worker
    /// has its own counter whatever the world hands its script; a world that keeps the shared schedule still
    /// numbers a worker by the workers the world started before it.
    fn next_turn(&self, thread: &str) -> usize {
        let own = {
            let mut turns = self.turns.lock().unwrap();
            let count = turns.entry(thread.to_owned()).or_insert(0);
            let own = *count;
            *count += 1;
            own
        };
        let handed = if self.own_turns {
            own
        } else {
            self.schedule.fetch_add(1, Ordering::SeqCst)
        };
        self.handed
            .lock()
            .unwrap()
            .push((thread.to_owned(), handed));
        handed
    }
}

#[async_trait]
impl EngineDriver for Fake {
    fn family(&self) -> String {
        self.family.clone()
    }

    fn instance(
        &self,
        instance: &orbyn_contracts::ProviderInstance,
    ) -> Option<Arc<dyn EngineDriver>> {
        let mut driver = self.clone();
        driver.id.clone_from(&instance.id);
        Some(Arc::new(driver))
    }
    fn supports_approval(&self) -> bool {
        true
    }

    async fn start_approval(
        &self,
        spec: ApprovalSpec,
        events: mpsc::Sender<EngineEvent>,
    ) -> Result<RunHandle> {
        self.approval_specs
            .lock()
            .unwrap()
            .push((self.id.clone(), spec.clone()));
        let turn = self
            .approval_turns
            .lock()
            .unwrap()
            .get(&self.id)
            .cloned()
            .unwrap_or(ApprovalTurn::Fail);
        let cancel = tokio_util::sync::CancellationToken::new();
        let stop = cancel.clone();
        let task = tokio::spawn(async move {
            let event = match turn {
                ApprovalTurn::Reply(text) => {
                    if events.send(EngineEvent::Usage(orbyn_contracts::ThreadUsage { total_tokens: Some(20), context_window: None, raw: json!({ "input_tokens": 10, "output_tokens": 10, "estimated_cost_usd": 0.001 }) })).await.is_err() { return; }
                    for event in [
                        orbyn_contracts::AgUiEvent::TextMessageStart {
                            message_id: "approval".into(),
                            role: "assistant".into(),
                        },
                        orbyn_contracts::AgUiEvent::TextMessageContent {
                            message_id: "approval".into(),
                            delta: text,
                        },
                        orbyn_contracts::AgUiEvent::TextMessageEnd {
                            message_id: "approval".into(),
                        },
                    ] {
                        if events.send(event.into()).await.is_err() {
                            return;
                        }
                    }
                    EngineEvent::Finished(EngineOutcome::Completed)
                }
                ApprovalTurn::Hang => {
                    stop.cancelled().await;
                    EngineEvent::Finished(EngineOutcome::Cancelled)
                }
                ApprovalTurn::Session => EngineEvent::Session("forbidden-session".into()),
                ApprovalTurn::OverBudget => EngineEvent::Usage(orbyn_contracts::ThreadUsage {
                    total_tokens: Some(spec.max_reported_tokens + 1),
                    context_window: None,
                    raw: json!({}),
                }),
                ApprovalTurn::Permission => {
                    let (reply, _answer) = tokio::sync::oneshot::channel();
                    EngineEvent::Permission(crate::agents::driver::PermissionRequest {
                        kind: orbyn_contracts::ActionKind::Command,
                        target: "must-not-execute".into(),
                        summary: "forbidden".into(),
                        detail: json!({ "command": "must-not-execute" }),
                        paths: vec![],
                        reply,
                    })
                }
                ApprovalTurn::Fail => {
                    EngineEvent::Finished(EngineOutcome::Failed("unavailable".into()))
                }
            };
            let _ = events.send(event).await;
        });
        Ok(RunHandle {
            cancel,
            task,
            input: None,
            control: None,
        })
    }
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            id: self.id.clone(),
            name: self.id.clone(),
            tier: EngineTier::Acp,
            capabilities: caps(),
        }
    }

    async fn probe(&self) -> EngineProbe {
        EngineProbe {
            path: Some("/bin/sh".into()),
            version: Some("1.0.0".into()),
            auth: Some(AuthState::NotApplicable),
            ..EngineProbe::default()
        }
    }

    async fn models(&self) -> Result<Option<ModelCatalog>> {
        let mut models: Vec<ModelInfo> = vec![model(&format!("{}-model", self.id), &self.id, true)];
        if self.tiered {
            // The review tests assign each fake's default `-model` to the lead tier (see `world_tiered`); a nondefault `-lead`
            // model stays listed to show that an assignment does not promote it. A lead-tier approver run asks for
            // `medium` effort, so both models offer it.
            let mut lead = model(&format!("{}-lead", self.id), &self.id, false);
            lead.options.push(crate::agents::driver::effort_option(
                ["medium"],
                Some("medium"),
            ));
            models.push(lead);
            models[0].options.push(crate::agents::driver::effort_option(
                ["medium"],
                Some("medium"),
            ));
        }
        Ok(Some(ModelCatalog::engine(models)))
    }

    async fn start_run(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<EngineEvent>,
    ) -> Result<RunHandle> {
        self.prompts
            .lock()
            .unwrap()
            .push((spec.thread_id.clone(), spec.prompt.clone()));
        self.confines
            .lock()
            .unwrap()
            .push((spec.thread_id.clone(), spec.confine.clone()));
        self.crew_tasks
            .lock()
            .unwrap()
            .push((spec.thread_id.clone(), spec.crew_task.clone()));
        let core = self.core.get().and_then(Weak::upgrade).expect("a core");
        let spec_thread = spec.thread_id.clone();
        let worker = core
            .store
            .crew_task_for_worker(&spec.thread_id)
            .unwrap()
            .is_some();
        let turn = worker.then(|| self.next_turn(&spec.thread_id));
        let script = self.script.clone();
        let answers = self.answers.clone();
        let cancel = tokio_util::sync::CancellationToken::new();
        let stop = cancel.clone();
        let refuses_after = self.refuses_after;
        let refusal_events = events.clone();
        let task = tokio::spawn(async move {
            let outcome = match turn {
                None => EngineOutcome::Completed,
                Some(n) => match script(n, &spec.checkout) {
                    Turn::Hang => {
                        stop.cancelled().await;
                        EngineOutcome::Cancelled
                    }
                    Turn::Crash => EngineOutcome::Failed("the engine crashed".into()),
                    Turn::Silent => EngineOutcome::Completed,
                    Turn::Report(args) => {
                        send_report(&core, &spec, args, &answers).await;
                        EngineOutcome::Completed
                    }
                    Turn::Steps(steps) => {
                        for (tool, args) in steps {
                            send_tool(&core, &spec, tool, args, &answers).await;
                        }
                        EngineOutcome::Completed
                    }
                    Turn::Say(args) => {
                        for event in [
                            orbyn_contracts::AgUiEvent::TextMessageStart {
                                message_id: "say".into(),
                                role: "assistant".into(),
                            },
                            orbyn_contracts::AgUiEvent::TextMessageContent {
                                message_id: "say".into(),
                                delta: "Working on it.".into(),
                            },
                            orbyn_contracts::AgUiEvent::TextMessageEnd {
                                message_id: "say".into(),
                            },
                        ] {
                            let _ = events.send(event.into()).await;
                        }
                        send_report(&core, &spec, args, &answers).await;
                        EngineOutcome::Completed
                    }
                    Turn::Linger(args, release) => {
                        send_report(&core, &spec, args, &answers).await;
                        release.notified().await;
                        EngineOutcome::Completed
                    }
                },
            };
            let _ = events.send(EngineEvent::Finished(outcome)).await;
        });
        let input = self.steerable.then(|| {
            let (send, mut receive) = mpsc::unbounded_channel::<SteerMessage>();
            let (steered, thread) = (self.steered.clone(), spec_thread);
            let events = refusal_events;
            tokio::spawn(async move {
                let mut accepted = 0;
                while let Some(steer) = receive.recv().await {
                    steered
                        .lock()
                        .unwrap()
                        .push((thread.clone(), steer.prompt.clone()));
                    let refuse = match refuses_after {
                        None => false,
                        Some(after) if accepted < after => {
                            accepted += 1;
                            false
                        }
                        Some(_) => true,
                    };
                    // An engine that cannot take the text reports the refusal back, as a failed
                    // Codex turn/steer and a rejected host steering request do.
                    if refuse {
                        let _ = events.send(EngineEvent::SteerFailed(steer)).await;
                    }
                }
            });
            send
        });
        Ok(RunHandle {
            cancel,
            task,
            input,
            control: None,
        })
    }
}

async fn send_report(core: &Core, spec: &RunSpec, args: Value, answers: &Mutex<Vec<String>>) {
    send_tool(core, spec, "crew_report", args, answers).await;
}

async fn send_tool(
    core: &Core,
    spec: &RunSpec,
    tool: &str,
    args: Value,
    answers: &Mutex<Vec<String>>,
) {
    let token = spec.mcp_url.as_deref().and_then(|u| u.rsplit('/').next());
    let message = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": { "name": tool, "arguments": args } });
    let sent = core.mcp.handle(token.expect("an mcp url"), &message).await;
    assert!(sent.is_ok(), "the worker's token is valid");
    answers.lock().unwrap().push(format!("{sent:?}"));
}

pub struct World {
    pub approval_turns: ApprovalTurns,
    pub approval_specs: ApprovalSpecs,
    pub _dir: TempDir,
    pub core: Arc<Core>,
    pub repo: std::path::PathBuf,
    pub lead: String,
    pub lead_token: String,
    pub prompts: Arc<Mutex<Vec<(String, String)>>>,
    /// Read only by the Unix-only leak test, so it is dead code on Windows.
    #[cfg_attr(not(unix), allow(dead_code))]
    pub answers: Arc<Mutex<Vec<String>>>,
    /// What running turns were handed mid-turn: `(thread, text)`.
    pub steered: Arc<Mutex<Vec<(String, String)>>>,
    /// The restriction each run was started under: `(thread, rules)`.
    pub confines: Arc<Mutex<Vec<Confined>>>,
    pub crew_tasks: CrewTasks,
    /// Each worker's own turn count, from 0, keyed by its thread.
    pub turns: WorkerTurns,
    /// Every worker run and the turn number its script was handed: `(thread, turn)`.
    pub handed: HandedTurns,
}

/// A core with the Crew Preview flag as asked, one git project with a first commit, a lead
/// thread and a fake engine `alpha` that plays `script`.
pub async fn world(preview: bool, script: Script) -> World {
    world_with(preview, script, &[("alpha", false)]).await
}

/// `world`, with these fake engines: each is an id and whether it takes input mid-turn. The
/// lead runs on the first.
pub async fn world_with(preview: bool, script: Script, engines: &[(&str, bool)]) -> World {
    world_tiered(preview, script, engines, false).await
}

/// `world_with`, where every engine refuses the steers it is handed (SteerFailed), as an engine
/// whose turn cannot be steered does.
pub async fn world_refusing_steers(
    preview: bool,
    script: Script,
    engines: &[(&str, bool)],
) -> World {
    world_refusing_steers_after(preview, script, engines, 0).await
}

/// `world_with`, where every engine takes its first `accepted` steers and refuses every later one
/// (SteerFailed), as an engine answers the steer that reaches its running turn.
pub async fn world_refusing_steers_after(
    preview: bool,
    script: Script,
    engines: &[(&str, bool)],
    accepted: usize,
) -> World {
    world_full(
        preview,
        script,
        engines,
        WorldKnobs {
            refuses_after: Some(accepted),
            ..WorldKnobs::default()
        },
    )
    .await
}

/// `world_with`, with an explicit ACP lead assignment pinned to each fake's default
/// model. Builder admission remains; a nondefault model is not promoted by that assignment.
pub async fn world_tiered(
    preview: bool,
    script: Script,
    engines: &[(&str, bool)],
    tiered: bool,
) -> World {
    world_full(
        preview,
        script,
        engines,
        WorldKnobs {
            tiered,
            ..WorldKnobs::default()
        },
    )
    .await
}

pub async fn world_publication(script: Script, timeout: Option<std::time::Duration>) -> World {
    world_full(
        true,
        script,
        &[("alpha", false), ("beta", false)],
        WorldKnobs {
            tiered: true,
            approval_timeout: timeout,
            ..WorldKnobs::default()
        },
    )
    .await
}

/// `world`, with the restriction on workers left on, as it ships: a start is refused where
/// none can be applied, and a worker run is handed its rules.
pub async fn world_confined(script: Script) -> World {
    world_full(
        true,
        script,
        &[("alpha", false)],
        WorldKnobs {
            confine: true,
            ..WorldKnobs::default()
        },
    )
    .await
}

#[cfg(target_os = "macos")]
/// `world_confined`, with these fake engines (an id ending in `codex` is of the Codex family).
pub async fn world_confined_with(script: Script, engines: &[(&str, bool)]) -> World {
    world_full(
        true,
        script,
        engines,
        WorldKnobs {
            confine: true,
            ..WorldKnobs::default()
        },
    )
    .await
}

pub async fn world_forge(script: Script) -> World {
    world_full(
        true,
        script,
        &[("alpha", false)],
        WorldKnobs {
            merge_checks: vec!["ci".into()],
            ..WorldKnobs::default()
        },
    )
    .await
}

pub async fn world_forge_review(script: Script) -> World {
    world_full(
        true,
        script,
        &[("alpha", false), ("beta", false)],
        WorldKnobs {
            tiered: true,
            merge_checks: vec!["ci".into()],
            ..WorldKnobs::default()
        },
    )
    .await
}

/// `world`, where a worker's script is handed that worker's own turn number (from 0) instead of the
/// world's start order, so one worker's turns cannot shift another worker's replies (#1750).
pub async fn world_own_turns(preview: bool, script: Script) -> World {
    world_full(
        preview,
        script,
        &[("alpha", false)],
        WorldKnobs {
            own_turns: true,
            ..WorldKnobs::default()
        },
    )
    .await
}

/// The rarely changed parts of a test world, so a new one adds a field here and no argument to every call.
#[derive(Default)]
struct WorldKnobs {
    tiered: bool,
    confine: bool,
    merge_checks: Vec<String>,
    approval_timeout: Option<std::time::Duration>,
    refuses_after: Option<usize>,
    own_turns: bool,
}

async fn world_full(
    preview: bool,
    script: Script,
    engines: &[(&str, bool)],
    knobs: WorldKnobs,
) -> World {
    let WorldKnobs {
        tiered,
        confine,
        merge_checks,
        approval_timeout,
        refuses_after,
        own_turns,
    } = knobs;
    let dir = TempDir::new();
    let library = crate::testing::library(&dir.path().join("library"));
    let mut config = CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
    config.engines.lookup_latest = false;
    let absent = dir.path().join("absent-native");
    assert!(absent.is_absolute() && absent.starts_with(dir.path()) && !absent.exists());
    config.engines.claude_program = absent.join("claude.exe").to_string_lossy().into_owned();
    config.engines.codex_program = absent.join("codex.exe").to_string_lossy().into_owned();
    config.engines.known_engine_ids = Some(Vec::new());
    config.crew_preview = preview;
    config.code_mode = true;
    if let Some(timeout) = approval_timeout {
        config.approval_timeout = timeout;
    }
    config.crew_merge_required_checks = merge_checks;
    config.crew_confine_workers = confine;
    if tiered {
        let words = crate::crew::TierWords {
            lead: Some("lead".into()),
            worker: Some("model".into()),
            scout: Some("scout".into()),
        };
        config.crew_tiers = Some(crate::crew::RouterConfig {
            worker_provider: Default::default(),
            tiers: engines
                .iter()
                .map(|(id, _)| ((*id).to_owned(), words.clone()))
                .collect(),
            acp_tiers: engines
                .iter()
                .map(|(id, _)| {
                    (
                        (*id).to_owned(),
                        crate::crew::AcpTierAssignment {
                            tier: orbyn_contracts::RouteTier::Lead,
                            model: format!("{id}-model"),
                        },
                    )
                })
                .collect(),
        });
    }
    let core = Arc::new(Core::open(config).unwrap());
    core.mcp.bind_host(&core);
    let cell = Arc::new(OnceLock::new());
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let answers = Arc::new(Mutex::new(Vec::new()));
    let steered = Arc::new(Mutex::new(Vec::new()));
    let confines = Arc::new(Mutex::new(Vec::new()));
    let crew_tasks = Arc::new(Mutex::new(Vec::new()));
    let turns: WorkerTurns = Arc::default();
    let handed: HandedTurns = Arc::default();
    let schedule: Arc<AtomicUsize> = Arc::default();
    let approval_turns = Arc::new(Mutex::new(BTreeMap::new()));
    let approval_specs = Arc::new(Mutex::new(Vec::new()));
    for (id, steerable) in engines {
        core.engines.register(Arc::new(Fake {
            approval_turns: approval_turns.clone(),
            approval_specs: approval_specs.clone(),
            id: (*id).to_owned(),
            // An id that ends in `codex` is a Codex engine, so the run-start refusal for that family applies to it.
            family: if id.ends_with("codex") { "codex" } else { id }.to_owned(),
            steerable: *steerable,
            refuses_after,
            tiered,
            steered: steered.clone(),
            core: cell.clone(),
            script: script.clone(),
            schedule: schedule.clone(),
            turns: turns.clone(),
            own_turns,
            handed: handed.clone(),
            prompts: prompts.clone(),
            answers: answers.clone(),
            confines: confines.clone(),
            crew_tasks: crew_tasks.clone(),
        }));
    }
    cell.set(Arc::downgrade(&core)).unwrap();
    core.workspace
        .setup(WorkspaceSetup {
            url: None,
            adopt_path: Some(library.to_string_lossy().into_owned()),
        })
        .await
        .unwrap();
    let project = core.new_project("project", false, None).await.unwrap();
    core.store.trust_project(&project.id, "test").unwrap();
    let repo = std::path::PathBuf::from(&project.path);
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/base.txt"), "base\n").unwrap();
    run_git(&repo, &["add", "."]);
    run_git(
        &repo,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.test",
            "commit",
            "-q",
            "-m",
            "base",
        ],
    );
    let lead = core
        .create_thread(CreateThread {
            project_id: Some(project.id.clone()),
            engine: engines[0].0.into(),
            model: None,
            title: Some("lead".into()),
            worktree: None,
            skill_profile: None,
            base_branch: None,
            effort: None,
            options: BTreeMap::new(),
            branch_name_seed: None,
        })
        .await
        .unwrap();
    let lead_token = bind(&core, &lead.id, "run-lead");
    World {
        approval_turns,
        approval_specs,
        _dir: dir,
        core,
        repo,
        lead: lead.id,
        lead_token,
        prompts,
        answers,
        steered,
        confines,
        crew_tasks,
        turns,
        handed,
    }
}

/// A token for a thread, as the run manager would register for its run.
pub fn bind(core: &Core, thread: &str, run: &str) -> String {
    core.mcp
        .register(McpBinding {
            scope: RunScope {
                run_id: run.into(),
                thread_id: thread.into(),
                engine: "alpha".into(),
                family: "alpha".into(),
                write_scope: None,
                mode: PermissionMode::Supervised,
                plan: false,
            },
            permission_prompt: false,
            skills: Vec::new(),
        })
        .unwrap()
}

impl World {
    /// One tool call with `token`; the tool result's text and whether it is an error.
    pub async fn raw(&self, token: &str, tool: &str, arguments: Value) -> (String, bool) {
        let message = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": tool, "arguments": arguments } });
        let reply = self
            .core
            .mcp
            .handle(token, &message)
            .await
            .unwrap()
            .unwrap();
        (
            reply["result"]["content"][0]["text"]
                .as_str()
                .map_or_else(|| reply.to_string(), str::to_owned),
            reply["result"]["isError"] == json!(true) || reply.get("error").is_some(),
        )
    }

    /// One tool call with `token`; the tool result's JSON and whether it is an error.
    pub async fn call(&self, token: &str, tool: &str, arguments: Value) -> (Value, bool) {
        let message = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": tool, "arguments": arguments } });
        let reply = self
            .core
            .mcp
            .handle(token, &message)
            .await
            .unwrap()
            .unwrap();
        let Some(text) = reply["result"]["content"][0]["text"].as_str() else {
            return (reply, true);
        };
        // A tool that fails before it answers (its budget ran out) sends plain text, so show it.
        let answer = serde_json::from_str(text).unwrap_or_else(|why| {
            panic!("{tool} answered with text that is not JSON ({why}): {text}")
        });
        (answer, reply["result"]["isError"] == json!(true))
    }

    /// The lead starts a task.
    pub async fn start(&self, brief: Value, key: Value) -> (Value, bool) {
        self.call(
            &self.lead_token,
            "crew_start",
            json!({ "brief": brief, "answer_key": key }),
        )
        .await
    }

    pub fn task(&self, id: &str) -> CrewTask {
        self.core.store.crew_task(id).unwrap().unwrap()
    }

    /// Waits for `done`, which reads the world, on a clock this test owns rather than a production budget: a machine
    /// shared with other builds can be tens of times slower than an idle one, and a start that takes 20 seconds there is
    /// not a bug in the test. The panic says how long it waited, so a real hang still fails by name.
    pub async fn until(&self, what: &str, mut done: impl FnMut(&World) -> bool) {
        let give_up = std::time::Instant::now() + WAIT_LIMIT;
        while !done(self) {
            assert!(
                std::time::Instant::now() < give_up,
                "never happened in {WAIT_LIMIT:?}: {what}"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    pub async fn until_state(&self, id: &str, state: &str) {
        self.until(state, |w| w.task(id).state == state).await;
    }

    /// Every prompt a thread was sent.
    pub fn sent_to(&self, thread: &str) -> Vec<String> {
        self.prompts
            .lock()
            .unwrap()
            .iter()
            .filter(|(to, _)| to == thread)
            .map(|(_, text)| text.clone())
            .collect()
    }

    pub fn worker(&self, task: &str) -> String {
        self.core
            .store
            .crew_run(task)
            .unwrap()
            .unwrap()
            .worker_thread_id
            .unwrap()
    }

    /// The turn numbers this worker's script was handed, in the order it ran.
    pub fn turns_of(&self, thread: &str) -> Vec<usize> {
        self.handed
            .lock()
            .unwrap()
            .iter()
            .filter(|(to, _)| to == thread)
            .map(|(_, turn)| *turn)
            .collect()
    }

    /// How many turns this worker has taken: its own count, whichever number its script saw.
    pub fn own_turn_count(&self, thread: &str) -> usize {
        self.turns.lock().unwrap().get(thread).copied().unwrap_or(0)
    }

    /// A real restart: closes this core and opens a new one on the same data folder. Fake engines
    /// are not registered again, since a stored record and its replay only need the core.
    pub async fn restart(self) -> World {
        let World {
            _dir,
            core,
            lead,
            repo,
            prompts,
            answers,
            steered,
            confines,
            crew_tasks,
            turns,
            handed,
            approval_turns,
            approval_specs,
            ..
        } = self;
        let config = core.config.clone();
        // A process that dies takes its live scripts with it.
        core.code_runs.forget_all();
        core.shutdown().await;
        // An aborted script drops its hold on the core when the runtime next polls it. That is a
        // scheduling moment, not a product budget: wait on the harness clock, so a loaded runtime
        // polling a moment later cannot fail the restart (#1063, tdd 3.7).
        let until = std::time::Instant::now() + WAIT_LIMIT;
        while Arc::strong_count(&core) > 1 && std::time::Instant::now() < until {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let old = Arc::downgrade(&core);
        drop(core);
        assert!(old.upgrade().is_none(), "the old core is gone");
        let core = Arc::new(Core::open(config).expect("the restarted core opens"));
        let lead_token = bind(&core, &lead, "run-lead");
        World {
            approval_turns,
            approval_specs,
            _dir,
            core,
            repo,
            lead,
            lead_token,
            prompts,
            answers,
            steered,
            confines,
            crew_tasks,
            turns,
            handed,
        }
    }
}

/// Makes a running worker's run look like another process's, so the service cannot confirm its stop and a cancel fails closed.
/// Returns the epoch to put back with [`give_back_worker_run`].
pub fn take_worker_run(w: &World, task: &str) -> String {
    let db = rusqlite::Connection::open(w._dir.path().join("data/orbyn.sqlite3")).unwrap();
    db.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
    let run_id = w
        .core
        .store
        .thread(&w.worker(task))
        .unwrap()
        .active_run
        .unwrap()
        .id;
    let own: String = db
        .query_row(
            "SELECT owner_epoch FROM runs WHERE id = ?1",
            [&run_id],
            |row| row.get(0),
        )
        .unwrap();
    db.execute(
        "UPDATE runs SET owner_epoch = 'another-process' WHERE id = ?1",
        [&run_id],
    )
    .unwrap();
    own
}

/// Puts back the epoch [`take_worker_run`] saved, once the stop may be confirmed again.
pub fn give_back_worker_run(w: &World, task: &str, epoch: &str) {
    let db = rusqlite::Connection::open(w._dir.path().join("data/orbyn.sqlite3")).unwrap();
    let run_id = w
        .core
        .store
        .thread(&w.worker(task))
        .unwrap()
        .active_run
        .unwrap()
        .id;
    db.execute(
        "UPDATE runs SET owner_epoch = ?2 WHERE id = ?1",
        rusqlite::params![run_id, epoch],
    )
    .unwrap();
}
/// A brief draft that writes under `src/`, and a key whose checks keep to it. The command
/// check's text is what the leak tests look for.
pub const KEY_MARKER: &str = "orbyn-key-marker-77";

pub fn brief(extra: Value) -> Value {
    let mut brief = json!({
        "kind": "implement",
        "outcome": "Add a parse_duration function.",
        "inputs": [],
        "files": [{ "path": "src/**", "mode": "write" }],
        "allowed_effects": ["read_repo", "write_leased", "run_checks"],
        "stop_when": ["The function is added."],
        "budget": { "turns": 20, "minutes": 30 }
    });
    for (key, value) in extra.as_object().cloned().unwrap_or_default() {
        brief[key] = value;
    }
    brief
}

pub fn key() -> Value {
    json!({
        "checks": [
            { "id": "chk-scope", "kind": "file_scope", "params": { "only": ["src/**"] },
              "proves": "Only leased paths changed." },
            { "id": "chk-run", "kind": "command",
              "params": { "run": format!("echo {KEY_MARKER}") },
              "proves": "The command exits 0." },
            { "id": "chk-probe", "kind": "preview",
              "params": { "url": "http://127.0.0.1:9/health" },
              "proves": "The service answers." }
        ],
        "protected_paths": ["tests/**"]
    })
}

pub fn report(summary: &str) -> Value {
    json!({ "summary": summary })
}
