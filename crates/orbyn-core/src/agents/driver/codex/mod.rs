//! Codex through `codex app-server`: JSONL over stdio, without the `jsonrpc`
//! header. Login stays Codex's own; this driver never calls `account/login/*`.

mod events;
mod limits;
mod protocol;
use std::ffi::OsString;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use orbyn_contracts::{
    AgUiEvent, AuthState, EngineCapabilities, EngineFeedbackReceipt, EngineFeedbackRequest,
    EngineTier, ModelInfo, ModelOption, ModelOptionValue, PermissionMode, ProviderInstance,
    SkillDelivery,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::pipes::{EngineProcess, exit_message, one_shot_cli, resolve_engine, spawn_engine};
use super::rpc::{Inbound, RpcClient};
use super::{
    EngineDescriptor, EngineDriver, EngineEvent, EngineOutcome, EngineProbe, GoalAction,
    MCP_SERVER_NAME, ModelCatalog, RunHandle, RunKind, RunSpec, UpdatePlan, effort_option,
};
use crate::agents::console::{ConsoleCommand, ConsoleSpec};
use crate::agents::engines::maintenance;
use crate::agents::engines::probe::{probe_cli, read_login};
use crate::agents::selection::{SERVICE_TIER, STANDARD_TIER};
use crate::error::{CoreError, Result};
use crate::governance::approvals::Verdict;
use events::{TurnMapper, parse_goal};
use limits::limit_fields;
use protocol::{answer_request, feedback_params, feedback_receipt, is_missing_thread, read_app};

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

/// An app-server kept alive only for a read. Dropping it ends the process.
struct ShortSession {
    rpc: RpcClient,
    _process: crate::host::process::ManagedChild,
}

impl CodexDriver {
    async fn short_session(&self) -> Result<ShortSession> {
        self.short_session_in(&std::env::temp_dir()).await
    }

    /// The three status lists of a short app-server, reduced to names and states. Each is
    /// read on its own: one that does not answer leaves its group out rather than hiding
    /// the rest. The app and plugin lists are very large, so they never leave this function whole.
    async fn read_tools(&self, cwd: &std::path::Path) -> Option<orbyn_contracts::EngineTools> {
        let deadline = std::time::Instant::now() + TOOLS_DEADLINE;
        let left = || deadline.saturating_duration_since(std::time::Instant::now());
        let session = self.short_session_experimental(cwd).await.ok()?;
        let (servers, servers_cut) =
            listed_pages(&session.rpc, "mcpServerStatus/list", deadline).await;
        let (apps, apps_cut) = listed_pages(&session.rpc, "app/list", deadline).await;
        let plugins = if left().is_zero() {
            None
        } else {
            session
                .rpc
                .call("plugin/list", json!({}), left())
                .await
                .ok()
        };
        if servers.is_none() && apps.is_none() && plugins.is_none() {
            return None;
        }
        let mut tools = codex_tools(servers.as_ref(), apps.as_ref(), plugins.as_ref());
        tools.incomplete = servers_cut || apps_cut;
        Some(tools)
    }

    /// A short app-server that takes the experimental calls the status lists need.
    async fn short_session_experimental(&self, cwd: &std::path::Path) -> Result<ShortSession> {
        let command = resolve_engine(&self.program)?;
        let args = [OsString::from("app-server")];
        let process = spawn_engine(&command, &args, cwd)?;
        let (rpc, _inbound) = RpcClient::start(process.stdin, process.stdout, false);
        rpc.call(
            "initialize",
            json!({
                "clientInfo": { "name": "orbyn", "version": env!("CARGO_PKG_VERSION") },
                "capabilities": { "experimentalApi": true },
            }),
            CALL_TIMEOUT,
        )
        .await?;
        rpc.notify("initialized", None).await?;
        Ok(ShortSession {
            rpc,
            _process: process.child,
        })
    }

    async fn short_session_in(&self, cwd: &std::path::Path) -> Result<ShortSession> {
        let command = resolve_engine(&self.program)?;
        let args = [OsString::from("app-server")];
        let process = spawn_engine(&command, &args, cwd)?;
        let (rpc, _inbound) = RpcClient::start(process.stdin, process.stdout, false);
        rpc.call(
            "initialize",
            json!({ "clientInfo": { "name": "orbyn", "version": env!("CARGO_PKG_VERSION") } }),
            CALL_TIMEOUT,
        )
        .await?;
        rpc.notify("initialized", None).await?;
        Ok(ShortSession {
            rpc,
            _process: process.child,
        })
    }
}

/// One status list, following `nextCursor` while the shared deadline allows. Returns the
/// entries gathered as `{ "data": [...] }` (or `None` if the first page failed) and whether
/// the list was cut short by the deadline or the page cap.
async fn listed_pages(
    rpc: &RpcClient,
    method: &str,
    deadline: std::time::Instant,
) -> (Option<Value>, bool) {
    let left = || deadline.saturating_duration_since(std::time::Instant::now());
    let mut gathered: Vec<Value> = Vec::new();
    let mut cursor: Option<String> = None;
    let mut got_any = false;
    for _ in 0..TOOLS_MAX_PAGES {
        if left().is_zero() {
            return (got_any.then(|| json!({ "data": gathered })), true);
        }
        let params = cursor
            .as_ref()
            .map_or(json!({}), |at| json!({ "cursor": at }));
        let Ok(page) = rpc.call(method, params, left()).await else {
            return (got_any.then(|| json!({ "data": gathered })), got_any);
        };
        got_any = true;
        if let Some(items) = page.get("data").and_then(Value::as_array) {
            gathered.extend(items.iter().cloned());
        }
        match page.get("nextCursor").and_then(Value::as_str) {
            Some(next) if !next.is_empty() => cursor = Some(next.to_owned()),
            _ => return (Some(json!({ "data": gathered })), false),
        }
    }
    (Some(json!({ "data": gathered })), true)
}

/// The Tools list for Codex, from what `mcpServerStatus/list`, `app/list` and `plugin/list`
/// answered. MCP servers other than Orbyn's own and Codex's connector bridge are the
/// person's; connectors are the apps the person can reach; plugins are the installed and
/// enabled ones. Codex does not report built-in tools or skills here.
fn codex_tools(
    servers: Option<&Value>,
    apps: Option<&Value>,
    plugins: Option<&Value>,
) -> orbyn_contracts::EngineTools {
    use crate::agents::tools_report::{
        assemble, counted, description, detail, entry, group, reason,
    };
    use orbyn_contracts::ToolState;
    let mut groups = Vec::new();
    if let Some(list) = servers
        .and_then(|value| value.get("data"))
        .and_then(Value::as_array)
    {
        let mut mine = Vec::new();
        let mut orbyn = None;
        for server in list {
            let Some(name) = text(server, "name") else {
                continue;
            };
            let tools = server
                .get("tools")
                .and_then(Value::as_object)
                .map_or(0, serde_json::Map::len);
            if name == MCP_SERVER_NAME {
                orbyn = Some(tools);
                continue;
            }
            // The bridge to connectors and the bundled app server are not the person's.
            if name == "codex_apps" || name == "codex_app" {
                continue;
            }
            let failure = server
                .get("toolsError")
                .and_then(Value::as_str)
                .filter(|why| !why.is_empty());
            let signed_out =
                server.get("authStatus").and_then(Value::as_str) == Some("notLoggedIn");
            let state = match (failure, signed_out) {
                (_, true) => ToolState::NeedsSignIn,
                (Some(_), false) => ToolState::Failed,
                (None, false) => ToolState::Ready,
            };
            let mut one = entry(&name, state);
            one.tools = u32::try_from(tools).ok().filter(|count| *count > 0);
            one.description = server
                .pointer("/serverInfo/description")
                .and_then(Value::as_str)
                .and_then(description);
            if let Some(why) = failure {
                one.reason = Some(reason(why));
                one.detail = Some(detail(why));
            }
            mine.push(one);
        }
        if let Some(count) = orbyn {
            groups.push(counted("orbyn", count));
        }
        groups.push(group("mcp", mine));
    }
    if let Some(list) = apps
        .and_then(|value| value.get("data"))
        .and_then(Value::as_array)
    {
        // Thousands of apps are on offer; only the ones the person can reach are theirs.
        let mut mine = Vec::new();
        for app in list.iter().filter(|app| {
            app.get("isAccessible").and_then(Value::as_bool) == Some(true)
                && app.get("isEnabled").and_then(Value::as_bool) != Some(false)
        }) {
            let Some(name) = text(app, "name") else {
                continue;
            };
            let mut one = entry(&name, ToolState::Ready);
            one.description = app
                .get("description")
                .and_then(Value::as_str)
                .and_then(description);
            one.source = Some("Codex app".into());
            mine.push(one);
        }
        groups.push(group("connectors", mine));
    }
    if let Some(markets) = plugins
        .and_then(|value| value.get("marketplaces"))
        .and_then(Value::as_array)
    {
        let mut mine = Vec::new();
        for plugin in markets
            .iter()
            .filter_map(|market| market.get("plugins").and_then(Value::as_array))
            .flatten()
            .filter(|plugin| {
                plugin.get("installed").and_then(Value::as_bool) == Some(true)
                    && plugin.get("enabled").and_then(Value::as_bool) == Some(true)
            })
        {
            let Some(name) = plugin
                .pointer("/interface/displayName")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| text(plugin, "name"))
            else {
                continue;
            };
            let mut one = entry(&name, ToolState::Ready);
            one.description = plugin
                .pointer("/interface/shortDescription")
                .and_then(Value::as_str)
                .and_then(description);
            mine.push(one);
        }
        groups.push(group("plugins", mine));
    }
    assemble("codex", "check", groups, true)
}

