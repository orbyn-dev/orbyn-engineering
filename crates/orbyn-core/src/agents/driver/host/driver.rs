//! `HostDriver{family}`: an `EngineDriver` whose runs go through the engine host. Everything
//! that is not a run (probe, models, tools, updates, sign-in, history) stays with the Rust
//! driver it wraps, so turning the host on changes how a turn runs and nothing else.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::acp::{self, HostAcpAgent};
use super::connection::{Connection, RequestError};
use super::delta::DeltaMapper;
use super::frames::ErrorKind;
use super::outcome::HOST_STOPPED;
use super::pool::{HostStatus, Pool};
use super::run::Run;
use super::{confinement, options};
use crate::agents::driver::pipes::resolve_engine;
use crate::agents::driver::{
    ApprovalSpec, EngineDescriptor, EngineDriver, EngineEvent, EngineProbe, ModelCatalog,
    NativeSessionReplay, ProbeDir, RunHandle, RunSpec, UpdatePlan,
};
use crate::error::{CoreError, Result};

/// Which path a run takes: the SDK through the engine host, or the wrapped Rust (CLI) driver.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EnginePath {
    #[default]
    Sdk,
    Cli,
}

impl EnginePath {
    /// The `claude_engine_path` setting's value.
    pub fn from_preference(value: &Value) -> Result<Self> {
        match value.as_str() {
            Some("sdk") => Ok(Self::Sdk),
            Some("cli") => Ok(Self::Cli),
            _ => Err(CoreError::Invalid(
                "claude_engine_path is sdk or cli.".into(),
            )),
        }
    }
}

/// Read at the start of every run, so a change applies to the next run with no restart. An error
/// fails the run: an unreadable choice never becomes the other path.
pub type PathSetting = Arc<dyn Fn() -> Result<EnginePath> + Send + Sync>;

/// The SDK path always: families whose person has no choice.
pub(super) fn always_sdk() -> PathSetting {
    Arc::new(|| Ok(EnginePath::Sdk))
}

pub struct HostDriver {
    inner: Arc<dyn EngineDriver>,
    pool: Arc<Pool>,
    /// The person's own program (`claude`), which the SDK drives.
    program: String,
    /// The ACP agent this driver starts; `None` for every other family.
    acp: Option<HostAcpAgent>,
    path: PathSetting,
    /// Tests read the Claude history from a fixture root instead of the person's folder.
    #[cfg(test)]
    history_root: Option<PathBuf>,
}

impl HostDriver {
    pub fn new(inner: Arc<dyn EngineDriver>, pool: Arc<Pool>, program: String) -> Self {
        // A caller that builds the ACP pool directly gets the host's default agent entry
        // from the program it named; the engine manager passes the known engine's shape
        // through `for_acp` instead.
        let acp = if pool.family == "acp" {
            Some(HostAcpAgent::new(
                "default",
                program.clone(),
                Vec::new(),
                Vec::new(),
                None,
            ))
        } else {
            None
        };
        Self {
            inner,
            pool,
            program,
            acp,
            path: always_sdk(),
            #[cfg(test)]
            history_root: None,
        }
    }

    /// The ACP family, whose agent the core resolved before the host starts it.
    pub fn for_acp(inner: Arc<dyn EngineDriver>, pool: Arc<Pool>, agent: HostAcpAgent) -> Self {
        Self {
            inner,
            pool,
            program: String::new(),
            acp: Some(agent),
            path: always_sdk(),
            #[cfg(test)]
            history_root: None,
        }
    }

    /// Lets the person's setting pick the path for each run.
    #[must_use]
    pub fn with_path(mut self, path: PathSetting) -> Self {
        self.path = path;
        self
    }

    /// Reads Claude history from `root` instead of the person's folder, for a test fixture.
    #[cfg(test)]
    #[must_use]
    pub fn with_history_root(mut self, root: PathBuf) -> Self {
        self.history_root = Some(root);
        self
    }

