//! Claude Code through its own CLI in `-p` stream-json mode. The CLI keeps its
//! own login, so no authentication environment is touched and `--bare`, which
//! ignores OAuth, is never passed. Permissions reach Orbyn through the per-run
//! MCP `approve` tool, not through this process's stdio.

mod args;
mod usage;
mod workflows;

pub use args::{capability_args, console_args, run_args, session_cwd};

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use orbyn_contracts::{
    AgUiEvent, EngineAgent, EngineCapabilities, EngineTier, ModelInfo, ModelOption,
    ModelOptionValue, ProviderInstance, SkillDelivery, ThreadUsage,
};
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_util::codec::{FramedRead, LinesCodec};
use tokio_util::sync::CancellationToken;

use super::pipes::{
    EngineProcess, exit_message, lines, one_shot_at, resolve_engine, spawn_engine,
    spawn_engine_with,
};
use super::{
    EngineDescriptor, EngineDriver, EngineEvent, EngineOutcome, EngineProbe, ItemRecord,
    MCP_SERVER_NAME, ModelCatalog, ProbeDir, RunControl, RunHandle, RunKind, RunSpec, UpdatePlan,
    WarmProcess, effort_option, proposed_plan_events,
};
use crate::agents::console::{ConsoleCommand, ConsoleSpec};
use crate::agents::engines::maintenance;
use crate::agents::engines::probe::{probe_cli, read_login};
use crate::agents::selection::FAST_MODE;
use crate::error::{CoreError, Result};
use crate::util::{new_id, write_private};
use args::CAPABILITY_ENV;
use usage::parse_usage_report;
use workflows::Workflows;

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

fn run_settings_path(spec: &RunSpec) -> std::path::PathBuf {
    spec.run_dir.join("settings.json")
}

/// Tools that write, run commands or reach the network. Each is forced to ask in a run's own `--settings`, because
/// an `ask` rule beats an `allow` from any other source (checked live on 2.1.318: `allow Write` alone skipped the
/// prompt tool, `allow` plus `ask` reached it). Read-only tools are left alone.
const GUARDED_TOOLS: &[&str] = &["Bash", "Edit", "Write", "NotebookEdit", "WebFetch"];

/// The library settings a run or a console takes over. Never `permissions`, `hooks`, `apiKeyHelper`,
/// `statusLine` or anything else that decides a call or runs a command.
const LIBRARY_SETTING_KEYS: &[&str] = &["fastMode", "model", "effortLevel"];
const LIBRARY_ENV_KEYS: &[&str] = &[
    "CLAUDE_CODE_DISABLE_FAST_MODE",
    "CLAUDE_CODE_MAX_CONCURRENT_SUBAGENTS",
    "CLAUDE_CODE_MAX_SUBAGENT_SPAWN_DEPTH",
];

/// What every Orbyn-started Claude gets in `--settings`, whatever the checkout's own settings say: the allowlisted
/// library settings, then the two protections. Hooks are off, because a `PreToolUse` hook that answers allow is
/// decided before the permission tool is asked, and every guarded tool asks, so a project `allow` rule cannot
/// skip the prompt. These come last so nothing before them can override them.
fn base_settings(library: &Path) -> serde_json::Map<String, Value> {
    let mut settings = serde_json::Map::new();
    if let Some(from_library) = library_settings(library) {
        for key in LIBRARY_SETTING_KEYS {
            if let Some(value) = from_library.get(*key) {
                settings.insert((*key).into(), value.clone());
            }
        }
        if let Some(Value::Object(env)) = from_library.get("env") {
            let kept: serde_json::Map<String, Value> = env
                .iter()
                .filter(|(name, value)| {
                    LIBRARY_ENV_KEYS.contains(&name.as_str()) && value.is_string()
                })
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect();
            if !kept.is_empty() {
                settings.insert("env".into(), Value::Object(kept));
            }
        }
    }
    guard(&mut settings);
    settings
}

fn guard(settings: &mut serde_json::Map<String, Value>) {
    settings.insert("disableAllHooks".into(), json!(true));
    settings.insert("permissions".into(), json!({ "ask": GUARDED_TOOLS }));
}

/// The settings a run passes with `--settings`. Fast mode reaches only a model that reports `supportsFastMode`,
/// because admission refuses the option anywhere else.
fn run_settings(spec: &RunSpec, window: Option<u32>) -> Option<Value> {
    let mut settings = base_settings(&spec.library);
    if let Some(fast) = spec.options.get(FAST_MODE) {
        settings.insert("fastMode".into(), json!(fast == "on"));
    }
    if let Some(window) = window {
        settings.insert("autoCompactWindow".into(), json!(window));
    }
    Some(Value::Object(settings))
}

/// The library's own Claude settings, as the library tracks them.
fn library_settings(library: &Path) -> Option<serde_json::Map<String, Value>> {
    let text = read_capped(&library.join(".claude/settings.json"), MAX_SETTINGS_BYTES)?;
    match serde_json::from_str::<Value>(&text).ok()? {
        Value::Object(map) if !map.is_empty() => Some(map),
        _ => None,
    }
}

const MAX_SETTINGS_BYTES: u64 = 64 * 1024;
const MAX_AGENTS: usize = 64;
const MAX_AGENT_FILE_BYTES: u64 = 64 * 1024;
const MAX_AGENTS_BYTES: usize = 256 * 1024;
/// The effort levels `--agents` accepts (checked on 2.1.318, which rejects any other value).
const AGENT_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

