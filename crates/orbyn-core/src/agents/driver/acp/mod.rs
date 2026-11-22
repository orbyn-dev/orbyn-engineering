//! One driver for any Agent Client Protocol agent: JSON-RPC 2.0 over stdio,
//! protocol version 1. The client advertises no file or terminal methods, so an
//! agent cannot act through Orbyn without asking.

mod events;
mod models;
mod permissions;
mod replay;
mod sessions;
mod sign_in;

use std::ffi::OsString;
use std::time::Duration;

use async_trait::async_trait;
use orbyn_contracts::{
    AuthState, CustomEngine, EngineCapabilities, EngineStartIssueKind, EngineTier, SkillDelivery,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::pipes::{
    EngineProcess, exit_message, resolve_engine, spawn_engine_confined, spawn_engine_with,
};
use super::rpc::{Inbound, RpcCallError, RpcClient};
use super::worker_identity::{with_identity, worker_env};
use super::{
    EngineDescriptor, EngineDriver, EngineEvent, EngineOutcome, EngineProbe, MCP_SERVER_NAME,
    ModelCatalog, ProbeDir, RunHandle, RunSpec,
};
use crate::agents::engines::start_issue::{clear_start_issue, note_start_issue};
use crate::agents::selection::MODE;
use crate::error::{CoreError, Result};
use events::UpdateMapper;
use models::{
    delete_probe_session, parse_models, parse_session_options, probe_model_options,
    require_offered, select_model,
};
use permissions::{answer_request, stop_outcome_after};
use replay::{RestoreFailure, resume_without_replay};
use sign_in::SignInSlot;
pub use sign_in::known_sign_in;

const CALL_TIMEOUT: Duration = Duration::from_secs(60);
/// ACP's JSON-RPC code for a call the agent serves only after a sign-in. The message it
/// carries is the agent's own and is never taken as the reason.
const AUTH_REQUIRED: i64 = -32000;
/// How many models one probe tries, and how long it may take over them.
const MAX_PROBED_MODELS: usize = 24;
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);
const PROMPT_TIMEOUT: Duration = Duration::from_secs(24 * 60 * 60);
const CANCEL_GRACE: Duration = Duration::from_secs(5);
const METHOD_NOT_FOUND: i64 = -32601;
/// Prefix of a `ModelOption` id that names one of the agent's config options.
const CONFIG_PREFIX: &str = "config:";
/// The longest reason the core puts in a failed-start message.
const MAX_REASON_CHARS: usize = 160;

/// A start that failed, in the core's words. The agent's message is never kept: a status
/// note is shown to the person and must not carry an engine's raw output.
struct StartFailure {
    error: Box<CoreError>,
    /// The agent answered with ACP's auth-required code.
    auth_required: bool,
    reason: String,
}

impl StartFailure {
    /// A failure the RPC layer reported, with a stage the core wrote. The code is the
    /// only thing read from the agent.
    fn agent(error: RpcCallError, reason: &'static str) -> Self {
        let RpcCallError { code, error } = error;
        Self {
            error: Box::new(error),
            auth_required: code == Some(AUTH_REQUIRED),
            reason: reason.to_owned(),
        }
    }

    /// A failure this driver or the core produced. Only call this with an error whose
    /// text the core wrote, never with an agent's message.
    fn core(error: CoreError) -> Self {
        let reason = short_reason(&error);
        Self {
            error: Box::new(error),
            auth_required: false,
            reason,
        }
    }

    /// A core failure with a stage the core wrote, for an error whose own text may quote
    /// the agent.
    fn core_at(error: CoreError, reason: &'static str) -> Self {
        Self {
            error: Box::new(error),
            auth_required: false,
            reason: reason.to_owned(),
        }
    }

    fn restore(failure: RestoreFailure) -> Self {
        Self {
            error: failure.error,
            auth_required: failure.auth_required,
            reason: "the agent did not reopen the session".to_owned(),
        }
    }
}