    pub fn pool(&self) -> &Arc<Pool> {
        &self.pool
    }

    pub async fn host_status(&self) -> HostStatus {
        self.pool.status().await
    }

    /// The path of the person's engine, for families that drive one.
    fn engine_path(&self) -> Result<Option<PathBuf>> {
        if self.pool.family == "claude" {
            return Ok(Some(resolve_engine(&self.program)?.program));
        }
        Ok(None)
    }

    /// The turn's text alone: Orbyn's context goes in the system prompt (`options`).
    fn message(spec: &RunSpec) -> &str {
        &spec.user_prompt
    }
}

/// A request that failed, as the error `start_run` returns.
fn refused(error: RequestError) -> CoreError {
    match error {
        RequestError::Closed => CoreError::EngineUnavailable(HOST_STOPPED.to_owned()),
        RequestError::Unsupported(kind) => {
            CoreError::EngineUnavailable(format!("The engine host does not offer {kind}."))
        }
        RequestError::Failed(error) => match error.effective_kind() {
            ErrorKind::SessionLost => CoreError::SessionLost,
            ErrorKind::Unsupported => CoreError::EngineUnavailable(
                "The engine host cannot restrict this run, so it was not started.".to_owned(),
            ),
            _ => CoreError::EngineUnavailable(
                error
                    .message_text()
                    .unwrap_or("The engine host refused the request.")
                    .to_owned(),
            ),
        },
    }
}

/// An `open` that failed, as the error `start_run` returns. ACP's mapping is one place
/// (`acp::open_failed`) so a later status hook is a one-line change.
fn begin_failed(family: &str, resume: bool, engine_id: &str, error: RequestError) -> CoreError {
    if family != "acp" {
        return refused(error);
    }
    let failed = match &error {
        RequestError::Failed(error) => Some(error),
        _ => None,
    };
    if let Some(error) = failed {
        let kind = error.effective_kind();
        // A resume an agent does not implement is the same sentence the Rust driver gives.
        if resume && kind == ErrorKind::Unsupported {
            return CoreError::Unsupported("This agent cannot resume its saved session.".into());
        }
        return acp::open_failed(engine_id, kind, error.message_text());
    }
    refused(error)
}

async fn begin(
    connection: &Arc<Connection>,
    spec: &RunSpec,
    options: (Value, Value),
    confinement: Option<Value>,
    send_id: u64,
    message_id: &str,
    resume_at: Option<&str>,
) -> std::result::Result<(Value, Value), RequestError> {
    let operation = if spec.resume.is_some() {
        "resume"
    } else {
        "create"
    };
    let (open_options, send_options) = options;
    let mut open = json!({
        "operation": operation,
        "sessionId": spec.run_id,
        "cwd": spec.cwd,
        "options": open_options,
    });
    if let Some(resume) = &spec.resume {
        open["engineSessionId"] = json!(resume);
    }
    // Only a resume can rewind, and only the Claude family has this field. A create or
    // another family never carries a stale point.
    if operation == "resume"
        && let Some(resume_at) = resume_at
    {
        open["resumeAt"] = json!(resume_at);
    }
    if let Some(confinement) = confinement {
        open["confinement"] = confinement;
    }
    let opened = connection.request("open", open).await?;
    let accepted = connection
        .request_accepting(
            send_id,
            "send",
            json!({
                "sessionId": spec.run_id,
                "message": HostDriver::message(spec),
                "messageId": message_id,
                "options": send_options,
            }),
            send_id,
        )
        .await?;
    Ok((opened, accepted))
}

#[async_trait]
impl EngineDriver for HostDriver {
    fn descriptor(&self) -> EngineDescriptor {
        let mut descriptor = self.inner.descriptor();
        // The Cursor SDK has no approval request: its one mode is No approvals, whatever the Rust driver offered.
        if self.pool.family == "cursor" {
            descriptor.capabilities.permission_modes =
                vec![orbyn_contracts::PermissionMode::NoApprovals];
        }
        descriptor
    }