fn read_capped(path: &Path, max: u64) -> Option<String> {
    if std::fs::metadata(path).ok()?.len() > max {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

/// A front matter's top-level `key: value` pairs. Values may be quoted, continue on indented lines (a plain
/// multi-line scalar folds to one line), be a `|` or `>` block, or be a `- item` list (joined with commas).
fn front_matter(front: &str) -> std::collections::BTreeMap<String, String> {
    let lines: Vec<&str> = front.lines().collect();
    let mut fields = std::collections::BTreeMap::new();
    let mut at = 0;
    while at < lines.len() {
        let line = lines[at];
        at += 1;
        let indented = line.starts_with(' ') || line.starts_with('\t');
        let Some((key, value)) = line.split_once(':').filter(|_| !indented) else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() || key.contains(char::is_whitespace) {
            continue;
        }
        let mut rest: Vec<&str> = Vec::new();
        while at < lines.len()
            && (lines[at].starts_with(' ')
                || lines[at].starts_with('\t')
                || lines[at].trim().is_empty())
        {
            rest.push(lines[at].trim());
            at += 1;
        }
        while rest.last().is_some_and(|line| line.is_empty()) {
            rest.pop();
        }
        let value = value.trim();
        let text = if value.starts_with('|') {
            rest.join("\n")
        } else if value.starts_with('>') {
            rest.iter()
                .copied()
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        } else if rest.iter().any(|line| line.starts_with("- ")) && value.is_empty() {
            rest.iter()
                .filter_map(|line| line.strip_prefix("- "))
                .map(|item| unquote(item.trim()))
                .collect::<Vec<_>>()
                .join(",")
        } else {
            let mut folded = vec![value];
            folded.extend(rest.iter().copied());
            unquote(
                &folded
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join(" "),
            )
        };
        fields.insert(key.to_owned(), text);
    }
    fields
}

fn unquote(text: &str) -> String {
    let bytes = text.as_bytes();
    if bytes.len() >= 2
        && (bytes[0] == b'"' || bytes[0] == b'\'')
        && bytes[bytes.len() - 1] == bytes[0]
    {
        text[1..text.len() - 1].to_owned()
    } else {
        text.to_owned()
    }
}

/// The library's subagents (`.claude/agents/*.md`) as the `--agents` JSON, since `--add-dir` loads skills and
/// nothing else. Only the fields that option takes are carried over. At most 64 agents of 64 KiB each, 256 KiB of
/// prompts in all; anything past that is left out rather than failing the run.
fn library_agents(library: &Path) -> Option<Value> {
    let mut files: Vec<_> = std::fs::read_dir(library.join(".claude/agents"))
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .collect();
    files.sort();
    let mut agents = serde_json::Map::new();
    let mut total = 0usize;
    for path in files {
        if agents.len() >= MAX_AGENTS {
            break;
        }
        let Some(text) = read_capped(&path, MAX_AGENT_FILE_BYTES) else {
            continue;
        };
        let text = text.replace("\r\n", "\n");
        let Some(rest) = text.strip_prefix("---\n") else {
            continue;
        };
        let Some((front, body)) = rest
            .split_once("\n---\n")
            .or_else(|| rest.strip_suffix("\n---").map(|front| (front, "")))
        else {
            continue;
        };
        let fields = front_matter(front);
        let stem = path.file_stem().and_then(|stem| stem.to_str());
        let Some(name) = fields.get("name").map(String::as_str).or(stem) else {
            continue;
        };
        let prompt = body.trim();
        if total + prompt.len() > MAX_AGENTS_BYTES {
            break;
        }
        total += prompt.len();
        let mut agent = serde_json::Map::new();
        agent.insert(
            "description".into(),
            json!(
                fields
                    .get("description")
                    .map(String::as_str)
                    .unwrap_or(name)
            ),
        );
        agent.insert("prompt".into(), json!(prompt));
        if let Some(model) = fields.get("model") {
            agent.insert("model".into(), json!(model));
        }
        if let Some(effort) = fields
            .get("effort")
            .filter(|effort| AGENT_EFFORTS.contains(&effort.as_str()))
        {
            agent.insert("effort".into(), json!(effort));
        }
        for key in ["tools", "disallowedTools"] {
            if let Some(list) = fields.get(key) {
                let names: Vec<&str> = list
                    .split(',')
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .collect();
                agent.insert(key.into(), json!(names));
            }
        }
        agents.insert(name.to_owned(), Value::Object(agent));
    }
    (!agents.is_empty()).then_some(Value::Object(agents))
}

/// MCP servers other than our own that a Claude run has seen, by the name its init frame
/// gave them. Remembered for the life of the service so the next run's approval guard
/// can name them; the wildcard in the guard covers a server not yet seen.
fn seen_servers() -> &'static std::sync::Mutex<std::collections::BTreeSet<String>> {
    static SEEN: std::sync::OnceLock<std::sync::Mutex<std::collections::BTreeSet<String>>> =
        std::sync::OnceLock::new();
    SEEN.get_or_init(Default::default)
}

/// Most server names remembered for the guard; the `mcp__*` rule covers the rest.
const MAX_SEEN_SERVERS: usize = 64;

/// A server's name as it appears in tool names and permission rules: anything but a
/// letter, digit, `_` or `-` becomes `_`, so `claude.ai Vercel` is `claude_ai_Vercel`.
fn rule_name(server: &str) -> String {
    server
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Run settings plus the approval guard. The guard is `permissions.ask` for every MCP
/// server but our own, because an `ask` rule outranks an `allow` rule, so a connector
/// tool the person allowed in their own settings still reaches our permission tool
/// instead of running unseen. `mcp__*` covers any server, including one not yet seen
/// (checked live on 2.1.318, as were the per-server and `mcp__claude_ai_*` forms); the
/// per-server rules cover a CLI that does not read the wildcard. Our own tools answer
/// themselves in `approve`, so they pass through the guard unchanged.
fn launch_settings(spec: &RunSpec, window: Option<u32>) -> Option<Value> {
    let mut settings = match run_settings(spec, window) {
        Some(Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    let mut ask: Vec<String> = vec!["mcp__*".into()];
    ask.extend(
        seen_servers()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|name| **name != MCP_SERVER_NAME)
            .map(|name| format!("mcp__{name}")),
    );
    let permissions = settings
        .entry("permissions")
        .or_insert_with(|| json!({}))
        .as_object_mut()?;
    let existing = permissions
        .entry("ask")
        .or_insert_with(|| json!([]))
        .as_array_mut()?;
    for rule in ask {
        if !existing.iter().any(|entry| entry == &json!(rule)) {
            existing.push(json!(rule));
        }
    }
    Some(Value::Object(settings))
}

/// One line of `claude mcp list`: the server's name, and what the CLI says of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct McpListed {
    pub name: String,
    pub state: String,
}

impl McpListed {
    /// The state in the terms the Tools list uses, and a reason when it is not ready.
    fn tool_state(&self) -> (orbyn_contracts::ToolState, Option<String>) {
        use orbyn_contracts::ToolState;
        let text = self
            .state
            .trim_start_matches(|c: char| !c.is_alphanumeric());
        let lower = text.to_lowercase();
        if lower.starts_with("connected") {
            (ToolState::Ready, None)
        } else if lower.contains("authentication") || lower.contains("sign in") {
            (ToolState::NeedsSignIn, None)
        } else if lower.starts_with("connecting") || lower.starts_with("pending") {
            (ToolState::Starting, None)
        } else {
            let why = text
                .split_once(" — ")
                .map_or(text, |(_, rest)| rest)
                .to_owned();
            (ToolState::Failed, Some(why))
        }
    }
}

/// Reads `claude mcp list`. A line is `<name>: <url or command> - <state>`; a name may
/// hold spaces and colons but not `: `, so the first `: ` ends it. Banner and blank
/// lines carry no such separator after a name and are skipped.
pub(crate) fn parse_mcp_list(text: &str) -> Vec<McpListed> {
    text.lines()
        .filter_map(|line| {
            let (name, rest) = line.split_once(": ")?;
            if name.trim().is_empty() || name.starts_with("Checking") {
                return None;
            }
            let state = rest
                .rsplit_once(" - ")
                .map_or("", |(_, state)| state.trim());
            Some(McpListed {
                name: name.trim().to_owned(),
                state: state.to_owned(),
            })
        })
        .collect()
}

/// `claude mcp list` parsed and timed, or `None` when the CLI could not be asked.
async fn list_mcp(program: &str) -> Option<crate::agents::tools_report::ListedServers> {
    let command = crate::host::process::resolve(program)?;
    let output = crate::agents::engines::probe::capture(
        &command,
        &["mcp", "list"],
        std::time::Duration::from_secs(20),
    )
    .await?;
    (output.status == 0).then(|| crate::agents::tools_report::ListedServers {
        at: crate::util::now(),
        servers: parse_mcp_list(&output.stdout),
    })
}

const CLAUDE_CONNECTORS_PAGE: &str = "https://claude.ai/settings/connectors";

/// What the Tools list shows for Claude Code: the servers the CLI says it will load (or,
/// failing that, the ones the last run's frame named), the plugins, built-in tools and
/// skills that frame reported, and the count of Orbyn's own tools.
fn claude_tools(
    listed: Option<&crate::agents::tools_report::ListedServers>,
    init: Option<&Value>,
) -> orbyn_contracts::EngineTools {
    use crate::agents::tools_report::{assemble, counted, detail, entry, group, reason};
    use orbyn_contracts::ToolState;
    let mut mcp = Vec::new();
    let mut connectors = Vec::new();
    let mut place = |name: &str, state: ToolState, why: Option<String>, more: Option<String>| {
        let connector = name.strip_prefix("claude.ai ");
        let mut one = entry(connector.unwrap_or(name), state);
        one.reason = why.as_deref().map(reason);
        one.detail = more.as_deref().map(detail);
        if connector.is_some() {
            one.source = Some("claude.ai connector".into());
            if state == ToolState::NeedsSignIn {
                one.sign_in_url = Some(CLAUDE_CONNECTORS_PAGE.into());
            }
            connectors.push(one);
        } else {
            if name.starts_with("plugin:") {
                one.source = Some("plugin".into());
            }
            mcp.push(one);
        }
    };
    let from_frame: Vec<(String, ToolState)> = init
        .and_then(|frame| frame.get("mcp_servers"))
        .and_then(Value::as_array)
        .map(|servers| {
            servers
                .iter()
                .filter_map(|server| {
                    let name = server.get("name")?.as_str()?;
                    let state = match server.get("status").and_then(Value::as_str) {
                        Some("connected") => ToolState::Ready,
                        Some("pending") => ToolState::Starting,
                        Some("needs-auth") => ToolState::NeedsSignIn,
                        _ => ToolState::Failed,
                    };
                    Some((name.to_owned(), state))
                })
                .filter(|(name, _)| name != MCP_SERVER_NAME)
                .collect()
        })
        .unwrap_or_default();
    let reported_servers = listed.is_some() || !from_frame.is_empty();
    match listed {
        Some(listed) => {
            for server in listed
                .servers
                .iter()
                .filter(|server| server.name != MCP_SERVER_NAME)
            {
                let (state, why) = server.tool_state();
                let more = why.clone();
                place(&server.name, state, why, more);
            }
        }
        None => {
            for (name, state) in &from_frame {
                place(name, *state, None, None);
            }
        }
    }
    let mut groups = Vec::new();
    if reported_servers {
        groups.push(group("mcp", mcp));
        groups.push(group("connectors", connectors));
    }
    let string_list = |key: &str| -> Option<Vec<String>> {
        init?.get(key)?.as_array().map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
    };
    if let Some(names) = string_list("plugins") {
        let items = names
            .iter()
            .map(|name| entry(name, ToolState::Ready))
            .collect();
        groups.push(group("plugins", items));
    }
    if let Some(names) = string_list("builtin") {
        let items = names
            .iter()
            .map(|name| entry(name, ToolState::Ready))
            .collect();
        groups.push(group("builtin", items));
    }
    if let Some(count) = init
        .and_then(|frame| frame.get("orbyn_tools"))
        .and_then(Value::as_u64)
    {
        groups.push(counted("orbyn", usize::try_from(count).unwrap_or(0)));
    }
    if let Some(count) = init
        .and_then(|frame| frame.get("skills"))
        .and_then(Value::as_u64)
    {
        groups.push(counted("skills", usize::try_from(count).unwrap_or(0)));
    }
    // Where the list came from: a run's frame alone, the CLI alone, or the CLI for servers
    // and the frame for what only a run reports.
    let source = match (listed.is_some(), init.is_some()) {
        (true, true) => "check+run",
        (true, false) => "check",
        (false, _) => "run",
    };
    let mut tools = assemble("claude", source, groups, init.is_some());
    // The time `claude mcp list` ran, not the time this answer was put together.
    tools.checked_at = listed.map(|listed| listed.at.clone());
    if listed.is_none() {
        // The CLI could not be asked, which says nothing about whether it has servers.
        tools.error = Some("Claude Code did not list its servers.".into());
        tools
            .not_reported
            .retain(|id| id != "mcp" && id != "connectors");
    }
    tools
}

static SERVERS_LEARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Asks the CLI which MCP servers it will load and remembers their names for the
/// approval guard. The init frame of a run that passes `--mcp-config` lists only that
/// one server, and connectors appear after it, so the frame cannot name them. About
/// four seconds; only names are kept.
async fn learn_servers(program: &str) {
    // Tried once per service life whatever comes of it, so a CLI that cannot list
    // does not make every run wait.
    let _tried = scopeguard_flag();
    let Some(command) = crate::host::process::resolve(program) else {
        return;
    };
    let Some(output) = crate::agents::engines::probe::capture(
        &command,
        &["mcp", "list"],
        std::time::Duration::from_secs(20),
    )
    .await
    else {
        return;
    };
    let mut seen = seen_servers()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for server in parse_mcp_list(&output.stdout) {
        let name = rule_name(&server.name);
        if name != MCP_SERVER_NAME && seen.len() < MAX_SEEN_SERVERS {
            seen.insert(name);
        }
    }
}

/// Marks the server names as tried when dropped.
struct Tried;

impl Drop for Tried {
    fn drop(&mut self) {
        SERVERS_LEARNED.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

fn scopeguard_flag() -> Tried {
    Tried
}

/// What the init frame says this run has: server names with their state, plugin names
/// and how many tools it lists. Names only; a URL or token never leaves the frame.
fn init_tools(frame: &Value) -> Option<EngineEvent> {
    let servers: Vec<Value> = frame
        .get("mcp_servers")?
        .as_array()?
        .iter()
        .filter_map(|server| {
            let name = server.get("name")?.as_str()?;
            Some(json!({
                "name": name,
                "status": server.get("status").and_then(Value::as_str).unwrap_or("unknown"),
                "source": server.get("source").and_then(Value::as_str),
            }))
        })
        .collect();
    {
        let mut seen = seen_servers()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for server in &servers {
            // Bounded: a name is only a rule string, but the set lives as long as the service.
            if let Some(name) = server["name"].as_str().map(rule_name)
                && name != MCP_SERVER_NAME
                && seen.len() < MAX_SEEN_SERVERS
            {
                seen.insert(name);
            }
        }
    }
    // The CLI's own plugins say `builtin` for a path; only the person's are listed.
    let plugins: Vec<&str> = frame
        .get("plugins")
        .and_then(Value::as_array)
        .map(|plugins| {
            plugins
                .iter()
                .filter(|plugin| plugin.get("path").and_then(Value::as_str) != Some("builtin"))
                .filter_map(|plugin| plugin.get("name").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    let tools: Vec<&str> = frame
        .get("tools")
        .and_then(Value::as_array)
        .map(|tools| tools.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let own_prefix = format!("mcp__{MCP_SERVER_NAME}__");
    let builtin: Vec<&str> = tools
        .iter()
        .copied()
        .filter(|name| !name.starts_with("mcp__"))
        .collect();
    let orbyn_tools = tools
        .iter()
        .filter(|name| name.starts_with(&own_prefix))
        .count();
    Some(
        AgUiEvent::Custom {
            name: "engine.tools".into(),
            value: json!({
                "engine": "claude",
                "mcp_servers": servers,
                "plugins": plugins,
                "tools": tools.len(),
                "builtin": builtin,
                "orbyn_tools": orbyn_tools,
                "skills": frame.get("skills").and_then(Value::as_array).map_or(0, Vec::len),
            }),
        }
        .into(),
    )
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
        spawn_engine(&command, &args, &spec.cwd)
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

fn probe_failed() -> CoreError {
    CoreError::EngineUnavailable("Claude Code did not report a usable control response.".into())
}

/// Read only the matching success body. Never return a CLI error payload or log
/// frames: initialization and usage replies may also carry account information.
async fn read_control<R: AsyncRead + Unpin>(
    stream: &mut FramedRead<R, LinesCodec>,
    id: &str,
) -> Result<Value> {
    let mut seen = 0;
    while let Some(line) = stream.next().await {
        let line = line.map_err(|_| probe_failed())?;
        seen += line.len() + 1;
        if seen > MAX_INITIALIZE_BYTES {
            return Err(probe_failed());
        }
        let Ok(frame) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if frame.get("type").and_then(Value::as_str) != Some("control_response") {
            continue;
        }
        let Some(reply) = frame.get("response") else {
            continue;
        };
        if reply.get("request_id").and_then(Value::as_str) != Some(id) {
            continue;
        }
        if reply.get("subtype").and_then(Value::as_str) != Some("success") {
            return Err(probe_failed());
        }
        return reply
            .get("response")
            .filter(|body| body.is_object())
            .cloned()
            .ok_or_else(probe_failed);
    }
    Err(probe_failed())
}

/// Turns an `initialize` response into a catalog. Each model's effort list is its
/// own, and a model that reports no effort support gets no effort option.
pub fn parse_capabilities(response: &Value) -> ModelCatalog {
    let list = |key: &str| response.get(key).and_then(Value::as_array);
    let reported = list("models").into_iter().flatten();
    let mut models: Vec<ModelInfo> = reported.filter_map(parse_claude_model).collect();
    if !models.iter().any(|model| model.is_default)
        && let Some(first) = models.first_mut()
    {
        first.is_default = true;
    }
    ModelCatalog {
        models,
        commands: list("commands")
            .into_iter()
            .flatten()
            .filter_map(|command| {
                Some((
                    text_of(command, "name")?,
                    text_of(command, "description").unwrap_or_default(),
                ))
            })
            .collect(),
        agents: list("agents")
            .into_iter()
            .flatten()
            .filter_map(|agent| {
                Some(EngineAgent {
                    name: text_of(agent, "name")?,
                    description: text_of(agent, "description").unwrap_or_default(),
                    model: text_of(agent, "model"),
                })
            })
            .collect(),
        output_styles: list("available_output_styles")
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        output_style: text_of(response, "output_style"),
        ..ModelCatalog::engine(Vec::new())
    }
}

fn parse_claude_model(model: &Value) -> Option<ModelInfo> {
    let id = text_of(model, "value")?;
    let flag = |key: &str| model.get(key).and_then(Value::as_bool) == Some(true);
    let mut options = Vec::new();
    let levels: Vec<&str> = model
        .get("supportedEffortLevels")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if flag("supportsEffort") && !levels.is_empty() {
        options.push(effort_option(levels, None));
    }
    if flag("supportsFastMode") {
        options.push(fast_option());
    }
    Some(ModelInfo {
        label: text_of(model, "displayName").unwrap_or_else(|| id.clone()),
        description: text_of(model, "description"),
        is_default: id == "default",
        custom: false,
        options,
        input_modalities: Vec::new(),
        upgrade: None,
        id,
    })
}

fn fast_option() -> ModelOption {
    let value = |id: &str, label: &str, default: bool| ModelOptionValue {
        id: id.to_owned(),
        label: label.to_owned(),
        default,
        description: None,
    };
    ModelOption {
        id: FAST_MODE.to_owned(),
        label: "Speed".to_owned(),
        values: vec![value("off", "Standard", true), value("on", "Fast", false)],
    }
}

/// The aliases the CLI's docs name, for when it cannot be asked. An alias tracks the
/// vendor's latest, and any full model name also works as a custom model.
fn documented_catalog(reason: String) -> ModelCatalog {
    let model = |id: &str, label: &str, description: &str, is_default: bool| ModelInfo {
        id: id.to_owned(),
        label: label.to_owned(),
        description: Some(description.to_owned()),
        is_default,
        custom: false,
        options: vec![effort_option(DOCUMENTED_EFFORTS.iter().copied(), None)],
        input_modalities: Vec::new(),
        upgrade: None,
    };
    ModelCatalog {
        source: "documented",
        fallback: Some(reason),
        models: vec![
            model(
                "default",
                "Default",
                "What Claude Code picks for your plan",
                true,
            ),
            model("sonnet", "Sonnet", "The latest Sonnet", false),
            model("opus", "Opus", "The latest Opus", false),
            model("haiku", "Haiku", "The latest Haiku", false),
            model(
                "sonnet[1m]",
                "Sonnet (1M context)",
                "Sonnet with a 1M context window",
                false,
            ),
            model(
                "opus[1m]",
                "Opus (1M context)",
                "Opus with a 1M context window",
                false,
            ),
            model(
                "opusplan",
                "Opus for planning",
                "Opus in plan mode, Sonnet otherwise",
                false,
            ),
        ],
        ..ModelCatalog::default()
    }
}

/// What a run takes while it works: steering text and controls.
struct Inputs {
    steering: mpsc::UnboundedReceiver<String>,
    controls: mpsc::UnboundedReceiver<RunControl>,
}

async fn sleep_until(due: Option<tokio::time::Instant>) {
    match due {
        Some(due) => tokio::time::sleep_until(due).await,
        None => std::future::pending().await,
    }
}

async fn run_protocol(
    mut process: EngineProcess,
    spec: RunSpec,
    events: mpsc::Sender<EngineEvent>,
    inputs: Inputs,
    cancel: CancellationToken,
) {
    let Inputs {
        mut steering,
        mut controls,
    } = inputs;
    // The prompt goes over stdin, never argv: it can exceed ARG_MAX and argv is
    // visible to other processes.
    let prompt = user_message(&spec);
    let wrote = async {
        process.stdin.write_all(prompt.as_bytes()).await?;
        process.stdin.write_all(b"\n").await?;
        process.stdin.flush().await
    }
    .await;
    if let Err(error) = wrote {
        tracing::debug!(%error, "could not write the prompt");
    }
    let mut stdin = process.stdin;
    let mut mapper = StreamMapper::default();
    let mut stream = lines(process.stdout);
    let mut outcome = None;
    let mut grace = None;
    let stream_ended = loop {
        tokio::select! {
            biased;
            () = cancel.cancelled() => break false,
            () = sleep_until(grace) => {
                outcome = mapper.held.take();
                break false;
            }
            Some(control) = controls.recv() => {
                let RunControl::StopWorkflow { tool_call_id } = control;
                if let Some(frame) = mapper.workflows.stop_request(&tool_call_id) {
                    if let Err(error) = stdin.write_all(format!("{frame}\n").as_bytes()).await {
                        tracing::debug!(%error, "could not ask Claude to stop a workflow");
                    }
                    let _ = stdin.flush().await;
                } else if events.send(Workflows::already_ended(&tool_call_id)).await.is_err() {
                    return;
                }
            }
            Some(text) = steering.recv() => {
                // The CLI folds a message sent mid-turn into the running turn.
                let frame = format!("{}\n", user_frame(&text));
                if let Err(error) = stdin.write_all(frame.as_bytes()).await {
                    tracing::debug!(%error, "could not steer the run");
                }
                let _ = stdin.flush().await;
            }
            line = stream.next() => {
                let Some(line) = line else { break true };
                let Ok(line) = line else { continue };
                let Ok(frame) = serde_json::from_str::<Value>(&line) else {
                    tracing::debug!("skipping a non-JSON Claude line");
                    continue;
                };
                for event in mapper.on_frame(&frame) {
                    if events.send(event).await.is_err() {
                        return;
                    }
                }
                if let Some(result) = mapper.outcome.take() {
                    outcome = Some(result);
                    break false;
                }
                if mapper.waiting_on_hold() {
                    grace.get_or_insert_with(|| tokio::time::Instant::now() + HOLD_GRACE);
                } else {
                    grace = None;
                }
            }
        }
    };
    // The turn finished before the CLI went away; a workflow it held for is gone.
    let outcome = outcome.or_else(|| mapper.held.take());
    // Closing stdin is the CLI's signal to exit.
    drop(stdin);
    for event in mapper.close_open_blocks() {
        let _ = events.send(event).await;
    }
    for event in mapper.workflows.end_all() {
        let _ = events.send(event).await;
    }
    let cancelled = cancel.is_cancelled();
    let status = match tokio::time::timeout(EXIT_GRACE, process.child.wait()).await {
        Ok(status) => status.ok(),
        Err(_) => {
            process.child.terminate();
            None
        }
    };
    let outcome = match (outcome, cancelled, stream_ended) {
        (_, true, _) => EngineOutcome::Cancelled,
        (Some(result), _, _) => result,
        _ => {
            let message = exit_message(status, &process.stderr).await;
            if message.contains(NO_SESSION) {
                EngineOutcome::SessionLost
            } else {
                EngineOutcome::Failed(message)
            }
        }
    };
    let _ = events.send(EngineEvent::Finished(outcome)).await;
}

/// How the CLI reports a `--resume` id it does not have.
const NO_SESSION: &str = "No conversation found";

/// Slash commands reach the CLI bare, or it would read them as ordinary text.
fn user_message(spec: &RunSpec) -> String {
    match spec.kind {
        RunKind::Turn => user_frame(&spec.prompt),
        _ => user_frame(&spec.user_prompt),
    }
}

fn user_frame(text: &str) -> String {
    json!({ "type": "user", "message": { "role": "user", "content": text } }).to_string()
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

#[derive(Clone)]
enum Block {
    Text(String),
    Reasoning(String),
    Tool(String),
}

/// Maps Claude stream-json frames to events. Text and tool input come from the
/// `stream_event` partials; `assistant` frames only fill in when partials were absent.
#[derive(Default)]
struct StreamMapper {
    blocks: Vec<(u64, Block)>,
    saw_partials: bool,
    outcome: Option<EngineOutcome>,
    /// Plan tools already shown, since Claude can repeat a block across frames.
    plans_seen: HashSet<String>,
    /// Files each open write tool named, until its result arrives.
    writing: HashMap<String, Vec<String>>,
    /// Claude's modern task list, kept in the order it was reported.
    tasks: Vec<TaskStep>,
    /// Task tool calls held until their result reports the new state.
    task_calls: HashMap<String, (String, Value)>,
    /// Background workflows this run launched that still run.
    workflows: Workflows,
    /// A finished turn's outcome, held while a workflow it launched still runs.
    held: Option<EngineOutcome>,
    /// A follow-up turn started after the outcome was held.
    resumed: bool,
}

/// One row of the plan activity fed by Claude's TaskCreate/TaskUpdate/TaskList.
struct TaskStep {
    id: String,
    title: String,
    status: String,
}

impl StreamMapper {
    /// The turn is done and its workflows have ended, and no follow-up turn has
    /// started yet.
    fn waiting_on_hold(&self) -> bool {
        self.held.is_some() && self.workflows.is_empty() && !self.resumed
    }

    fn on_frame(&mut self, frame: &Value) -> Vec<EngineEvent> {
        // Subagent traffic carries a parent; the run shows its top-level turn.
        let nested = frame
            .get("parent_tool_use_id")
            .is_some_and(|parent| !parent.is_null());
        let kind = frame.get("type").and_then(Value::as_str);
        if self.held.is_some()
            && !nested
            && (matches!(kind, Some("assistant" | "stream_event"))
                || (kind == Some("system")
                    && frame.get("subtype").and_then(Value::as_str) == Some("init")))
        {
            self.resumed = true;
        }
        match kind {
            Some("control_response") => Workflows::control_response(frame),
            Some("system")
                if matches!(
                    frame.get("subtype").and_then(Value::as_str),
                    Some("task_notification" | "task_updated")
                ) =>
            {
                self.workflows.system(frame)
            }
            Some("system") if frame.get("subtype").and_then(Value::as_str) == Some("init") => {
                let mut events = session(frame);
                events.extend(init_commands(frame));
                events.extend(init_tools(frame));
                events
            }
            Some("stream_event") if !nested => frame
                .get("event")
                .map_or_else(Vec::new, |event| self.on_stream_event(event)),
            Some("assistant") if !nested => self.on_assistant(frame),
            Some("user") if !nested => {
                let mut events = self.writes_finished(frame);
                events.extend(self.tasks_finished(frame));
                events.extend(on_user(frame));
                events.extend(self.workflows.launched(frame));
                events
            }
            Some("assistant") => vec![
                AgUiEvent::Custom {
                    name: "subagent.activity".into(),
                    value: json!({ "parentToolCallId": frame.get("parent_tool_use_id") }),
                }
                .into(),
            ],
            Some("rate_limit_event") => rejected_limit(frame).into_iter().collect(),
            Some("system")
                if frame.get("subtype").and_then(Value::as_str) == Some("compact_boundary") =>
            {
                vec![
                    AgUiEvent::Custom {
                        name: "context.compacted".into(),
                        value: Value::Null,
                    }
                    .into(),
                ]
            }
            Some("result") => {
                let outcome = result_outcome(frame);
                // The CLI keeps a workflow running past the turn that launched it, and
                // closing its input would end the workflow with it.
                if outcome == EngineOutcome::Completed && !self.workflows.is_empty() {
                    self.held = Some(outcome);
                    self.resumed = false;
                } else {
                    self.held = None;
                    self.outcome = Some(outcome);
                }
                let mut events = session(frame);
                events.extend(frame.get("usage").map(usage_event));
                events
            }
            _ => Vec::new(),
        }
    }

    fn on_stream_event(&mut self, event: &Value) -> Vec<EngineEvent> {
        self.saw_partials = true;
        let index = event.get("index").and_then(Value::as_u64).unwrap_or(0);
        match event.get("type").and_then(Value::as_str) {
            Some("content_block_start") => {
                let Some(block) = event.get("content_block") else {
                    return Vec::new();
                };
                self.open_block(index, block)
            }
            Some("content_block_delta") => {
                let Some(open) = self.block(index) else {
                    return Vec::new();
                };
                let delta = event.get("delta").cloned().unwrap_or(Value::Null);
                match (&open, delta.get("type").and_then(Value::as_str)) {
                    (Block::Text(id), Some("text_delta")) => text_of(&delta, "text")
                        .map(|text| AgUiEvent::TextMessageContent {
                            message_id: id.clone(),
                            delta: text,
                        })
                        .into_iter()
                        .map(Into::into)
                        .collect(),
                    (Block::Reasoning(id), Some("thinking_delta")) => text_of(&delta, "thinking")
                        .map(|text| AgUiEvent::ReasoningMessageContent {
                            message_id: id.clone(),
                            delta: text,
                        })
                        .into_iter()
                        .map(Into::into)
                        .collect(),
                    (Block::Tool(id), Some("input_json_delta")) => text_of(&delta, "partial_json")
                        .map(|text| AgUiEvent::ToolCallArgs {
                            tool_call_id: id.clone(),
                            delta: text,
                        })
                        .into_iter()
                        .map(Into::into)
                        .collect(),
                    _ => Vec::new(),
                }
            }
            Some("content_block_stop") => {
                let position = self.blocks.iter().position(|(open, _)| *open == index);
                position
                    .map(|position| vec![end_event(&self.blocks.remove(position).1).into()])
                    .unwrap_or_default()
            }
            _ => Vec::new(),
        }
    }

    fn block(&self, index: u64) -> Option<Block> {
        self.blocks
            .iter()
            .find(|(open, _)| *open == index)
            .map(|(_, block)| block.clone())
    }

    fn open_block(&mut self, index: u64, block: &Value) -> Vec<EngineEvent> {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                let id = new_id();
                self.blocks.push((index, Block::Text(id.clone())));
                vec![
                    AgUiEvent::TextMessageStart {
                        message_id: id,
                        role: "assistant".into(),
                    }
                    .into(),
                ]
            }
            Some("thinking") => {
                let id = new_id();
                self.blocks.push((index, Block::Reasoning(id.clone())));
                vec![
                    AgUiEvent::ReasoningMessageStart {
                        message_id: id,
                        role: "reasoning".into(),
                    }
                    .into(),
                ]
            }
            Some("tool_use") => {
                let id = text_of(block, "id").unwrap_or_else(new_id);
                self.blocks.push((index, Block::Tool(id.clone())));
                vec![
                    AgUiEvent::ToolCallStart {
                        tool_call_id: id,
                        tool_call_name: text_of(block, "name").unwrap_or_else(|| "tool".into()),
                    }
                    .into(),
                ]
            }
            _ => Vec::new(),
        }
    }

    fn on_assistant(&mut self, frame: &Value) -> Vec<EngineEvent> {
        let content = frame
            .pointer("/message/content")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut events: Vec<EngineEvent> = content
            .iter()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_use"))
            .filter_map(|block| {
                Some(EngineEvent::Item(ItemRecord {
                    id: text_of(block, "id")?,
                    kind: "tool_use".to_owned(),
                    raw: block.clone(),
                    exit_code: None,
                }))
            })
            .collect();
        for block in &content {
            events.extend(self.plan_events(block));
            self.write_started(block);
            self.task_started(block);
        }
        if self.saw_partials {
            return events;
        }
        for block in content {
            match block.get("type").and_then(Value::as_str) {
                Some("text") => {
                    let id = new_id();
                    events.push(
                        AgUiEvent::TextMessageStart {
                            message_id: id.clone(),
                            role: "assistant".into(),
                        }
                        .into(),
                    );
                    events.push(
                        AgUiEvent::TextMessageContent {
                            message_id: id.clone(),
                            delta: text_of(&block, "text").unwrap_or_default(),
                        }
                        .into(),
                    );
                    events.push(AgUiEvent::TextMessageEnd { message_id: id }.into());
                }
                Some("tool_use") => {
                    let id = text_of(&block, "id").unwrap_or_else(new_id);
                    events.push(
                        AgUiEvent::ToolCallStart {
                            tool_call_id: id.clone(),
                            tool_call_name: text_of(&block, "name")
                                .unwrap_or_else(|| "tool".into()),
                        }
                        .into(),
                    );
                    events.push(
                        AgUiEvent::ToolCallArgs {
                            tool_call_id: id.clone(),
                            delta: block.get("input").map(Value::to_string).unwrap_or_default(),
                        }
                        .into(),
                    );
                    events.push(AgUiEvent::ToolCallEnd { tool_call_id: id }.into());
                }
                _ => {}
            }
        }
        events
    }

    /// Remembers what a write tool names until its result says it ran.
    fn write_started(&mut self, block: &Value) {
        if block.get("type").and_then(Value::as_str) != Some("tool_use") {
            return;
        }
        let paths = crate::conversation::checkpoints::claude_write_paths(block);
        if let (false, Some(id)) = (paths.is_empty(), text_of(block, "id")) {
            self.writing.insert(id, paths);
        }
    }

    fn writes_finished(&mut self, frame: &Value) -> Vec<EngineEvent> {
        let empty = Vec::new();
        frame
            .pointer("/message/content")
            .and_then(Value::as_array)
            .unwrap_or(&empty)
            .iter()
            .filter_map(|block| self.writing.remove(block.get("tool_use_id")?.as_str()?))
            .map(EngineEvent::Files)
            .collect()
    }

    /// `TodoWrite` is the plan as a checklist and `ExitPlanMode` carries a proposed
    /// plan. Their input is only whole in the `assistant` frame, which is why this reads
    /// them there and not from the partials.
    fn plan_events(&mut self, block: &Value) -> Vec<EngineEvent> {
        if block.get("type").and_then(Value::as_str) != Some("tool_use") {
            return Vec::new();
        }
        let Some(id) = text_of(block, "id") else {
            return Vec::new();
        };
        let name = block.get("name").and_then(Value::as_str);
        let input = block.get("input").cloned().unwrap_or(Value::Null);
        let events = match name {
            Some("TodoWrite") => vec![
                AgUiEvent::ActivitySnapshot {
                    message_id: "plan".into(),
                    activity_type: "plan".into(),
                    content: todo_content(&input),
                    replace: Some(true),
                }
                .into(),
            ],
            Some("ExitPlanMode") => text_of(&input, "plan")
                .map(|plan| proposed_plan_events(&id, &plan))
                .unwrap_or_default(),
            _ => return Vec::new(),
        };
        if self.plans_seen.insert(id) {
            events
        } else {
            Vec::new()
        }
    }

    /// Claude's modern task tools only report their state in the result, so the
    /// call is held until it arrives.
    fn task_started(&mut self, block: &Value) {
        if block.get("type").and_then(Value::as_str) != Some("tool_use") {
            return;
        }
        let Some(name) = block.get("name").and_then(Value::as_str) else {
            return;
        };
        if !matches!(name, "TaskCreate" | "TaskUpdate" | "TaskList") {
            return;
        }
        if let Some(id) = text_of(block, "id") {
            self.task_calls.insert(
                id,
                (
                    name.to_owned(),
                    block.get("input").cloned().unwrap_or(Value::Null),
                ),
            );
        }
    }

    fn tasks_finished(&mut self, frame: &Value) -> Vec<EngineEvent> {
        let Some(content) = frame.pointer("/message/content").and_then(Value::as_array) else {
            return Vec::new();
        };
        let mut changed = false;
        for block in content {
            if block.get("type").and_then(Value::as_str) != Some("tool_result") {
                continue;
            }
            let Some(id) = text_of(block, "tool_use_id") else {
                continue;
            };
            let Some((name, input)) = self.task_calls.remove(&id) else {
                continue;
            };
            if block.get("is_error").and_then(Value::as_bool) == Some(true) {
                continue;
            }
            let result = task_result(frame, block);
            changed |= self.apply_task(&name, &input, &result);
        }
        if !changed {
            return Vec::new();
        }
        vec![
            AgUiEvent::ActivitySnapshot {
                message_id: "plan".into(),
                activity_type: "plan".into(),
                content: task_content(&self.tasks),
                replace: Some(true),
            }
            .into(),
        ]
    }

    fn apply_task(&mut self, name: &str, input: &Value, result: &Value) -> bool {
        match name {
            "TaskList" => {
                let Some(listed) = result.get("tasks").and_then(Value::as_array) else {
                    return false;
                };
                let mut next = Vec::new();
                for task in listed {
                    let Some(id) = text_of(task, "id") else {
                        continue;
                    };
                    next.push(TaskStep {
                        id,
                        title: text_of(task, "subject").unwrap_or_default(),
                        status: task_status(text_of(task, "status").as_deref())
                            .unwrap_or("pending")
                            .to_owned(),
                    });
                }
                self.tasks = next;
                true
            }
            "TaskCreate" => {
                let created = result.get("task");
                let from_text = created_from_text(result);
                let id = created
                    .and_then(|task| text_of(task, "id"))
                    .or_else(|| from_text.as_ref().map(|(id, _)| id.clone()));
                let title = created
                    .and_then(|task| text_of(task, "subject"))
                    .or_else(|| from_text.map(|(_, subject)| subject))
                    .or_else(|| text_of(input, "subject"));
                let (Some(id), Some(title)) = (id, title) else {
                    return false;
                };
                match self.tasks.iter_mut().find(|task| task.id == id) {
                    Some(task) => task.title = title,
                    None => self.tasks.push(TaskStep {
                        id,
                        title,
                        status: "pending".into(),
                    }),
                }
                true
            }
            "TaskUpdate" => {
                if result.get("success").and_then(Value::as_bool) == Some(false) {
                    return false;
                }
                if let Some(text) = result.as_str()
                    && !text.starts_with("Updated task #")
                {
                    return false;
                }
                let id = text_of(input, "taskId").or_else(|| text_of(result, "taskId"));
                let Some(index) =
                    id.and_then(|id| self.tasks.iter().position(|task| task.id == id))
                else {
                    return false;
                };
                let native = text_of(input, "status");
                if native.as_deref() == Some("deleted") {
                    self.tasks.remove(index);
                    return true;
                }
                let title =
                    text_of(input, "subject").unwrap_or_else(|| self.tasks[index].title.clone());
                let status = native
                    .as_deref()
                    .and_then(|value| task_status(Some(value)))
                    .map(str::to_owned)
                    .unwrap_or_else(|| self.tasks[index].status.clone());
                let changed =
                    self.tasks[index].title != title || self.tasks[index].status != status;
                self.tasks[index].title = title;
                self.tasks[index].status = status;
                changed
            }
            _ => false,
        }
    }

    fn close_open_blocks(&mut self) -> Vec<EngineEvent> {
        self.blocks
            .drain(..)
            .map(|(_, block)| end_event(&block).into())
            .collect()
    }
}

/// `TodoWrite`'s list in the `plan` activity's shape. Its statuses are the same words.
fn todo_content(input: &Value) -> Value {
    let steps: Vec<Value> = input
        .get("todos")
        .and_then(Value::as_array)
        .map(|todos| {
            todos
                .iter()
                .map(|todo| {
                    json!({
                        "title": text_of(todo, "content").unwrap_or_default(),
                        "status": text_of(todo, "status").unwrap_or_else(|| "pending".into()),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    json!({ "steps": steps })
}

/// The structured result when Claude sends one, otherwise the text the tool
/// result carries.
fn task_result(frame: &Value, block: &Value) -> Value {
    if let Some(result) = frame
        .get("tool_use_result")
        .filter(|value| !value.is_null())
    {
        return result.clone();
    }
    match block.get("content") {
        Some(Value::Array(parts)) => Value::String(
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(""),
        ),
        Some(value) => value.clone(),
        None => Value::Null,
    }
}

/// A subagent result carries no structured task, only the created line.
fn created_from_text(result: &Value) -> Option<(String, String)> {
    let rest = result.as_str()?.strip_prefix("Task #")?;
    let (id, subject) = rest.split_once(" created successfully: ")?;
    Some((id.trim().to_owned(), subject.trim().to_owned()))
}

fn task_status(value: Option<&str>) -> Option<&'static str> {
    match value {
        Some("pending") => Some("pending"),
        Some("in_progress") => Some("running"),
        Some("completed") => Some("completed"),
        _ => None,
    }
}

/// The task list in the same `plan` activity shape `TodoWrite` produces.
fn task_content(tasks: &[TaskStep]) -> Value {
    let steps: Vec<Value> = tasks
        .iter()
        .map(|task| json!({ "title": task.title, "status": task.status }))
        .collect();
    json!({ "steps": steps })
}

fn end_event(block: &Block) -> AgUiEvent {
    match block {
        Block::Text(id) => AgUiEvent::TextMessageEnd {
            message_id: id.clone(),
        },
        Block::Reasoning(id) => AgUiEvent::ReasoningMessageEnd {
            message_id: id.clone(),
        },
        Block::Tool(id) => AgUiEvent::ToolCallEnd {
            tool_call_id: id.clone(),
        },
    }
}

fn usage_event(usage: &Value) -> EngineEvent {
    let count = |key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0);
    let total = count("input_tokens")
        + count("output_tokens")
        + count("cache_read_input_tokens")
        + count("cache_creation_input_tokens");
    EngineEvent::Usage(ThreadUsage {
        total_tokens: Some(total),
        context_window: None,
        raw: usage.clone(),
    })
}

fn session(frame: &Value) -> Vec<EngineEvent> {
    text_of(frame, "session_id")
        .map(EngineEvent::Session)
        .into_iter()
        .collect()
}

/// The init frame lists names alone.
fn init_commands(frame: &Value) -> Option<EngineEvent> {
    let names = frame.get("slash_commands")?.as_array()?;
    Some(EngineEvent::Commands(
        names
            .iter()
            .filter_map(Value::as_str)
            .map(|name| (name.to_owned(), String::new()))
            .collect(),
    ))
}

fn text_of(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn on_user(frame: &Value) -> Vec<EngineEvent> {
    let Some(content) = frame.pointer("/message/content").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut events = Vec::new();
    for block in content
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_result"))
    {
        let Some(id) = text_of(block, "tool_use_id") else {
            continue;
        };
        events.push(
            AgUiEvent::ToolCallResult {
                message_id: new_id(),
                tool_call_id: id.clone(),
                content: tool_text(block.get("content")),
                role: Some("tool".into()),
            }
            .into(),
        );
        let outcome = match block.get("is_error") {
            None | Some(Value::Bool(false)) => Some("success"),
            Some(Value::Bool(true)) => Some("failure"),
            _ => None,
        };
        if let Some(outcome) = outcome {
            events.push(
                AgUiEvent::Custom {
                    name: "tool.outcome".into(),
                    value: json!({ "toolCallId": id, "outcome": outcome }),
                }
                .into(),
            );
        }
    }
    events
}

fn tool_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

/// Claude reports limits as it goes; only a rejected one stops the turn.
fn rejected_limit(frame: &Value) -> Option<EngineEvent> {
    let info = frame.get("rate_limit_info")?;
    (info.get("status").and_then(Value::as_str) == Some("rejected")).then(|| EngineEvent::Limited {
        resets_at: info.get("resetsAt").and_then(Value::as_i64),
    })
}

fn result_outcome(frame: &Value) -> EngineOutcome {
    let failed = frame.get("is_error").and_then(Value::as_bool) == Some(true)
        || frame
            .get("subtype")
            .and_then(Value::as_str)
            .is_some_and(|subtype| subtype != "success");
    if !failed {
        return EngineOutcome::Completed;
    }
    let lost = |text: &str| text.contains(NO_SESSION);
    let errors = frame.get("errors").and_then(Value::as_array).map(|errors| {
        errors
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join("; ")
    });
    let message = errors
        .filter(|text| !text.is_empty())
        .or_else(|| text_of(frame, "result").filter(|text| !text.is_empty()))
        .or_else(|| text_of(frame, "subtype"))
        .unwrap_or_else(|| "Claude ended the turn with an error.".to_owned());
    if lost(&message) {
        return EngineOutcome::SessionLost;
    }
    EngineOutcome::Failed(message)
}

#[cfg(test)]
mod tests;