/// The core's own short words for a failure.
fn short_reason(error: &CoreError) -> String {
    let words = match error {
        CoreError::NotFound(text)
        | CoreError::Invalid(text)
        | CoreError::Unsupported(text)
        | CoreError::EngineUnavailable(text)
        | CoreError::UnsupportedOption(text) => text.clone(),
        CoreError::SessionLost => "the engine no longer has this session".to_owned(),
        other => other.public_message(),
    };
    let line = words.lines().next().unwrap_or_default().trim();
    let short: String = line.chars().take(MAX_REASON_CHARS).collect();
    if short.is_empty() {
        "the agent did not answer".to_owned()
    } else {
        short
    }
}

/// Writes the core's sentence for a start failure, keyed by the engine id the sign-in
/// capture uses. An agent's own words never reach it.
fn note_start_failure(id: &str, name: &str, failure: &StartFailure) {
    let (kind, message) = if failure.auth_required {
        (
            EngineStartIssueKind::SignedOut,
            match known_sign_in(id) {
                Some(command) => format!("{name} isn't signed in. Run {command}, then try again."),
                None => {
                    format!("{name} isn't signed in. Sign in with its own app, then try again.")
                }
            },
        )
    } else {
        let reason = failure.reason.trim_end_matches('.').trim();
        let reason = if reason.is_empty() {
            "the agent did not answer"
        } else {
            reason
        };
        let mut chars = reason.chars();
        let reason: String = chars
            .next()
            .map(|first| first.to_uppercase().chain(chars).collect())
            .unwrap_or_default();
        (
            EngineStartIssueKind::Failed,
            format!("Couldn't run {name}. {reason}."),
        )
    };
    note_start_issue(id, kind, &message);
}

/// Which engine a spawned run belongs to, for its start issue.
struct StartNotes {
    id: String,
    name: String,
}

impl StartNotes {
    /// The handshake and session opened: nothing is wrong with this engine's start.
    fn started(&self) {
        clear_start_issue(&self.id);
    }

    fn failed(&self, failure: &StartFailure) {
        note_start_failure(&self.id, &self.name, failure);
    }
}

pub struct AcpDriver {
    engine: CustomEngine,
    env: &'static [(&'static str, &'static str)],
    /// False for an agent that cannot ask before it edits.
    asks: bool,
    modes: Vec<orbyn_contracts::PermissionMode>,
    /// The known engine this one was made from, which names the CLIs it must not start.
    known: Option<&'static crate::agents::engines::known::KnownEngine>,
}

impl AcpDriver {
    /// Why `command` must not be started for this engine, when it is another CLI.
    fn refusal(&self, command: &crate::host::process::ResolvedCommand) -> Option<CoreError> {
        let known = self.known.filter(|known| known.is_other(command))?;
        Some(CoreError::EngineUnavailable(format!(
            "Found a program at {} with the name of {}, but it is another program, so Orbyn did not start it. Get {} from its official page.",
            command.path.display(),
            known.name,
            known.name
        )))
    }

    /// The program to start, and never one that is another CLI under this engine's name.
    /// Every start of this engine goes through here, so none can run the wrong program.
    fn resolve(&self) -> Result<crate::host::process::ResolvedCommand> {
        let found = resolve_engine(&self.engine.target)?;
        match self.refusal(&found) {
            Some(error) => Err(error),
            None => Ok(found),
        }
    }
    fn sign_in_slot(&self) -> SignInSlot {
        SignInSlot {
            id: self.engine.id.clone(),
            launch: std::iter::once(self.engine.target.clone())
                .chain(self.engine.args.iter().cloned())
                .collect(),
        }
    }

    fn note_start_failure(&self, failure: &StartFailure) {
        note_start_failure(&self.engine.id, &self.engine.name, failure);
    }

    pub fn new(engine: CustomEngine) -> Self {
        Self {
            engine,
            env: &[],
            asks: true,
            modes: orbyn_contracts::PermissionMode::ALL.to_vec(),
            known: None,
        }
    }

