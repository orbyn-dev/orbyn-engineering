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
    EngineDescriptor, EngineDriver, EngineEvent, EngineOutcome, EngineProbe, ModelCatalog,
    RunHandle, RunSpec,
};
use crate::app::Core;
use crate::error::Result;
use crate::governance::approvals::RunScope;
use crate::testing::{TempDir, run_git};
use crate::tools::mcp::McpBinding;
use crate::{CoreConfig, store::CrewTask};

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
}

/// The worker's work: given the turn's number (from 0) and the checkout, do it and say what
/// happens next.
pub type Script = Arc<dyn Fn(usize, &std::path::Path) -> Turn + Send + Sync>;

pub struct Fake {
    id: String,
    /// Takes input while a turn runs, as Claude and Codex do; false is an engine that does not.
    steerable: bool,
    /// Lists a lead-tier model as well, so a review role can be routed to it.
    tiered: bool,
    /// What was handed to a running turn's input: `(thread, text)`.
    steered: Arc<Mutex<Vec<(String, String)>>>,
    core: Arc<OnceLock<Weak<Core>>>,
    script: Script,
    turns: Arc<AtomicUsize>,
    /// Every prompt any thread was sent: `(thread, text)`.
    pub prompts: Arc<Mutex<Vec<(String, String)>>>,
    /// What `crew_report` answered each time.
    pub answers: Arc<Mutex<Vec<String>>>,
    /// The restriction each worker run was started under: `(thread, rules)`.
    confines: Arc<Mutex<Vec<Confined>>>,
}

#[async_trait]
impl EngineDriver for Fake {
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
            // The review tests map each fake's `-lead` model to the lead tier (see `world_tiered`).
            models.push(model(&format!("{}-lead", self.id), &self.id, false));
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
        let core = self.core.get().and_then(Weak::upgrade).expect("a core");
        let spec_thread = spec.thread_id.clone();
        let worker = core
            .store
            .crew_task_for_worker(&spec.thread_id)
            .unwrap()
            .is_some();
        let turn = worker.then(|| self.turns.fetch_add(1, Ordering::SeqCst));
        let script = self.script.clone();
        let answers = self.answers.clone();
        let cancel = tokio_util::sync::CancellationToken::new();
        let stop = cancel.clone();
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
            let (send, mut receive) = mpsc::unbounded_channel::<String>();
            let (steered, thread) = (self.steered.clone(), spec_thread);
            tokio::spawn(async move {
                while let Some(text) = receive.recv().await {
                    steered.lock().unwrap().push((thread.clone(), text));
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

/// `world_with`, and with `tiered` every fake also lists a lead-tier model and the router maps it: `-lead` is the
/// lead tier and `-model` the worker tier, so a review role (lead tier) can be routed to any of them.
pub async fn world_tiered(
    preview: bool,
    script: Script,
    engines: &[(&str, bool)],
    tiered: bool,
) -> World {
    world_full(preview, script, engines, tiered, false).await
}

/// `world`, with the restriction on workers left on, as it ships: a start is refused where
/// none can be applied, and a worker run is handed its rules.
pub async fn world_confined(script: Script) -> World {
    world_full(true, script, &[("alpha", false)], false, true).await
}

async fn world_full(
    preview: bool,
    script: Script,
    engines: &[(&str, bool)],
    tiered: bool,
    confine: bool,
) -> World {
    let dir = TempDir::new();
    let library = crate::testing::library(&dir.path().join("library"));
    let mut config = CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
    config.engines.lookup_latest = false;
    config.engines.claude_program = "orbyn-test-no-such-claude".into();
    config.engines.codex_program = "orbyn-test-no-such-codex".into();
    config.engines.known_engine_ids = Some(Vec::new());
    config.crew_preview = preview;
    config.crew_confine_workers = confine;
    // The development stand-in for the Engine, so a test can sign in and "Confirm it's you" (approve once needs it).
    config.account_gate = crate::account::AccountGate::LocalOnly;
    config.account_backend = crate::account::AccountBackendKind::Development;
    if tiered {
        let words = crate::crew::TierWords {
            lead: Some("lead".into()),
            worker: Some("model".into()),
            scout: Some("scout".into()),
        };
        config.crew_tiers = Some(crate::crew::RouterConfig {
            tiers: engines
                .iter()
                .map(|(id, _)| ((*id).to_owned(), words.clone()))
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
    let turns: Arc<AtomicUsize> = Arc::default();
    for (id, steerable) in engines {
        core.engines.register(Arc::new(Fake {
            id: (*id).to_owned(),
            steerable: *steerable,
            tiered,
            steered: steered.clone(),
            core: cell.clone(),
            script: script.clone(),
            turns: turns.clone(),
            prompts: prompts.clone(),
            answers: answers.clone(),
            confines: confines.clone(),
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
        })
        .await
        .unwrap();
    let lead_token = bind(&core, &lead.id, "run-lead");
    World {
        _dir: dir,
        core,
        repo,
        lead: lead.id,
        lead_token,
        prompts,
        answers,
        steered,
        confines,
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
        (
            serde_json::from_str(text).unwrap(),
            reply["result"]["isError"] == json!(true),
        )
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

    /// Waits (bounded) for `done`, which reads the world.
    pub async fn until(&self, what: &str, mut done: impl FnMut(&World) -> bool) {
        for _ in 0..3000 {
            if done(self) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("never happened: {what}");
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
            { "id": "chk-probe", "kind": "http_probe",
              "params": { "url": "http://127.0.0.1:9/health", "expect_status": 200 },
              "proves": "The service answers." }
        ],
        "protected_paths": ["tests/**"]
    })
}

pub fn report(summary: &str) -> Value {
    json!({ "summary": summary })
}