/// Reads one `model/list` entry. Hidden models stay out of the picker.
fn parse_model(value: &Value) -> Option<ModelInfo> {
    if value.get("hidden").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let id = text(value, "model").or_else(|| text(value, "id"))?;
    let default_effort = text(value, "defaultReasoningEffort");
    let reported = value
        .get("supportedReasoningEfforts")
        .and_then(Value::as_array);
    let mut effort = effort_option(
        reported
            .into_iter()
            .flatten()
            .filter_map(|option| option.get("reasoningEffort")?.as_str()),
        default_effort.as_deref(),
    );
    for (choice, option) in effort.values.iter_mut().zip(reported.into_iter().flatten()) {
        choice.description = text(option, "description").filter(|text| !text.is_empty());
    }
    let mut options = Vec::new();
    if !effort.values.is_empty() {
        options.push(effort);
    }
    options.extend(tier_option(value));
    Some(ModelInfo {
        label: text(value, "displayName").unwrap_or_else(|| id.clone()),
        description: text(value, "description"),
        is_default: value.get("isDefault").and_then(Value::as_bool) == Some(true),
        custom: false,
        options,
        input_modalities: value
            .get("inputModalities")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        upgrade: text(value, "upgrade"),
        id,
    })
}

/// Service tiers, or the older speed tier list from a CLI that has no `serviceTiers`.
/// Standard is always offered, and a model with neither has no such choice at all.
fn tier_option(value: &Value) -> Option<ModelOption> {
    let list = |key: &str| value.get(key).and_then(Value::as_array);
    let mut tiers: Vec<ModelOptionValue> = list("serviceTiers")
        .into_iter()
        .flatten()
        .filter_map(|tier| {
            let id = text(tier, "id")?;
            Some(ModelOptionValue {
                label: text(tier, "name").unwrap_or_else(|| id.clone()),
                default: false,
                description: text(tier, "description").filter(|text| !text.is_empty()),
                id,
            })
        })
        .collect();
    if tiers.is_empty() {
        tiers = list("additionalSpeedTiers")
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(|id| ModelOptionValue {
                id: id.to_owned(),
                label: if id == "fast" { "Fast" } else { id }.to_owned(),
                default: false,
                description: None,
            })
            .collect();
    }
    if tiers.is_empty() {
        return None;
    }
    let default = text(value, "defaultServiceTier")
        .filter(|id| tiers.iter().any(|tier| tier.id == *id))
        .unwrap_or_else(|| STANDARD_TIER.to_owned());
    tiers.insert(
        0,
        ModelOptionValue {
            id: STANDARD_TIER.to_owned(),
            label: "Standard".to_owned(),
            default: false,
            description: None,
        },
    );
    for tier in &mut tiers {
        tier.default = tier.id == default;
    }
    Some(ModelOption {
        id: SERVICE_TIER.to_owned(),
        label: "Speed".to_owned(),
        values: tiers,
    })
}