    /// `engine`, launched the way the table says that agent has to be.
    pub fn known(
        engine: CustomEngine,
        known: &'static crate::agents::engines::known::KnownEngine,
    ) -> Self {
        Self {
            engine,
            env: known.env,
            asks: known.asks,
            // An engine that cannot ask before it acts has one mode: no approvals. Supervised and Auto edits
            // would tell a person they have approvals Orbyn cannot enforce (rule 8).
            modes: if known.asks {
                crate::agents::permission_modes::offered(known.id)
            } else {
                vec![orbyn_contracts::PermissionMode::NoApprovals]
            },
            known: Some(known),
        }
    }
}

impl AcpDriver {
    async fn models_result(&self) -> std::result::Result<ModelCatalog, StartFailure> {
        let command = self.resolve().map_err(StartFailure::core)?;
        let args: Vec<OsString> = self.engine.args.iter().map(OsString::from).collect();
        let dir = ProbeDir::new().map_err(StartFailure::core)?;
        let process =
            spawn_engine_with(&command, &args, dir.path(), self.env).map_err(StartFailure::core)?;
        let (rpc, _inbound) = RpcClient::start(process.stdin, process.stdout, true);
        let (id, session, init) =
            open_raw(&rpc, dir.path(), None, dir.path(), &self.sign_in_slot()).await?;
        let options = parse_session_options(&session);
        let mut models = parse_models(&session);
        let probing = probe_model_options(&rpc, &id, &session, &options, &mut models);
        if tokio::time::timeout(PROBE_TIMEOUT, probing).await.is_err() {
            tracing::debug!("an agent took too long to list its models' options");
        }
        delete_probe_session(&rpc, &init, &id).await;
        Ok(ModelCatalog {
            options,
            ..ModelCatalog::engine(models)
        })
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
        self.resolve().map(|_| ())
    }

