//! One driver for any Agent Client Protocol agent: JSON-RPC 2.0 over stdio,
//! protocol version 1. The client advertises no file or terminal methods, so an
//! agent cannot act through Orbyn without asking.

mod events;
mod models;
mod permissions;
mod replay;
mod sessions;

use std::collections::HashMap;
use std::ffi::OsString;
use std::time::Duration;

use async_trait::async_trait;
use orbyn_contracts::{AuthState, CustomEngine, EngineCapabilities, EngineTier, SkillDelivery};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::pipes::{EngineProcess, exit_message, resolve_engine, spawn_engine_with};
use super::rpc::{Inbound, RpcClient};
use super::{
    EngineDescriptor, EngineDriver, EngineEvent, EngineOutcome, EngineProbe, MCP_SERVER_NAME,
    ModelCatalog, ProbeDir, RunHandle, RunSpec,
};
use crate::agents::selection::MODE;
use crate::error::{CoreError, Result};
use events::UpdateMapper;
use models::{
    delete_probe_session, parse_models, parse_session_options, probe_model_options,
    require_offered, select_model,
};
use permissions::{answer_request, stop_outcome};
use replay::resume_without_replay;

const CALL_TIMEOUT: Duration = Duration::from_secs(60);
/// How many models one probe tries, and how long it may take over them.
const MAX_PROBED_MODELS: usize = 24;
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);
const PROMPT_TIMEOUT: Duration = Duration::from_secs(24 * 60 * 60);
const CANCEL_GRACE: Duration = Duration::from_secs(5);
const METHOD_NOT_FOUND: i64 = -32601;
/// Prefix of a `ModelOption` id that names one of the agent's config options.
const CONFIG_PREFIX: &str = "config:";
/// Bounds on a terminal sign-in an agent names: arguments, and characters in each.
const MAX_SIGN_IN_ARGS: usize = 16;
const MAX_SIGN_IN_WORD: usize = 256;

/// The terminal sign-in each ACP engine last named in its handshake, by engine id.
/// Orbyn only shows it for the person to run; it never runs it or `authenticate`.
static SIGN_IN: std::sync::LazyLock<std::sync::Mutex<HashMap<String, String>>> =
    std::sync::LazyLock::new(Default::default);

/// The sign-in command an ACP engine named, if one has run its handshake here.
pub fn known_sign_in(id: &str) -> Option<String> {
    SIGN_IN.lock().ok()?.get(id).cloned()
}

/// Which engine a handshake belongs to and the command that launched it.
#[derive(Clone)]
struct SignInSlot {
    id: String,
    launch: Vec<String>,
}

impl SignInSlot {
    fn record(&self, init: &Value) {
        let Ok(mut known) = SIGN_IN.lock() else {
            return;
        };
        match terminal_sign_in(&self.launch, init) {
            Some(command) => known.insert(self.id.clone(), command),
            None => known.remove(&self.id),
        };
    }
}

/// The first `terminal` auth method an agent offers, as one command line: the launch
/// command followed by the method's arguments. A method that sets environment
/// variables, or a word that would need shell-specific quoting, gives none, because
/// the line has to mean the same in a Windows and a Unix terminal.
fn terminal_sign_in(launch: &[String], init: &Value) -> Option<String> {
    let method = init
        .get("authMethods")?
        .as_array()?
        .iter()
        .take(MAX_SIGN_IN_ARGS)
        .find(|method| method.get("type").and_then(Value::as_str) == Some("terminal"))?;
    match method.get("env") {
        None | Some(Value::Null) => {}
        Some(Value::Object(env)) if env.is_empty() => {}
        Some(_) => return None,
    }
    let args: Vec<&str> = match method.get("args") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(args)) if args.len() <= MAX_SIGN_IN_ARGS => {
            args.iter().map(Value::as_str).collect::<Option<_>>()?
        }
        Some(_) => return None,
    };
    let words = launch
        .iter()
        .map(String::as_str)
        .chain(args)
        .map(shell_word)
        .collect::<Option<Vec<_>>>()?;
    (!words.is_empty()).then(|| words.join(" "))
}

/// One word of a command line, quoted when it holds spaces, or none when it holds a
/// character a Windows or Unix shell would read differently.
fn shell_word(word: &str) -> Option<String> {
    if word.is_empty()
        || word.chars().count() > MAX_SIGN_IN_WORD
        || word
            .chars()
            .any(|c| c.is_control() || "\"'`$%!^&|<>;(){}[]*?~#".contains(c))
    {
        return None;
    }
    Some(if word.contains(' ') {
        format!("\"{word}\"")
    } else {
        word.to_owned()
    })
}

