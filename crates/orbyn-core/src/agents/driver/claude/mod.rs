//! Claude Code through its own CLI in `-p` stream-json mode. The CLI keeps its
//! own login, so no authentication environment is touched and `--bare`, which
//! ignores OAuth, is never passed. Permissions reach Orbyn through the per-run
//! MCP `approve` tool, not through this process's stdio.

mod args;
mod catalog;
mod events;
mod mcp;
mod plan;
mod protocol;
mod settings;
mod usage;
mod workflows;

pub use args::{capability_args, console_args, run_args, session_cwd};
pub use catalog::parse_capabilities;
pub(crate) use mcp::McpListed;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use orbyn_contracts::{EngineCapabilities, EngineTier, ProviderInstance, SkillDelivery};
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio_util::codec::{FramedRead, LinesCodec};
use tokio_util::sync::CancellationToken;

use super::pipes::{
    EngineProcess, one_shot_at, resolve_engine, spawn_engine_confined, spawn_engine_with,
};
use super::{
    EngineDescriptor, EngineDriver, EngineEvent, EngineProbe, MCP_SERVER_NAME, ModelCatalog,
    ProbeDir, RunHandle, RunSpec, UpdatePlan, WarmProcess,
};
use crate::agents::console::{ConsoleCommand, ConsoleSpec};
use crate::agents::engines::maintenance;
use crate::agents::engines::probe::{probe_cli, read_login};
use crate::error::{CoreError, Result};
use crate::util::write_private;
use args::CAPABILITY_ENV;
use catalog::{documented_catalog, probe_failed, read_control};
use mcp::{SERVERS_LEARNED, claude_tools, learn_servers, list_mcp};
use protocol::{Inputs, run_protocol};
use settings::{launch_settings, run_settings_path};
use usage::parse_usage_report;

const EXIT_GRACE: Duration = Duration::from_secs(3);
/// How long a run held open for a workflow waits, once the last one ends, for the
/// follow-up turn the CLI starts after a finished workflow. A stopped one starts none.
const HOLD_GRACE: Duration = Duration::from_secs(5);
const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(10);
const INITIALIZE_ID: &str = "orbyn-capabilities";
const USAGE_ID: &str = "orbyn-usage";
const USAGE_TIMEOUT: Duration = Duration::from_secs(4);
/// More than any real reply. A process that keeps talking is not answering.
const MAX_INITIALIZE_BYTES: usize = 4 * 1024 * 1024;
/// Efforts the docs name, for the stand-in list only.
const DOCUMENTED_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

pub struct ClaudeDriver {
    program: String,
    /// The id and name of an instance, when this is one.
    named: Option<(String, String)>,
    /// Tokens before Claude compacts, from the instance.
    auto_compact_window: Option<u32>,
}

impl ClaudeDriver {
    pub fn new(program: String) -> Self {
        Self {
            program,
            named: None,
            auto_compact_window: None,
        }
    }
}

#[async_trait]
impl EngineDriver for ClaudeDriver {
    fn descriptor(&self) -> EngineDescriptor {
        let (id, name) = self
            .named
            .clone()
            .unwrap_or_else(|| ("claude".into(), "Claude Code".into()));
        EngineDescriptor {
            id,
            name,
            tier: EngineTier::Native,
            capabilities: EngineCapabilities {
                streaming: true,
                approvals: true,
                resume: true,
                mcp: true,
                sandbox: false,
                managed: true,
                skill_delivery: vec![
                    SkillDelivery::NativeSkillRoot,
                    SkillDelivery::McpSkillServer,
                ],
                goal_control: false,
                decline_feedback: true,
                permission_modes: orbyn_contracts::PermissionMode::ALL.to_vec(),
            },
        }
    }

    fn family(&self) -> String {
        "claude".into()
    }

    fn instance(&self, instance: &ProviderInstance) -> Option<Arc<dyn EngineDriver>> {
        Some(Arc::new(Self {
            program: instance.program.clone().unwrap_or(self.program.clone()),
            named: Some((
                instance.id.clone(),
                instance.name.clone().unwrap_or(instance.id.clone()),
            )),
            auto_compact_window: instance
                .auto_compact_window
                .as_deref()
                .and_then(|window| window.parse().ok()),
        }))
    }

