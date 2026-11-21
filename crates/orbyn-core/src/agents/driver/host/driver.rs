//! `HostDriver{family}`: an `EngineDriver` whose runs go through the engine host. Everything
//! that is not a run (probe, models, tools, updates, sign-in, history) stays with the Rust
//! driver it wraps, so turning the host on changes how a turn runs and nothing else.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

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
    NativeSessionReplay, RunHandle, RunSpec, UpdatePlan,
};
use crate::error::{CoreError, Result};

pub struct HostDriver {
    inner: Arc<dyn EngineDriver>,
    pool: Arc<Pool>,
    /// The person's own program (`claude`), which the SDK drives.
    program: String,
}

impl HostDriver {
    pub fn new(inner: Arc<dyn EngineDriver>, pool: Arc<Pool>, program: String) -> Self {
        Self {
            inner,
            pool,
            program,
        }
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

async fn begin(
    connection: &Arc<Connection>,
    spec: &RunSpec,
    family: &str,
    claude_path: Option<&Path>,
    confinement: Option<Value>,
    send_id: u64,
    message_id: &str,
) -> Result<Value> {
    let operation = if spec.resume.is_some() {
        "resume"
    } else {
        "create"
    };
    let options = match family {
        "claude" => options::claude(spec, claude_path),
        _ => json!({}),
    };
    let mut open = json!({
        "operation": operation,
        "sessionId": spec.run_id,
        "cwd": spec.cwd,
        "options": options,
    });
    if let Some(resume) = &spec.resume {
        open["engineSessionId"] = json!(resume);
    }
    if let Some(confinement) = confinement {
        open["confinement"] = confinement;
    }
    connection.request("open", open).await.map_err(refused)?;
    connection
        .request_as(
            send_id,
            "send",
            json!({ "sessionId": spec.run_id, "message": HostDriver::message(spec), "messageId": message_id }),
        )
        .await
        .map_err(refused)
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
        let family = self.pool.family.clone();
        let mapper = DeltaMapper::for_family(&family).ok_or_else(|| {
            CoreError::EngineUnavailable("This engine has no engine host mapping yet.".into())
        })?;
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
        let accepted = match begin(
            &connection,
            &spec,
            &family,
            claude_path.as_deref(),
            confinement,
            send_id,
            &message_id,
        )
        .await
        {
            Ok(accepted) => accepted,
            Err(error) => {
                connection.fire("closeSession", json!({ "sessionId": spec.run_id }));
                return Err(error);
            }
        };
        let cancel = CancellationToken::new();
        let (input, steering) = mpsc::unbounded_channel();
        let (control, controls) = mpsc::unbounded_channel();
        let run = Run {
            pool: self.pool.clone(),
            connection,
            route,
            send_id,
            session: spec.run_id.clone(),
            mapper,
            events,
            cancel: cancel.clone(),
            steering,
            controls,
            cancel_grace: self.pool.timings().cancel_grace,
            accepted,
            message_id,
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
        self.inner.models().await
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
        self.inner.revert_session(cwd, session, before_turn).await
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