pub struct AcpDriver {
    engine: CustomEngine,
    env: &'static [(&'static str, &'static str)],
    /// False for an agent that cannot ask before it edits.
    asks: bool,
    modes: Vec<orbyn_contracts::PermissionMode>,
}

impl AcpDriver {
    fn sign_in_slot(&self) -> SignInSlot {
        SignInSlot {
            id: self.engine.id.clone(),
            launch: std::iter::once(self.engine.target.clone())
                .chain(self.engine.args.iter().cloned())
                .collect(),
        }
    }

    pub fn new(engine: CustomEngine) -> Self {
        Self {
            engine,
            env: &[],
            asks: true,
            modes: orbyn_contracts::PermissionMode::ALL.to_vec(),
        }
    }

    /// `engine`, launched the way the table says that agent has to be.
    pub fn known(engine: CustomEngine, known: &crate::agents::engines::known::KnownEngine) -> Self {
        Self {
            engine,
            env: known.env,
            asks: known.asks,
            modes: crate::agents::permission_modes::offered(known.id),
        }
    }
}

#[async_trait]
impl EngineDriver for AcpDriver {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            id: self.engine.id.clone(),
            name: self.engine.name.clone(),
            tier: EngineTier::Acp,
            capabilities: EngineCapabilities {
                streaming: true,
                approvals: self.asks,
                resume: true,
                mcp: true,
                sandbox: false,
                managed: self.asks,
                skill_delivery: vec![SkillDelivery::McpSkillServer],
                goal_control: false,
                decline_feedback: false,
                permission_modes: self.modes.clone(),
            },
        }
    }

    fn check_launch_available(&self) -> Result<()> {
        resolve_engine(&self.engine.target).map(|_| ())
    }

    async fn native_session_capabilities(
        &self,
    ) -> Result<Option<orbyn_contracts::AcpSessionCapabilities>> {
        match sessions::perform(&self.engine, self.env, sessions::Operation::Capabilities).await? {
            sessions::Output::Capabilities(caps) => Ok(Some(caps)),
            _ => unreachable!("capability operation returns capabilities"),
        }
    }

    async fn list_native_sessions(
        &self,
        cwd: &std::path::Path,
        cursor: Option<&str>,
    ) -> Result<orbyn_contracts::AcpNativeSessionPage> {
        match sessions::perform(
            &self.engine,
            self.env,
            sessions::Operation::List { cwd, cursor },
        )
        .await?
        {
            sessions::Output::Page(page) => Ok(page),
            _ => unreachable!("list operation returns a page"),
        }
    }

    async fn load_native_session(
        &self,
        cwd: &std::path::Path,
        session_id: &str,
    ) -> Result<super::NativeSessionReplay> {
        match sessions::perform(
            &self.engine,
            self.env,
            sessions::Operation::Load {
                cwd,
                id: session_id,
            },
        )
        .await?
        {
            sessions::Output::Replay(replay) => Ok(replay),
            _ => unreachable!("load operation returns replay"),
        }
    }

    async fn delete_native_session(&self, _cwd: &std::path::Path, session_id: &str) -> Result<()> {
        match sessions::perform(
            &self.engine,
            self.env,
            sessions::Operation::Delete { id: session_id },
        )
        .await?
        {
            sessions::Output::Deleted => Ok(()),
            _ => unreachable!("delete operation returns acknowledgement"),
        }
    }

    async fn probe(&self) -> EngineProbe {
        EngineProbe {
            path: crate::host::process::resolve(&self.engine.target).map(|command| command.path),
            auth: Some(AuthState::NotApplicable),
            sign_in_command: known_sign_in(&self.engine.id),
            ..EngineProbe::default()
        }
    }

    fn sign_in_command(&self) -> Option<String> {
        known_sign_in(&self.engine.id)
    }

    /// ACP has no listing call, so this opens a session in a temp folder, reads
    /// what the agent reports for it, and ends the process. No prompt is sent.
    async fn models(&self) -> Result<Option<ModelCatalog>> {
        let command = resolve_engine(&self.engine.target)?;
        let args: Vec<OsString> = self.engine.args.iter().map(OsString::from).collect();
        let dir = ProbeDir::new()?;
        let process = spawn_engine_with(&command, &args, dir.path(), self.env)?;
        let (rpc, _inbound) = RpcClient::start(process.stdin, process.stdout, true);
        let (id, session, init) = open_raw(&rpc, dir.path(), None, &self.sign_in_slot()).await?;
        let options = parse_session_options(&session);
        let mut models = parse_models(&session);
        let probing = probe_model_options(&rpc, &id, &session, &options, &mut models);
        if tokio::time::timeout(PROBE_TIMEOUT, probing).await.is_err() {
            tracing::debug!("an agent took too long to list its models' options");
        }
        delete_probe_session(&rpc, &init, &id).await;
        Ok(Some(ModelCatalog {
            options,
            ..ModelCatalog::engine(models)
        }))
    }

    async fn start_run(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<EngineEvent>,
    ) -> Result<RunHandle> {
        let command = resolve_engine(&self.engine.target)?;
        let args: Vec<OsString> = self.engine.args.iter().map(OsString::from).collect();
        let process = spawn_engine_with(&command, &args, &spec.cwd, self.env)?;
        let cancel = CancellationToken::new();
        let slot = self.sign_in_slot();
        let task = tokio::spawn(run_protocol(process, spec, events, cancel.clone(), slot));
        Ok(RunHandle {
            cancel,
            task,
            input: None,
            control: None,
        })
    }
}