    fn start_dir(&self, root: &Path, checkout: &Path, session: Option<&str>) -> Option<PathBuf> {
        Some(session_cwd(root, checkout, session))
    }

    fn console(&self, console: &ConsoleSpec<'_>) -> Option<ConsoleCommand> {
        Some(ConsoleCommand {
            program: self.program.clone(),
            args: console_args(console),
            cwd: session_cwd(console.root, console.checkout, Some(console.session)),
            title: "Claude console".into(),
        })
    }

    fn sign_in_command(&self) -> Option<String> {
        Some("claude auth login".into())
    }

    async fn tools(
        &self,
        _cwd: &Path,
        init: Option<&Value>,
        refresh: bool,
        probe: &crate::agents::tools_report::ToolsProbe,
    ) -> Result<Option<orbyn_contracts::EngineTools>> {
        // Asked in a neutral folder on purpose: `claude mcp list` starts the servers it
        // checks, and a project's `.mcp.json` must not run just because a page was opened.
        // A project's own servers show through the frame its runs report. The answer is
        // kept per engine id, so every thread of the engine shares it for half a minute.
        let listed = probe
            .claude
            .get(refresh, std::time::Duration::from_secs(30), || {
                list_mcp(&self.program)
            })
            .await;
        Ok(Some(claude_tools(listed.as_ref(), init)))
    }

    fn check_launch_available(&self) -> Result<()> {
        resolve_engine(&self.program).map(|_| ())
    }

    async fn probe(&self) -> EngineProbe {
        let mut probe = probe_cli(&self.program, &["--version"]).await;
        if probe.path.is_some() {
            let (auth, account) = read_login(
                &self.program,
                &["auth", "status"],
                parse_auth_status,
                parse_auth_account,
            )
            .await;
            probe.auth = Some(auth);
            probe.account = account;
        }
        probe.sign_in_command = Some("claude auth login".into());
        if probe.path.is_some() {
            let program = self.program.clone();
            tokio::spawn(async move { learn_servers(&program).await });
        }
        probe
    }

    async fn update_plan(&self, probe: &EngineProbe) -> Option<UpdatePlan> {
        maintenance::plan_update(probe, &maintenance::CLAUDE).await
    }

    fn manual_hint(&self) -> Option<String> {
        Some("Update Claude Code with the installer you used, then refresh.".into())
    }

    fn npm_package(&self) -> Option<&str> {
        Some(maintenance::CLAUDE.npm_package)
    }

    /// Asks the CLI itself, which works logged out and spends no model usage. If it
    /// cannot say, the aliases its docs name stand in and say so.
    async fn models(&self) -> Result<Option<ModelCatalog>> {
        Ok(Some(match self.initialize().await {
            Ok(response) => parse_capabilities(&response),
            Err(error) => documented_catalog(error.public_message()),
        }))
    }

    async fn limits(&self) -> Result<Option<Value>> {
        parse_usage_report(&self.control_probe(true).await?)
    }

    /// The list follows the CLI and its login: signing in changes which models
    /// appear, and an update can change everything.
    async fn catalog_key(&self) -> Option<String> {
        let probe = self.probe().await;
        Some(format!("{:?}|{:?}", probe.version, probe.auth))
    }

    async fn one_shot(&self, prompt: &str, model: Option<&str>) -> Result<String> {
        // Title generation and the like read nothing and write nothing, so they run as `write_text` does: no tools,
        // strict MCP, hooks off and the permission mode that denies whatever would ask. They used to start with the
        // user's whole config, hooks and servers included.
        self.write_text(prompt, model).await
    }

