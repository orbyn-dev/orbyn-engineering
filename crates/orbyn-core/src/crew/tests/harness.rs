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
    /// Reports, then keeps its turn open until told, as a worker that goes on working would.
    Linger(Value, Arc<tokio::sync::Notify>),
}

/// The worker's work: given the turn's number (from 0) and the checkout, do it and say what
/// happens next.
pub type Script = Arc<dyn Fn(usize, &std::path::Path) -> Turn + Send + Sync>;

pub struct Fake {
    core: Arc<OnceLock<Weak<Core>>>,
    script: Script,
    turns: Arc<AtomicUsize>,
    /// Every prompt any thread was sent: `(thread, text)`.
    pub prompts: Arc<Mutex<Vec<(String, String)>>>,
    /// What `crew_report` answered each time.
    pub answers: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl EngineDriver for Fake {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            id: "alpha".into(),
            name: "alpha".into(),
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
        let models: Vec<ModelInfo> = vec![model("alpha-model", "alpha", true)];
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
        let core = self.core.get().and_then(Weak::upgrade).expect("a core");
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
                    Turn::Linger(args, release) => {
                        send_report(&core, &spec, args, &answers).await;
                        release.notified().await;
                        EngineOutcome::Completed
                    }
                },
            };
            let _ = events.send(EngineEvent::Finished(outcome)).await;
        });
        Ok(RunHandle {
            cancel,
            task,
            input: None,
            control: None,
        })
    }
}

async fn send_report(core: &Core, spec: &RunSpec, args: Value, answers: &Mutex<Vec<String>>) {
    let token = spec.mcp_url.as_deref().and_then(|u| u.rsplit('/').next());
    let message = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": { "name": "crew_report", "arguments": args } });
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
}

/// A core with the Crew Preview flag as asked, one git project with a first commit, a lead
/// thread and a fake engine `alpha` that plays `script`.
pub async fn world(preview: bool, script: Script) -> World {
    let dir = TempDir::new();
    let library = crate::testing::library(&dir.path().join("library"));
    let mut config = CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
    config.engines.lookup_latest = false;
    config.engines.claude_program = "orbyn-test-no-such-claude".into();
    config.engines.codex_program = "orbyn-test-no-such-codex".into();
    config.engines.known_engine_ids = Some(Vec::new());
    config.crew_preview = preview;
    let core = Arc::new(Core::open(config).unwrap());
    core.mcp.bind_host(&core);
    let cell = Arc::new(OnceLock::new());
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let answers = Arc::new(Mutex::new(Vec::new()));
    core.engines.register(Arc::new(Fake {
        core: cell.clone(),
        script,
        turns: Arc::default(),
        prompts: prompts.clone(),
        answers: answers.clone(),
    }));
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
            engine: "alpha".into(),
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