async fn run_protocol(
    mut process: EngineProcess,
    spec: RunSpec,
    events: mpsc::Sender<EngineEvent>,
    cancel: CancellationToken,
    slot: SignInSlot,
) {
    let (rpc, mut inbound) = RpcClient::start(process.stdin, process.stdout, true);
    let opened = tokio::select! {
        biased;
        () = cancel.cancelled() => Err(CoreError::RunClosed),
        result = open_session(&rpc, &mut inbound, &spec, &slot) => result,
    };
    let session = match opened {
        Ok((session, persistent)) => {
            if persistent
                && events
                    .send(EngineEvent::Session(session.clone()))
                    .await
                    .is_err()
            {
                return;
            }
            session
        }
        Err(error) => {
            let outcome = if cancel.is_cancelled() {
                EngineOutcome::Cancelled
            } else if matches!(error, CoreError::SessionLost) {
                EngineOutcome::SessionLost
            } else {
                EngineOutcome::Failed(error.to_string())
            };
            let _ = events.send(EngineEvent::Finished(outcome)).await;
            return;
        }
    };
    let prompt_call = rpc.call(
        "session/prompt",
        json!({ "sessionId": session, "prompt": [{ "type": "text", "text": spec.prompt }] }),
        PROMPT_TIMEOUT,
    );
    tokio::pin!(prompt_call);
    let mut mapper = UpdateMapper::default();
    let mut cancel_sent = false;
    let outcome = loop {
        tokio::select! {
            biased;
            () = cancel.cancelled(), if !cancel_sent => {
                cancel_sent = true;
                let _ = rpc
                    .notify("session/cancel", Some(json!({ "sessionId": session })))
                    .await;
            }
            () = tokio::time::sleep(CANCEL_GRACE), if cancel_sent => {
                break EngineOutcome::Cancelled;
            }
            message = inbound.recv() => {
                match message {
                    Some(Inbound::Notification { method, params }) => {
                        if method == "session/update" {
                            for event in mapper.on_update(params.get("update").unwrap_or(&Value::Null)) {
                                if events.send(event).await.is_err() {
                                    return;
                                }
                            }
                        }
                    }
                    Some(Inbound::Request { id, method, params }) => {
                        answer_request(&rpc, &events, &cancel, id, method, params);
                    }
                    None => {
                        if cancel.is_cancelled() {
                            break EngineOutcome::Cancelled;
                        }
                        let status = tokio::time::timeout(CANCEL_GRACE, process.child.wait()).await;
                        let status = status.ok().and_then(Result::ok);
                        break EngineOutcome::Failed(exit_message(status, &process.stderr).await);
                    }
                }
            }
            result = &mut prompt_call => {
                // Updates sent before the reply are already queued; drain them.
                while let Ok(message) = inbound.try_recv() {
                    if let Inbound::Notification { method, params } = message
                        && method == "session/update"
                    {
                        for event in mapper.on_update(params.get("update").unwrap_or(&Value::Null)) {
                            let _ = events.send(event).await;
                        }
                    }
                }
                break match result {
                    Ok(reply) => stop_outcome(&reply),
                    Err(error) => EngineOutcome::Failed(error.to_string()),
                };
            }
        }
    };
    for event in mapper.close() {
        let _ = events.send(event).await;
    }
    let outcome = if cancel.is_cancelled() {
        EngineOutcome::Cancelled
    } else {
        outcome
    };
    let _ = events.send(EngineEvent::Finished(outcome)).await;
}