    async fn write_text(&self, prompt: &str, model: Option<&str>) -> Result<String> {
        let dir = ProbeDir::new()?;
        let mcp = dir.path().join("mcp.json");
        let settings = dir.path().join("settings.json");
        write_private(&mcp, br#"{"mcpServers":{}}"#)?;
        write_private(&settings, br#"{"disableAllHooks":true}"#)?;
        let mcp = mcp
            .to_str()
            .ok_or_else(|| CoreError::Invalid("Unsupported temporary path.".into()))?;
        let settings = settings
            .to_str()
            .ok_or_else(|| CoreError::Invalid("Unsupported temporary path.".into()))?;
        let mut args = vec![
            "-p",
            "--output-format",
            "text",
            "--tools",
            "",
            "--no-session-persistence",
            "--strict-mcp-config",
            "--mcp-config",
            mcp,
            "--settings",
            settings,
            "--disable-slash-commands",
            "--permission-mode",
            "dontAsk",
        ];
        if let Some(model) = model {
            args.extend(["--model", model]);
        }
        one_shot_at(&self.program, &args, prompt, dir.path(), CAPABILITY_ENV).await
    }

    fn supports_writing(&self) -> bool {
        true
    }

    async fn start_run(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<EngineEvent>,
    ) -> Result<RunHandle> {
        // An older CLI reads only per-server rules, so a run that comes before the status
        // probe has named the servers waits for it, once, rather than start unguarded.
        if !SERVERS_LEARNED.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(8),
                learn_servers(&self.program),
            )
            .await;
        }
        let process = self.launch(&spec)?;
        Ok(handle_for(process, spec, events, None))
    }

    fn can_warm(&self) -> bool {
        true
    }

    fn warm_key(&self, spec: &RunSpec) -> Option<String> {
        let command = resolve_engine(&self.program).ok()?;
        // A reinstall at the same path is a different program.
        let installed = std::fs::metadata(&command.program).ok()?;
        let modified = installed
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?;
        // The run's own folder differs per run; what goes in it is compared instead.
        let mut neutral = spec.clone();
        neutral.run_dir = std::path::PathBuf::from("run");
        let args: Vec<String> = run_args(&neutral, Path::new("run/mcp.json"))
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        Some(
            json!({
                "program": command.program,
                "prefix": command.prefix.iter().map(|part| part.to_string_lossy()).collect::<Vec<_>>(),
                "size": installed.len(),
                "modified": modified.as_nanos().to_string(),
                "args": args,
                "cwd": spec.cwd,
                "mcp": spec.mcp_url,
                "settings": launch_settings(spec, self.auto_compact_window),
                "mode": spec.mode,
                "saved_login": crate::host::child_env::prefers_saved_login(),
            })
            .to_string(),
        )
    }

    async fn start_warm(&self, mut spec: RunSpec) -> Result<Option<WarmProcess>> {
        let Some(key) = self.warm_key(&spec) else {
            return Ok(None);
        };
        let dir = ProbeDir::new()?;
        spec.run_dir = dir.path().to_path_buf();
        let process = self.launch(&spec)?;
        Ok(Some(WarmProcess { key, process, dir }))
    }

    async fn adopt_run(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<EngineEvent>,
        warm: WarmProcess,
    ) -> Result<RunHandle> {
        let WarmProcess { process, dir, .. } = warm;
        Ok(handle_for(process, spec, events, Some(dir)))
    }
}

/// Starts `run_protocol` on a process. A warm process's folder lives as long as it.
fn handle_for(
    process: EngineProcess,
    spec: RunSpec,
    events: mpsc::Sender<EngineEvent>,
    dir: Option<ProbeDir>,
) -> RunHandle {
    let cancel = CancellationToken::new();
    let (input, steering) = mpsc::unbounded_channel();
    let (control, controls) = mpsc::unbounded_channel();
    let protocol = run_protocol(
        process,
        spec,
        events,
        Inputs { steering, controls },
        cancel.clone(),
    );
    let task = tokio::spawn(async move {
        let _dir = dir;
        protocol.await;
    });
    RunHandle {
        cancel,
        task,
        input: Some(input),
        control: Some(control),
    }
}