    async fn native_session_capabilities(
        &self,
    ) -> Result<Option<orbyn_contracts::AcpSessionCapabilities>> {
        match sessions::perform(
            self.resolve()?,
            &self.engine,
            self.env,
            sessions::Operation::Capabilities,
        )
        .await?
        {
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
            self.resolve()?,
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
            self.resolve()?,
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
            self.resolve()?,
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
        match self.models_result().await {
            Ok(catalog) => {
                clear_start_issue(&self.engine.id);
                Ok(Some(catalog))
            }
            Err(failure) => {
                self.note_start_failure(&failure);
                Err(*failure.error)
            }
        }
    }

    async fn start_run(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<EngineEvent>,
    ) -> Result<RunHandle> {
        let command = self.resolve().map_err(|error| {
            let failure = StartFailure::core(error);
            self.note_start_failure(&failure);
            *failure.error
        })?;
        let args: Vec<OsString> = self.engine.args.iter().map(OsString::from).collect();
        let identity = spec.crew_task.as_deref().map_or_else(Vec::new, |task| {
            worker_env(task, &self.engine.id, spec.model.as_deref())
        });
        let env = with_identity(self.env, &identity);
        let process =
            spawn_engine_confined(&command, &args, &spec.cwd, &env, spec.confine.as_ref())
                .map_err(|error| {
                    let failure = StartFailure::core(error);
                    self.note_start_failure(&failure);
                    *failure.error
                })?;
        let cancel = CancellationToken::new();
        let slot = self.sign_in_slot();
        let path = command.path.display().to_string();
        let notes = StartNotes {
            id: self.engine.id.clone(),
            name: self.engine.name.clone(),
        };
        let task = tokio::spawn(run_protocol(
            process,
            spec,
            events,
            cancel.clone(),
            slot,
            path,
            notes,
        ));
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
    path: String,
    notes: StartNotes,
) {
    let (rpc, mut inbound) = RpcClient::start(process.stdin, process.stdout, true);
    let agent_label;
    let opened = tokio::select! {
        biased;
        () = cancel.cancelled() => Err(StartFailure::core(CoreError::RunClosed)),
        result = open_session(&rpc, &mut inbound, &spec, &slot) => result,
    };
    let session = match opened {
        Ok((session, persistent, agent)) => {
            notes.started();
            agent_label = agent_label_for(&agent, &path);
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
        Err(failure) => {
            if !cancel.is_cancelled() {
                notes.failed(&failure);
            }
            let outcome = if cancel.is_cancelled() {
                EngineOutcome::Cancelled
            } else if matches!(&*failure.error, CoreError::SessionLost) {
                EngineOutcome::SessionLost
            } else {
                EngineOutcome::Failed(failure.error.to_string())
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
    let mut sent = 0usize;
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
                        if method == "session/update" && about_session(&params, &session) {
                            for event in mapper.on_update(params.get("update").unwrap_or(&Value::Null)) {
                                if events.send(event).await.is_err() {
                                    return;
                                }
                                sent += 1;
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
                        && about_session(&params, &session)
                    {
                        for event in mapper.on_update(params.get("update").unwrap_or(&Value::Null)) {
                            let _ = events.send(event).await;
                            sent += 1;
                        }
                    }
                }
                break match result {
                    Ok(reply) => stop_outcome_after(&reply, sent, &agent_label),
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

/// An update belongs to this run unless it names another session. An agent may
/// speak for a session it keeps besides ours; that must not reach this run's log.
/// An update with no `sessionId` is taken as ours.
fn about_session(params: &Value, session: &str) -> bool {
    params
        .get("sessionId")
        .and_then(Value::as_str)
        .is_none_or(|id| id == session)
}

async fn open_session(
    rpc: &RpcClient,
    inbound: &mut mpsc::UnboundedReceiver<Inbound>,
    spec: &RunSpec,
    slot: &SignInSlot,
) -> std::result::Result<(String, bool, (String, String)), StartFailure> {
    if spec
        .resume
        .as_deref()
        .is_some_and(|id| id.is_empty() || id.chars().count() > 1024)
    {
        return Err(StartFailure::core(CoreError::Invalid(
            "Invalid ACP session identity.".into(),
        )));
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
            return Err(StartFailure::core(CoreError::Unsupported(
                "This agent cannot resume its saved session.".into(),
            )));
        };
        let params = json!({"sessionId":id,"cwd":spec.cwd,"additionalDirectories":[],"mcpServers":mcp_servers(&init,spec.mcp_url.as_deref(),&spec.run_dir)});
        let session = resume_without_replay(rpc, inbound, method, params, id)
            .await
            .map_err(StartFailure::restore)?;
        (id.clone(), session)
    } else {
        let session = rpc
            .call_detailed(
                "session/new",
                json!({"cwd":spec.cwd,"mcpServers":mcp_servers(&init,spec.mcp_url.as_deref(),&spec.run_dir)}),
                CALL_TIMEOUT,
            )
            .await
            .map_err(|error| StartFailure::agent(error, "the agent did not open a session"))?;
        let id = session
            .get("sessionId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty() && s.chars().count() <= 1024)
            .map(str::to_owned)
            .ok_or_else(|| {
                StartFailure::core(CoreError::EngineUnavailable(
                    "The agent returned no session id.".into(),
                ))
            })?;
        (id, session)
    };
    // Mode first: an agent can change what it offers per mode and per model, so each
    // step reads the reply of the one before it, and the prompt waits for them all.
    if let Some(mode) = spec.options.get(MODE) {
        require_offered(&session, MODE, mode)
            .map_err(|error| StartFailure::core_at(error, "the agent did not accept the mode"))?;
        let params = json!({ "sessionId": id, "modeId": mode });
        rpc.call_detailed("session/set_mode", params, CALL_TIMEOUT)
            .await
            .map_err(|error| StartFailure::agent(error, "the agent did not accept the mode"))?;
    }
    if let Some(model) = &spec.model {
        session = select_model(rpc, &id, &session, model)
            .await
            .map_err(|error| StartFailure::core_at(error, "the agent did not accept the model"))?;
    }
    for (option, value) in &spec.options {
        let Some(config) = option.strip_prefix(CONFIG_PREFIX) else {
            continue;
        };
        require_offered(&session, option, value)
            .map_err(|error| StartFailure::core_at(error, "the agent did not accept an option"))?;
        let params = json!({ "sessionId": id, "configId": config, "value": value });
        let reply = rpc
            .call_detailed("session/set_config_option", params, CALL_TIMEOUT)
            .await
            .map_err(|error| StartFailure::agent(error, "the agent did not accept an option"))?;
        if reply.get("configOptions").is_some() {
            session = reply;
        }
    }
    Ok((id, persistent, agent_info(&init)))
}

/// Initializes and opens a session, returning its id and the agent's whole reply.
async fn open_raw(
    rpc: &RpcClient,
    cwd: &std::path::Path,
    mcp_url: Option<&str>,
    run_dir: &std::path::Path,
    slot: &SignInSlot,
) -> std::result::Result<(String, Value, Value), StartFailure> {
    let init = initialize(rpc).await?;
    slot.record(&init);
    let servers = mcp_servers(&init, mcp_url, run_dir);
    let session = rpc
        .call_detailed(
            "session/new",
            json!({ "cwd": cwd, "mcpServers": servers }),
            CALL_TIMEOUT,
        )
        .await
        .map_err(|error| StartFailure::agent(error, "the agent did not open a session"))?;
    let id = session
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            StartFailure::core(CoreError::EngineUnavailable(
                "The agent returned no session id.".into(),
            ))
        })?;
    Ok((id, session, init))
}

/// What the agent calls itself and its version, from `initialize`.
fn agent_info(init: &Value) -> (String, String) {
    let text = |key: &str| {
        init.pointer(&format!("/agentInfo/{key}"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    (text("name"), text("version"))
}

/// "OpenCode (/opt/homebrew/bin/opencode 1.15.10)": which program, and which version of it, ran.
fn agent_label_for((name, version): &(String, String), path: &str) -> String {
    let name = if name.is_empty() { "The agent" } else { name };
    let version = if version.is_empty() {
        String::new()
    } else {
        format!(" {version}")
    };
    format!("{name} ({path}{version})")
}

async fn initialize(rpc: &RpcClient) -> std::result::Result<Value, StartFailure> {
    let init = rpc
        .call_detailed("initialize", sessions::initialize_params(), CALL_TIMEOUT)
        .await
        .map_err(|error| StartFailure::agent(error, "the agent did not answer its handshake"))?;
    sessions::check_version(&init).map_err(StartFailure::core)?;
    Ok(init)
}

fn mcp_servers(init: &Value, url: Option<&str>, run_dir: &std::path::Path) -> Value {
    servers_for(
        init,
        url,
        run_dir,
        crate::agents::acp_mcp_bridge::bridge_program,
    )
}

/// HTTP when the agent says it supports it. Otherwise stdio, which ACP requires of
/// every agent, through the sibling `orbyn` bridge; none when that program is absent
/// or the endpoint cannot be left in `run_dir`. The bridge is given that file's path,
/// never the endpoint, so the token is in no process's arguments or environment.
fn servers_for(
    init: &Value,
    url: Option<&str>,
    run_dir: &std::path::Path,
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
    let Some(program) = bridge() else {
        return json!([]);
    };
    match crate::agents::acp_mcp_bridge::write_endpoint(run_dir, url) {
        Ok(file) => json!([crate::agents::acp_mcp_bridge::stdio_server(&program, &file)]),
        Err(error) => {
            tracing::warn!(%error, "the MCP endpoint could not be left for the stdio bridge");
            json!([])
        }
    }
}

#[cfg(test)]
mod tests;
