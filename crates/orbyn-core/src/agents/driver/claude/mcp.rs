use orbyn_contracts::AgUiEvent;
use serde_json::{Value, json};

use super::settings::{MAX_SEEN_SERVERS, rule_name, seen_servers};
use crate::agents::driver::{EngineEvent, MCP_SERVER_NAME};

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
pub(super) async fn list_mcp(program: &str) -> Option<crate::agents::tools_report::ListedServers> {
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
pub(super) fn claude_tools(
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

pub(super) static SERVERS_LEARNED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Asks the CLI which MCP servers it will load and remembers their names for the
/// approval guard. The init frame of a run that passes `--mcp-config` lists only that
/// one server, and connectors appear after it, so the frame cannot name them. About
/// four seconds; only names are kept.
pub(super) async fn learn_servers(program: &str) {
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
pub(super) fn init_tools(frame: &Value) -> Option<EngineEvent> {
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