impl ClaudeDriver {
    /// Writes a run's files into its folder and starts the CLI on them.
    fn launch(&self, spec: &RunSpec) -> Result<EngineProcess> {
        let command = resolve_engine(&self.program)?;
        let mcp_config = spec.run_dir.join("mcp.json");
        let config = json!({
            "mcpServers": { (MCP_SERVER_NAME): { "type": "http", "url": spec.mcp_url } }
        });
        write_private(&mcp_config, config.to_string().as_bytes())?;
        if let Some(settings) = launch_settings(spec, self.auto_compact_window) {
            write_private(&run_settings_path(spec), settings.to_string().as_bytes())?;
        }
        let args = run_args(spec, &mcp_config);
        spawn_engine_confined(&command, &args, &spec.cwd, &[], spec.confine.as_ref())
    }

    /// Reads the CLI's `initialize` response from a process that never gets a
    /// prompt, in an empty folder of its own.
    async fn initialize(&self) -> Result<Value> {
        self.control_probe(false).await
    }

    async fn control_probe(&self, usage: bool) -> Result<Value> {
        let command = resolve_engine(&self.program)?;
        let dir = ProbeDir::new()?;
        let mcp_config = dir.path().join("mcp.json");
        let settings = dir.path().join("settings.json");
        write_private(
            &mcp_config,
            json!({ "mcpServers": {} }).to_string().as_bytes(),
        )?;
        write_private(
            &settings,
            json!({ "disableAllHooks": true }).to_string().as_bytes(),
        )?;
        let args = capability_args(&mcp_config, &settings);
        let mut process = spawn_engine_with(&command, &args, dir.path(), CAPABILITY_ENV)?;
        let mut stream = FramedRead::new(
            process.stdout,
            LinesCodec::new_with_max_length(MAX_INITIALIZE_BYTES),
        );
        let response = async {
            let requests = if usage {
                vec![
                    (INITIALIZE_ID, "initialize", INITIALIZE_TIMEOUT),
                    (USAGE_ID, "get_usage", USAGE_TIMEOUT),
                ]
            } else {
                vec![(INITIALIZE_ID, "initialize", INITIALIZE_TIMEOUT)]
            };
            let mut answer = Value::Null;
            for (id, subtype, deadline) in requests {
                let frame = json!({ "type": "control_request", "request_id": id,
                    "request": { "subtype": subtype } });
                answer = tokio::time::timeout(deadline, async {
                    process
                        .stdin
                        .write_all(format!("{frame}\n").as_bytes())
                        .await?;
                    process.stdin.flush().await?;
                    read_control(&mut stream, id).await
                })
                .await
                .map_err(|_| probe_failed())??;
            }
            Ok(answer)
        }
        .await;
        drop(process.stdin);
        drop(stream);
        // A control-only probe has no turn to preserve. Always stop and reap it,
        // including malformed replies and deadline failures, before its cwd goes.
        process.child.terminate();
        if !matches!(
            tokio::time::timeout(EXIT_GRACE, process.child.wait()).await,
            Ok(Ok(_))
        ) && !matches!(
            tokio::time::timeout(EXIT_GRACE, process.child.child_mut().kill()).await,
            Ok(Ok(()))
        ) {
            return Err(probe_failed());
        }
        response
    }
}

fn parse_auth_status(status: i32, stdout: &str) -> orbyn_contracts::AuthState {
    use orbyn_contracts::AuthState;
    match serde_json::from_str::<Value>(stdout) {
        Ok(value) => match value.get("loggedIn").and_then(Value::as_bool) {
            Some(true) => AuthState::LoggedIn,
            Some(false) => AuthState::LoggedOut,
            None => AuthState::Unknown,
        },
        Err(_) if status == 0 => AuthState::Unknown,
        Err(_) => AuthState::LoggedOut,
    }
}

/// The account `claude auth status` names, if it names one.
fn parse_auth_account(stdout: &str) -> Option<String> {
    let value: Value = serde_json::from_str(stdout).ok()?;
    let email = value.get("email")?.as_str()?.trim();
    (!email.is_empty() && email.len() <= 254 && !email.chars().any(char::is_control))
        .then(|| email.to_owned())
}

#[cfg(test)]
mod tests;