/// The tier in the person's Codex config becomes the default wherever a model
/// offers it. Config says `fast` for what a catalog can call `priority`.
fn apply_configured_tier(models: &mut [ModelInfo], configured: &str) {
    let options = models.iter_mut().flat_map(|model| &mut model.options);
    for option in options.filter(|option| option.id == SERVICE_TIER) {
        let found = option
            .values
            .iter()
            .find(|value| value.id == configured)
            .or_else(|| {
                let alias = (configured == "fast").then_some("priority")?;
                option.values.iter().find(|value| value.id == alias)
            })
            .map(|value| value.id.clone());
        if let Some(found) = found {
            for value in &mut option.values {
                value.default = value.id == found;
            }
        }
    }
}

/// What to send for the tier. `None` leaves it out, which keeps what the thread has,
/// so that happens only when no default could be read. Standard is a clear.
fn tier_param(spec: &RunSpec) -> Option<Value> {
    let tier = spec.options.get(SERVICE_TIER)?;
    Some(if tier == STANDARD_TIER {
        Value::Null
    } else {
        json!(tier)
    })
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

async fn run_protocol(
    mut process: EngineProcess,
    spec: RunSpec,
    sandbox: String,
    events: mpsc::Sender<EngineEvent>,
    mut steering: mpsc::UnboundedReceiver<String>,
    cancel: CancellationToken,
) {
    let (rpc, mut inbound) = RpcClient::start(process.stdin, process.stdout, false);
    let opened = tokio::select! {
        biased;
        () = cancel.cancelled() => Err(CoreError::RunClosed),
        result = open_turn(&rpc, &spec, &sandbox, &events) => result,
    };
    let (session, mut turn_id) = match opened {
        Ok(ids) => ids,
        Err(error) => {
            let outcome = if cancel.is_cancelled() {
                EngineOutcome::Cancelled
            } else if matches!(error, CoreError::SessionLost) {
                EngineOutcome::SessionLost
            } else {
                let status = tokio::time::timeout(EXIT_WAIT, process.child.wait()).await;
                let exited = status.ok().and_then(Result::ok);
                match exited {
                    Some(status) => {
                        EngineOutcome::Failed(exit_message(Some(status), &process.stderr).await)
                    }
                    None => EngineOutcome::Failed(error.to_string()),
                }
            };
            let _ = events.send(EngineEvent::Finished(outcome)).await;
            return;
        }
    };
    if let Some(turn) = &turn_id {
        let _ = events.send(EngineEvent::Turn(turn.clone())).await;
    }
    let mut mapper = TurnMapper::new(session.clone());
    let mut interrupted_at: Option<tokio::time::Instant> = None;
    let outcome = loop {
        let deadline = interrupted_at.map(|at| at + INTERRUPT_GRACE);
        tokio::select! {
            biased;
            () = cancel.cancelled(), if interrupted_at.is_none() => {
                interrupted_at = Some(tokio::time::Instant::now());
                if let Some(turn) = &turn_id {
                    let rpc = rpc.clone();
                    let params = json!({ "threadId": session, "turnId": turn });
                    tokio::spawn(async move {
                        let _ = rpc.call("turn/interrupt", params, INTERRUPT_GRACE).await;
                    });
                }
            }
            () = sleep_until(deadline), if deadline.is_some() => break EngineOutcome::Cancelled,
            Some(text) = steering.recv() => {
                if let Some(turn) = &turn_id {
                    let rpc = rpc.clone();
                    let params = json!({
                        "threadId": session,
                        "expectedTurnId": turn,
                        "input": [{ "type": "text", "text": text, "text_elements": [] }],
                    });
                    let refused = events.clone();
                    tokio::spawn(async move {
                        if let Err(error) = rpc.call("turn/steer", params, CALL_TIMEOUT).await {
                            tracing::debug!(%error, "could not steer the run");
                            let _ = refused.send(EngineEvent::SteerFailed(text)).await;
                        }
                    });
                }
            }
            message = inbound.recv() => {
                let Some(message) = message else {
                    if interrupted_at.is_some() {
                        break EngineOutcome::Cancelled;
                    }
                    let status = tokio::time::timeout(EXIT_WAIT, process.child.wait()).await;
                    let status = status.ok().and_then(Result::ok);
                    break EngineOutcome::Failed(exit_message(status, &process.stderr).await);
                };
                match message {
                    Inbound::Notification { method, params } => {
                        if method == "turn/started"
                            && turn_id.is_none()
                            && let Some(id) = params.pointer("/turn/id").and_then(Value::as_str)
                        {
                            turn_id = Some(id.to_owned());
                            if events.send(EngineEvent::Turn(id.to_owned())).await.is_err() {
                                return;
                            }
                        }
                        let step = mapper.on_notification(&method, &params);
                        for event in step.events {
                            if events.send(event).await.is_err() {
                                return;
                            }
                        }
                        if method == "item/completed"
                            && let Some(app) = read_app(&rpc, &session, &params).await
                            && events.send(EngineEvent::McpApp(app)).await.is_err()
                        {
                            return;
                        }
                        if let Some(outcome) = step.outcome {
                            break outcome;
                        }
                    }
                    Inbound::Request { id, method, params } => {
                        answer_request(&rpc, &events, &mapper, id, method, params);
                    }
                }
            }
        }
    };
    for event in mapper.close_open() {
        let _ = events.send(event).await;
    }
    let outcome = if cancel.is_cancelled() {
        EngineOutcome::Cancelled
    } else {
        outcome
    };
    let _ = events.send(EngineEvent::Finished(outcome)).await;
}

const EXIT_WAIT: Duration = Duration::from_secs(2);

async fn sleep_until(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

/// Runs the handshake and starts the turn. Returns the Codex thread id and the
/// turn id when the response has already carried it.
async fn open_turn(
    rpc: &RpcClient,
    spec: &RunSpec,
    sandbox: &str,
    events: &mpsc::Sender<EngineEvent>,
) -> Result<(String, Option<String>)> {
    rpc.call(
        "initialize",
        json!({
            "clientInfo": {
                "name": "orbyn",
                "title": "Orbyn Engineering",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "capabilities": { "experimentalApi": true },
        }),
        CALL_TIMEOUT,
    )
    .await?;
    rpc.notify("initialized", None).await?;
    let config = thread_config(spec);
    // Said on every resume too: a saved session keeps the policy it last had, so
    // leaving full access has to be stated to take effect.
    // At the library, the model often answers that it has no file tools. Starting
    // here loads the checkout's `.codex` config, which folder trust already gates.
    let mut params = json!({
        "cwd": spec.checkout,
        "approvalPolicy": approval_policy(spec.mode),
        "sandbox": if spec.plan { "read-only" } else { sandbox },
        "config": config,
    });
    // The note stands without a context too: the connectors are there either way.
    params["developerInstructions"] = json!(if spec.context.is_empty() {
        CONNECTOR_NOTE.to_owned()
    } else {
        format!("{}\n{CONNECTOR_NOTE}", spec.context)
    });
    if let Some(model) = &spec.model {
        params["model"] = json!(model);
    }
    if let Some(tier) = tier_param(spec) {
        params["serviceTier"] = tier;
    }
    // The same thread takes a new model: a resume carries it, and the history stays.
    let thread = match &spec.resume {
        Some(session) => {
            params["threadId"] = json!(session);
            match rpc.call("thread/resume", params, CALL_TIMEOUT).await {
                Ok(thread) => thread,
                Err(error) if is_missing_thread(&error.to_string()) => {
                    return Err(CoreError::SessionLost);
                }
                Err(error) => return Err(error),
            }
        }
        None => rpc.call("thread/start", params, CALL_TIMEOUT).await?,
    };
    let session = thread
        .pointer("/thread/id")
        .and_then(Value::as_str)
        .ok_or_else(|| CoreError::EngineUnavailable("Codex returned no thread id.".into()))?
        .to_owned();
    let _ = events.send(EngineEvent::Session(session.clone())).await;
    match &spec.kind {
        RunKind::Compact => {
            rpc.call(
                "thread/compact/start",
                json!({ "threadId": session }),
                CALL_TIMEOUT,
            )
            .await?;
            return Ok((session, None));
        }
        RunKind::Goal(action) => {
            let (method, params) = match action {
                GoalAction::Set(objective) => (
                    "thread/goal/set",
                    json!({ "threadId": session, "objective": objective, "status": "active" }),
                ),
                GoalAction::Clear => ("thread/goal/clear", json!({ "threadId": session })),
                GoalAction::Pause => (
                    "thread/goal/set",
                    json!({ "threadId": session, "status": "paused" }),
                ),
                GoalAction::Resume => (
                    "thread/goal/set",
                    json!({ "threadId": session, "status": "active" }),
                ),
            };
            let reply = rpc.call(method, params, CALL_TIMEOUT).await?;
            // The reply carries the goal; a notification could arrive after the
            // run has already stopped listening.
            let goal = reply.get("goal").filter(|goal| !goal.is_null());
            let _ = events.send(EngineEvent::Goal(goal.map(parse_goal))).await;
            if matches!(action, GoalAction::Clear | GoalAction::Pause) {
                let _ = events
                    .send(EngineEvent::Finished(EngineOutcome::Completed))
                    .await;
                return Ok((session, None));
            }
        }
        RunKind::Turn => {}
    }
    // Older versions lack this method; a run goes ahead either way.
    let _ = rpc
        .call("mcpServerStatus/list", json!({}), TOOLS_LIMIT)
        .await;
    let text = match &spec.kind {
        RunKind::Goal(GoalAction::Resume) => "Continue working toward the goal.",
        // In the user's message, the context often led the model to answer that it
        // had no tools. It goes in the thread's instructions instead.
        _ => spec.user_prompt.as_str(),
    };
    let policy = if spec.plan { "read-only" } else { sandbox };
    let mut params = json!({
        "threadId": session,
        "input": [{ "type": "text", "text": text, "text_elements": [] }],
        "cwd": spec.checkout,
        "sandboxPolicy": sandbox_policy(policy, &spec.checkout, spec.mode),
        "approvalPolicy": approval_policy(spec.mode),
        "model": spec.model,
        "effort": spec.effort,
        "collaborationMode": collaboration_mode(spec, &thread)?,
    });
    if let Some(tier) = tier_param(spec) {
        params["serviceTier"] = tier;
    }
    let turn = rpc.call("turn/start", params, CALL_TIMEOUT).await?;
    let turn_id = turn
        .pointer("/turn/id")
        .and_then(Value::as_str)
        .map(str::to_owned);
    Ok((session, turn_id))
}

/// Collaboration mode controls native planning/question behavior independently
/// of the sandbox. State it on ordinary turns too, to leave a resumed plan.
fn collaboration_mode(spec: &RunSpec, thread: &Value) -> Result<Value> {
    let model = spec
        .model
        .as_deref()
        .filter(|model| !model.is_empty())
        .or_else(|| {
            thread
                .get("model")
                .and_then(Value::as_str)
                .filter(|model| !model.is_empty())
        })
        .ok_or_else(|| {
            CoreError::EngineUnavailable(
                "Codex returned no effective model for collaboration mode.".into(),
            )
        })?;
    let effort = spec
        .effort
        .as_deref()
        .or_else(|| thread.get("reasoningEffort").and_then(Value::as_str));
    Ok(json!({
        "mode": if spec.plan { "plan" } else { "default" },
        "settings": {
            "model": model,
            "reasoning_effort": effort,
            // Use Codex's built-in mode instructions. The thread's separate
            // developerInstructions still carry Orbyn's run context.
            "developer_instructions": null,
        },
    }))
}

/// The thread and each turn run in the selected checkout, the only writable root,
/// so the library stays read-only. Skills reach Codex through the run's MCP tools.
///
/// Full access stops asking and opens the network, but the writable root stays:
/// `dangerFullAccess` would make the library and other projects writable.
fn sandbox_policy(sandbox: &str, checkout: &std::path::Path, mode: PermissionMode) -> Value {
    match sandbox {
        "read-only" => json!({ "type": "readOnly" }),
        "danger-full-access" => json!({ "type": "dangerFullAccess" }),
        _ => json!({
            "type": "workspaceWrite",
            "writableRoots": [checkout],
            "networkAccess": mode == PermissionMode::FullAccess,
        }),
    }
}

/// Codex's interactive command line on a run's session, in the thread's checkout.
/// Plan mode stays read-only, and Codex asks before anything else, as it would
/// for a person: Orbyn's approval gate does not reach this console.
pub fn console_args(console: &ConsoleSpec<'_>, sandbox: &str) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec!["resume".into(), console.session.into()];
    if let Some(model) = console.model {
        args.extend(["--model".into(), model.into()]);
    }
    args.extend([
        "--sandbox".into(),
        if console.plan { "read-only" } else { sandbox }.into(),
        "--ask-for-approval".into(),
        "on-request".into(),
        "--cd".into(),
        console.checkout.into(),
    ]);
    args
}

/// Codex has no edits-only policy, so the lesser modes keep asking and the gate
/// answers for them. That also keeps its deny rules in force.
fn approval_policy(mode: PermissionMode) -> &'static str {
    if mode == PermissionMode::FullAccess {
        "never"
    } else {
        "on-request"
    }
}

/// How long Codex waits for a user's optional MCP servers to finish starting before
/// the first turn, so a slow connector is ready when the model first looks for tools.
/// It is a ceiling, not a delay: ready servers cost nothing. Each run starts a new
/// app-server, so a server that never comes up costs every first turn the whole wait.
/// Measured with a hung server on 0.166.0 (first token): none 3.6 s, 3 s grace 4.7 s,
/// 10 s grace 11.8 s.
const MCP_STARTUP_GRACE_MS: u64 = 3_000;

/// A line for the model: connectors Codex loads for the user are not in the top-level
/// tool list but inside its code-mode `exec` tool.
const CONNECTOR_NOTE: &str = "With an exec tool, the person's connectors and MCP servers (GitHub, Gmail, Calendar) are mcp__* functions inside it.";

/// The per-run MCP server. Orbyn's tools carry their own policy gate, so Codex
/// does not ask a second time before calling them. The dotted key sets only this one
/// entry, so a user server of the same table keeps every field it has; a nested
/// `mcp_servers` object would replace the table.
fn thread_config(spec: &RunSpec) -> Value {
    let mut config = json!({ "mcp_optional_startup_grace_ms": MCP_STARTUP_GRACE_MS });
    if let Some(url) = &spec.mcp_url {
        config[format!("mcp_servers.{MCP_SERVER_NAME}")] =
            json!({ "url": url, "default_tools_approval_mode": "approve" });
    }
    config
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

#[cfg(test)]
mod tests;