async fn open_session(
    rpc: &RpcClient,
    inbound: &mut mpsc::UnboundedReceiver<Inbound>,
    spec: &RunSpec,
    slot: &SignInSlot,
) -> Result<(String, bool)> {
    if spec
        .resume
        .as_deref()
        .is_some_and(|id| id.is_empty() || id.chars().count() > 1024)
    {
        return Err(CoreError::Invalid("Invalid ACP session identity.".into()));
    }
    let init = initialize(rpc).await?;
    slot.record(&init);
    let caps = sessions::capabilities(&init);
    let persistent = caps.can_load || caps.can_resume;
    let (id, mut session) = if let Some(id) = &spec.resume {
        let method = if caps.can_resume {
            "session/resume"
        } else if caps.can_load {
            "session/load"
        } else {
            return Err(CoreError::Unsupported(
                "This agent cannot resume its saved session.".into(),
            ));
        };
        let params = json!({"sessionId":id,"cwd":spec.cwd,"additionalDirectories":[],"mcpServers":mcp_servers(&init,spec.mcp_url.as_deref())});
        let session = resume_without_replay(rpc, inbound, method, params, id).await?;
        (id.clone(), session)
    } else {
        let session = rpc
            .call(
                "session/new",
                json!({"cwd":spec.cwd,"mcpServers":mcp_servers(&init,spec.mcp_url.as_deref())}),
                CALL_TIMEOUT,
            )
            .await?;
        let id = session
            .get("sessionId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty() && s.chars().count() <= 1024)
            .map(str::to_owned)
            .ok_or_else(|| {
                CoreError::EngineUnavailable("The agent returned no session id.".into())
            })?;
        (id, session)
    };
    // Mode first: an agent can change what it offers per mode and per model, so each
    // step reads the reply of the one before it, and the prompt waits for them all.
    if let Some(mode) = spec.options.get(MODE) {
        require_offered(&session, MODE, mode)?;
        let params = json!({ "sessionId": id, "modeId": mode });
        rpc.call("session/set_mode", params, CALL_TIMEOUT).await?;
    }
    if let Some(model) = &spec.model {
        session = select_model(rpc, &id, &session, model).await?;
    }
    for (option, value) in &spec.options {
        let Some(config) = option.strip_prefix(CONFIG_PREFIX) else {
            continue;
        };
        require_offered(&session, option, value)?;
        let params = json!({ "sessionId": id, "configId": config, "value": value });
        let reply = rpc
            .call("session/set_config_option", params, CALL_TIMEOUT)
            .await?;
        if reply.get("configOptions").is_some() {
            session = reply;
        }
    }
    Ok((id, persistent))
}

/// Initializes and opens a session, returning its id and the agent's whole reply.
async fn open_raw(
    rpc: &RpcClient,
    cwd: &std::path::Path,
    mcp_url: Option<&str>,
    slot: &SignInSlot,
) -> Result<(String, Value, Value)> {
    let init = initialize(rpc).await?;
    slot.record(&init);
    let servers = mcp_servers(&init, mcp_url);
    let session = rpc
        .call(
            "session/new",
            json!({ "cwd": cwd, "mcpServers": servers }),
            CALL_TIMEOUT,
        )
        .await?;
    let id = session
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| CoreError::EngineUnavailable("The agent returned no session id.".into()))?;
    Ok((id, session, init))
}

async fn initialize(rpc: &RpcClient) -> Result<Value> {
    let init = rpc
        .call("initialize", sessions::initialize_params(), CALL_TIMEOUT)
        .await?;
    sessions::check_version(&init)?;
    Ok(init)
}

fn mcp_servers(init: &Value, url: Option<&str>) -> Value {
    servers_for(init, url, crate::agents::acp_mcp_bridge::bridge_program)
}

/// HTTP when the agent says it supports it. Otherwise stdio, which ACP requires of
/// every agent, through the sibling `orbyn` bridge; none when that program is absent.
fn servers_for(
    init: &Value,
    url: Option<&str>,
    bridge: impl FnOnce() -> Option<std::path::PathBuf>,
) -> Value {
    let Some(url) = url else {
        return json!([]);
    };
    let http = init
        .pointer("/agentCapabilities/mcpCapabilities/http")
        .and_then(Value::as_bool)
        == Some(true);
    if http {
        return json!([{"type":"http","name":MCP_SERVER_NAME,"url":url,"headers":[]}]);
    }
    match bridge() {
        Some(program) => json!([crate::agents::acp_mcp_bridge::stdio_server(&program, url)]),
        None => json!([]),
    }
}

#[cfg(test)]
mod tests;
