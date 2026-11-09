//! Codex through `codex app-server`: JSONL over stdio, without the `jsonrpc`
//! header. Login stays Codex's own; this driver never calls `account/login/*`.

mod catalog;
mod events;
mod limits;
mod protocol;
mod sandbox;
mod short_sessions;
mod tools;
mod turn;

use std::ffi::OsString;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use orbyn_contracts::{
    AuthState, EngineCapabilities, EngineFeedbackReceipt, EngineFeedbackRequest, EngineTier,
    ProviderInstance, SkillDelivery,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::pipes::{one_shot_cli, resolve_engine, spawn_engine};
use super::{
    EngineDescriptor, EngineDriver, EngineEvent, EngineProbe, ModelCatalog, RunHandle, RunSpec,
    UpdatePlan,
};
use crate::agents::console::{ConsoleCommand, ConsoleSpec};
use crate::agents::engines::maintenance;
use crate::agents::engines::probe::{probe_cli, read_login};
use crate::error::{CoreError, Result};
use catalog::{apply_configured_tier, parse_model};
use limits::limit_fields;
use protocol::{feedback_params, feedback_receipt, is_missing_thread};
pub use sandbox::console_args;
use turn::run_protocol;

const CALL_TIMEOUT: Duration = Duration::from_secs(60);
/// The whole status check: starting an app-server and reading three lists, some with
/// thousands of entries. One deadline for all of it, not one per call.
const TOOLS_DEADLINE: Duration = Duration::from_secs(60);
/// Pages followed for one list. A list longer than this is reported as possibly incomplete.
const TOOLS_MAX_PAGES: usize = 20;
/// An upload is slower than a read, so it does not share the read allowance.
const FEEDBACK_TIMEOUT: Duration = Duration::from_secs(120);
const INTERRUPT_GRACE: Duration = Duration::from_secs(5);
/// An MCP App page read holds the turn's event loop, so it is kept short.
const APP_READ_TIMEOUT: Duration = Duration::from_secs(10);
/// A turn started before tool startup finishes can reach the model with no usable
/// tools, and it then answers that it cannot read the files. The startup status
/// updates end too soon to show this; the status list blocks until it is over.
/// A server that never settles must not hold the run.
const TOOLS_LIMIT: Duration = Duration::from_secs(15);
/// More pages than any real catalog, so a looping cursor still ends.
const MODEL_PAGES: usize = 10;

/// JSON-RPC method-not-found, used to refuse server requests Orbyn does not serve.
const METHOD_NOT_FOUND: i64 = -32601;

pub struct CodexDriver {
    program: String,
    sandbox: String,
    /// The id and name of an instance, when this is one.
    named: Option<(String, String)>,
}

impl CodexDriver {
    /// `sandbox` is one of Codex's `SandboxMode` names.
    pub fn new(program: String, sandbox: String) -> Self {
        Self {
            program,
            sandbox,
            named: None,
        }
    }
}

#[async_trait]
impl EngineDriver for CodexDriver {
    fn descriptor(&self) -> EngineDescriptor {
        let (id, name) = self
            .named
            .clone()
            .unwrap_or_else(|| ("codex".into(), "Codex".into()));
        EngineDescriptor {
            id,
            name,
            tier: EngineTier::Native,
            capabilities: EngineCapabilities {
                streaming: true,
                approvals: true,
                resume: true,
                mcp: true,
                sandbox: true,
                managed: true,
                // A turn builds its skill list from its own cwd, the checkout, so the
                // library's `.agents/skills` never reach the model.
                skill_delivery: vec![SkillDelivery::McpSkillServer],
                goal_control: true,
                decline_feedback: false,
                permission_modes: orbyn_contracts::PermissionMode::ALL.to_vec(),
            },
        }
    }

    fn family(&self) -> String {
        "codex".into()
    }

    fn instance(&self, instance: &ProviderInstance) -> Option<Arc<dyn EngineDriver>> {
        Some(Arc::new(Self {
            program: instance.program.clone().unwrap_or(self.program.clone()),
            sandbox: self.sandbox.clone(),
            named: Some((
                instance.id.clone(),
                instance.name.clone().unwrap_or(instance.id.clone()),
            )),
        }))
    }

    fn check_launch_available(&self) -> Result<()> {
        resolve_engine(&self.program).map(|_| ())
    }

    async fn probe(&self) -> EngineProbe {
        let mut probe = probe_cli(&self.program, &["--version"]).await;
        if probe.path.is_some() {
            // Codex names the sign-in method, not the account.
            let (auth, account) =
                read_login(&self.program, &["login", "status"], parse_login, |_| None).await;
            probe.auth = Some(auth);
            probe.account = account;
        }
        probe.sign_in_command = Some("codex login".into());
        probe
    }

    async fn update_plan(&self, probe: &EngineProbe) -> Option<UpdatePlan> {
        maintenance::plan_update(probe, &maintenance::CODEX).await
    }

    fn sign_in_command(&self) -> Option<String> {
        Some("codex login".into())
    }

    fn console(&self, console: &ConsoleSpec<'_>) -> Option<ConsoleCommand> {
        Some(ConsoleCommand {
            program: self.program.clone(),
            args: console_args(console, &self.sandbox),
            cwd: console.checkout.to_path_buf(),
            title: "Codex console".into(),
        })
    }

    fn manual_hint(&self) -> Option<String> {
        Some("Update Codex with the installer you used, then refresh.".into())
    }

    fn npm_package(&self) -> Option<&str> {
        Some(maintenance::CODEX.npm_package)
    }

    async fn one_shot(&self, prompt: &str, model: Option<&str>) -> Result<String> {
        let mut args = vec![
            "exec",
            "--ephemeral",
            "--skip-git-repo-check",
            "--sandbox",
            "read-only",
            "--color",
            "never",
            "-c",
            "model_reasoning_effort=\"low\"",
        ];
        if let Some(model) = model {
            args.extend(["--model", model]);
        }
        args.push("-");
        one_shot_cli(&self.program, &args, prompt).await
    }

    /// Asks a short-lived app-server. Codex answers from its own login, and this
    /// never uses `account/login/*`. The plan comes from `account/read`, and only
    /// the plan: a failure there leaves the limits standing.
    async fn limits(&self) -> Result<Option<Value>> {
        let session = self.short_session().await?;
        let mut limits = limit_fields(
            session
                .rpc
                .call("account/rateLimits/read", Value::Null, CALL_TIMEOUT)
                .await?,
        );
        if let Ok(account) = session
            .rpc
            .call(
                "account/read",
                json!({ "refreshToken": false }),
                CALL_TIMEOUT,
            )
            .await
            && let (Some(plan), Some(object)) = (
                account.pointer("/account/planType").cloned(),
                limits.as_object_mut(),
            )
        {
            object.insert("planType".into(), plan);
        }
        Ok(Some(limits))
    }

    async fn models(&self) -> Result<Option<ModelCatalog>> {
        let session = self.short_session().await?;
        let mut models = Vec::new();
        let mut cursor = Value::Null;
        for _ in 0..MODEL_PAGES {
            let page = session
                .rpc
                .call(
                    "model/list",
                    json!({ "cursor": cursor, "limit": 100, "includeHidden": false }),
                    CALL_TIMEOUT,
                )
                .await?;
            let items = page.get("data").and_then(Value::as_array);
            models.extend(items.into_iter().flatten().filter_map(parse_model));
            let next = page.get("nextCursor").cloned().unwrap_or(Value::Null);
            if next.is_null() || next == cursor {
                break;
            }
            cursor = next;
        }
        // Read without a folder, so one project's config cannot shape the list. A
        // failure only costs the configured tier its default mark.
        let configured = session
            .rpc
            .call(
                "config/read",
                json!({ "includeLayers": false }),
                CALL_TIMEOUT,
            )
            .await
            .ok()
            .and_then(|reply| text(reply.get("config")?, "service_tier"));
        if let Some(tier) = configured {
            apply_configured_tier(&mut models, &tier);
        }
        Ok(Some(ModelCatalog::engine(models)))
    }

    /// A report goes through Codex's own `feedback/upload`, so the Codex login on
    /// this machine owns what reaches OpenAI. No thread is started for it.
    async fn feedback(&self, request: &EngineFeedbackRequest) -> Result<EngineFeedbackReceipt> {
        let session = self.short_session().await?;
        let reply = session
            .rpc
            .call(
                "feedback/upload",
                feedback_params(request),
                FEEDBACK_TIMEOUT,
            )
            .await?;
        feedback_receipt(&reply)
    }

    /// Codex keeps a turn's model, effort and tier for the turns after it.
    fn keeps_turn_overrides(&self) -> bool {
        true
    }

    /// A tool call from an MCP App page, on a short-lived app-server in the thread's
    /// checkout. Codex 0.166.0 answers `mcpServer/tool/call` only for a thread it has
    /// loaded, so an ephemeral, read-only thread is started for the call; nothing is
    /// sent to a model and the thread's own session is not touched.
    async fn mcp_app_tool_call(
        &self,
        cwd: &std::path::Path,
        server: &str,
        tool: &str,
        arguments: &Value,
    ) -> Result<Value> {
        let session = self.short_session_in(cwd).await?;
        let thread = session
            .rpc
            .call(
                "thread/start",
                json!({ "cwd": cwd, "sandbox": "read-only", "approvalPolicy": "never", "ephemeral": true }),
                CALL_TIMEOUT,
            )
            .await?;
        let thread = thread
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                CoreError::EngineUnavailable("Codex started no thread for the app's call.".into())
            })?;
        session
            .rpc
            .call(
                "mcpServer/tool/call",
                json!({ "server": server, "threadId": thread, "tool": tool, "arguments": arguments }),
                CALL_TIMEOUT,
            )
            .await
    }

    async fn tools(
        &self,
        cwd: &std::path::Path,
        _init: Option<&Value>,
        refresh: bool,
        probe: &crate::agents::tools_report::ToolsProbe,
    ) -> Result<Option<orbyn_contracts::EngineTools>> {
        // Kept per engine id in the probe, so every thread shares it, and one overall
        // deadline covers starting the app-server and all three lists.
        let answer = probe
            .codex
            .get(refresh, std::time::Duration::from_secs(30), || async {
                tokio::time::timeout(TOOLS_DEADLINE, self.read_tools(cwd))
                    .await
                    .ok()
                    .flatten()
            })
            .await;
        answer
            .map(Some)
            .ok_or_else(|| CoreError::Internal("Codex did not answer.".into()))
    }

    /// Codex 0.166.0's `thread/revert` keeps the turns before `before_turn` as the
    /// thread's durable history and leaves files alone. It answers only for a thread
    /// it has loaded, so a short-lived app-server in the checkout resumes it first.
    async fn revert_session(
        &self,
        cwd: &std::path::Path,
        session: &str,
        before_turn: &str,
    ) -> Result<()> {
        let short = self.short_session_in(cwd).await?;
        match short
            .rpc
            .call(
                "thread/resume",
                json!({ "threadId": session, "cwd": cwd }),
                CALL_TIMEOUT,
            )
            .await
        {
            Ok(_) => {}
            Err(error) if is_missing_thread(&error.to_string()) => {
                return Err(CoreError::SessionLost);
            }
            Err(error) => return Err(error),
        }
        short
            .rpc
            .call(
                "thread/revert",
                json!({ "threadId": session, "beforeTurnId": before_turn }),
                CALL_TIMEOUT,
            )
            .await?;
        Ok(())
    }

    async fn start_run(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<EngineEvent>,
    ) -> Result<RunHandle> {
        let command = resolve_engine(&self.program)?;
        let args = [OsString::from("app-server")];
        let process = spawn_engine(&command, &args, &spec.cwd)?;
        let cancel = CancellationToken::new();
        let (input, steering) = mpsc::unbounded_channel();
        let task = tokio::spawn(run_protocol(
            process,
            spec,
            self.sandbox.clone(),
            events,
            steering,
            cancel.clone(),
        ));
        Ok(RunHandle {
            cancel,
            task,
            input: Some(input),
            control: None,
        })
    }
}

fn parse_login(status: i32, stdout: &str) -> AuthState {
    if status == 0 && stdout.to_ascii_lowercase().contains("logged in") {
        AuthState::LoggedIn
    } else if status != 0 || stdout.to_ascii_lowercase().contains("not logged in") {
        AuthState::LoggedOut
    } else {
        AuthState::Unknown
    }
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

#[cfg(test)]
mod tests;