    fn supports_approval(&self) -> bool {
        self.inner.supports_approval()
    }

    fn supports_approval_version(&self, version: Option<&str>) -> bool {
        self.inner.supports_approval_version(version)
    }

    fn approval_model(&self, selected: &str, effective: Option<&str>) -> Option<String> {
        self.inner.approval_model(selected, effective)
    }

    async fn start_approval(
        &self,
        spec: ApprovalSpec,
        events: mpsc::Sender<EngineEvent>,
    ) -> Result<RunHandle> {
        self.inner.start_approval(spec, events).await
    }

    fn check_launch_available(&self) -> Result<()> {
        self.inner.check_launch_available()
    }

    async fn native_session_capabilities(
        &self,
    ) -> Result<Option<orbyn_contracts::AcpSessionCapabilities>> {
        self.inner.native_session_capabilities().await
    }

    async fn list_native_sessions(
        &self,
        cwd: &Path,
        cursor: Option<&str>,
    ) -> Result<orbyn_contracts::AcpNativeSessionPage> {
        self.inner.list_native_sessions(cwd, cursor).await
    }

    async fn load_native_session(
        &self,
        cwd: &Path,
        session_id: &str,
    ) -> Result<NativeSessionReplay> {
        self.inner.load_native_session(cwd, session_id).await
    }

    async fn delete_native_session(&self, cwd: &Path, session_id: &str) -> Result<()> {
        self.inner.delete_native_session(cwd, session_id).await
    }

    async fn probe(&self) -> EngineProbe {
        let mut probe = self.inner.probe().await;
        let state = self.pool.state();
        if probe.problem.is_none() {
            probe.problem = state.problem();
        }
        probe
    }

    fn family(&self) -> String {
        self.inner.family()
    }

    fn instance(
        &self,
        instance: &orbyn_contracts::ProviderInstance,
    ) -> Option<Arc<dyn EngineDriver>> {
        // Named instances keep the Rust driver: the host is one process per family.
        self.inner.instance(instance)
    }

    fn resolves_from_checkout(&self) -> bool {
        self.inner.resolves_from_checkout()
    }

    fn start_dir(&self, root: &Path, checkout: &Path, session: Option<&str>) -> Option<PathBuf> {
        self.inner.start_dir(root, checkout, session)
    }

    async fn update_plan(&self, probe: &EngineProbe) -> Option<UpdatePlan> {
        self.inner.update_plan(probe).await
    }

    fn manual_hint(&self) -> Option<String> {
        self.inner.manual_hint()
    }

    async fn latest_version(&self) -> Option<String> {
        self.inner.latest_version().await
    }

    fn npm_package(&self) -> Option<&str> {
        self.inner.npm_package()
    }

    async fn start_run(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<EngineEvent>,
    ) -> Result<RunHandle> {
        let cli = (self.path)()? == EnginePath::Cli;
        let family = self.pool.family.clone();
        let engine = self.inner.descriptor().id;
        // A run that is not the rewound resume makes a waiting point stale: a create starts
        // a new session, and any other session is not the one the point names.
        if family == "claude" {
            match spec.resume.as_deref() {
                Some(session) if !cli => self.pool.clear_resume_at_except(&engine, session),
                _ => self.pool.clear_resume_at(&engine),
            }
        }
        // The person chose the CLI: the run is the Rust driver's, as if no host were here. Never a fallback.
        if cli {
            return self.inner.start_run(spec, events).await;
        }
        let mapper = DeltaMapper::for_family(&family)
            .ok_or_else(|| {
                CoreError::EngineUnavailable("This engine has no engine host mapping yet.".into())
            })?
            .with_cwd(&spec.cwd);
        // Refused here, before any process: a restriction that cannot be built is no run.
        let confinement = confinement::build(&spec, &family)?;
        let claude_path = self.engine_path()?;
        let connection = self
            .pool
            .acquire()
            .await
            .map_err(|unavailable| CoreError::EngineUnavailable(unavailable.0))?;
        let send_id = connection.next_id();
        let route = connection
            .register(send_id, &spec.run_id, &spec.thread_id)
            .map_err(|_| {
                CoreError::EngineUnavailable(
                    "The engine host is stopping. Send your message again.".into(),
                )
            })?;
        let message_id = crate::agents::driver::claude::prompt_uuid();
        // A rewind is one open, on the Claude family and a resume only. Taking it here
        // consumes it, so a failed open or a later run never sends it again.
        let resume_at = if family == "claude" {
            spec.resume.as_deref().and_then(|session| {
                self.pool
                    .take_resume_at(&engine, session)
                    .filter(|at| crate::agents::claude_history::valid_uuid(at))
            })
        } else {
            None
        };
        let options = match family.as_str() {
            "claude" => options::claude(&spec, claude_path.as_deref()),
            "cursor" => options::cursor(&spec),
            "acp" => match self.acp.as_ref() {
                Some(agent) => options::acp(&spec, agent)?,
                None => {
                    return Err(CoreError::EngineUnavailable(
                        "The ACP engine was not given an agent.".into(),
                    ));
                }
            },
            _ => json!({}),
        };
        // Cursor's model and mode are per-send overrides as well as per-agent options; the
        // host hands them to `agent.send`.
        let send_options = if family == "cursor" {
            options.clone()
        } else {
            json!({})
        };
        let (opened, accepted) = match begin(
            &connection,
            &spec,
            (options, send_options),
            confinement,
            send_id,
            &message_id,
            resume_at.as_deref(),
        )
        .await
        {
            Ok(pair) => {
                // The SDK loaded before the host accepted the turn, so this build carries it.
                self.pool.note_sdk_present();
                pair
            }
            Err(error) => {
                let sdk_missing = error.sdk_missing();
                if sdk_missing {
                    self.pool.note_sdk_missing();
                }
                let error = begin_failed(&family, spec.resume.is_some(), &engine, error);
                connection.fire("closeSession", json!({ "sessionId": spec.run_id }));
                if sdk_missing {
                    // Say what is wrong: the SDK is not in this build, not that the run could not be restricted.
                    return Err(CoreError::EngineUnavailable(
                        "This build does not include the engine's SDK.".into(),
                    ));
                }
                return Err(error);
            }
        };
        let open_session = opened
            .get("engineSessionId")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let accepted_session = accepted
            .get("engineSessionId")
            .and_then(Value::as_str)
            .map(str::to_owned);
        if let Some(session) = accepted_session.as_ref().or(open_session.as_ref()) {
            self.pool.note_session(&engine, &spec.thread_id, session);
        }
        let cancel = CancellationToken::new();
        let (input, steering) = mpsc::unbounded_channel();
        let (control, controls) = mpsc::unbounded_channel();
        let run = Run {
            pool: self.pool.clone(),
            connection,
            route,
            send_id,
            session: spec.run_id.clone(),
            open_session,
            family,
            thread: spec.thread_id.clone(),
            engine,
            mapper,
            events,
            cancel: cancel.clone(),
            steering,
            controls,
            cancel_grace: self.pool.timings().cancel_grace,
            accepted,
            message_id,
            prompt_accepted: self.pool.family != "acp",
        };
        Ok(RunHandle {
            cancel,
            task: tokio::spawn(run.drive()),
            input: Some(input),
            control: Some(control),
        })
    }

    async fn tools(
        &self,
        cwd: &Path,
        init: Option<&Value>,
        refresh: bool,
        probe: &crate::agents::tools_report::ToolsProbe,
    ) -> Result<Option<orbyn_contracts::EngineTools>> {
        self.inner.tools(cwd, init, refresh, probe).await
    }

    async fn limits(&self) -> Result<Option<Value>> {
        self.inner.limits().await
    }

    async fn models(&self) -> Result<Option<ModelCatalog>> {
        let Some(agent) = &self.acp else {
            return self.inner.models().await;
        };
        let engine = self.inner.descriptor().id;
        let connection = self
            .pool
            .acquire()
            .await
            .map_err(|unavailable| CoreError::EngineUnavailable(unavailable.0))?;
        // A live run's session answers for its own catalog; without one the host starts a
        // throwaway agent in a private folder, as the Rust driver's probe does.
        let _probe;
        let fields = match self.pool.open_session(&engine) {
            Some(session) => json!({ "sessionId": session }),
            None => {
                _probe = ProbeDir::new()?;
                agent.probe_fields(_probe.path())?
            }
        };
        match connection.request("models", fields).await {
            Ok(reply) => Ok(Some(acp::catalog(&reply))),
            Err(RequestError::Unsupported(_)) => Ok(None),
            Err(error) => Err(refused(error)),
        }
    }

    async fn mcp_app_tool_call(
        &self,
        cwd: &Path,
        server: &str,
        tool: &str,
        arguments: &Value,
    ) -> Result<Value> {
        self.inner
            .mcp_app_tool_call(cwd, server, tool, arguments)
            .await
    }

    async fn mcp_app_tool_info(
        &self,
        cwd: &Path,
        server: &str,
        tool: &str,
    ) -> Result<Option<Value>> {
        self.inner.mcp_app_tool_info(cwd, server, tool).await
    }

    async fn feedback(
        &self,
        request: &orbyn_contracts::EngineFeedbackRequest,
    ) -> Result<orbyn_contracts::EngineFeedbackReceipt> {
        self.inner.feedback(request).await
    }

    async fn revert_session(&self, cwd: &Path, session: &str, before_turn: &str) -> Result<()> {
        if self.pool.family != "claude" {
            return self.inner.revert_session(cwd, session, before_turn).await;
        }
        // The CLI path resumes the same file but has no `resumeAt`; a rewind there must
        // leave the handoff rather than claim a native rollback it cannot apply.
        if (self.path)()? == EnginePath::Cli {
            return Err(CoreError::Unsupported(
                "The Claude CLI path cannot rewind its saved session natively.".into(),
            ));
        }
        let engine = self.inner.descriptor().id;
        #[cfg(test)]
        if let Some(root) = self.history_root.as_deref() {
            let at = crate::agents::claude_history::resume_point_in(
                root,
                session,
                before_turn,
                Some(cwd),
            )?;
            self.pool.note_resume_at(&engine, session, &at)?;
            return Ok(());
        }
        let at = crate::agents::claude_history::resume_point(session, before_turn, Some(cwd))?;
        self.pool.note_resume_at(&engine, session, &at)
    }

    async fn catalog_key(&self) -> Option<String> {
        self.inner.catalog_key().await
    }

    fn console(
        &self,
        console: &crate::agents::console::ConsoleSpec<'_>,
    ) -> Option<crate::agents::console::ConsoleCommand> {
        self.inner.console(console)
    }

    fn sign_in_command(&self) -> Option<String> {
        self.inner.sign_in_command()
    }

    fn keeps_turn_overrides(&self) -> bool {
        self.inner.keeps_turn_overrides()
    }

    // `can_warm` stays false: a warm process is a CLI process, which the host replaces.

    fn supports_writing(&self) -> bool {
        self.inner.supports_writing()
    }

    async fn one_shot(&self, prompt: &str, model: Option<&str>) -> Result<String> {
        self.inner.one_shot(prompt, model).await
    }

    async fn write_text(&self, prompt: &str, model: Option<&str>) -> Result<String> {
        self.inner.write_text(prompt, model).await
    }
}
